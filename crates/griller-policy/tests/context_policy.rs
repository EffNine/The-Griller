//! M4 -> M3 policy bridge tests: context status maps to the auditable table
//! verdicts, severities and required actions.

use griller_context::assess::ContextAssessor;
use griller_context::project::ProjectIdentity;
use griller_context::status::ContextStatus;
use griller_policy::context::{
    context_reason_code, context_required_action, context_verdict_for, ContextFinding,
};
use griller_policy::decision::RequiredAction;
use griller_policy::severity::Severity;
use griller_policy::verdict::Verdict;

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
        top_level_entries: vec!["src".into(), "crates".into()],
        important_paths: vec!["src/lib.rs".into()],
        all_paths: vec!["Cargo.toml".into(), "src/lib.rs".into()],
        symbols: vec!["grill".into(), "fact".into()],
        readme_title: Some("CodeBro".into()),
        vocabulary: vec!["claim".into(), "evidence".into(), "mcp".into()],
    }
}

fn finding(task: &str) -> ContextFinding {
    let assessment = ContextAssessor::deterministic()
        .assess(task, &codebro_project())
        .unwrap();
    ContextFinding::from_assessment(assessment)
}

/// The status -> (severity, verdict, reason_code, required_action) mapping is
/// fully deterministic and lives in the policy table.
#[test]
fn status_to_policy_mapping_is_auditable() {
    let cases: Vec<(&str, Severity, Verdict, &str, RequiredAction)> = vec![
        (
            "Fix CodeBro claim assess.",
            Severity::Info,
            Verdict::Pass,
            "MATCHED_CONTEXT_PASS",
            RequiredAction::None,
        ),
        (
            "Implement P10.5.1 Fact Store.",
            Severity::Low,
            Verdict::Pass,
            "LIKELY_MATCH_CONTEXT_PASS",
            RequiredAction::None,
        ),
        (
            "Implement Atlas P10.5.1 Fact Store.",
            Severity::High,
            Verdict::Block,
            "MISMATCH_CONTEXT_BLOCK",
            RequiredAction::ConfirmProject,
        ),
    ];
    for (task, severity, verdict, reason_code, action) in cases {
        let f = finding(task);
        assert_eq!(f.severity, severity, "task {task}");
        assert_eq!(f.verdict, verdict, "task {task}");
        assert_eq!(f.reason_code, reason_code, "task {task}");
        assert_eq!(f.required_action, action, "task {task}");
    }
}

#[test]
fn mismatch_blocks_and_never_passes() {
    let f = finding("Implement Atlas P10.5.1 Fact Store.");
    assert!(f.verdict.is_block());
    assert_eq!(f.required_action, RequiredAction::ConfirmProject);
    assert_eq!(
        f.required_action_message,
        "Stop before modifying the repository and confirm the intended project/repository."
    );
}

#[test]
fn generic_task_never_blocks() {
    let f = finding("Fix the bug in the login flow.");
    assert_ne!(f.verdict, Verdict::Block);
    assert_eq!(f.required_action, RequiredAction::None);
}

#[test]
fn ambiguous_conflict_warns_with_confirm_action() {
    // Meridian-style conflict: foreign project with weak presence -> WARN +
    // confirm, never BLOCK.
    let mut meridian = codebro_project();
    meridian.root_dir_name = "meridian".into();
    meridian.project_names = vec!["Meridian".into()];
    meridian.vocabulary.push("atlas".into());
    meridian.vocabulary.sort();
    let assessment = ContextAssessor::deterministic()
        .assess("Implement Atlas P10.5.1 Fact Store.", &meridian)
        .unwrap();
    assert_eq!(assessment.status, ContextStatus::Ambiguous);
    let f = ContextFinding::from_assessment(assessment);
    assert_eq!(f.verdict, Verdict::Warn);
    assert_eq!(f.severity, Severity::Medium);
    assert_eq!(f.required_action, RequiredAction::ConfirmProject);
    assert_eq!(f.reason_code, "AMBIGUOUS_CONTEXT_WARN");
}

#[test]
fn reason_codes_and_helpers_are_consistent() {
    for status in [
        ContextStatus::Matched,
        ContextStatus::LikelyMatch,
        ContextStatus::Ambiguous,
        ContextStatus::WeakRelevance,
        ContextStatus::InsufficientContext,
        ContextStatus::Mismatch,
    ] {
        let verdict = context_verdict_for(status);
        let code = context_reason_code(status);
        let action = context_required_action(status);
        assert!(
            code.ends_with("_CONTEXT_PASS")
                || code.ends_with("_CONTEXT_WARN")
                || code.ends_with("_CONTEXT_BLOCK")
        );
        assert_eq!(
            code,
            format!("{}_CONTEXT_{}", status_code(status), verdict_code(verdict))
        );
        if status == ContextStatus::Mismatch {
            assert_eq!(verdict, Verdict::Block);
            assert_eq!(action, RequiredAction::ConfirmProject);
        }
        if matches!(status, ContextStatus::Matched | ContextStatus::LikelyMatch) {
            assert_eq!(verdict, Verdict::Pass);
        }
    }
}

fn status_code(status: ContextStatus) -> &'static str {
    match status {
        ContextStatus::Matched => "MATCHED",
        ContextStatus::LikelyMatch => "LIKELY_MATCH",
        ContextStatus::Ambiguous => "AMBIGUOUS",
        ContextStatus::WeakRelevance => "WEAK_RELEVANCE",
        ContextStatus::InsufficientContext => "INSUFFICIENT_CONTEXT",
        ContextStatus::Mismatch => "MISMATCH",
    }
}

fn verdict_code(verdict: Verdict) -> &'static str {
    match verdict {
        Verdict::Pass => "PASS",
        Verdict::Warn => "WARN",
        Verdict::Block => "BLOCK",
    }
}
