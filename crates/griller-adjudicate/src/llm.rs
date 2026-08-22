//! The LLM output boundary.
//!
//! LLM normalization output is **untrusted data**. An LLM may propose claim
//! structure (subject, predicate, object, target, polarity, kind). It may
//! **not** propose an epistemic status, a verdict, or a truth assignment —
//! those are decided exclusively by the deterministic adjudication layer.
//!
//! The architectural guarantee is twofold:
//!
//! 1. [`LlmClaimProposal`] contains only claim-structure fields and is parsed
//!    with `deny_unknown_fields`, so fields such as `epistemic_status`,
//!    `status` or `verdict` are **rejected outright** by deserialization.
//! 2. [`NormalizedClaim`] cannot even represent an epistemic status, so no
//!    amount of malformed output can influence adjudication through this path.
//!
//! A well-formed proposal is applied as an *overlay* on top of the
//! deterministic normalization: missing fields fall back to deterministic
//! values, and the original verbatim text always comes from the
//! [`ClaimInput`], never from the LLM.

use crate::claim::{ClaimInput, ClaimKind, ClaimTarget, TargetScope};
use crate::error::NormalizationError;
use crate::normalized::{parse_polarity, NormalizationMode, NormalizationQuality, NormalizedClaim};
use crate::predicate::Proposition;
use serde::{Deserialize, Serialize};

/// The only shape accepted from an LLM normalizer.
///
/// `deny_unknown_fields` makes epistemic-status / verdict injection a hard
/// deserialization error rather than silently-ignored data.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case", deny_unknown_fields)]
pub struct LlmClaimProposal {
    /// The claim text as understood by the LLM. Used only as a cross-check;
    /// the authoritative verbatim text always comes from the [`ClaimInput`].
    #[serde(default)]
    pub claim: Option<String>,
    /// Proposed kind (`completion`, `fact`, `causal`, `requirement`, `design`).
    #[serde(default)]
    pub kind: Option<String>,
    /// Proposed canonical subject.
    #[serde(default)]
    pub subject: Option<String>,
    /// Proposed canonical predicate verb.
    #[serde(default)]
    pub predicate: Option<String>,
    /// Proposed object.
    #[serde(default)]
    pub object: Option<String>,
    /// Proposed polarity (`positive` | `negative`).
    #[serde(default)]
    pub polarity: Option<String>,
    /// Proposed target scope (`repository` | `file` | `function` | `module` |
    /// `command` | `test` | `artifact`).
    #[serde(default)]
    pub target_scope: Option<String>,
    /// Proposed target name.
    #[serde(default)]
    pub target_name: Option<String>,
}

/// Parse raw LLM output strictly.
///
/// Any unknown field (e.g. `epistemic_status`, `status`, `verdict`,
/// `confidence`) causes a parse error. The caller decides how to handle it;
/// the recommended behaviour is deterministic fallback (see
/// [`crate::normalizer::FallbackNormalizer`]).
pub fn parse_llm_proposal(raw: &str) -> Result<LlmClaimProposal, NormalizationError> {
    serde_json::from_str(raw).map_err(|err| {
        NormalizationError::InvalidLlmOutput(format!(
            "untrusted LLM normalization output rejected: {err}"
        ))
    })
}

/// Overlay a parsed proposal on top of a deterministic base normalization.
///
/// Fields present in the proposal replace the deterministic values; missing
/// fields keep the deterministic fallback. The original text is always the
/// base's original text.
pub fn apply_proposal(base: &NormalizedClaim, proposal: &LlmClaimProposal) -> NormalizedClaim {
    let kind = proposal
        .kind
        .as_deref()
        .and_then(parse_kind)
        .unwrap_or(base.kind);

    let target = {
        let scope = proposal
            .target_scope
            .as_deref()
            .and_then(parse_scope)
            .unwrap_or(base.target.scope);
        let name = match (&proposal.target_name, &base.target.name) {
            (Some(n), _) if !n.is_empty() => Some(n.clone()),
            (None, b) => b.clone(),
            _ => base.target.name.clone(),
        };
        ClaimTarget::new(scope, name)
    };

    let proposition = match &proposal.predicate {
        Some(pred) if !pred.trim().is_empty() => {
            let polarity = proposal
                .polarity
                .as_deref()
                .and_then(parse_polarity)
                .unwrap_or(base.proposition.polarity);
            let subject = proposal
                .subject
                .as_deref()
                .filter(|s| !s.trim().is_empty())
                .unwrap_or(&base.proposition.subject)
                .to_string();
            let object = match &proposal.object {
                Some(o) if !o.trim().is_empty() => Some(o.clone()),
                _ => base.proposition.object.clone(),
            };
            Proposition {
                subject,
                predicate: pred.trim().to_lowercase(),
                polarity,
                object,
            }
        }
        _ => base.proposition.clone(),
    };

    // The LLM contributed a full subject+predicate: mark quality Complete if
    // the deterministic pass was at least partial.
    let quality = if proposal.predicate.is_some() && proposal.subject.is_some() {
        NormalizationQuality::Complete
    } else {
        base.quality
    };

    NormalizedClaim::new(
        base.original.clone(),
        base.source,
        kind,
        target,
        proposition,
        base.requirement_provenance,
        NormalizationMode::Llm,
        quality,
        Vec::new(),
    )
}

/// Parse a claim-kind token produced by an LLM.
pub fn parse_kind(value: &str) -> Option<ClaimKind> {
    match value.trim().to_lowercase().as_str() {
        "completion" => Some(ClaimKind::Completion),
        "fact" => Some(ClaimKind::Fact),
        "causal" => Some(ClaimKind::Causal),
        "requirement" => Some(ClaimKind::Requirement),
        "design" => Some(ClaimKind::Design),
        _ => None,
    }
}

/// Parse a target-scope token produced by an LLM.
pub fn parse_scope(value: &str) -> Option<TargetScope> {
    match value.trim().to_lowercase().as_str() {
        "repository" => Some(TargetScope::Repository),
        "file" => Some(TargetScope::File),
        "function" => Some(TargetScope::Function),
        "module" => Some(TargetScope::Module),
        "command" => Some(TargetScope::Command),
        "test" => Some(TargetScope::Test),
        "artifact" => Some(TargetScope::Artifact),
        _ => None,
    }
}

/// An injectable, LLM-backed normalizer used for testing and for future
/// providers.
///
/// In M2 there is deliberately **no provider**. This struct models the
/// boundary: it holds the raw output an LLM produced and feeds it through
/// [`parse_llm_proposal`]. A future provider adapter constructs it the same
/// way: call the model, then parse through the strict boundary.
#[derive(Debug, Clone)]
pub struct LlmNormalizer {
    /// The canned, untrusted raw LLM output.
    raw: String,
}

impl LlmNormalizer {
    /// Create a normalizer that will consume the given raw LLM output.
    pub fn new(raw: impl Into<String>) -> Self {
        Self { raw: raw.into() }
    }
}

impl crate::normalizer::ClaimNormalizer for LlmNormalizer {
    fn normalize(&self, input: &ClaimInput) -> Result<NormalizedClaim, NormalizationError> {
        let proposal = parse_llm_proposal(&self.raw)?;
        let base = crate::normalizer::normalize_deterministic(input);
        Ok(apply_proposal(&base, &proposal))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::ClaimSource;
    use crate::normalizer::{ClaimNormalizer, DeterministicNormalizer};
    use crate::predicate::Polarity;

    #[test]
    fn well_formed_proposal_is_accepted() {
        let raw = r#"{
            "claim": "tests pass",
            "kind": "completion",
            "subject": "tests",
            "predicate": "pass",
            "polarity": "positive",
            "target_scope": "test"
        }"#;
        let input = ClaimInput::new("tests pass", ClaimSource::Agent);
        let nc = LlmNormalizer::new(raw).normalize(&input).unwrap();
        assert_eq!(nc.mode, NormalizationMode::Llm);
        assert_eq!(nc.proposition.predicate, "pass");
        assert_eq!(nc.proposition.subject, "tests");
        assert_eq!(nc.target.scope, TargetScope::Test);
        assert_eq!(nc.original, "tests pass");
    }

    #[test]
    fn epistemic_status_field_is_rejected() {
        let raw = r#"{
            "claim": "tests pass",
            "subject": "tests",
            "predicate": "pass",
            "epistemic_status": "VERIFIED",
            "verdict": "BLOCK"
        }"#;
        let input = ClaimInput::new("tests pass", ClaimSource::Agent);
        let err = LlmNormalizer::new(raw).normalize(&input).unwrap_err();
        assert!(
            matches!(err, NormalizationError::InvalidLlmOutput(_)),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn fallback_degrades_on_malicious_output() {
        let raw = r#"{"claim":"tests pass","epistemic_status":"VERIFIED","verdict":"BLOCK"}"#;
        let fallback =
            crate::normalizer::FallbackNormalizer::over(Some(Box::new(LlmNormalizer::new(raw))));
        let input = ClaimInput::new("tests pass", ClaimSource::Agent);
        let nc = fallback.normalize(&input).unwrap();
        // Degraded to the deterministic path: no status anywhere, correct
        // structure from deterministic rules.
        assert_eq!(nc.mode, NormalizationMode::Deterministic);
        assert_eq!(nc.proposition.predicate, "pass");
        assert_eq!(nc.proposition.polarity, Polarity::Positive);
    }

    #[test]
    fn proposal_overlay_fills_missing_fields() {
        let raw = r#"{"subject":"integration","predicate":"pass"}"#;
        let input = ClaimInput::new("Integration tests pass.", ClaimSource::User);
        let nc = LlmNormalizer::new(raw).normalize(&input).unwrap();
        assert_eq!(nc.proposition.subject, "integration");
        assert_eq!(nc.proposition.predicate, "pass");
        // Kind and scope fall back to deterministic inference.
        assert_eq!(nc.target.scope, TargetScope::Test);
        assert_eq!(nc.kind, ClaimKind::Completion);
    }

    #[test]
    fn deterministic_normalizer_is_available_without_llm() {
        let nc = DeterministicNormalizer
            .normalize(&ClaimInput::new("Build succeeds.", ClaimSource::User))
            .unwrap();
        assert_eq!(nc.proposition.predicate, "build");
        assert_eq!(nc.mode, NormalizationMode::Deterministic);
    }
}
