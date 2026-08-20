//! End-to-end tests for the M4 `griller context check` command.
//!
//! The binary is invoked via `CARGO_BIN_EXE_griller`. All assertions are on
//! structured JSON output and process exit codes.

use std::path::{Path, PathBuf};
use std::process::Command;

fn griller() -> Command {
    Command::new(env!("CARGO_BIN_EXE_griller"))
}

fn fixture(name: &str) -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
        .join("tests")
        .join("fixtures")
        .join(name)
}

fn run(args: &[&str]) -> (serde_json::Value, Option<i32>) {
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

fn run_ok(args: &[&str]) -> serde_json::Value {
    let (value, code) = run(args);
    assert!(
        code == Some(0),
        "griller {args:?} should exit 0, got {code:?}"
    );
    value
}

fn check(task: &str, repo: &Path) -> (serde_json::Value, Option<i32>) {
    let repo_arg = repo.to_str().unwrap();
    run(&["context", "check", "--task", task, "--repo", repo_arg])
}

// ---------------------------------------------------------------------------
// Q. CLI end-to-end
// ---------------------------------------------------------------------------

#[test]
fn wrong_project_task_outputs_machine_readable_block() {
    let repo = fixture("codebro");
    let (value, code) = check("Implement Atlas P10.5.1 Fact Store.", &repo);
    assert_eq!(code, Some(2), "wrong-project task must BLOCK");
    // Required machine-readable fields are all present.
    assert_eq!(value["context_status"], "mismatch");
    assert_eq!(value["operational_decision"], "block");
    assert_eq!(value["severity"], "high");
    assert_eq!(value["reason_code"], "MISMATCH_CONTEXT_BLOCK");
    assert_eq!(value["required_action"], "confirm_project");
    assert_eq!(value["exit_code"], 2);
    assert!(value["task_identity"]["original"] == "Implement Atlas P10.5.1 Fact Store.");
    assert!(value["project_identity"]["root_dir_name"] == "codebro");
    assert!(value["positive_evidence"].is_array());
    assert!(value["negative_evidence"].is_array());
    // The negative evidence contains an explicit mismatch receipt.
    let receipts: Vec<&str> = value["negative_evidence"]
        .as_array()
        .unwrap()
        .iter()
        .filter_map(|e| e["kind"].as_str())
        .collect();
    assert!(receipts.contains(&"explicit_project_mismatch"));
}

#[test]
fn correct_project_task_passes() {
    let repo = fixture("codebro");
    let (value, code) = check("Fix CodeBro MCP tool registration.", &repo);
    assert_eq!(code, Some(0));
    assert_eq!(value["context_status"], "matched");
    assert_eq!(value["operational_decision"], "pass");
    assert_eq!(value["severity"], "info");
    assert_eq!(value["reason_code"], "MATCHED_CONTEXT_PASS");
}

#[test]
fn positive_fixture_is_not_blocked() {
    let repo = fixture("atlas");
    let (value, code) = check("Implement P10.5.1 Fact Store.", &repo);
    assert_eq!(code, Some(0));
    assert_eq!(value["context_status"], "likely_match");
    assert_eq!(value["operational_decision"], "pass");
}

#[test]
fn generic_task_is_not_blocked() {
    let repo = fixture("codebro");
    let (value, code) = check("Fix the bug in the login flow.", &repo);
    assert_ne!(code, Some(2), "generic task must never BLOCK");
    assert_ne!(value["context_status"], "mismatch");
    assert_eq!(value["required_action"], "none");
}

#[test]
fn conflicting_identity_is_ambiguous_warn() {
    let repo = fixture("meridian");
    let (value, code) = check("Implement Atlas P10.5.1 Fact Store.", &repo);
    assert_eq!(code, Some(1));
    assert_eq!(value["context_status"], "ambiguous");
    assert_eq!(value["operational_decision"], "warn");
    assert_eq!(value["required_action"], "confirm_project");
}

#[test]
fn diva_text_goes_to_stderr_and_json_stays_on_stdout() {
    let repo = fixture("codebro");
    let repo_arg = repo.to_str().unwrap();
    let output = griller()
        .args([
            "context",
            "check",
            "--task",
            "Implement Atlas P10.5.1 Fact Store.",
            "--repo",
            repo_arg,
            "--diva",
        ])
        .output()
        .unwrap();
    // stdout is pure JSON.
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["context_status"], "mismatch");
    // stderr carries the persona text.
    let stderr = String::from_utf8_lossy(&output.stderr);
    assert!(
        stderr.contains("Wrong repo, darling"),
        "diva line should appear on stderr: {stderr}"
    );
    assert!(stderr.contains("Atlas"));
    assert!(stderr.contains("CodeBro"));
}

#[test]
fn pretty_output_is_valid_json() {
    let repo = fixture("codebro");
    let repo_arg = repo.to_str().unwrap();
    let output = griller()
        .args([
            "context",
            "check",
            "--task",
            "Implement Atlas P10.5.1 Fact Store.",
            "--repo",
            repo_arg,
            "--pretty",
        ])
        .output()
        .unwrap();
    let value: serde_json::Value = serde_json::from_slice(&output.stdout).unwrap();
    assert_eq!(value["context_status"], "mismatch");
    assert_eq!(output.status.code(), Some(2));
}

#[test]
fn git_remote_identity_matches_via_cli() {
    // A temp git repo whose only identity tie is the remote URL.
    let temp = tempfile::tempdir().unwrap();
    let repo = temp.path();
    git(repo, &["init", "-q", "-b", "main"]);
    git(repo, &["config", "user.email", "t@example.com"]);
    git(repo, &["config", "user.name", "Test"]);
    std::fs::write(repo.join("README.md"), "# Atlas Service\n").unwrap();
    git(repo, &["add", "-A"]);
    git(repo, &["commit", "-q", "-m", "init"]);
    git(
        repo,
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/atlas.git",
        ],
    );

    // The remote name matches the task project.
    let (value, code) = check("Implement Atlas P10.5.1 Fact Store.", repo);
    assert_eq!(code, Some(0), "remote-derived identity must match");
    assert!(matches!(
        value["context_status"].as_str(),
        Some("matched") | Some("likely_match")
    ));
}

#[test]
fn cli_context_describe_still_works() {
    let repo = fixture("atlas");
    let repo_arg = repo.to_str().unwrap();
    let value = run_ok(&["context", "--repo", repo_arg]);
    assert_eq!(value["project_name"], "atlas");
}

fn git(dir: &std::path::Path, args: &[&str]) {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {:?}: {}",
        dir,
        String::from_utf8_lossy(&output.stderr)
    );
}
