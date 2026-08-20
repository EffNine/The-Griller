//! M4 golden test corpus (specification sections 16–25, cases A–Q).
//!
//! Every case asserts a *documented* rule, not a guessed one. The core
//! invariant is exercised throughout: **absence is not mismatch**.

mod common;

use common::{atlas_project, codebro_project, generic_project, meridian_project};
use griller_context::assess::ContextAssessor;
use griller_context::evidence::{ContextEvidenceKind, EvidenceStrength};
use griller_context::project::ProjectIdentity;
use griller_context::status::ContextStatus;

fn assess(task: &str, project: &ProjectIdentity) -> griller_context::assess::ContextAssessment {
    ContextAssessor::deterministic()
        .assess(task, project)
        .unwrap()
}

fn has_kind(
    kind: ContextEvidenceKind,
    evidence: &[griller_context::evidence::ContextEvidence],
) -> bool {
    evidence.iter().any(|e| e.kind == kind)
}

fn mismatch_of(assessment: &griller_context::assess::ContextAssessment) -> Option<String> {
    assessment.primary_mismatch().map(|e| e.subject.clone())
}

// ---------------------------------------------------------------------------
// A. Exact project match -> MATCHED
// ---------------------------------------------------------------------------

#[test]
fn a_exact_project_match_is_matched() {
    let assessment = assess("Fix CodeBro MCP tool registration.", &codebro_project());
    assert_eq!(assessment.status, ContextStatus::Matched);
    assert!(has_kind(
        ContextEvidenceKind::ExplicitProjectMatch,
        &assessment.positive_evidence
    ));
}

// ---------------------------------------------------------------------------
// B. Explicit other-project mismatch -> MISMATCH + HIGH + BLOCK
// ---------------------------------------------------------------------------

#[test]
fn b_explicit_other_project_mismatch_is_blockworthy() {
    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &codebro_project());
    assert_eq!(assessment.status, ContextStatus::Mismatch);
    let mismatch = assessment
        .primary_mismatch()
        .expect("explicit mismatch evidence expected");
    assert_eq!(mismatch.kind, ContextEvidenceKind::ExplicitProjectMismatch);
    assert_eq!(mismatch.strength, EvidenceStrength::Strong);
    assert_eq!(mismatch.subject, "Atlas");
    assert_eq!(mismatch.repository_identity.as_deref(), Some("CodeBro"));

    // M3 bridge (griller-policy) maps this to a BLOCK at HIGH severity.
    let finding = griller_policy::ContextFinding::from_assessment(assessment);
    assert_eq!(finding.severity, griller_policy::Severity::High);
    assert_eq!(finding.verdict, griller_policy::Verdict::Block);
    assert_eq!(finding.reason_code, "MISMATCH_CONTEXT_BLOCK");
}

// ---------------------------------------------------------------------------
// C. Referenced file exists -> positive evidence
// ---------------------------------------------------------------------------

#[test]
fn c_referenced_file_exists_is_positive() {
    let assessment = assess("Fix src/lib.rs in the CodeBro repo.", &codebro_project());
    assert!(has_kind(
        ContextEvidenceKind::ReferencedPathExists,
        &assessment.positive_evidence
    ));
}

// ---------------------------------------------------------------------------
// D. Referenced file absent but task generic -> no BLOCK
// ---------------------------------------------------------------------------

#[test]
fn d_referenced_file_absent_generic_task_never_blocks() {
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

// ---------------------------------------------------------------------------
// E. Referenced module absent + explicit mismatch -> strong negative evidence
// ---------------------------------------------------------------------------

#[test]
fn e_absent_module_plus_explicit_mismatch_is_strong_negative() {
    // The task explicitly names a foreign project AND an explicit module that
    // does not exist here: strong negative evidence from both.
    let assessment = assess(
        "Implement Atlas P10.5.1 in the fact_store module.",
        &codebro_project(),
    );
    assert_eq!(assessment.status, ContextStatus::Mismatch);
    assert!(has_kind(
        ContextEvidenceKind::ExplicitProjectMismatch,
        &assessment.negative_evidence
    ));
    assert!(has_kind(
        ContextEvidenceKind::ReferencedEntityAbsent,
        &assessment.negative_evidence
    ));
    // The receipt carries the task entity, the repository identity and the
    // absence evidence.
    assert_eq!(mismatch_of(&assessment).as_deref(), Some("Atlas"));
}

// ---------------------------------------------------------------------------
// F. Git remote matches task project -> positive evidence
// ---------------------------------------------------------------------------

#[test]
fn f_git_remote_match_is_positive() {
    let assessment = assess("Fix codebro remote sync.", &codebro_project());
    assert!(has_kind(
        ContextEvidenceKind::RemoteMatch,
        &assessment.positive_evidence
    ));
    assert_eq!(assessment.status, ContextStatus::Matched);
}

// ---------------------------------------------------------------------------
// G. Manifest name matches -> positive evidence
// ---------------------------------------------------------------------------

#[test]
fn g_manifest_name_match_is_positive() {
    let assessment = assess("Fix griller-core.", &codebro_project());
    assert!(has_kind(
        ContextEvidenceKind::ManifestMatch,
        &assessment.positive_evidence
    ));
    assert_eq!(assessment.status, ContextStatus::Matched);
}

// ---------------------------------------------------------------------------
// H. Project name normalization -> equivalent names match
// ---------------------------------------------------------------------------

#[test]
fn h_name_normalization_equates_variants() {
    // code-bro, CodeBro and codebro all normalize to the same identity.
    for variant in [
        "Fix code-bro claim assess.",
        "Fix CodeBro claim assess.",
        "Fix codebro claim assess.",
    ] {
        let assessment = assess(variant, &codebro_project());
        assert_eq!(
            assessment.status,
            ContextStatus::Matched,
            "variant {variant:?}"
        );
    }
}

// ---------------------------------------------------------------------------
// I. Generic task -> no mismatch
// ---------------------------------------------------------------------------

#[test]
fn i_generic_task_has_no_mismatch() {
    let assessment = assess("Fix the bug in the login flow.", &generic_project());
    assert!(!assessment.is_mismatch());
    assert!(!has_kind(
        ContextEvidenceKind::ExplicitProjectMismatch,
        &assessment.negative_evidence
    ));
}

// ---------------------------------------------------------------------------
// J. Conflicting project identity -> AMBIGUOUS (documented resolution rule)
// ---------------------------------------------------------------------------

#[test]
fn j_conflicting_identity_is_ambiguous() {
    // Task names Atlas; Meridian is the repo identity but has a weak Atlas
    // adapter presence. Documented rule: a foreign project with weak presence
    // is a structured conflict, not a clean mismatch -> AMBIGUOUS.
    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &meridian_project());
    assert_eq!(assessment.status, ContextStatus::Ambiguous);
    assert!(has_kind(
        ContextEvidenceKind::ConflictingEntityReference,
        &assessment.negative_evidence
    ));
    assert!(!has_kind(
        ContextEvidenceKind::ExplicitProjectMismatch,
        &assessment.negative_evidence
    ));
}

// ---------------------------------------------------------------------------
// O. Positive fixture -> not blocked
// ---------------------------------------------------------------------------

#[test]
fn o_positive_fixture_is_not_blocked() {
    let assessment = assess("Implement P10.5.1 Fact Store.", &atlas_project());
    assert_eq!(assessment.status, ContextStatus::LikelyMatch);
    // Substantial positive evidence: the fact store module/path or its symbol
    // is present.
    assert!(
        has_kind(
            ContextEvidenceKind::ReferencedModuleExists,
            &assessment.positive_evidence
        ) || has_kind(
            ContextEvidenceKind::ReferencedSymbolExists,
            &assessment.positive_evidence
        )
    );
    let finding = griller_policy::ContextFinding::from_assessment(assessment);
    assert_ne!(finding.verdict, griller_policy::Verdict::Block);
    assert_eq!(
        finding.required_action,
        griller_policy::RequiredAction::None
    );
}

// ---------------------------------------------------------------------------
// P. Original CodeBro/Atlas-style regression -> BLOCK
// ---------------------------------------------------------------------------

#[test]
fn p_original_wrong_project_regression_blocks() {
    let assessment = assess("Implement Atlas P10.5.1 Fact Store.", &codebro_project());
    let finding = griller_policy::ContextFinding::from_assessment(assessment);
    assert_eq!(finding.verdict, griller_policy::Verdict::Block);
    assert_eq!(finding.reason_code, "MISMATCH_CONTEXT_BLOCK");
    assert_eq!(
        finding.required_action,
        griller_policy::RequiredAction::ConfirmProject
    );
}

// ---------------------------------------------------------------------------
// Section 25: ABSENCE IS NOT MISMATCH (dedicated regression tests)
// ---------------------------------------------------------------------------

#[test]
fn absent_referenced_file_alone_is_not_mismatch() {
    let assessment = assess("Fix src/nope.rs.", &codebro_project());
    assert!(!assessment.is_mismatch());
    assert!(has_kind(
        ContextEvidenceKind::ReferencedEntityAbsent,
        &assessment.negative_evidence
    ));
}

#[test]
fn absent_referenced_module_alone_is_not_mismatch() {
    let assessment = assess("Fix the login module.", &codebro_project());
    assert!(!assessment.is_mismatch());
}

#[test]
fn unknown_word_alone_is_not_mismatch() {
    // "Frobnicate" is not a project name (lowercase, not decorated) and its
    // absence is not a mismatch.
    let assessment = assess("Fix the frobnicate issue.", &codebro_project());
    assert!(!assessment.is_mismatch());
}

#[test]
fn unrelated_repository_evidence_cannot_create_mismatch() {
    // A task with no project-name candidates can never reach MISMATCH, no
    // matter how little it overlaps the repository.
    let assessment = assess("Fix the bug.", &generic_project());
    assert!(!assessment.is_mismatch());
    assert_ne!(assessment.status, ContextStatus::Mismatch);
}

#[test]
fn mismatch_requires_explicit_contradictory_identity() {
    // Absent entities alone (weak negative) must not produce MISMATCH even in
    // combination: the task references an absent module and an absent path but
    // names no foreign project.
    let assessment = assess(
        "Fix the missing module and src/missing.rs.",
        &codebro_project(),
    );
    assert!(!assessment.is_mismatch());
    assert!(!has_kind(
        ContextEvidenceKind::ExplicitProjectMismatch,
        &assessment.negative_evidence
    ));
}
