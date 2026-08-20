//! Structured context evidence.
//!
//! Evidence is the explainable core of M4. Nothing is reduced to a single
//! similarity score: every signal is a typed [`ContextEvidence`] with a
//! direction, kind, subject, strength, evidence references and an
//! explanation. The final [`ContextAssessment`](crate::assess::ContextAssessment)
//! preserves all of it.

use serde::{Deserialize, Serialize};

/// The direction of a piece of context evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceDirection {
    /// The evidence supports the task belonging to this repository.
    Positive,
    /// The evidence opposes the task belonging to this repository.
    Negative,
}

/// The kind of context evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextEvidenceKind {
    // --- Positive ---------------------------------------------------------
    /// The task explicitly names this repository's project.
    ExplicitProjectMatch,
    /// A manifest/package/crate name matches.
    ManifestMatch,
    /// A git remote repository name matches.
    RemoteMatch,
    /// A referenced file/path exists in the repository.
    ReferencedPathExists,
    /// A referenced module/directory exists.
    ReferencedModuleExists,
    /// A referenced symbol/function/class exists.
    ReferencedSymbolExists,
    /// A relevant manifest/package is present.
    ManifestPresent,
    /// Feature/domain terminology overlap with the repository.
    DomainTermMatch,
    /// The repository's technology stack is compatible with the task.
    TechnologyCompatibility,
    /// A task entity (typically a foreign project name) has weak presence in
    /// the repository (path component, symbol, vocabulary).
    EntityPresentInRepository,
    // --- Negative ---------------------------------------------------------
    /// The task explicitly names another project/repository with no
    /// supporting presence in this repository. Strong, block-worthy signal.
    ExplicitProjectMismatch,
    /// The task names another project that has only weak supporting presence
    /// here — a structured conflict rather than a clean mismatch.
    ConflictingEntityReference,
    /// An explicitly referenced entity (path/module/symbol) is absent.
    ReferencedEntityAbsent,
}

impl ContextEvidenceKind {
    /// Stable machine-readable code.
    pub fn code(self) -> &'static str {
        match self {
            ContextEvidenceKind::ExplicitProjectMatch => "explicit_project_match",
            ContextEvidenceKind::ManifestMatch => "manifest_match",
            ContextEvidenceKind::RemoteMatch => "remote_match",
            ContextEvidenceKind::ReferencedPathExists => "referenced_path_exists",
            ContextEvidenceKind::ReferencedModuleExists => "referenced_module_exists",
            ContextEvidenceKind::ReferencedSymbolExists => "referenced_symbol_exists",
            ContextEvidenceKind::ManifestPresent => "manifest_present",
            ContextEvidenceKind::DomainTermMatch => "domain_term_match",
            ContextEvidenceKind::TechnologyCompatibility => "technology_compatibility",
            ContextEvidenceKind::EntityPresentInRepository => "entity_present_in_repository",
            ContextEvidenceKind::ExplicitProjectMismatch => "explicit_project_mismatch",
            ContextEvidenceKind::ConflictingEntityReference => "conflicting_entity_reference",
            ContextEvidenceKind::ReferencedEntityAbsent => "referenced_entity_absent",
        }
    }
}

/// Evidence strength.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    /// Weak signal; never sufficient alone for a mismatch or a match.
    Weak,
    /// Moderate signal.
    Medium,
    /// Strong signal (explicit project name, existing path, explicit
    /// contradictory identity).
    Strong,
}

impl EvidenceStrength {
    /// True for the strongest signals.
    pub fn is_strong(self) -> bool {
        matches!(self, EvidenceStrength::Strong)
    }
}

/// A single structured piece of context evidence.
///
/// The shape doubles as the "receipt" required by the M4 spec for mismatch
/// detection: for an explicit mismatch, `repository_identity` names the
/// repository the task was handed and `evidence_refs` lists the supporting
/// absence references.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextEvidence {
    /// Positive or negative.
    pub direction: EvidenceDirection,
    /// The evidence kind.
    pub kind: ContextEvidenceKind,
    /// The task entity or repository signal this evidence is about.
    pub subject: String,
    /// The repository identity this evidence was compared against (filled for
    /// mismatch/conflict receipts).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub repository_identity: Option<String>,
    /// Deterministic evidence references (paths, names, ids).
    #[serde(default)]
    pub evidence_refs: Vec<String>,
    /// Signal strength.
    pub strength: EvidenceStrength,
    /// Deterministic human-readable explanation.
    pub explanation: String,
}

impl ContextEvidence {
    /// Build a positive evidence record.
    pub fn positive(
        kind: ContextEvidenceKind,
        subject: impl Into<String>,
        strength: EvidenceStrength,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            direction: EvidenceDirection::Positive,
            kind,
            subject: subject.into(),
            repository_identity: None,
            evidence_refs: Vec::new(),
            strength,
            explanation: explanation.into(),
        }
    }

    /// Build a negative evidence record.
    pub fn negative(
        kind: ContextEvidenceKind,
        subject: impl Into<String>,
        strength: EvidenceStrength,
        explanation: impl Into<String>,
    ) -> Self {
        Self {
            direction: EvidenceDirection::Negative,
            kind,
            subject: subject.into(),
            repository_identity: None,
            evidence_refs: Vec::new(),
            strength,
            explanation: explanation.into(),
        }
    }

    /// Attach evidence references.
    pub fn with_refs(mut self, refs: impl IntoIterator<Item = impl Into<String>>) -> Self {
        let mut collected: Vec<String> = refs.into_iter().map(Into::into).collect();
        collected.sort();
        collected.dedup();
        self.evidence_refs = collected;
        self
    }

    /// Attach the repository identity for mismatch/conflict receipts.
    pub fn with_repository_identity(mut self, identity: impl Into<String>) -> Self {
        self.repository_identity = Some(identity.into());
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn evidence_kind_codes_are_stable() {
        assert_eq!(
            ContextEvidenceKind::ExplicitProjectMismatch.code(),
            "explicit_project_mismatch"
        );
        assert_eq!(
            ContextEvidenceKind::DomainTermMatch.code(),
            "domain_term_match"
        );
    }

    #[test]
    fn evidence_roundtrips_through_json() {
        let e = ContextEvidence::negative(
            ContextEvidenceKind::ExplicitProjectMismatch,
            "Atlas",
            EvidenceStrength::Strong,
            "Task names Atlas; the repository has no Atlas presence.",
        )
        .with_repository_identity("CodeBro")
        .with_refs(["no atlas path", "no atlas module"]);
        let json = serde_json::to_string(&e).unwrap();
        assert!(json.contains("explicit_project_mismatch"));
        let back: ContextEvidence = serde_json::from_str(&json).unwrap();
        assert_eq!(e, back);
    }
}
