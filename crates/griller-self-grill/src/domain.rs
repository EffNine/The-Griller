//! The self-grill domain model.
//!
//! Structured types for the self-audit input, output, and findings.

use serde::{Deserialize, Serialize};

use griller_adjudicate::assess::ClaimAssessment;
use griller_adjudicate::attack_coverage::AttackCoverage;
use griller_adjudicate::claim::{ClaimKind, ClaimSource, ClaimTarget};
use griller_adjudicate::status::EpistemicStatus;
use griller_context::assess::ContextAssessment;
use griller_core::evidence::Evidence;
use griller_policy::decision::Decision;

// ---------------------------------------------------------------------------
// Input
// ---------------------------------------------------------------------------

/// What the self-grill needs to audit one decision.
///
/// All fields are treated as immutable: the self-grill reads them but never
/// mutates them.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfGrillInput {
    /// The original claim text.
    pub claim_text: String,
    /// Who made the claim.
    pub claim_source: ClaimSource,
    /// The claim kind.
    pub claim_kind: ClaimKind,
    /// The claim target.
    pub claim_target: ClaimTarget,
    /// The M2 adjudication assessment.
    pub assessment: ClaimAssessment,
    /// The M3 policy decision.
    pub decision: Decision,
    /// The M4 context assessment (may be None if context was not checked).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub context_assessment: Option<ContextAssessment>,
    /// The M5 attack coverage report (may be None if no attacks were run).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub attack_coverage: Option<AttackCoverage>,
    /// The full evidence set used in adjudication.
    pub evidence: Vec<Evidence>,
    /// The severity used for the policy decision.
    pub severity: griller_policy::severity::Severity,
    /// How the severity was obtained.
    pub severity_source: griller_policy::severity::SeveritySource,
}

impl SelfGrillInput {
    /// Build input from the canonical pipeline outputs.
    pub fn from_pipeline(
        assessment: &ClaimAssessment,
        decision: &Decision,
        context_assessment: Option<&ContextAssessment>,
        attack_coverage: Option<&AttackCoverage>,
        evidence: &[Evidence],
        severity: griller_policy::severity::Severity,
        severity_source: griller_policy::severity::SeveritySource,
    ) -> Self {
        Self {
            claim_text: assessment.claim.text.clone(),
            claim_source: assessment.claim.source,
            claim_kind: assessment.claim.kind,
            claim_target: assessment.claim.target.clone(),
            assessment: assessment.clone(),
            decision: decision.clone(),
            context_assessment: context_assessment.cloned(),
            attack_coverage: attack_coverage.cloned(),
            evidence: evidence.to_vec(),
            severity,
            severity_source,
        }
    }

    /// The epistemic status that was decided.
    pub fn status(&self) -> EpistemicStatus {
        self.assessment.status
    }

    /// The operational verdict that was decided.
    pub fn verdict(&self) -> griller_policy::verdict::Verdict {
        self.decision.verdict
    }
}

// ---------------------------------------------------------------------------
// Outcome
// ---------------------------------------------------------------------------

/// The result of one self-grill check.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfGrillOutcome {
    /// The original decision survives this check.
    Holds,
    /// Confidence/evidence strength is reduced but the decision remains
    /// possible.
    Weakened,
    /// A legitimate contradiction was found in the original assessment.
    Refuted,
    /// The check could not determine whether the original decision is sound.
    Inconclusive,
    /// No meaningful self-audit could be performed for this check.
    NotApplicable,
}

impl SelfGrillOutcome {
    /// Priority ordering: lower value = worse outcome.
    ///
    /// REFUTED > WEAKENED > INCONCLUSIVE > HOLDS > NOT_APPLICABLE
    pub fn priority(&self) -> u8 {
        match self {
            SelfGrillOutcome::Refuted => 0,
            SelfGrillOutcome::Weakened => 1,
            SelfGrillOutcome::Inconclusive => 2,
            SelfGrillOutcome::Holds => 3,
            SelfGrillOutcome::NotApplicable => 4,
        }
    }

    /// True when this outcome is the worst possible (REFUTED).
    pub fn is_refuted(self) -> bool {
        matches!(self, SelfGrillOutcome::Refuted)
    }

    /// True when this outcome weakens the original decision.
    pub fn is_weakened(self) -> bool {
        matches!(self, SelfGrillOutcome::Weakened)
    }
}

// ---------------------------------------------------------------------------
// Finding
// ---------------------------------------------------------------------------

/// One issue discovered by a self-grill check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfGrillFinding {
    /// The check that produced this finding.
    pub check: String,
    /// The outcome of this check.
    pub outcome: SelfGrillOutcome,
    /// Machine-readable severity.
    pub severity: SelfGrillSeverity,
    /// Human-readable explanation.
    pub message: String,
    /// Evidence ids relevant to this finding, if any.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub evidence_refs: Vec<String>,
}

/// Severity of a self-grill finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfGrillSeverity {
    /// Structural invariant violation — always refutes.
    Critical,
    /// Strong evidence of a problem — usually refutes.
    High,
    /// Moderate concern — may weaken.
    Medium,
    /// Minor issue — unlikely to affect the decision.
    Low,
}

// ---------------------------------------------------------------------------
// Check kinds
// ---------------------------------------------------------------------------

/// Which self-grill check produced a finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SelfGrillCheck {
    /// Check #1: Evidence completeness — every reference resolves.
    EvidenceCompleteness,
    /// Check #2: Evidence independence — no double-counting sources.
    EvidenceIndependence,
    /// Check #3: Trust tier — agent claims cannot be upgraded to verified.
    TrustTier,
    /// Check #4: Claim scope — evidence covers the claim scope.
    ClaimScope,
    /// Check #5: Context consistency — M4 MISMATCH + PASS => refuted.
    ContextConsistency,
    /// Check #6: Attack coverage — ADVERSARIALLY_VERIFIED strict conditions.
    AttackCoverage,
    /// Check #7: Contradictory evidence — structural contradictions.
    ContradictoryEvidence,
    /// Check #8: Execution validity — provenance and enforcement truth.
    ExecutionValidity,
    /// Check #9: Policy consistency — recompute M3 from structured inputs.
    PolicyConsistency,
    /// Check #10: Reason/action consistency — reason_code matches verdict.
    ReasonActionConsistency,
    /// Check #11: Required action consistency — BLOCK has meaningful remediation.
    RequiredActionConsistency,
    /// Check #12: Evidence hash integrity — recompute and verify hashes.
    EvidenceHashIntegrity,
    /// Check #13: Environment reproducibility — provenance limitations correct.
    EnvironmentReproducibility,
    /// Check #14: Redaction integrity — pipeline invariant holds.
    RedactionIntegrity,
    /// Check #15: Context/claim consistency — identity and target agree.
    ContextClaimConsistency,
    /// Check #16: Attack target consistency — attacks targeted the claim.
    AttackTargetConsistency,
}

impl SelfGrillCheck {
    /// Human-readable label for this check.
    pub fn label(self) -> &'static str {
        match self {
            SelfGrillCheck::EvidenceCompleteness => "evidence_completeness",
            SelfGrillCheck::EvidenceIndependence => "evidence_independence",
            SelfGrillCheck::TrustTier => "trust_tier",
            SelfGrillCheck::ClaimScope => "claim_scope",
            SelfGrillCheck::ContextConsistency => "context_consistency",
            SelfGrillCheck::AttackCoverage => "attack_coverage",
            SelfGrillCheck::ContradictoryEvidence => "contradictory_evidence",
            SelfGrillCheck::ExecutionValidity => "execution_validity",
            SelfGrillCheck::PolicyConsistency => "policy_consistency",
            SelfGrillCheck::ReasonActionConsistency => "reason_action_consistency",
            SelfGrillCheck::RequiredActionConsistency => "required_action_consistency",
            SelfGrillCheck::EvidenceHashIntegrity => "evidence_hash_integrity",
            SelfGrillCheck::EnvironmentReproducibility => "environment_reproducibility",
            SelfGrillCheck::RedactionIntegrity => "redaction_integrity",
            SelfGrillCheck::ContextClaimConsistency => "context_claim_consistency",
            SelfGrillCheck::AttackTargetConsistency => "attack_target_consistency",
        }
    }
}

// ---------------------------------------------------------------------------
// Coverage
// ---------------------------------------------------------------------------

/// Descriptive coverage of what the self-grill examined.
///
/// Purely descriptive — never converted into a fake numerical confidence.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfGrillCoverage {
    /// Number of checks that were applicable.
    pub applicable: usize,
    /// Number of checks that were actually executed.
    pub executed: usize,
    /// Number of checks that returned HOLDS.
    pub holds: usize,
    /// Number of checks that returned WEAKENED.
    pub weakened: usize,
    /// Number of checks that returned REFUTED.
    pub refuted: usize,
    /// Number of checks that returned INCONCLUSIVE.
    pub inconclusive: usize,
    /// Number of checks that returned NOT_APPLICABLE.
    pub not_applicable: usize,
}

impl SelfGrillCoverage {
    /// Increment the counter for a check outcome.
    pub fn record(&mut self, outcome: SelfGrillOutcome) {
        self.executed += 1;
        match outcome {
            SelfGrillOutcome::Holds => self.holds += 1,
            SelfGrillOutcome::Weakened => self.weakened += 1,
            SelfGrillOutcome::Refuted => self.refuted += 1,
            SelfGrillOutcome::Inconclusive => self.inconclusive += 1,
            SelfGrillOutcome::NotApplicable => self.not_applicable += 1,
        }
    }

    /// Mark a check as applicable but not executed.
    pub fn mark_applicable(&mut self) {
        self.applicable += 1;
    }

    /// True when at least one check returned REFUTED.
    pub fn has_refuted(&self) -> bool {
        self.refuted > 0
    }

    /// True when at least one check returned WEAKENED.
    pub fn has_weakened(&self) -> bool {
        self.weakened > 0
    }
}

// ---------------------------------------------------------------------------
// Result
// ---------------------------------------------------------------------------

/// The complete result of one self-grill pass.
///
/// The original decision remains immutable; this is a separate audit result.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfGrillResult {
    /// The overall outcome of the self-audit.
    pub outcome: SelfGrillOutcome,
    /// Descriptive coverage of checks run.
    pub coverage: SelfGrillCoverage,
    /// Individual findings from each check.
    pub findings: Vec<SelfGrillFinding>,
    /// Issues that do not map to a single check (meta-issues).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub issues: Vec<SelfGrillIssue>,
}

impl SelfGrillResult {
    /// Build an empty result (no checks ran).
    pub fn empty() -> Self {
        Self {
            outcome: SelfGrillOutcome::NotApplicable,
            coverage: SelfGrillCoverage::default(),
            findings: Vec::new(),
            issues: Vec::new(),
        }
    }

    /// Build a result where no checks were applicable.
    pub fn not_applicable() -> Self {
        Self {
            outcome: SelfGrillOutcome::NotApplicable,
            coverage: SelfGrillCoverage {
                applicable: 0,
                executed: 0,
                holds: 0,
                weakened: 0,
                refuted: 0,
                inconclusive: 0,
                not_applicable: 1,
            },
            findings: Vec::new(),
            issues: Vec::new(),
        }
    }

    /// Aggregate the outcome from findings.
    ///
    /// Priority: REFUTED > WEAKENED > INCONCLUSIVE > HOLDS > NOT_APPLICABLE
    pub fn aggregate_outcome(findings: &[SelfGrillFinding]) -> SelfGrillOutcome {
        if findings.iter().any(|f| f.outcome.is_refuted()) {
            return SelfGrillOutcome::Refuted;
        }
        if findings.iter().any(|f| f.outcome.is_weakened()) {
            return SelfGrillOutcome::Weakened;
        }
        if findings
            .iter()
            .any(|f| matches!(f.outcome, SelfGrillOutcome::Inconclusive))
        {
            return SelfGrillOutcome::Inconclusive;
        }
        if findings.is_empty() {
            return SelfGrillOutcome::NotApplicable;
        }
        SelfGrillOutcome::Holds
    }
}

// ---------------------------------------------------------------------------
// Issue (meta-level problems)
// ---------------------------------------------------------------------------

/// A meta-issue discovered during self-grill that does not map to a single check.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SelfGrillIssue {
    /// Machine-readable code.
    pub code: String,
    /// Human-readable explanation.
    pub message: String,
    /// Severity.
    pub severity: SelfGrillSeverity,
}

// ---------------------------------------------------------------------------
// Original decision snapshot (immutable)
// ---------------------------------------------------------------------------

/// An immutable snapshot of the original decision for before/after auditing.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OriginalDecision {
    /// The original claim text.
    pub claim_text: String,
    /// The original epistemic status.
    pub epistemic_status: EpistemicStatus,
    /// The original operational verdict.
    pub verdict: griller_policy::verdict::Verdict,
    /// The original reason code.
    pub reason_code: String,
    /// The original required action.
    pub required_action: griller_policy::decision::RequiredAction,
    /// The severity used.
    pub severity: griller_policy::severity::Severity,
}

impl OriginalDecision {
    /// Build from input and decision.
    pub fn from_input(input: &SelfGrillInput, decision: &Decision) -> Self {
        Self {
            claim_text: input.claim_text.clone(),
            epistemic_status: input.status(),
            verdict: decision.verdict,
            reason_code: decision.reason_code.clone(),
            required_action: decision.required_action,
            severity: input.severity,
        }
    }
}
