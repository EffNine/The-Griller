//! The finding model and overall-decision aggregation.

use serde::{Deserialize, Serialize};
use serde_json::json;

use griller_adjudicate::assess::ClaimAssessment;
use griller_adjudicate::claim::Claim;
use griller_core::hash::{blake3_hex, canonical_bytes};

use crate::decision::{Decision, RequiredAction};
use crate::policy::PolicyContext;
use crate::severity::{Severity, SeveritySource};
use crate::verdict::Verdict;

/// A structured finding: one claim, its assessment, severity and verdict.
///
/// The finding embeds the full [`ClaimAssessment`], which references evidence
/// **by id** (matrix entries carry evidence ids, not payloads), so no large
/// evidence payload is duplicated.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Finding {
    /// Content-addressed identifier over (assessment claim id, severity,
    /// verdict, reason code).
    pub id: String,
    /// The assessment that produced the epistemic status.
    pub assessment: ClaimAssessment,
    /// Severity of accepting the claim.
    pub severity: Severity,
    /// How the severity was obtained.
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

impl Finding {
    /// Build a finding from an assessment, a context and a decision.
    pub fn from_assessment(
        assessment: &ClaimAssessment,
        context: PolicyContext,
        decision: Decision,
    ) -> Self {
        let payload = json!({
            "claim_id": assessment.claim.id,
            "severity": context.severity,
            "verdict": decision.verdict,
            "reason_code": decision.reason_code,
        });
        let id =
            blake3_hex(&canonical_bytes(&payload).expect("finding id serialization is infallible"));
        Self {
            id,
            assessment: assessment.clone(),
            severity: context.severity,
            severity_source: context.severity_source,
            verdict: decision.verdict,
            reason_code: decision.reason_code,
            reason: decision.reason,
            required_action: decision.required_action,
            required_action_message: decision.required_action_message,
        }
    }

    /// The claim this finding is about (from the embedded assessment).
    pub fn claim(&self) -> &Claim {
        &self.assessment.claim
    }
}

/// The overall result of deciding several findings.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OverallDecision {
    /// The aggregated verdict.
    pub verdict: Verdict,
    /// All findings, ordered deterministically by finding id.
    pub findings: Vec<Finding>,
}

/// Deterministic aggregation rule.
///
/// ```text
/// ANY BLOCK  => BLOCK
/// ELSE ANY WARN => WARN
/// ELSE        => PASS
/// ```
///
/// A low-severity PASS can never cancel a high-severity BLOCK.
pub fn aggregate(findings: &[Finding]) -> Verdict {
    if findings.iter().any(|f| f.verdict == Verdict::Block) {
        Verdict::Block
    } else if findings.iter().any(|f| f.verdict == Verdict::Warn) {
        Verdict::Warn
    } else {
        Verdict::Pass
    }
}
