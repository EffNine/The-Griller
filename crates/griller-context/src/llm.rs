//! The LLM output boundary for task normalization.
//!
//! LLM task-normalization output is **untrusted data**. An LLM may propose
//! candidate task entities. It may **not** propose a context status, a
//! severity, or a verdict — those are decided exclusively by the
//! deterministic layers.
//!
//! The architectural guarantee is twofold:
//!
//! 1. [`LlmTaskProposal`] contains only entity fields and is parsed with
//!    `deny_unknown_fields`, so fields such as `status`, `context_status`,
//!    `verdict`, `severity` or `confidence` are **rejected outright** by
//!    deserialization.
//! 2. [`TaskIdentity`] cannot even represent a status or verdict, so no
//!    amount of malformed output can influence status assignment through this
//!    path.
//!
//! A well-formed proposal is applied as an *overlay* on top of the
//! deterministic normalization: proposed entities are merged with the
//! deterministic candidates, and the verbatim original text always comes from
//! the caller, never from the LLM.

use crate::error::NormalizationError;
use crate::name::normalize_name;
use crate::task::{EntityKind, TaskIdentity};
use serde::{Deserialize, Serialize};

/// The only shape accepted from an LLM task normalizer.
///
/// `deny_unknown_fields` makes status/verdict/severity injection a hard
/// deserialization error rather than silently-ignored data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LlmTaskProposal {
    /// Proposed candidate entities. Only entity information is accepted.
    #[serde(default)]
    pub entities: Vec<LlmEntityProposal>,
}

/// A single candidate entity proposed by the LLM.
///
/// `deny_unknown_fields` rejects verdict/status fields even at entity level.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LlmEntityProposal {
    /// The proposed entity name (required).
    pub name: String,
    /// Proposed kind: `project` | `path` | `module` | `symbol` | `issue` |
    /// `domain_term` | `technology` | `subsystem`. Unknown values cause the
    /// entity to be ignored (it is only a candidate).
    #[serde(default)]
    pub kind: Option<String>,
}

/// Parse raw LLM output strictly.
///
/// Any unknown field (e.g. `status`, `context_status`, `verdict`,
/// `severity`, `confidence`) causes a parse error. The caller decides how to
/// handle it; the recommended behaviour is deterministic fallback (see
/// [`crate::normalizer::FallbackTaskNormalizer`]).
pub fn parse_llm_task_proposal(raw: &str) -> Result<LlmTaskProposal, NormalizationError> {
    serde_json::from_str(raw).map_err(|err| {
        NormalizationError::InvalidLlmOutput(format!(
            "untrusted LLM task-normalization output rejected: {err}"
        ))
    })
}

/// Parse an entity-kind token produced by an LLM.
pub fn parse_entity_kind(value: &str) -> Option<EntityKind> {
    match value.trim().to_lowercase().as_str() {
        "project" | "repository" | "product" | "repo" => Some(EntityKind::Project),
        "path" | "file" => Some(EntityKind::Path),
        "module" | "crate" | "package" => Some(EntityKind::Module),
        "symbol" | "function" | "method" | "class" => Some(EntityKind::Symbol),
        "issue" | "task" | "ticket" => Some(EntityKind::Issue),
        "domain_term" | "domain" | "feature" => Some(EntityKind::DomainTerm),
        "technology" | "tech" | "ecosystem" => Some(EntityKind::Technology),
        "subsystem" => Some(EntityKind::Subsystem),
        _ => None,
    }
}

/// Overlay LLM-proposed entities on top of a deterministic base task identity.
///
/// Only entities are merged (deduplicated by `(kind, normalized)`). The
/// verbatim original text and `content_tokens` always come from the base,
/// never from the LLM. Mode is upgraded to `Llm` when the proposal carried at
/// least one well-formed entity.
pub fn apply_entity_proposals(base: &TaskIdentity, proposal: &LlmTaskProposal) -> TaskIdentity {
    let mut entities = base.entities.clone();
    let mut accepted = false;
    for proposed in &proposal.entities {
        let name = proposed.name.trim();
        if name.is_empty() {
            continue;
        }
        let kind = proposed
            .kind
            .as_deref()
            .and_then(parse_entity_kind)
            .unwrap_or(EntityKind::DomainTerm);
        let normalized = normalize_name(name);
        if normalized.is_empty() {
            continue;
        }
        let duplicate = entities
            .iter()
            .any(|e| e.kind == kind && e.normalized == normalized);
        if !duplicate {
            entities.push(crate::task::TaskEntity::new(kind, name));
            accepted = true;
        }
    }
    entities
        .sort_by(|a, b| (a.kind, &a.normalized, &a.name).cmp(&(b.kind, &b.normalized, &b.name)));
    entities.dedup();

    let mode = if accepted {
        crate::task::NormalizationMode::Llm
    } else {
        base.mode
    };

    TaskIdentity {
        original: base.original.clone(),
        entities,
        content_tokens: base.content_tokens.clone(),
        mode,
        quality: base.quality,
    }
}

/// An injectable, LLM-backed task normalizer used for testing and for future
/// providers.
///
/// Like M2, there is deliberately **no provider** in M4. This struct models
/// the boundary: it holds the raw output an LLM produced and feeds it through
/// [`parse_llm_task_proposal`]. A future provider adapter constructs it the
/// same way: call the model, then parse through the strict boundary.
#[derive(Debug, Clone)]
pub struct LlmTaskNormalizer {
    /// The canned, untrusted raw LLM output.
    raw: String,
}

impl LlmTaskNormalizer {
    /// Create a normalizer that will consume the given raw LLM output.
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}

impl crate::normalizer::TaskContextNormalizer for LlmTaskNormalizer {
    fn normalize(&self, text: &str) -> Result<TaskIdentity, NormalizationError> {
        let proposal = parse_llm_task_proposal(&self.raw)?;
        let base = crate::normalizer::normalize_deterministic(text);
        Ok(apply_entity_proposals(&base, &proposal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::normalizer::{normalize_deterministic, TaskContextNormalizer};

    #[test]
    fn well_formed_entity_proposal_is_accepted() {
        let raw = r#"{"entities":[{"name":"Atlas","kind":"project"},{"name":"fact_store","kind":"module"}]}"#;
        let normalizer = LlmTaskNormalizer::new(raw);
        let identity = normalizer
            .normalize("Implement P10.5.1 Fact Store.")
            .unwrap();
        assert_eq!(identity.mode, crate::task::NormalizationMode::Llm);
        assert!(identity
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Project && e.normalized == "atlas"));
        assert!(identity
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Module && e.name == "fact_store"));
        // Original text is preserved from the caller, never the LLM.
        assert_eq!(identity.original, "Implement P10.5.1 Fact Store.");
    }

    #[test]
    fn verdict_and_status_fields_are_rejected() {
        let raw = r#"{
            "entities":[{"name":"Atlas","kind":"project"}],
            "context_status":"MATCHED",
            "verdict":"PASS",
            "severity":"low"
        }"#;
        let normalizer = LlmTaskNormalizer::new(raw);
        let err = normalizer
            .normalize("Implement P10.5.1 Fact Store.")
            .unwrap_err();
        assert!(
            matches!(err, NormalizationError::InvalidLlmOutput(_)),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn entity_level_verdict_fields_are_rejected() {
        let raw = r#"{"entities":[{"name":"Atlas","kind":"project","verdict":"PASS"}]}"#;
        let normalizer = LlmTaskNormalizer::new(raw);
        let err = normalizer
            .normalize("Implement P10.5.1 Fact Store.")
            .unwrap_err();
        assert!(matches!(err, NormalizationError::InvalidLlmOutput(_)));
    }

    #[test]
    fn fallback_degrades_on_malicious_output() {
        let raw = r#"{"context_status":"MATCHED","verdict":"PASS"}"#;
        let fallback = crate::normalizer::FallbackTaskNormalizer::over(Some(Box::new(
            LlmTaskNormalizer::new(raw),
        )));
        let identity = fallback
            .normalize("Implement Atlas P10.5.1 Fact Store.")
            .unwrap();
        // Degraded to the deterministic path: no status anywhere, correct
        // deterministic entities.
        assert_eq!(identity.mode, crate::task::NormalizationMode::Deterministic);
        assert!(identity
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Project && e.normalized == "atlas"));
    }

    #[test]
    fn original_text_never_comes_from_llm() {
        let raw = r#"{"entities":[{"name":"LIES","kind":"project"}]}"#;
        let normalizer = LlmTaskNormalizer::new(raw);
        let identity = normalizer
            .normalize("Fix the real bug in the repo.")
            .unwrap();
        assert_eq!(identity.original, "Fix the real bug in the repo.");
        assert!(identity
            .entities
            .iter()
            .any(|e| e.kind == EntityKind::Project && e.normalized == "lies"));
        // Content tokens are deterministic, from the original text only.
        assert!(identity.content_tokens.contains(&"bug".to_string()));
        assert!(!identity.content_tokens.contains(&"lies".to_string()));
    }

    #[test]
    fn overlay_deduplicates_and_sorts() {
        let raw =
            r#"{"entities":[{"name":"Atlas","kind":"project"},{"name":"atlas","kind":"project"}]}"#;
        let base = normalize_deterministic("Implement Atlas Fact Store.");
        let proposal = parse_llm_task_proposal(raw).unwrap();
        let merged = apply_entity_proposals(&base, &proposal);
        let projects: Vec<String> = merged
            .entities_of(EntityKind::Project)
            .map(|e| e.normalized.clone())
            .collect();
        assert_eq!(projects, vec!["atlas".to_string()]);
    }
}
