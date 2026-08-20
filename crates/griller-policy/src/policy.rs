//! The deterministic policy interface.
//!
//! A policy consumes an already-adjudicated [`ClaimAssessment`] plus a small
//! [`PolicyContext`] and produces a [`Decision`]. The policy layer is pure:
//! no filesystem, no network, no LLM, no subprocesses.

use griller_adjudicate::assess::ClaimAssessment;

use crate::decision::Decision;
use crate::finding::{aggregate, Finding, OverallDecision};
use crate::severity::{Severity, SeveritySource};
use crate::table;

/// Structured context a policy may use.
///
/// Deliberately small. The policy decides from structured facts — it never
/// receives raw repository data. Severity is already resolved by
/// [`crate::severity::SeverityResolver`] (or declared by the caller); an LLM
/// never contributes to this struct.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct PolicyContext {
    /// The severity of accepting the claim.
    pub severity: Severity,
    /// How the severity was obtained (`declared` by the caller or `derived`
    /// deterministically).
    pub severity_source: SeveritySource,
}

impl PolicyContext {
    /// Construct a policy context from a resolved severity.
    pub fn new(severity: Severity, severity_source: SeveritySource) -> Self {
        Self {
            severity,
            severity_source,
        }
    }
}

/// One claim awaiting a policy decision.
#[derive(Debug, Clone)]
pub struct PolicyInput {
    /// The adjudicated claim.
    pub assessment: ClaimAssessment,
    /// The policy context (severity etc.).
    pub context: PolicyContext,
}

/// The policy boundary.
///
/// Implementations are deterministic: the same assessment + context always
/// produce the same decision. No async is needed.
pub trait Policy: Send + Sync {
    /// Decide the operational verdict for one adjudicated claim.
    fn decide(&self, assessment: &ClaimAssessment, context: &PolicyContext) -> Decision;

    /// Evaluate one input into a full [`Finding`].
    fn evaluate(&self, input: &PolicyInput) -> Finding {
        Finding::from_assessment(
            &input.assessment,
            input.context,
            self.decide(&input.assessment, &input.context),
        )
    }

    /// Evaluate several claims and aggregate into an [`OverallDecision`].
    ///
    /// Aggregation is deterministic:
    /// any `BLOCK` => `BLOCK`; else any `WARN` => `WARN`; else `PASS`.
    /// A low-severity PASS can never cancel a high-severity BLOCK.
    fn decide_many(&self, inputs: &[PolicyInput]) -> OverallDecision {
        let findings: Vec<Finding> = inputs.iter().map(|i| self.evaluate(i)).collect();
        let verdict = aggregate(&findings);
        OverallDecision { verdict, findings }
    }
}

/// The default M3 policy: the auditable [`crate::table`] matrix.
#[derive(Debug, Clone, Copy, Default)]
pub struct PolicyTable;

impl Policy for PolicyTable {
    fn decide(&self, assessment: &ClaimAssessment, context: &PolicyContext) -> Decision {
        let kind = assessment.claim.kind;
        let status = assessment.status;
        let verdict = table::verdict_for(kind, status, context.severity);
        Decision {
            verdict,
            reason_code: table::reason_code(kind, status, verdict),
            reason: table::reason_message(kind, status, context.severity, verdict),
            required_action: table::required_action_for(status),
            required_action_message: table::required_action_message(status),
        }
    }
}
