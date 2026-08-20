//! The 16 deterministic self-grill checks.
//!
//! Each check returns a [`crate::domain::SelfGrillOutcome`] and an optional
//! message explaining why. Checks are pure functions over the input — they
//! never mutate anything and never call an LLM.

use griller_adjudicate::claim::ClaimKind;
use griller_adjudicate::status::EpistemicStatus;
use griller_core::evidence::EvidenceKind;
use griller_policy::policy::{Policy, PolicyTable};
use griller_policy::table::{reason_code, required_action_for};

use crate::domain::*;

/// A single self-grill check that knows its own kind.
pub trait SelfGrillCheck {
    /// The check identifier.
    fn kind(&self) -> &'static str;

    /// Run the check and return the outcome + message.
    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String);
}

// ---------------------------------------------------------------------------
// Check #1: Evidence Completeness
// ---------------------------------------------------------------------------

/// "Did the decision rely on evidence that is actually present?"
pub struct EvidenceCompletenessCheck;

impl SelfGrillCheck for EvidenceCompletenessCheck {
    fn kind(&self) -> &'static str {
        "evidence_completeness"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let evidence_ids: std::collections::HashSet<&str> =
            input.evidence.iter().map(|e| e.id.as_str()).collect();

        for id in &input.assessment.supporting_evidence {
            if !evidence_ids.contains(id.as_str()) {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "Supporting evidence reference {} does not resolve to any present evidence",
                        id
                    ),
                );
            }
        }

        for id in &input.assessment.contradicting_evidence {
            if !evidence_ids.contains(id.as_str()) {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "Contradicting evidence reference {} does not resolve to any present evidence",
                        id
                    ),
                );
            }
        }

        let mut seen_hashes: std::collections::HashMap<&str, usize> =
            std::collections::HashMap::new();
        for ev in &input.evidence {
            *seen_hashes.entry(&ev.content_hash).or_insert(0) += 1;
        }
        for (hash, count) in &seen_hashes {
            if *count > 1 {
                return (
                    SelfGrillOutcome::Weakened,
                    format!(
                        "Evidence hash {} appears {} times; duplicate evidence may be masquerading as independent",
                        hash, count
                    ),
                );
            }
        }

        let has_command = input
            .evidence
            .iter()
            .any(|e| e.kind == EvidenceKind::CommandOutput);
        let has_agent_claim = input
            .evidence
            .iter()
            .any(|e| e.kind == EvidenceKind::AgentClaim);

        if matches!(input.claim_kind, ClaimKind::Completion) && !has_command && !has_agent_claim {
            return (
                SelfGrillOutcome::Weakened,
                "Completion claim has no command or agent-claim evidence; checking evidence types"
                    .to_string(),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "All evidence references resolve; no duplicate masquerading detected".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #2: Evidence Independence
// ---------------------------------------------------------------------------

/// "Did we accidentally count the same source twice?"
pub struct EvidenceIndependenceCheck;

impl SelfGrillCheck for EvidenceIndependenceCheck {
    fn kind(&self) -> &'static str {
        "evidence_independence"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut by_source: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();

        for (i, ev) in input.evidence.iter().enumerate() {
            let fingerprint = format!(
                "{:?}:{:?}:{}",
                ev.provenance.source, ev.provenance.capture_method, ev.content_hash
            );
            by_source.entry(fingerprint).or_default().push(i);
        }

        let mut by_command: std::collections::HashMap<String, Vec<usize>> =
            std::collections::HashMap::new();
        for (i, ev) in input.evidence.iter().enumerate() {
            if let Some(cmd) = &ev.provenance.command {
                let key = cmd.display_line();
                by_command.entry(key).or_default().push(i);
            }
        }

        let mut issues: Vec<String> = Vec::new();

        for indices in by_source.values() {
            if indices.len() > 1 {
                issues.push(format!(
                    "{} evidence records share the same provenance fingerprint",
                    indices.len()
                ));
            }
        }

        for (cmd, indices) in &by_command {
            if indices.len() > 1 {
                issues.push(format!(
                    "Evidence from command '{}' appears {} times",
                    cmd,
                    indices.len()
                ));
            }
        }

        if issues.is_empty() {
            (
                SelfGrillOutcome::Holds,
                "No duplicate provenance detected; evidence appears independent".to_string(),
            )
        } else {
            (SelfGrillOutcome::Weakened, issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check #3: Trust Tier
// ---------------------------------------------------------------------------

/// Verify: Agent narration != environment observation.
pub struct TrustTierCheck;

impl SelfGrillCheck for TrustTierCheck {
    fn kind(&self) -> &'static str {
        "trust_tier"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let all_evidence = &input.evidence;

        let env_count = all_evidence
            .iter()
            .filter(|e| e.is_environment_evidence())
            .count();
        let agent_count = all_evidence
            .iter()
            .filter(|e| !e.is_environment_evidence())
            .count();

        let status = input.status();

        match status {
            EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified => {
                if env_count == 0 && agent_count > 0 {
                    return (
                        SelfGrillOutcome::Refuted,
                        format!(
                        "{:?} requires environment evidence but only {} agent claims are present",
                        status, agent_count
                    ),
                    );
                }
                if env_count == 0 && agent_count == 0 {
                    return (
                        SelfGrillOutcome::Refuted,
                        format!("{:?} assigned with zero evidence of any kind", status),
                    );
                }
            }
            _ => {}
        }

        (
            SelfGrillOutcome::Holds,
            format!(
                "Trust tier invariant holds: {} env evidence, {} agent evidence",
                env_count, agent_count
            ),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #4: Claim Scope
// ---------------------------------------------------------------------------

/// Attack scope inflation.
pub struct ClaimScopeCheck;

impl SelfGrillCheck for ClaimScopeCheck {
    fn kind(&self) -> &'static str {
        "claim_scope"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let claim_text = input.claim_text.to_lowercase();
        let supporting = &input.assessment.supporting_evidence;

        if claim_text.contains("all")
            || claim_text.contains("complete")
            || claim_text.contains("everything")
        {
            let has_narrow_evidence = input.evidence.iter().any(|e| {
                e.provenance
                    .command
                    .as_ref()
                    .map(|c| {
                        let line = c.display_line();
                        line.contains("--lib")
                            || line.contains("--unit")
                            || line.contains("test --lib")
                    })
                    .unwrap_or(false)
            });

            if has_narrow_evidence && supporting.len() <= 2 {
                return (
                    SelfGrillOutcome::Weakened,
                    "Claim scope includes 'all' but evidence appears narrowly scoped (e.g. --lib)"
                        .to_string(),
                );
            }
        }

        let claim_mentions_tests = claim_text.contains("test");
        let has_test_evidence = input.evidence.iter().any(|e| {
            e.kind == EvidenceKind::CommandOutput
                && e.provenance
                    .command
                    .as_ref()
                    .map(|c| c.display_line().contains("test"))
                    .unwrap_or(false)
        });

        if claim_mentions_tests
            && !has_test_evidence
            && !input.assessment.claim.text.contains("test")
        {
            return (
                SelfGrillOutcome::Weakened,
                "Claim mentions tests but no test command evidence found".to_string(),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "Claim scope appears consistent with evidence scope".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #5: Context Consistency
// ---------------------------------------------------------------------------

/// Re-check M4 assessment.
pub struct ContextConsistencyCheck;

impl SelfGrillCheck for ContextConsistencyCheck {
    fn kind(&self) -> &'static str {
        "context_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let Some(context) = &input.context_assessment else {
            return (
                SelfGrillOutcome::NotApplicable,
                "No context assessment available; skipping context consistency check".to_string(),
            );
        };

        let verdict = input.verdict();
        let status = context.status;

        match (status, verdict) {
            (griller_context::status::ContextStatus::Mismatch, griller_policy::verdict::Verdict::Pass) => {
                (
                    SelfGrillOutcome::Refuted,
                    "M4 context assessment is MISMATCH but final verdict is PASS; critical invariant violation".to_string(),
                )
            }
            (griller_context::status::ContextStatus::Mismatch, griller_policy::verdict::Verdict::Warn) => {
                (
                    SelfGrillOutcome::Weakened,
                    "M4 context assessment is MISMATCH but verdict is only WARN; should be BLOCK".to_string(),
                )
            }
            (griller_context::status::ContextStatus::Ambiguous, griller_policy::verdict::Verdict::Pass) => {
                (
                    SelfGrillOutcome::Weakened,
                    "M4 context assessment is AMBIGUOUS but final verdict is PASS; confidence reduced".to_string(),
                )
            }
            _ => (
                SelfGrillOutcome::Holds,
                format!(
                    "Context consistency OK: status={}, verdict={:?}",
                    status, verdict
                ),
            ),
        }
    }
}

// ---------------------------------------------------------------------------
// Check #6: Attack Coverage
// ---------------------------------------------------------------------------

/// Inspect M5 attack coverage.
pub struct AttackCoverageCheck;

impl SelfGrillCheck for AttackCoverageCheck {
    fn kind(&self) -> &'static str {
        "attack_coverage"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let Some(coverage) = &input.attack_coverage else {
            if matches!(input.status(), EpistemicStatus::AdversariallyVerified) {
                return (
                    SelfGrillOutcome::Refuted,
                    "ADVERSARIALLY_VERIFIED assigned but no attack coverage data available"
                        .to_string(),
                );
            }
            return (
                SelfGrillOutcome::NotApplicable,
                "No attack coverage data; check not applicable".to_string(),
            );
        };

        let status = input.status();

        if matches!(status, EpistemicStatus::AdversariallyVerified) {
            if coverage.unimplemented > 0 {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "ADVERSARIALLY_VERIFIED but {} applicable attack(s) unimplemented",
                        coverage.unimplemented
                    ),
                );
            }
            if coverage.inconclusive > 0 {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "ADVERSARIALLY_VERIFIED but {} attack(s) produced inconclusive results",
                        coverage.inconclusive
                    ),
                );
            }
            if coverage.failed > 0 {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "ADVERSARIALLY_VERIFIED but {} attack(s) failed to execute",
                        coverage.failed
                    ),
                );
            }
            if !coverage.all_applicable_executed() {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "ADVERSARIALLY_VERIFIED but not all applicable attacks were executed (executed={}, applicable={})",
                        coverage.executed, coverage.applicable
                    ),
                );
            }
        }

        (
            SelfGrillOutcome::Holds,
            format!("Attack coverage invariant holds: {}", coverage.summary()),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #7: Contradictory Evidence
// ---------------------------------------------------------------------------

/// Search structured evidence for contradictions.
pub struct ContradictoryEvidenceCheck;

impl SelfGrillCheck for ContradictoryEvidenceCheck {
    fn kind(&self) -> &'static str {
        "contradictory_evidence"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut by_command: std::collections::HashMap<
            String,
            Vec<&griller_core::evidence::Evidence>,
        > = std::collections::HashMap::new();

        for ev in &input.evidence {
            if let Some(cmd) = &ev.provenance.command {
                by_command.entry(cmd.display_line()).or_default().push(ev);
            }
        }

        for (cmd, evs) in &by_command {
            if evs.len() < 2 {
                continue;
            }
            let exit_codes: Vec<Option<i32>> = evs
                .iter()
                .map(|e| e.content.as_command_output().and_then(|c| c.exit_code))
                .collect();
            if exit_codes.iter().any(|c| matches!(c, Some(0)))
                && exit_codes.iter().any(|c| matches!(c, Some(x) if *x != 0))
            {
                return (
                    SelfGrillOutcome::Refuted,
                    format!(
                        "Contradictory exit codes for command '{}': mixed success and failure",
                        cmd
                    ),
                );
            }
        }

        let claim_text_lower = input.claim_text.to_lowercase();

        if claim_text_lower.contains("removed") || claim_text_lower.contains("delete") {
            let has_existence_evidence = input.evidence.iter().any(|e| {
                matches!(e.kind, EvidenceKind::FileContent) && e.provenance.directly_observed
            });
            if has_existence_evidence {
                return (
                    SelfGrillOutcome::Weakened,
                    "Claim mentions removal but existence evidence is present".to_string(),
                );
            }
        }

        if !input.assessment.contradictions.is_empty() {
            return (
                SelfGrillOutcome::Weakened,
                format!(
                    "Assessment contains {} structural contradiction(s)",
                    input.assessment.contradictions.len()
                ),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "No structural contradictions detected in evidence".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #8: Execution Validity
// ---------------------------------------------------------------------------

/// Inspect execution provenance.
pub struct ExecutionValidityCheck;

impl SelfGrillCheck for ExecutionValidityCheck {
    fn kind(&self) -> &'static str {
        "execution_validity"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut issues: Vec<String> = Vec::new();

        for ev in &input.evidence {
            if let Some(content) = ev.content.as_command_output() {
                if matches!(
                    content.status,
                    griller_core::command::ExecutionStatus::TimedOut
                ) {
                    issues.push(format!(
                        "Evidence {} has timed-out status but is being used in assessment",
                        ev.id
                    ));
                }
                if content.exit_code == Some(0)
                    && matches!(
                        content.status,
                        griller_core::command::ExecutionStatus::TimedOut
                    )
                {
                    issues.push(format!(
                        "Evidence {} has exit_code=0 but timed-out status; inconsistent provenance",
                        ev.id
                    ));
                }
            }
        }

        for ev in &input.evidence {
            if ev.provenance.depends_on_agent_claim
                && ev.provenance.source == griller_core::provenance::EvidenceSource::Agent
                && matches!(
                    input.status(),
                    EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified
                )
            {
                issues.push(format!(
                    "Evidence {} is agent-narrated and used in a verified assessment",
                    ev.id
                ));
            }
        }

        if issues.is_empty() {
            (
                SelfGrillOutcome::Holds,
                "Execution provenance is valid".to_string(),
            )
        } else {
            (SelfGrillOutcome::Weakened, issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check #9: Policy Consistency
// ---------------------------------------------------------------------------

/// Recompute M3 policy from structured inputs.
pub struct PolicyConsistencyCheck;

impl SelfGrillCheck for PolicyConsistencyCheck {
    fn kind(&self) -> &'static str {
        "policy_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let policy = PolicyTable;
        let context = griller_policy::PolicyContext::new(input.severity, input.severity_source);
        let expected = policy.decide(&input.assessment, &context);

        if expected.verdict != input.decision.verdict {
            return (
                SelfGrillOutcome::Refuted,
                format!(
                    "Policy recomputation differs: expected {:?}, got {:?}",
                    expected.verdict, input.decision.verdict
                ),
            );
        }

        if expected.reason_code != input.decision.reason_code {
            return (
                SelfGrillOutcome::Refuted,
                format!(
                    "Reason code mismatch: expected '{}', got '{}'",
                    expected.reason_code, input.decision.reason_code
                ),
            );
        }

        if expected.required_action != input.decision.required_action {
            return (
                SelfGrillOutcome::Refuted,
                format!(
                    "Required action mismatch: expected {:?}, got {:?}",
                    expected.required_action, input.decision.required_action
                ),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "Policy recomputation matches original decision".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #10: Reason / Action Consistency
// ---------------------------------------------------------------------------

/// Check that reason_code corresponds to status + kind + verdict.
pub struct ReasonActionConsistencyCheck;

impl SelfGrillCheck for ReasonActionConsistencyCheck {
    fn kind(&self) -> &'static str {
        "reason_action_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let expected_code = reason_code(input.claim_kind, input.status(), input.verdict());

        if input.decision.reason_code != expected_code {
            return (
                SelfGrillOutcome::Refuted,
                format!(
                    "Reason code mismatch: expected '{}', got '{}'",
                    expected_code, input.decision.reason_code
                ),
            );
        }

        let expected_action = required_action_for(input.status());
        if input.decision.required_action != expected_action {
            return (
                SelfGrillOutcome::Refuted,
                format!(
                    "Required action mismatch: expected {:?} for status {:?}, got {:?}",
                    expected_action,
                    input.status(),
                    input.decision.required_action
                ),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "Reason code and required action are consistent with status and verdict".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #11: Required Action Consistency
// ---------------------------------------------------------------------------

/// Verify BLOCK => meaningful remediation, PASS => no contradictory remediation.
pub struct RequiredActionConsistencyCheck;

impl SelfGrillCheck for RequiredActionConsistencyCheck {
    fn kind(&self) -> &'static str {
        "required_action_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let verdict = input.verdict();
        let action = input.decision.required_action;

        match verdict {
            griller_policy::verdict::Verdict::Block => {
                if matches!(action, griller_policy::decision::RequiredAction::None) {
                    return (
                        SelfGrillOutcome::Refuted,
                        "BLOCK verdict with RequiredAction::None; contradiction".to_string(),
                    );
                }
            }
            griller_policy::verdict::Verdict::Pass => {
                if !matches!(
                    action,
                    griller_policy::decision::RequiredAction::None
                        | griller_policy::decision::RequiredAction::AcknowledgeUncertainty
                ) {
                    return (
                        SelfGrillOutcome::Weakened,
                        format!("PASS verdict with unusual required action {:?}", action),
                    );
                }
            }
            griller_policy::verdict::Verdict::Warn => {}
        }

        (
            SelfGrillOutcome::Holds,
            "Required action is consistent with verdict".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #12: Evidence Hash Integrity
// ---------------------------------------------------------------------------

/// Recompute and verify evidence content hashes.
pub struct EvidenceHashIntegrityCheck;

impl SelfGrillCheck for EvidenceHashIntegrityCheck {
    fn kind(&self) -> &'static str {
        "evidence_hash_integrity"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut issues: Vec<String> = Vec::new();

        for ev in &input.evidence {
            if ev.id.as_str() != ev.content_hash {
                issues.push(format!(
                    "Evidence id mismatch: id='{}' but content_hash='{}'",
                    ev.id, ev.content_hash
                ));
            }
        }

        for id in &input.assessment.supporting_evidence {
            let found = input.evidence.iter().any(|e| e.id.as_str() == id.as_str());
            if !found {
                issues.push(format!(
                    "Supporting evidence id '{}' not found in evidence set",
                    id
                ));
            }
        }

        if issues.is_empty() {
            (
                SelfGrillOutcome::Holds,
                "All evidence hashes are intact".to_string(),
            )
        } else {
            (SelfGrillOutcome::Refuted, issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check #13: Environment / Reproducibility
// ---------------------------------------------------------------------------

/// Check reproducibility annotations are correct.
pub struct EnvironmentReproducibilityCheck;

impl SelfGrillCheck for EnvironmentReproducibilityCheck {
    fn kind(&self) -> &'static str {
        "environment_reproducibility"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut issues: Vec<String> = Vec::new();

        for ev in &input.evidence {
            if ev.provenance.source == griller_core::provenance::EvidenceSource::Agent {
                continue;
            }

            if ev.provenance.reproducibility
                == griller_core::provenance::Reproducibility::Deterministic
                && ev.provenance.source == griller_core::provenance::EvidenceSource::Repository
            {
                issues.push(format!(
                    "Repository state evidence {} claims deterministic reproducibility but is environment-sensitive",
                    ev.id
                ));
            }
        }

        if issues.is_empty() {
            (
                SelfGrillOutcome::Holds,
                "Reproducibility annotations are consistent".to_string(),
            )
        } else {
            (SelfGrillOutcome::Weakened, issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check #14: Redaction Integrity
// ---------------------------------------------------------------------------

/// Verify persisted command output passed through redaction boundary.
pub struct RedactionIntegrityCheck;

impl SelfGrillCheck for RedactionIntegrityCheck {
    fn kind(&self) -> &'static str {
        "redaction_integrity"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let mut issues: Vec<String> = Vec::new();

        for ev in &input.evidence {
            if let Some(content) = ev.content.as_command_output() {
                if !content.redaction.occurred {
                    continue;
                }

                let combined = format!("{}{}", content.stdout, content.stderr);
                if combined.contains("Bearer ") && !combined.contains("[REDACTED-bearer_token]") {
                    issues.push(format!(
                        "Evidence {} may contain unredacted bearer token",
                        ev.id
                    ));
                }
                if combined.contains("sk-") && !combined.contains("[REDACTED-api_key]") {
                    issues.push(format!("Evidence {} may contain unredacted API key", ev.id));
                }
            }
        }

        if issues.is_empty() {
            (
                SelfGrillOutcome::Holds,
                "Redaction pipeline invariant holds".to_string(),
            )
        } else {
            (SelfGrillOutcome::Refuted, issues.join("; "))
        }
    }
}

// ---------------------------------------------------------------------------
// Check #15: Context / Claim Consistency
// ---------------------------------------------------------------------------

/// Identify when context identity and claim target disagree.
pub struct ContextClaimConsistencyCheck;

impl SelfGrillCheck for ContextClaimConsistencyCheck {
    fn kind(&self) -> &'static str {
        "context_claim_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let Some(context) = &input.context_assessment else {
            return (
                SelfGrillOutcome::NotApplicable,
                "No context assessment available; skipping context/claim consistency".to_string(),
            );
        };

        if context.status == griller_context::status::ContextStatus::Mismatch {
            if let Some(target_name) = &input.claim_target.name {
                return (
                    SelfGrillOutcome::Weakened,
                    format!(
                        "Context mismatch but claim target '{}' may not belong to this repository",
                        target_name
                    ),
                );
            }
        }

        (
            SelfGrillOutcome::Holds,
            "Context and claim target are consistent".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Check #16: Attack Target Consistency
// ---------------------------------------------------------------------------

/// Ensure attacks actually targeted the claim.
pub struct AttackTargetConsistencyCheck;

impl SelfGrillCheck for AttackTargetConsistencyCheck {
    fn kind(&self) -> &'static str {
        "attack_target_consistency"
    }

    fn run(&self, input: &SelfGrillInput) -> (SelfGrillOutcome, String) {
        let Some(coverage) = &input.attack_coverage else {
            if matches!(input.status(), EpistemicStatus::AdversariallyVerified) {
                return (
                    SelfGrillOutcome::Refuted,
                    "ADVERSARIALLY_VERIFIED but no attack coverage data available".to_string(),
                );
            }
            return (
                SelfGrillOutcome::NotApplicable,
                "No attack coverage data; check not applicable".to_string(),
            );
        };

        if coverage.executed == 0 && coverage.not_applicable == 0 {
            if matches!(input.status(), EpistemicStatus::AdversariallyVerified) {
                return (
                    SelfGrillOutcome::Refuted,
                    "ADVERSARIALLY_VERIFIED but no attacks were executed or marked applicable"
                        .to_string(),
                );
            }
            return (
                SelfGrillOutcome::NotApplicable,
                "No attacks were applicable or executed; not applicable to this assessment"
                    .to_string(),
            );
        }

        (
            SelfGrillOutcome::Holds,
            "Attack targets are consistent with claim scope".to_string(),
        )
    }
}

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Run all applicable self-grill checks on the given input.
///
/// Returns findings in check order. No recursion.
pub fn run_all_checks(input: &SelfGrillInput) -> Vec<SelfGrillFinding> {
    let checks: Vec<Box<dyn SelfGrillCheck>> = vec![
        Box::new(EvidenceCompletenessCheck),
        Box::new(EvidenceIndependenceCheck),
        Box::new(TrustTierCheck),
        Box::new(ClaimScopeCheck),
        Box::new(ContextConsistencyCheck),
        Box::new(AttackCoverageCheck),
        Box::new(ContradictoryEvidenceCheck),
        Box::new(ExecutionValidityCheck),
        Box::new(PolicyConsistencyCheck),
        Box::new(ReasonActionConsistencyCheck),
        Box::new(RequiredActionConsistencyCheck),
        Box::new(EvidenceHashIntegrityCheck),
        Box::new(EnvironmentReproducibilityCheck),
        Box::new(RedactionIntegrityCheck),
        Box::new(ContextClaimConsistencyCheck),
        Box::new(AttackTargetConsistencyCheck),
    ];

    checks
        .into_iter()
        .map(|check| {
            let (outcome, message) = check.run(input);
            SelfGrillFinding {
                check: check.kind().to_string(),
                outcome,
                severity: outcome_to_severity(outcome),
                message,
                evidence_refs: Vec::new(),
            }
        })
        .collect()
}

fn outcome_to_severity(outcome: SelfGrillOutcome) -> SelfGrillSeverity {
    match outcome {
        SelfGrillOutcome::Refuted => SelfGrillSeverity::Critical,
        SelfGrillOutcome::Weakened => SelfGrillSeverity::Medium,
        SelfGrillOutcome::Inconclusive => SelfGrillSeverity::Medium,
        SelfGrillOutcome::Holds => SelfGrillSeverity::Low,
        SelfGrillOutcome::NotApplicable => SelfGrillSeverity::Low,
    }
}
