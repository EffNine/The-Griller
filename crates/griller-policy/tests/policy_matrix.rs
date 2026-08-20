//! Deterministic policy matrix tests (specification sections 18 and 20).
//!
//! Constructs `ClaimAssessment` values directly so every (kind, status)
//! combination can be exercised without fighting the M2 normalizer, plus
//! end-to-end adjudication for representative cases.

mod common;

use griller_adjudicate::assess::ClaimAssessment;
use griller_adjudicate::claim::{Claim, ClaimInput, ClaimKind, ClaimSource};
use griller_adjudicate::normalizer::{ClaimNormalizer, DeterministicNormalizer};
use griller_adjudicate::status::EpistemicStatus;
use griller_adjudicate::support::SupportMatrix;
use griller_core::hash::canonical_string;

use griller_policy::decision::RequiredAction;
use griller_policy::finding::{aggregate, Finding, OverallDecision};
use griller_policy::policy::{Policy, PolicyContext, PolicyInput, PolicyTable};
use griller_policy::severity::{Severity, SeverityResolver, SeveritySignals, SeveritySource};
use griller_policy::table::{rule_for, verdict_from_rule, Rule};
use griller_policy::verdict::Verdict;
use griller_policy::DivaRenderer;

/// Build an assessment with an explicit claim kind and epistemic status,
/// independent of the M2 normalizer/matcher.
fn assessment(kind: ClaimKind, status: EpistemicStatus) -> ClaimAssessment {
    let input = ClaimInput::new("test claim", ClaimSource::Agent).with_kind(kind);
    let normalized = DeterministicNormalizer
        .normalize(&input)
        .expect("deterministic normalization is infallible");
    let claim = Claim::new(
        ClaimSource::Agent,
        kind,
        "test claim",
        normalized.target.clone(),
        normalized.predicates.clone(),
        None,
    );
    let matrix = SupportMatrix::new(normalized.proposition.clone(), vec![]);
    ClaimAssessment {
        claim,
        normalized,
        matrix,
        contradictions: vec![],
        status,
        reasons: vec![],
        supporting_evidence: vec![],
        contradicting_evidence: vec![],
        insufficient: vec![],
    }
}

fn decide(
    kind: ClaimKind,
    status: EpistemicStatus,
    severity: Severity,
) -> griller_policy::Decision {
    let ctx = PolicyContext::new(severity, SeveritySource::Declared);
    PolicyTable.decide(&assessment(kind, status), &ctx)
}

fn ctx(severity: Severity) -> PolicyContext {
    PolicyContext::new(severity, SeveritySource::Declared)
}

// ---------------------------------------------------------------------------
// A–K: single-finding matrix
// ---------------------------------------------------------------------------

#[test]
fn a_completion_verified_low_is_pass() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Verified,
        Severity::Low,
    );
    assert_eq!(d.verdict, Verdict::Pass);
    assert_eq!(d.reason_code, "VERIFIED_COMPLETION_PASS");
    assert_eq!(d.required_action, RequiredAction::None);
}

#[test]
fn b_completion_verified_high_is_pass() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Verified,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Pass);
}

#[test]
fn c_completion_unsupported_low_is_warn() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Unsupported,
        Severity::Low,
    );
    assert_eq!(d.verdict, Verdict::Warn);
    assert_eq!(d.reason_code, "UNSUPPORTED_COMPLETION_WARN");
    assert_eq!(d.required_action, RequiredAction::ProvideEvidence);
}

#[test]
fn d_completion_unsupported_high_is_block() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Unsupported,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
    assert_eq!(d.reason_code, "UNSUPPORTED_COMPLETION_BLOCK");
}

#[test]
fn e_completion_unverifiable_high_is_block() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Unverifiable,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
    // UNVERIFIABLE is never rendered as FALSE (spec section 7).
    assert!(
        !d.reason.to_lowercase().contains("false"),
        "unverifiable reason must not claim falsity: {}",
        d.reason
    );
    assert_eq!(d.required_action, RequiredAction::MakeVerificationAvailable);
}

#[test]
fn f_completion_contradicted_high_is_block() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Contradicted,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
    assert_eq!(d.reason_code, "CONTRADICTED_COMPLETION_BLOCK");
    assert_eq!(d.required_action, RequiredAction::ResolveConflict);
}

#[test]
fn g_completion_false_high_is_block() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::False,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
    assert_eq!(d.reason_code, "FALSE_COMPLETION_BLOCK");
    assert_eq!(d.required_action, RequiredAction::CorrectClaim);
}

#[test]
fn h_design_unsupported_low_is_warn() {
    let d = decide(
        ClaimKind::Design,
        EpistemicStatus::Unsupported,
        Severity::Low,
    );
    assert_eq!(d.verdict, Verdict::Warn);
}

#[test]
fn i_design_uncertain_medium_is_warn() {
    let d = decide(
        ClaimKind::Design,
        EpistemicStatus::Uncertain,
        Severity::Medium,
    );
    assert_eq!(d.verdict, Verdict::Warn);
    assert_eq!(d.required_action, RequiredAction::AcknowledgeUncertainty);
}

#[test]
fn j_design_verified_is_pass() {
    for severity in [
        Severity::Info,
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ] {
        let d = decide(ClaimKind::Design, EpistemicStatus::Verified, severity);
        assert_eq!(d.verdict, Verdict::Pass);
    }
}

#[test]
fn k_fact_contradicted_high_is_block() {
    let d = decide(
        ClaimKind::Fact,
        EpistemicStatus::Contradicted,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
}

// ---------------------------------------------------------------------------
// L: conservative default for unmatched combinations
// ---------------------------------------------------------------------------

#[test]
fn l_no_rule_defaults_to_warn() {
    // The NoRule path can never PASS, even at Critical severity.
    for severity in [Severity::Info, Severity::Medium, Severity::Critical] {
        assert_eq!(verdict_from_rule(Rule::NoRule, severity), Verdict::Warn);
    }
    // Every currently-known (kind, status) pair has an explicit rule.
    use griller_adjudicate::claim::ClaimKind::*;
    use griller_adjudicate::status::EpistemicStatus::*;
    for kind in [Completion, Fact, Causal, Requirement, Design] {
        for status in [
            Verified,
            AdversariallyVerified,
            Unsupported,
            Unverifiable,
            Uncertain,
            Contradicted,
            False,
        ] {
            assert_ne!(
                rule_for(kind, status),
                Rule::NoRule,
                "{kind:?} + {status:?} must have an explicit rule"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// M–O: multiple-finding aggregation
// ---------------------------------------------------------------------------

fn finding_for(kind: ClaimKind, status: EpistemicStatus, severity: Severity) -> Finding {
    let input = PolicyInput {
        assessment: assessment(kind, status),
        context: ctx(severity),
    };
    PolicyTable.evaluate(&input)
}

#[test]
fn m_one_block_beats_many_pass() {
    let findings = vec![
        finding_for(
            ClaimKind::Completion,
            EpistemicStatus::Verified,
            Severity::Low,
        ),
        finding_for(
            ClaimKind::Design,
            EpistemicStatus::Verified,
            Severity::Medium,
        ),
        finding_for(
            ClaimKind::Completion,
            EpistemicStatus::Unsupported,
            Severity::High,
        ),
        finding_for(ClaimKind::Fact, EpistemicStatus::Verified, Severity::Low),
    ];
    assert_eq!(aggregate(&findings), Verdict::Block);
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.verdict == Verdict::Pass)
            .count(),
        3
    );
    assert_eq!(
        findings
            .iter()
            .filter(|f| f.verdict == Verdict::Block)
            .count(),
        1
    );
}

#[test]
fn n_one_warn_beats_many_pass() {
    let findings = vec![
        finding_for(
            ClaimKind::Completion,
            EpistemicStatus::Verified,
            Severity::Low,
        ),
        finding_for(
            ClaimKind::Design,
            EpistemicStatus::Uncertain,
            Severity::Medium,
        ),
        finding_for(ClaimKind::Fact, EpistemicStatus::Verified, Severity::Low),
    ];
    assert_eq!(aggregate(&findings), Verdict::Warn);
}

#[test]
fn o_all_pass_is_pass() {
    let findings = vec![
        finding_for(
            ClaimKind::Completion,
            EpistemicStatus::Verified,
            Severity::Low,
        ),
        finding_for(ClaimKind::Design, EpistemicStatus::Verified, Severity::High),
    ];
    assert_eq!(aggregate(&findings), Verdict::Pass);
}

// ---------------------------------------------------------------------------
// P: UNVERIFIABLE is never rendered as FALSE
// ---------------------------------------------------------------------------

#[test]
fn p_unverifiable_is_never_rendered_as_false() {
    let d = decide(
        ClaimKind::Completion,
        EpistemicStatus::Unverifiable,
        Severity::High,
    );
    assert_eq!(d.verdict, Verdict::Block);
    assert_eq!(d.reason_code, "UNVERIFIABLE_COMPLETION_BLOCK");
    assert!(
        !d.reason.to_lowercase().contains("false"),
        "blocking reason for UNVERIFIABLE must not claim falsity: {}",
        d.reason
    );
    let diva = DivaRenderer.render_verdict(d.verdict, Some(EpistemicStatus::Unverifiable));
    // The persona line must frame the block around *inability to verify*,
    // not a claim of falsity: "can't prove you're wrong ... not a PASS".
    assert!(diva.contains("can't prove you're wrong"));
    assert!(diva.contains("That's not a PASS"));
}

// ---------------------------------------------------------------------------
// Q: agent claim alone cannot create PASS
// ---------------------------------------------------------------------------

#[test]
fn q_agent_claim_alone_cannot_pass() {
    let assessment = common::assess("Tests pass.", &[agent_claim("tests pass")]);
    assert_eq!(assessment.status, EpistemicStatus::Unsupported);
    for severity in [
        Severity::Info,
        Severity::Low,
        Severity::Medium,
        Severity::High,
        Severity::Critical,
    ] {
        let d = PolicyTable.decide(&assessment, &ctx(severity));
        assert_ne!(
            d.verdict,
            Verdict::Pass,
            "agent narration must never produce PASS at {severity}"
        );
    }
}

use common::agent_claim;

// ---------------------------------------------------------------------------
// S: determinism
// ---------------------------------------------------------------------------

#[test]
fn s_same_policy_input_byte_identical_decision() {
    let a = assessment(ClaimKind::Completion, EpistemicStatus::Unsupported);
    let d1 = PolicyTable.decide(&a, &ctx(Severity::High));
    let d2 = PolicyTable.decide(&a, &ctx(Severity::High));
    assert_eq!(
        canonical_string(&d1).unwrap(),
        canonical_string(&d2).unwrap()
    );
}

// ---------------------------------------------------------------------------
// T: adding unrelated findings cannot change a single finding's decision
// ---------------------------------------------------------------------------

#[test]
fn t_unrelated_findings_cannot_change_a_decision() {
    let block = finding_for(
        ClaimKind::Completion,
        EpistemicStatus::Contradicted,
        Severity::High,
    );
    assert_eq!(block.verdict, Verdict::Block);

    let mut extras = vec![
        finding_for(ClaimKind::Design, EpistemicStatus::Verified, Severity::Low),
        finding_for(ClaimKind::Fact, EpistemicStatus::Verified, Severity::Medium),
        finding_for(ClaimKind::Causal, EpistemicStatus::Verified, Severity::Low),
    ];
    extras.push(block.clone());
    assert_eq!(aggregate(&extras), Verdict::Block);
    // The individual finding is unchanged.
    assert_eq!(extras.last().unwrap().verdict, Verdict::Block);
    assert_eq!(extras.last().unwrap().reason_code, block.reason_code);
}

// ---------------------------------------------------------------------------
// Severity derivation integration
// ---------------------------------------------------------------------------

#[test]
fn derived_completion_severity_drives_verdict() {
    // Derived (no declared) completion severity is MEDIUM; unsupported at
    // MEDIUM is WARN. With a production signal it becomes HIGH => BLOCK.
    let resolver = SeverityResolver;
    let derived = resolver.resolve(ClaimKind::Completion, &SeveritySignals::default());
    assert_eq!(derived.severity, Severity::Medium);
    assert_eq!(derived.source, SeveritySource::Derived);
    assert_eq!(
        decide(
            ClaimKind::Completion,
            EpistemicStatus::Unsupported,
            derived.severity
        )
        .verdict,
        Verdict::Warn
    );

    let production = resolver.resolve(
        ClaimKind::Completion,
        &SeveritySignals {
            declared: None,
            touches_production: true,
            simple_refactor: false,
        },
    );
    assert_eq!(production.severity, Severity::High);
    assert_eq!(
        decide(
            ClaimKind::Completion,
            EpistemicStatus::Unsupported,
            production.severity
        )
        .verdict,
        Verdict::Block
    );
}

// ---------------------------------------------------------------------------
// Exit codes
// ---------------------------------------------------------------------------

#[test]
fn verdict_exit_codes_match_specification() {
    assert_eq!(Verdict::Pass.exit_code(), 0);
    assert_eq!(Verdict::Warn.exit_code(), 1);
    assert_eq!(Verdict::Block.exit_code(), 2);
}

// ---------------------------------------------------------------------------
// Reason codes are stable and non-empty for every combination
// ---------------------------------------------------------------------------

#[test]
fn reason_codes_are_stable_for_the_whole_matrix() {
    use griller_adjudicate::claim::ClaimKind::*;
    use griller_adjudicate::status::EpistemicStatus::*;
    for kind in [Completion, Fact, Causal, Requirement, Design] {
        for status in [
            Verified,
            AdversariallyVerified,
            Unsupported,
            Unverifiable,
            Uncertain,
            Contradicted,
            False,
        ] {
            let d = decide(kind, status, Severity::High);
            assert!(
                !d.reason_code.starts_with("NO_POLICY_RULE"),
                "all combinations must have explicit codes"
            );
            assert!(!d.reason_code.is_empty());
            assert!(!d.reason.is_empty());
            assert!(!d.required_action_message.is_empty());
        }
    }
}

// ---------------------------------------------------------------------------
// OverallDecision shape
// ---------------------------------------------------------------------------

#[test]
fn overall_decision_is_deterministic_and_serializable() {
    let inputs = vec![
        PolicyInput {
            assessment: assessment(ClaimKind::Completion, EpistemicStatus::Verified),
            context: ctx(Severity::Low),
        },
        PolicyInput {
            assessment: assessment(ClaimKind::Completion, EpistemicStatus::Unsupported),
            context: ctx(Severity::High),
        },
    ];
    let overall: OverallDecision = PolicyTable.decide_many(&inputs);
    assert_eq!(overall.verdict, Verdict::Block);
    assert_eq!(overall.findings.len(), 2);
    let json = canonical_string(&overall).unwrap();
    assert!(json.contains("UNSUPPORTED_COMPLETION_BLOCK"));
}
