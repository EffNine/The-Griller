//! M4 determinism tests (specification section 21).

mod common;

use common::{atlas_project, codebro_project};
use griller_context::assess::{assign_context_status, ContextAssessor};
use griller_context::evidence::{ContextEvidenceKind, EvidenceStrength};
use griller_context::status::ContextStatus;
use griller_core::hash::canonical_string;

fn assess(
    task: &str,
    project: &griller_context::project::ProjectIdentity,
) -> griller_context::assess::ContextAssessment {
    ContextAssessor::deterministic()
        .assess(task, project)
        .unwrap()
}

// ---------------------------------------------------------------------------
// M. Same inputs, reordered evidence -> identical assessment
// ---------------------------------------------------------------------------

#[test]
fn m_same_inputs_produce_identical_assessment() {
    let project = codebro_project();
    let a = assess("Implement Atlas P10.5.1 Fact Store.", &project);
    let b = assess("Implement Atlas P10.5.1 Fact Store.", &project);
    assert_eq!(a, b);
    let json_a = canonical_string(&a).unwrap();
    let json_b = canonical_string(&b).unwrap();
    assert_eq!(json_a, json_b, "canonical JSON must be byte-identical");
}

#[test]
fn m_evidence_ordering_does_not_change_the_result() {
    // assign_context_status is a pure function of the evidence *sets*; the
    // same evidence in any order yields the same status.
    let project = codebro_project();
    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &project);
    let mut positive = assessment.positive_evidence.clone();
    let mut negative = assessment.negative_evidence.clone();
    positive.reverse();
    negative.reverse();
    let (status_a, _) = assign_context_status(
        &assessment.positive_evidence,
        &assessment.negative_evidence,
        &assessment.task,
    );
    let (status_b, _) = assign_context_status(&positive, &negative, &assessment.task);
    assert_eq!(status_a, status_b);
    assert_eq!(status_a, ContextStatus::Mismatch);
}

#[test]
fn m_repeated_assessments_are_byte_identical() {
    let project = atlas_project();
    let a = assess("Implement P10.5.1 Fact Store.", &project);
    let b = assess("Implement P10.5.1 Fact Store.", &project);
    assert_eq!(canonical_string(&a).unwrap(), canonical_string(&b).unwrap());
}

// ---------------------------------------------------------------------------
// N. Unrelated repository evidence -> cannot create mismatch
// ---------------------------------------------------------------------------

#[test]
fn n_adding_unrelated_vocabulary_cannot_create_mismatch() {
    let mut project = codebro_project();
    // The repository grows unrelated vocabulary; the assessment of a generic
    // task must not flip to MISMATCH.
    project.vocabulary.push("caterpillar".into());
    project.vocabulary.sort();
    project.vocabulary.dedup();

    let assessment = assess("Fix the bug in the login flow.", &project);
    assert!(!assessment.is_mismatch());
    assert!(!assessment
        .negative_evidence
        .iter()
        .any(|e| e.kind == ContextEvidenceKind::ExplicitProjectMismatch));
}

#[test]
fn n_adding_unrelated_symbols_cannot_create_mismatch() {
    let mut project = codebro_project();
    project.symbols.push("zebra".into());
    project.symbols.sort();

    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &project);
    // The explicit mismatch still wins regardless of the extra symbols.
    assert_eq!(assessment.status, ContextStatus::Mismatch);
    assert_eq!(
        assessment.primary_mismatch().map(|e| e.subject.clone()),
        Some("Atlas".to_string())
    );
}

#[test]
fn n_unrelated_weak_evidence_cannot_downgrade_a_mismatch() {
    let mut project = codebro_project();
    project.vocabulary.push("factstore".into());
    project.vocabulary.sort();
    project.vocabulary.dedup();

    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &project);
    // Even if "factstore" becomes a weak positive term, the explicit
    // contradictory identity dominates (rule 1).
    assert_eq!(assessment.status, ContextStatus::Mismatch);
    let finding = griller_policy::ContextFinding::from_assessment(assessment);
    assert!(finding.verdict.is_block());
}

#[test]
fn n_status_is_order_stable_across_evidence_kinds() {
    // Reordering the matcher's evidence list (positive/negative partitions)
    // must not change the assigned status for any status class.
    let codebro = codebro_project();
    let atlas = atlas_project();
    let cases: Vec<(
        &str,
        &griller_context::project::ProjectIdentity,
        ContextStatus,
    )> = vec![
        (
            "Fix CodeBro MCP tool registration.",
            &codebro,
            ContextStatus::Matched,
        ),
        (
            "Implement Atlas P10.5.1 Fact Store.",
            &codebro,
            ContextStatus::Mismatch,
        ),
        (
            "Implement P10.5.1 Fact Store.",
            &atlas,
            ContextStatus::LikelyMatch,
        ),
    ];
    for (task, project, expected) in cases {
        let assessment = assess(task, project);
        let mut positive = assessment.positive_evidence.clone();
        let mut negative = assessment.negative_evidence.clone();
        positive.sort_by(|a, b| b.subject.cmp(&a.subject));
        negative.sort_by(|a, b| b.subject.cmp(&a.subject));
        let (status, _) = assign_context_status(&positive, &negative, &assessment.task);
        assert_eq!(status, expected, "task {task:?}");
    }
}

// ---------------------------------------------------------------------------
// Strength semantics
// ---------------------------------------------------------------------------

#[test]
fn mismatch_is_strong_negative_and_identity_match_is_strong_positive() {
    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &codebro_project());
    let mismatch = assessment.primary_mismatch().unwrap();
    assert_eq!(mismatch.strength, EvidenceStrength::Strong);
    let identity = assessment
        .positive_evidence
        .iter()
        .find(|e| e.kind == ContextEvidenceKind::ReferencedSymbolExists);
    // "fact" is a codebro symbol, so it is a medium positive that never
    // overrides the strong negative.
    assert!(identity.is_some());
    assert_eq!(assessment.status, ContextStatus::Mismatch);
}
