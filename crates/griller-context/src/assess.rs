//! The context adjudication pipeline and its result type.
//!
//! ```text
//! Task + ProjectIdentity
//!   -> normalize    (TaskContextNormalizer)
//!   -> match        (ContextMatcher -> ContextEvidence)
//!   -> assign status (pure function -> ContextStatus)
//! ```
//!
//! The result — a [`ContextAssessment`] — is deterministic after task
//! normalization and preserves **all** evidence. The assessment never
//! contains a verdict: PASS/WARN/BLOCK belongs to M3
//! (`griller-policy::context`).

use serde::{Deserialize, Serialize};

use crate::error::ContextError;
use crate::evidence::{
    ContextEvidence, ContextEvidenceKind as Kind, EvidenceDirection, EvidenceStrength as Strength,
};
use crate::matcher::{ContextMatcher, DeterministicContextMatcher};
use crate::normalizer::{DeterministicTaskNormalizer, TaskContextNormalizer};
use crate::project::ProjectIdentity;
use crate::status::ContextStatus;
use crate::task::TaskIdentity;

/// A single deterministic reason for the assigned status.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ContextReason {
    /// Stable machine-readable code.
    pub code: String,
    /// Deterministic human-readable explanation.
    pub message: String,
}

impl ContextReason {
    /// Build a reason.
    pub fn new(code: impl Into<String>, message: impl Into<String>) -> Self {
        Self {
            code: code.into(),
            message: message.into(),
        }
    }
}

/// The complete, machine-readable result of assessing one task against one
/// repository.
///
/// Deterministic: the same task + same project identity always produce an
/// identical [`ContextAssessment`] (byte-for-byte identical canonical JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextAssessment {
    /// The normalized task identity.
    pub task: TaskIdentity,
    /// The repository's deterministic project identity.
    pub project: ProjectIdentity,
    /// All positive evidence, sorted.
    pub positive_evidence: Vec<ContextEvidence>,
    /// All negative evidence, sorted.
    pub negative_evidence: Vec<ContextEvidence>,
    /// The deterministic context status.
    pub status: ContextStatus,
    /// Deterministic reasons for the status, sorted by code.
    pub reasons: Vec<ContextReason>,
}

impl ContextAssessment {
    /// True when the status is the block-worthy mismatch.
    pub fn is_mismatch(&self) -> bool {
        self.status.is_mismatch()
    }

    /// The strongest mismatch evidence, when present (for receipts/diva).
    pub fn primary_mismatch(&self) -> Option<&ContextEvidence> {
        self.negative_evidence
            .iter()
            .find(|e| e.kind == Kind::ExplicitProjectMismatch)
    }
}

/// The context adjudication pipeline.
///
/// A thin wiring of two small, replaceable components — not an "engine". The
/// default configuration is fully deterministic and works without an LLM.
pub struct ContextAssessor {
    normalizer: Box<dyn TaskContextNormalizer>,
    matcher: Box<dyn ContextMatcher>,
}

impl Default for ContextAssessor {
    fn default() -> Self {
        Self::new(
            Box::new(DeterministicTaskNormalizer),
            Box::new(DeterministicContextMatcher),
        )
    }
}

impl ContextAssessor {
    /// Build an assessor with explicit stage implementations.
    pub fn new(
        normalizer: Box<dyn TaskContextNormalizer>,
        matcher: Box<dyn ContextMatcher>,
    ) -> Self {
        Self {
            normalizer,
            matcher,
        }
    }

    /// Replace the normalizer.
    pub fn with_normalizer(mut self, normalizer: Box<dyn TaskContextNormalizer>) -> Self {
        self.normalizer = normalizer;
        self
    }

    /// Replace the matcher.
    pub fn with_matcher(mut self, matcher: Box<dyn ContextMatcher>) -> Self {
        self.matcher = matcher;
        self
    }

    /// The deterministic default assessor.
    pub fn deterministic() -> Self {
        Self::default()
    }

    /// Normalize a task without assessing it (useful for inspection).
    pub fn normalize(&self, text: &str) -> Result<TaskIdentity, ContextError> {
        Ok(self.normalizer.normalize(text)?)
    }

    /// Assess a task against a project identity.
    pub fn assess(
        &self,
        text: &str,
        project: &ProjectIdentity,
    ) -> Result<ContextAssessment, ContextError> {
        let task = self.normalizer.normalize(text)?;
        let mut evidence = self.matcher.match_context(&task, project);
        evidence.sort_by(|a, b| {
            (a.direction, a.kind, &a.subject, &a.explanation).cmp(&(
                b.direction,
                b.kind,
                &b.subject,
                &b.explanation,
            ))
        });
        evidence.dedup();

        let (positive, negative): (Vec<_>, Vec<_>) = evidence
            .into_iter()
            .partition(|e| e.direction == EvidenceDirection::Positive);

        let (status, reasons) = assign_context_status(&positive, &negative, &task);

        let mut reasons = reasons;
        reasons.sort_by(|a, b| a.code.cmp(&b.code));

        Ok(ContextAssessment {
            task,
            project: project.clone(),
            positive_evidence: positive,
            negative_evidence: negative,
            status,
            reasons,
        })
    }
}

// ---------------------------------------------------------------------------
// Status assignment (pure, documented rules)
// ---------------------------------------------------------------------------

/// The deterministic status assignment function.
///
/// ## Documented resolution rules (priority order)
///
/// 1. **Explicit project mismatch** present:
///    - plus a strong repository identity match in the same task => `AMBIGUOUS`
///      (the task names both this repository and a foreign project; the
///      conflict is preserved and surfaced, but it is not a clean mismatch)
///    - otherwise => `MISMATCH`
/// 2. **Conflicting entity reference** present (a foreign project name with
///    only weak presence here) => `AMBIGUOUS`
/// 3. **Strong repository identity match** (explicit project / manifest /
///    remote) => `MATCHED`
/// 4. **Substantial positive evidence** (strong or medium) => `LIKELY_MATCH`
/// 5. **Only weak positive evidence** => `WEAK_RELEVANCE`
/// 6. **No informative task signal** => `INSUFFICIENT_CONTEXT`
/// 7. otherwise => `WEAK_RELEVANCE`
///
/// The core invariant holds: **absence is not mismatch**. `MISMATCH` is only
/// reachable via rule 1 with an explicit contradictory project identity; a
/// generic task with unfamiliar words reaches at most `WEAK_RELEVANCE`.
pub fn assign_context_status(
    positive: &[ContextEvidence],
    negative: &[ContextEvidence],
    task: &TaskIdentity,
) -> (ContextStatus, Vec<ContextReason>) {
    let has_mismatch = negative
        .iter()
        .any(|e| e.kind == Kind::ExplicitProjectMismatch && e.strength == Strength::Strong);
    let has_conflict = negative
        .iter()
        .any(|e| e.kind == Kind::ConflictingEntityReference);
    let has_strong_identity = positive.iter().any(|e| {
        matches!(
            e.kind,
            Kind::ExplicitProjectMatch | Kind::ManifestMatch | Kind::RemoteMatch
        ) && e.strength == Strength::Strong
    });
    let has_substantial_positive = positive
        .iter()
        .any(|e| e.strength == Strength::Strong || e.strength == Strength::Medium);

    let mut reasons = Vec::new();

    if has_mismatch {
        reasons.push(ContextReason::new(
            "explicit_project_mismatch",
            "The task explicitly names a project with no supporting presence in this repository.",
        ));
        if has_strong_identity {
            reasons.push(ContextReason::new(
                "conflicting_repo_identity",
                "The task also names this repository's identity, so the signals conflict.",
            ));
            return (ContextStatus::Ambiguous, reasons);
        }
        return (ContextStatus::Mismatch, reasons);
    }

    if has_conflict {
        reasons.push(ContextReason::new(
            "conflicting_entity_reference",
            "The task names a foreign project that has only weak presence in this repository.",
        ));
        if has_strong_identity {
            reasons.push(ContextReason::new(
                "conflicting_repo_identity",
                "The task also names this repository's identity, so the signals conflict.",
            ));
        }
        return (ContextStatus::Ambiguous, reasons);
    }

    if has_strong_identity {
        reasons.push(ContextReason::new(
            "positive_repo_identity_match",
            "The task matches this repository's identity (project, manifest or remote).",
        ));
        return (ContextStatus::Matched, reasons);
    }

    if has_substantial_positive {
        reasons.push(ContextReason::new(
            "substantial_positive_evidence",
            "The task matches existing paths, modules, symbols or terminology.",
        ));
        return (ContextStatus::LikelyMatch, reasons);
    }

    if positive.iter().any(|e| e.strength == Strength::Weak) {
        reasons.push(ContextReason::new(
            "weak_positive_evidence_only",
            "Only weak token-level relevance is present.",
        ));
        return (ContextStatus::WeakRelevance, reasons);
    }

    if !task.has_signal() {
        reasons.push(ContextReason::new(
            "insufficient_task_context",
            "The task carries no informative identity signal.",
        ));
        return (ContextStatus::InsufficientContext, reasons);
    }

    reasons.push(ContextReason::new(
        "no_context_signal",
        "No positive or negative context signal could be established.",
    ));
    (ContextStatus::WeakRelevance, reasons)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::project::ProjectIdentity;

    fn codebro_project() -> ProjectIdentity {
        ProjectIdentity {
            root_dir_name: "codebro".into(),
            project_names: vec!["CodeBro".into()],
            package_names: vec!["griller-core".into()],
            repository_names: vec!["codebro".into()],
            remotes: Vec::new(),
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            top_level_entries: vec!["src".into()],
            important_paths: vec!["src/lib.rs".into()],
            all_paths: vec!["Cargo.toml".into(), "src/lib.rs".into()],
            symbols: vec!["claim".into()],
            readme_title: Some("CodeBro".into()),
            vocabulary: vec!["claim".into(), "evidence".into()],
        }
    }

    fn assess(text: &str, project: &ProjectIdentity) -> ContextAssessment {
        ContextAssessor::deterministic()
            .assess(text, project)
            .unwrap()
    }

    #[test]
    fn original_wrong_project_case_is_mismatch() {
        let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &codebro_project());
        assert_eq!(assessment.status, ContextStatus::Mismatch);
        assert!(assessment.primary_mismatch().is_some());
    }

    #[test]
    fn positive_fixture_is_not_blocked() {
        let atlas = ProjectIdentity {
            root_dir_name: "atlas".into(),
            project_names: vec!["Atlas".into()],
            package_names: Vec::new(),
            repository_names: vec!["atlas".into()],
            remotes: Vec::new(),
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            top_level_entries: vec!["src".into()],
            important_paths: vec!["src/fact_store.rs".into()],
            all_paths: vec!["Cargo.toml".into(), "src/fact_store.rs".into()],
            symbols: vec!["factstore".into()],
            readme_title: Some("Atlas Fact Store".into()),
            vocabulary: vec!["atlas".into(), "fact".into(), "store".into()],
        };
        let assessment = assess("Implement P10.5.1 Fact Store.", &atlas);
        assert_eq!(assessment.status, ContextStatus::LikelyMatch);
        assert!(!assessment.is_mismatch());
    }

    #[test]
    fn generic_task_is_never_mismatch() {
        let assessment = assess("Fix the bug in the login flow.", &codebro_project());
        assert!(!assessment.is_mismatch());
        assert!(
            matches!(
                assessment.status,
                ContextStatus::WeakRelevance | ContextStatus::InsufficientContext
            ),
            "unexpected status {:?}",
            assessment.status
        );
    }

    #[test]
    fn assessor_is_deterministic() {
        let project = codebro_project();
        let a = assess("Implement Atlas P10.5.1 Fact Store.", &project);
        let b = assess("Implement Atlas P10.5.1 Fact Store.", &project);
        assert_eq!(a, b);
    }
}
