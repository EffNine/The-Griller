//! Invariant / property tests for M2 (specification section 16).
//!
//! These tests assert invariants over a corpus of claims and evidence
//! combinations rather than single examples. They do not depend on any test
//! framework beyond `#[test]`.

mod common;

use common::*;
use griller_adjudicate::claim::{ClaimInput, ClaimSource};
use griller_adjudicate::relationship::{EvidenceMatch, EvidenceRelationship};
use griller_adjudicate::status::EpistemicStatus;
use griller_adjudicate::{Adjudicator, ClaimAssessment};
use griller_core::command::ExecutionStatus;
use griller_core::evidence::{Evidence, EvidenceKind};
use griller_core::hash::canonical_string;

fn assess(text: &str, evidence: &[Evidence]) -> ClaimAssessment {
    Adjudicator::deterministic()
        .assess(&ClaimInput::new(text, ClaimSource::Agent), evidence, None)
        .unwrap()
}

/// A corpus of claims the invariants are checked against.
fn claim_corpus() -> Vec<&'static str> {
    vec![
        "Tests pass.",
        "Tests fail.",
        "The project builds successfully.",
        "The build fails.",
        "Project formatting passes.",
        "Authentication tests pass.",
        "The file src/auth.rs was changed.",
        "The file src/old.rs was removed.",
        "The file src/auth.rs exists.",
    ]
}

/// A corpus of evidence sets.
fn evidence_corpus() -> Vec<Vec<Evidence>> {
    vec![
        vec![],
        vec![agent_claim("tests pass")],
        vec![test_ok()],
        vec![test_fail()],
        vec![test_unavailable()],
        vec![test_timeout()],
        vec![fmt_ok()],
        vec![lint_ok()],
        vec![build_ok()],
        vec![deletion_diff("src/old.rs")],
        vec![file_content("src/auth.rs", "pub fn login() {}")],
        vec![test_ok(), agent_claim("tests pass")],
        vec![test_ok(), test_fail()],
        vec![git_diff("", "", "")],
    ]
}

// ---------------------------------------------------------------------------
// Invariant 1 — no evidence => never VERIFIED
// ---------------------------------------------------------------------------

#[test]
fn invariant_1_no_evidence_never_verified() {
    for claim in claim_corpus() {
        let a = assess(claim, &[]);
        assert_ne!(
            a.status,
            EpistemicStatus::Verified,
            "claim {claim:?} with no evidence must not be VERIFIED"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 2 — agent narration alone => never VERIFIED
// ---------------------------------------------------------------------------

#[test]
fn invariant_2_narration_alone_never_verified() {
    for claim in claim_corpus() {
        let a = assess(claim, &[agent_claim("I did it and it works")]);
        assert_ne!(
            a.status,
            EpistemicStatus::Verified,
            "claim {claim:?} with narration alone must not be VERIFIED"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 3 — irrelevant evidence cannot support a claim
// ---------------------------------------------------------------------------

#[test]
fn invariant_3_irrelevant_evidence_cannot_support() {
    let irrelevant: Vec<Evidence> = vec![
        command(
            "echo",
            &["hello"],
            "hello\n",
            "",
            Some(0),
            ExecutionStatus::Completed,
        ),
        command(
            "git",
            &["status", "--porcelain"],
            "",
            "",
            Some(0),
            ExecutionStatus::Completed,
        ),
        agent_claim("unrelated stuff"),
    ];
    for claim in claim_corpus() {
        let a = assess(claim, &irrelevant);
        for m in &a.matrix.matches {
            if m.relationship == EvidenceRelationship::Irrelevant {
                assert!(!m.quality.relevant);
            }
            assert_ne!(m.relationship, EvidenceRelationship::Supports);
        }
        assert!(a.supporting_evidence.is_empty());
        assert_ne!(a.status, EpistemicStatus::Verified);
    }
}

// ---------------------------------------------------------------------------
// Invariant 4 — command failure alone cannot imply FALSE
// ---------------------------------------------------------------------------

#[test]
fn invariant_4_command_failure_never_implies_false() {
    for claim in claim_corpus() {
        for evidence in [vec![test_unavailable()], vec![test_timeout()]] {
            let a = assess(claim, &evidence);
            assert_ne!(
                a.status,
                EpistemicStatus::False,
                "claim {claim:?}: unavailable/timeout evidence must not imply FALSE"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Invariant 5 — LLM output cannot directly assign epistemic status
// ---------------------------------------------------------------------------

#[test]
fn invariant_5_llm_cannot_assign_status() {
    use griller_adjudicate::llm::LlmNormalizer;
    use griller_adjudicate::normalizer::{ClaimNormalizer, FallbackNormalizer};
    use griller_adjudicate::NormalizationError;

    let raw = r#"{"claim":"tests pass","epistemic_status":"VERIFIED","verdict":"BLOCK","status":"ADVERSARIALLY_VERIFIED"}"#;

    // Rejected at the boundary.
    assert!(matches!(
        LlmNormalizer::new(raw).normalize(&ClaimInput::new("tests pass", ClaimSource::Agent)),
        Err(NormalizationError::InvalidLlmOutput(_))
    ));

    // Through the full pipeline with fallback, the only statuses reachable
    // are the evidence-derived ones.
    let adjudicator = Adjudicator::deterministic().with_normalizer(Box::new(
        FallbackNormalizer::over(Some(Box::new(LlmNormalizer::new(raw)))),
    ));
    let input = ClaimInput::new("tests pass", ClaimSource::Agent);
    let empty = adjudicator.assess(&input, &[], None).unwrap();
    assert_eq!(empty.status, EpistemicStatus::Unsupported);
    let ok = adjudicator.assess(&input, &[test_ok()], None).unwrap();
    assert_eq!(ok.status, EpistemicStatus::Verified);
    let fail = adjudicator.assess(&input, &[test_fail()], None).unwrap();
    assert_eq!(fail.status, EpistemicStatus::Contradicted);
}

// ---------------------------------------------------------------------------
// Invariant 6 — adding irrelevant evidence cannot flip VERIFIED to CONTRADICTED
// ---------------------------------------------------------------------------

#[test]
fn invariant_6_irrelevant_evidence_cannot_change_verified() {
    let verified_claims = [
        ("Tests pass.", vec![test_ok()]),
        ("The project builds successfully.", vec![build_ok()]),
        (
            "The file src/auth.rs exists.",
            vec![file_content("src/auth.rs", "fn x(){}")],
        ),
    ];
    for (claim, base) in verified_claims {
        let a = assess(claim, &base);
        assert_eq!(a.status, EpistemicStatus::Verified);
        let mut extended = base.clone();
        extended.push(fmt_ok());
        extended.push(agent_claim("I verified it myself"));
        let b = assess(claim, &extended);
        assert_eq!(
            b.status,
            EpistemicStatus::Verified,
            "claim {claim:?}: irrelevant evidence must not flip VERIFIED"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 7 — adding direct contradictory evidence must not leave VERIFIED
// ---------------------------------------------------------------------------

#[test]
fn invariant_7_contradictory_evidence_must_break_verified() {
    type Contradictor = Box<dyn Fn() -> Evidence>;
    let cases: Vec<(&str, Vec<Evidence>, Contradictor)> = vec![
        ("Tests pass.", vec![test_ok()], Box::new(test_fail)),
        (
            "The project builds successfully.",
            vec![build_ok()],
            Box::new(build_fail),
        ),
        (
            "Project formatting passes.",
            vec![fmt_ok()],
            Box::new(|| {
                command(
                    "cargo",
                    &["fmt", "--check"],
                    "",
                    "diff detected\n",
                    Some(1),
                    griller_core::command::ExecutionStatus::Completed,
                )
            }),
        ),
    ];
    for (claim, base, contradictory) in cases {
        let a = assess(claim, &base);
        assert_eq!(a.status, EpistemicStatus::Verified);
        let mut extended = base.clone();
        extended.push(contradictory());
        let b = assess(claim, &extended);
        assert_ne!(
            b.status,
            EpistemicStatus::Verified,
            "claim {claim:?}: direct contradiction must break VERIFIED"
        );
    }
}

// ---------------------------------------------------------------------------
// Invariant 8 — same inputs => same assessment (also order-independent)
// ---------------------------------------------------------------------------

#[test]
fn invariant_8_same_inputs_same_assessment() {
    for claim in claim_corpus() {
        for evidence in evidence_corpus() {
            let a = assess(claim, &evidence);
            let b = assess(claim, &evidence);
            assert_eq!(
                canonical_string(&a).unwrap(),
                canonical_string(&b).unwrap(),
                "claim {claim:?}: repeated assessment must be identical"
            );
            // Evidence order must not matter.
            let mut shuffled = evidence.clone();
            shuffled.reverse();
            let c = assess(claim, &shuffled);
            assert_eq!(
                canonical_string(&a).unwrap(),
                canonical_string(&c).unwrap(),
                "claim {claim:?}: evidence order must not matter"
            );
        }
    }
}

// ---------------------------------------------------------------------------
// Extra invariants beyond the numbered list
// ---------------------------------------------------------------------------

/// ADVERSARIALLY_VERIFIED is reserved for M5 and must never be produced by M2.
#[test]
fn adversarially_verified_is_unreachable_in_m2() {
    for claim in claim_corpus() {
        for evidence in evidence_corpus() {
            let a = assess(claim, &evidence);
            assert_ne!(
                a.status,
                EpistemicStatus::AdversariallyVerified,
                "claim {claim:?}: M2 must never produce ADVERSARIALLY_VERIFIED"
            );
        }
    }
}

/// Matches are ordered deterministically by evidence id.
#[test]
fn matrix_is_ordered_by_evidence_id() {
    let evidence = vec![test_fail(), test_ok(), agent_claim("x")];
    let a = assess("Tests pass.", &evidence);
    let ids: Vec<&str> = a
        .matrix
        .matches
        .iter()
        .map(|m| m.evidence_id.as_str())
        .collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

/// The status is never a PASS/WARN/BLOCK verdict: the assessment has no
/// verdict field, and no serialization contains a verdict key.
#[test]
fn no_operational_verdict_is_produced() {
    for claim in claim_corpus() {
        for evidence in evidence_corpus() {
            let a = assess(claim, &evidence);
            let json = canonical_string(&a).unwrap();
            let value: serde_json::Value = serde_json::from_str(&json).unwrap();
            assert!(
                value.get("verdict").is_none(),
                "claim {claim:?}: assessment must not carry an operational verdict"
            );
            assert!(
                value.get("policy").is_none(),
                "claim {claim:?}: assessment must not carry a policy decision"
            );
        }
    }
}

/// Every environment-derived match carries a provenance-based quality record.
#[test]
fn environment_matches_are_independent_and_direct() {
    for claim in claim_corpus() {
        let a = assess(claim, &[test_ok(), deletion_diff("src/old.rs")]);
        for m in &a.matrix.matches {
            if m.evidence_kind != EvidenceKind::AgentClaim {
                assert!(m.quality.direct_observation);
                assert!(m.quality.independent);
            }
        }
    }
}

/// Dedup: the same evidence id appears at most once in the matrix.
#[test]
fn matrix_deduplicates_evidence_by_id() {
    let ok = test_ok();
    let evidence = vec![ok.clone(), ok.clone()];
    let a = assess("Tests pass.", &evidence);
    assert_eq!(a.matrix.matches.len(), 1);
    assert_eq!(a.matrix.matches[0].evidence_id, ok.id);
}

/// Every match's `relevant` flag is consistent with its relationship.
#[test]
fn relevant_flag_is_consistent_with_relationship() {
    for claim in claim_corpus() {
        for evidence in evidence_corpus() {
            let a = assess(claim, &evidence);
            for m in &a.matrix.matches {
                let expected = m.relationship != EvidenceRelationship::Irrelevant;
                assert_eq!(m.relevant, expected);
            }
        }
    }
}

/// Convenience accessor so `EvidenceMatch` stays imported (used above).
#[allow(dead_code)]
fn _evidence_match_marker(_m: &EvidenceMatch) {}
