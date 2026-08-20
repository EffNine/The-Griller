//! The normalized task identity model.
//!
//! A task arrives as natural language. Normalization turns it into candidate
//! entities (project names, paths, modules, symbols, issue ids, domain terms,
//! technologies) that the deterministic matcher can reason about. The
//! verbatim original task text is **never** discarded.
//!
//! ## Trust boundary
//!
//! * An LLM may propose entities via [`crate::llm`]; everything it says is a
//!   *candidate*. The deterministic matcher decides relevance.
//! * An LLM can never propose a context status, a severity, or a verdict —
//!   the [`TaskIdentity`] type cannot even represent them.
//! * `content_tokens` are always derived deterministically from the original
//!   text; they are never taken from the LLM.

use serde::{Deserialize, Serialize};

use crate::name::normalize_name;

/// How the normalized task was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMode {
    /// Deterministic rule-based normalization (no LLM involved).
    Deterministic,
    /// An LLM proposed candidate entities; the output passed the LLM boundary.
    Llm,
}

/// Structural quality of the normalization.
///
/// This is a structural signal about how much of the task could be turned
/// into machine structure. It is **not** a probability and it never directly
/// influences the context status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationQuality {
    /// At least one informative entity or content token was extracted.
    Complete,
    /// Structure was extracted but it is weak or ambiguous.
    Partial,
    /// No informative signal could be extracted; the task is opaque/generic.
    Unrecognized,
}

/// The kind of a candidate task entity.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EntityKind {
    /// A project/repository/product name candidate (explicit, capitalized or
    /// decorated). This is the kind that can produce an explicit mismatch.
    Project,
    /// A file path reference (contains `/`, a dotted extension, or `./`).
    Path,
    /// A module / crate / package reference.
    Module,
    /// A function / class / symbol reference.
    Symbol,
    /// An issue / task identifier (e.g. `#42`, `PROJ-123`, `P10.5.1`).
    Issue,
    /// A feature / domain term.
    DomainTerm,
    /// An explicit technology name (e.g. `rust`, `cargo`, `mcp`).
    Technology,
    /// An explicit subsystem name.
    Subsystem,
}

/// A single candidate entity extracted from a task.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct TaskEntity {
    /// The entity kind.
    pub kind: EntityKind,
    /// The raw text as it appears in the task.
    pub name: String,
    /// The normalized form used for comparison.
    pub normalized: String,
}

impl TaskEntity {
    /// Build an entity, computing the normalized form.
    pub fn new(kind: EntityKind, name: impl Into<String>) -> Self {
        let name = name.into();
        let normalized = normalize_entity(&name);
        Self {
            kind,
            name,
            normalized,
        }
    }
}

/// Normalize an entity name for comparison.
pub fn normalize_entity(name: &str) -> String {
    normalize_name(name)
}

/// The result of normalizing a task.
///
/// The verbatim original text is always preserved in [`TaskIdentity::original`];
/// it is never thrown away, regardless of how the structure was produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct TaskIdentity {
    /// The verbatim original task text.
    pub original: String,
    /// Candidate entities, sorted by (kind, normalized) and deduplicated.
    pub entities: Vec<TaskEntity>,
    /// Informative normalized content tokens (deterministic, never from an
    /// LLM), sorted and deduplicated.
    pub content_tokens: Vec<String>,
    /// How the structure was produced.
    pub mode: NormalizationMode,
    /// Structural quality signal.
    pub quality: NormalizationQuality,
}

impl TaskIdentity {
    /// True when the task carries no informative signal at all.
    pub fn has_signal(&self) -> bool {
        !self.content_tokens.is_empty() || !self.entities.is_empty()
    }

    /// All entities of the given kind.
    pub fn entities_of(&self, kind: EntityKind) -> impl Iterator<Item = &TaskEntity> {
        self.entities.iter().filter(move |e| e.kind == kind)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entity_normalization_applies_name_rules() {
        let e = TaskEntity::new(EntityKind::Project, "CodeBro");
        assert_eq!(e.normalized, "codebro");
        let e = TaskEntity::new(EntityKind::Path, "./src/main.rs");
        assert_eq!(e.name, "./src/main.rs");
    }
}
