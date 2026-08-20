//! M4 -> M3 context policy bridge.
//!
//! M4 produces a [`ContextAssessment`](griller_context::assess::ContextAssessment)
//! with a deterministic [`ContextStatus`](griller_context::status::ContextStatus).
//! M3 decides what the system should do with it (an operational verdict).
//!
//! This module lives in `griller-policy` because it is a **policy decision**,
//! not a context description. M4 describes context; M3 decides action. The
//! mapping is fully deterministic and lives in the auditable policy table
//! (reusing [`crate::table::Rule`] / `verdict_from_rule` — the PolicyTable is
//! never bypassed).
//!
//! ## The mapping
//!
//! | ContextStatus | Severity (derived) | Rule | Verdict |
//! | --- | --- | --- | --- |
//! | `MATCHED` | `info` | AlwaysPass | PASS |
//! | `LIKELY_MATCH` | `low` | AlwaysPass | PASS |
//! | `AMBIGUOUS` | `medium` | AlwaysWarn | WARN |
//! | `WEAK_RELEVANCE` | `low` | AlwaysWarn | WARN |
//! | `INSUFFICIENT_CONTEXT` | `low` | AlwaysWarn | WARN |
//! | `MISMATCH` | `high` | AlwaysBlock | BLOCK |
//!
//! The LLM never contributes to this mapping: severity is derived from the
//! deterministic status and the verdict follows from the table.

use griller_context::assess::ContextAssessment;
use griller_context::status::ContextStatus;
use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::decision::RequiredAction;
use crate::severity::{Severity, SeveritySource};
use crate::table::{verdict_from_rule, Rule};
use crate::verdict::Verdict;

/// The derived severity for a context status.
///
/// Explicit project mismatch is HIGH (the consequence of proceeding against
/// the wrong repository is severe). Ambiguity is MEDIUM. Everything else is
/// low/informational. `critical` and `info` are never auto-derived except for
/// the positive-match informational case; the LLM can never assign severity.
pub fn context_severity(status: ContextStatus) -> Severity {
    match status {
        ContextStatus::Matched => Severity::Info,
        ContextStatus::LikelyMatch => Severity::Low,
        ContextStatus::Ambiguous => Severity::Medium,
        ContextStatus::WeakRelevance | ContextStatus::InsufficientContext => Severity::Low,
        ContextStatus::Mismatch => Severity::High,
    }
}

/// The policy rule for a context status (reuses the auditable `Rule` type).
pub fn context_rule_for(status: ContextStatus) -> Rule {
    match status {
        ContextStatus::Matched | ContextStatus::LikelyMatch => Rule::AlwaysPass,
        ContextStatus::Ambiguous
        | ContextStatus::WeakRelevance
        | ContextStatus::InsufficientContext => Rule::AlwaysWarn,
        ContextStatus::Mismatch => Rule::AlwaysBlock,
    }
}

/// The operational verdict for a context status at its derived severity.
pub fn context_verdict_for(status: ContextStatus) -> Verdict {
    verdict_from_rule(context_rule_for(status), context_severity(status))
}

/// Stable machine-readable reason code, e.g. `MISMATCH_CONTEXT_BLOCK`.
pub fn context_reason_code(status: ContextStatus) -> String {
    format!(
        "{}_CONTEXT_{}",
        status_code(status),
        verdict_code(context_verdict_for(status))
    )
}

/// Deterministic human-readable reason. Never contains untrusted task text.
pub fn context_reason_message(status: ContextStatus) -> String {
    match status {
        ContextStatus::Matched => {
            "The task matches this repository's identity and supporting evidence.".to_string()
        }
        ContextStatus::LikelyMatch => {
            "The task matches existing modules, paths, symbols or terminology.".to_string()
        }
        ContextStatus::Ambiguous => {
            "The task's context signals conflict; the intended target is unclear.".to_string()
        }
        ContextStatus::WeakRelevance => {
            "Only weak token-level relevance was found; not a confirmed mismatch.".to_string()
        }
        ContextStatus::InsufficientContext => {
            "Not enough context signal to assess whether the task belongs.".to_string()
        }
        ContextStatus::Mismatch => {
            "The task explicitly names another project with no supporting presence here."
                .to_string()
        }
    }
}

/// The required action for a context status.
///
/// A new [`RequiredAction::ConfirmProject`] variant is introduced
/// deliberately for context mismatch/ambiguity: the correct response to a
/// wrong-repo task is to **stop and ask the user to confirm the intended
/// project/repository**, never to modify the repository.
pub fn context_required_action(status: ContextStatus) -> RequiredAction {
    match status {
        ContextStatus::Mismatch | ContextStatus::Ambiguous => RequiredAction::ConfirmProject,
        _ => RequiredAction::None,
    }
}

/// Human-readable explanation of the required action.
pub fn context_required_action_message(status: ContextStatus) -> String {
    match status {
        ContextStatus::Mismatch => {
            "Stop before modifying the repository and confirm the intended project/repository."
                .to_string()
        }
        ContextStatus::Ambiguous => {
            "The task references conflicting projects; confirm the intended target before proceeding."
                .to_string()
        }
        _ => "No action required.".to_string(),
    }
}

/// A structured finding: one context assessment, its derived severity and
/// operational verdict.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ContextFinding {
    /// Content-addressed identifier.
    pub id: String,
    /// The M4 context assessment.
    pub assessment: ContextAssessment,
    /// Derived severity.
    pub severity: Severity,
    /// Always `derived` — the LLM can never assign context severity.
    pub severity_source: SeveritySource,
    /// The operational verdict.
    pub verdict: Verdict,
    /// Stable machine-readable reason code.
    pub reason_code: String,
    /// Deterministic human-readable reason.
    pub reason: String,
    /// The required action.
    pub required_action: RequiredAction,
    /// Human-readable explanation of the required action.
    pub required_action_message: String,
}

impl ContextFinding {
    /// Build a finding from a context assessment using the auditable table.
    pub fn from_assessment(assessment: ContextAssessment) -> Self {
        let status = assessment.status;
        let severity = context_severity(status);
        let verdict = context_verdict_for(status);
        let payload = json!({
            "status": status,
            "severity": severity,
            "verdict": verdict,
            "reason_code": context_reason_code(status),
        });
        let id = griller_core::hash::blake3_hex(
            &griller_core::hash::canonical_bytes(&payload)
                .expect("context finding id serialization is infallible"),
        );
        Self {
            id,
            assessment,
            severity,
            severity_source: SeveritySource::Derived,
            verdict,
            reason_code: context_reason_code(status),
            reason: context_reason_message(status),
            required_action: context_required_action(status),
            required_action_message: context_required_action_message(status),
        }
    }

    /// The context status of the embedded assessment.
    pub fn status(&self) -> ContextStatus {
        self.assessment.status
    }
}

fn status_code(status: ContextStatus) -> String {
    match status {
        ContextStatus::Matched => "MATCHED",
        ContextStatus::LikelyMatch => "LIKELY_MATCH",
        ContextStatus::Ambiguous => "AMBIGUOUS",
        ContextStatus::WeakRelevance => "WEAK_RELEVANCE",
        ContextStatus::InsufficientContext => "INSUFFICIENT_CONTEXT",
        ContextStatus::Mismatch => "MISMATCH",
    }
    .to_string()
}

fn verdict_code(verdict: Verdict) -> String {
    format!("{verdict:?}").to_uppercase()
}

#[cfg(test)]
mod tests {
    use super::*;
    use griller_context::assess::ContextAssessor;
    use griller_context::project::ProjectIdentity;

    fn codebro_project() -> ProjectIdentity {
        ProjectIdentity {
            root_dir_name: "codebro".into(),
            project_names: vec!["CodeBro".into()],
            package_names: Vec::new(),
            repository_names: vec!["codebro".into()],
            remotes: Vec::new(),
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            top_level_entries: vec!["src".into()],
            important_paths: vec!["src/lib.rs".into()],
            all_paths: vec!["Cargo.toml".into(), "src/lib.rs".into()],
            symbols: Vec::new(),
            readme_title: Some("CodeBro".into()),
            vocabulary: vec!["claim".into(), "evidence".into()],
        }
    }

    fn finding_for(task: &str) -> ContextFinding {
        let assessment = ContextAssessor::deterministic()
            .assess(task, &codebro_project())
            .unwrap();
        ContextFinding::from_assessment(assessment)
    }

    #[test]
    fn mismatch_blocks_at_high_severity() {
        let finding = finding_for("Implement Atlas P10.5.1 Fact Store.");
        assert_eq!(finding.status().code(), "mismatch");
        assert_eq!(finding.severity, Severity::High);
        assert_eq!(finding.verdict, Verdict::Block);
        assert_eq!(finding.reason_code, "MISMATCH_CONTEXT_BLOCK");
        assert_eq!(finding.required_action, RequiredAction::ConfirmProject);
        assert_eq!(finding.severity_source, SeveritySource::Derived);
    }

    #[test]
    fn positive_match_passes() {
        let finding = finding_for("Fix CodeBro claim assess.");
        assert_eq!(finding.verdict, Verdict::Pass);
        assert_eq!(finding.reason_code, "MATCHED_CONTEXT_PASS");
    }

    #[test]
    fn generic_task_warns_not_blocks() {
        let finding = finding_for("Fix the bug in the login flow.");
        assert_ne!(finding.verdict, Verdict::Block);
        assert_eq!(finding.required_action, RequiredAction::None);
    }

    #[test]
    fn severity_mapping_is_derived_and_never_critical() {
        assert_eq!(context_severity(ContextStatus::Matched), Severity::Info);
        assert_eq!(context_severity(ContextStatus::LikelyMatch), Severity::Low);
        assert_eq!(context_severity(ContextStatus::Ambiguous), Severity::Medium);
        assert_eq!(
            context_severity(ContextStatus::WeakRelevance),
            Severity::Low
        );
        assert_eq!(
            context_severity(ContextStatus::InsufficientContext),
            Severity::Low
        );
        assert_eq!(context_severity(ContextStatus::Mismatch), Severity::High);
        // The finding's severity source is always `derived`; the LLM can never
        // assign a context severity.
        assert_eq!(
            finding_for("Implement Atlas P10.5.1 Fact Store.").severity_source,
            SeveritySource::Derived
        );
    }
}
