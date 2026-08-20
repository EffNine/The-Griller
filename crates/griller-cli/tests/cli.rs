//! End-to-end tests for the `griller claim` CLI commands.
//!
//! The binary is invoked via `CARGO_BIN_EXE_griller`. All assertions are on
//! structured JSON output.

use std::path::PathBuf;
use std::process::Command;

fn griller() -> Command {
    Command::new(env!("CARGO_BIN_EXE_griller"))
}

fn run_json(args: &[&str]) -> serde_json::Value {
    let (value, code) = run_json_with_exit(args);
    assert!(
        code == Some(0),
        "griller {args:?} should exit 0, got {code:?}"
    );
    value
}

/// Run the binary and return (JSON output, exit code). Unlike `run_json`,
/// non-zero exits are allowed: WARN=1 and BLOCK=2 are valid decisions, and
/// only `run_json` callers assert success.
fn run_json_with_exit(args: &[&str]) -> (serde_json::Value, Option<i32>) {
    let output = griller().args(args).output().expect("run griller binary");
    let value = serde_json::from_slice(&output.stdout).unwrap_or_else(|err| {
        panic!(
            "griller {args:?} output is not JSON: {err}\nstdout: {}\nstderr: {}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr),
        )
    });
    (value, output.status.code())
}

fn fixture_basic_rust() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join("basic_rust")
}

#[test]
fn claim_normalize_produces_structured_json() {
    let value = run_json(&[
        "claim",
        "normalize",
        "--claim",
        "Authentication tests pass.",
    ]);
    assert_eq!(value["mode"], "deterministic");
    assert_eq!(value["proposition"]["predicate"], "pass");
    assert_eq!(value["proposition"]["subject"], "authentication");
    assert_eq!(value["target"]["scope"], "test");
    assert_eq!(value["original"], "Authentication tests pass.");
}

#[test]
fn claim_assess_with_no_evidence_is_unsupported() {
    let value = run_json(&["claim", "assess", "--claim", "Tests pass"]);
    assert_eq!(value["assessment"]["status"], "unsupported");
    assert_eq!(value["evidence_used"], 0);
}

#[test]
fn claim_assess_from_evidence_store() {
    use griller_core::command::{CommandSpec, ExecutionStatus};
    use griller_core::evidence::{CommandOutputContent, Evidence, EvidenceContent, EvidenceKind};
    use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance, Reproducibility};
    use griller_core::redaction::RedactionReport;
    use griller_runtime::store::{EvidenceStore, JsonStore};

    let dir = tempfile::tempdir().unwrap();
    let spec = CommandSpec::new("cargo").with_args(["test"]);
    let evidence = Evidence::new(
        EvidenceKind::CommandOutput,
        Provenance {
            source: EvidenceSource::Command,
            capture_method: CaptureMethod::ProcessExecution,
            command: Some(spec.clone()),
            working_directory: Some("/tmp/repo".into()),
            environment_fingerprint: None,
            tool: None,
            reproducibility: Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        },
        EvidenceContent::CommandOutput(CommandOutputContent {
            spec,
            stdout: "ok\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
            status: ExecutionStatus::Completed,
            enforcement: griller_core::command::Enforcement::default(),
            redaction: RedactionReport::default(),
            duration_ms: 0,
        }),
        1,
    );
    let mut store = JsonStore::new(dir.path()).unwrap();
    store.save(&evidence).unwrap();

    let store_arg = format!("{}", dir.path().display());
    let value = run_json(&[
        "claim",
        "assess",
        "--claim",
        "Tests pass",
        "--evidence-dir",
        &store_arg,
    ]);
    assert_eq!(value["assessment"]["status"], "verified");
    assert_eq!(value["evidence_used"], 1);
}

#[test]
fn claim_assess_auto_collect_runs_cargo_test() {
    // Runs `cargo test` inside an isolated worktree of the self-contained
    // basic_rust fixture. The fixture has no tests, so cargo test exits 0 and
    // the "tests pass" claim is VERIFIED by direct environment evidence.
    let repo = fixture_basic_rust();
    let repo_arg = repo.to_str().unwrap();
    let value = run_json(&[
        "claim",
        "assess",
        "--claim",
        "Tests pass",
        "--repo",
        repo_arg,
        "--auto-collect",
    ]);
    assert_eq!(value["evidence_used"], 1);
    assert_eq!(value["assessment"]["status"], "verified");
}

#[test]
fn assess_output_contains_no_operational_verdict() {
    let value = run_json(&["claim", "assess", "--claim", "Tests pass"]);
    assert!(value.get("verdict").is_none());
    assert_eq!(
        value["assessment"].get("status").map(|s| s.as_str()),
        Some(Some("unsupported"))
    );
}

/// The decide command outputs structured JSON with an aggregated verdict and a
/// machine-readable reason code.
#[test]
fn claim_decide_outputs_structured_decision() {
    let (value, code) = run_json_with_exit(&[
        "claim",
        "decide",
        "--claim",
        "Tests pass",
        "--severity",
        "high",
    ]);
    assert_eq!(code, Some(2));
    assert_eq!(value["decision"]["verdict"], "block");
    assert_eq!(value["exit_code"], 2);
    assert_eq!(
        value["decision"]["findings"][0]["reason_code"],
        "UNSUPPORTED_COMPLETION_BLOCK"
    );
    assert_eq!(
        value["decision"]["findings"][0]["required_action"],
        "provide_evidence"
    );
    assert_eq!(
        value["decision"]["findings"][0]["required_action_message"],
        "Run the relevant verification, attach the resulting evidence, and re-grill."
    );
}

/// A PASS decision exits 0, a WARN exits 1, a BLOCK exits 2, and a system
/// error exits 3 — BLOCK is never conflated with a system failure.
#[test]
fn decide_exit_codes() {
    // PASS: real verification evidence via auto-collect on the basic_rust
    // fixture (cargo test passes).
    let repo = fixture_basic_rust();
    let repo_arg = repo.to_str().unwrap();
    let pass = griller()
        .args([
            "claim",
            "decide",
            "--claim",
            "Tests pass",
            "--repo",
            repo_arg,
            "--auto-collect",
        ])
        .output()
        .unwrap();
    assert!(pass.status.success(), "PASS must exit 0");

    // WARN: no evidence at low severity.
    let warn = griller()
        .args([
            "claim",
            "decide",
            "--claim",
            "Tests pass",
            "--severity",
            "low",
        ])
        .output()
        .unwrap();
    assert_eq!(warn.status.code(), Some(1), "WARN must exit 1");

    // BLOCK: no evidence at high severity.
    let block = griller()
        .args([
            "claim",
            "decide",
            "--claim",
            "Tests pass",
            "--severity",
            "high",
        ])
        .output()
        .unwrap();
    assert_eq!(block.status.code(), Some(2), "BLOCK must exit 2");

    // System error: invalid severity.
    let error = griller()
        .args([
            "claim",
            "decide",
            "--claim",
            "Tests pass",
            "--severity",
            "bogus",
        ])
        .output()
        .unwrap();
    assert_eq!(error.status.code(), Some(3), "system error must exit 3");
}

/// Diva output goes to stderr; JSON stays on stdout and remains parseable.
#[test]
fn diva_output_is_decorative_and_separate() {
    let output = griller()
        .args([
            "claim",
            "decide",
            "--claim",
            "Tests pass",
            "--severity",
            "high",
            "--diva",
        ])
        .output()
        .unwrap();
    // stdout is pure JSON.
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["decision"]["verdict"], "block");
    // stderr carries the persona text.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Absolutely not"),
        "diva line should appear on stderr: {stderr}"
    );
}
