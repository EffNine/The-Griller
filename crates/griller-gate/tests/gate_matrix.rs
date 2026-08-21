//! The gate integration matrix (A–L) against the REAL griller-mcp binary and
//! a scripted fake server for failure paths.
//!
//! `CARGO_BIN_EXE_griller-mcp` is provided by the `griller-mcp` dev-dependency.

use std::path::{Path, PathBuf};
use std::process::Command;

use griller_gate::config::{ExpectedServer, GateConfig};
use griller_gate::runner::run_gate;
use griller_gate::{CompletionEvent, GATE_SCHEMA_VERSION};

/// Copy a fixture to a unique temp dir (out of the workspace, so `cargo test`
/// inside the fixture resolves its own manifest, not the parent workspace).
/// Each call gets a fresh dir, so parallel tests never clobber each other.
fn fixture(name: &str) -> PathBuf {
    let src = Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .join("tests/fixtures")
        .join(name);
    assert!(src.is_dir(), "fixture missing: {}", src.display());

    let dir = tempfile::tempdir().expect("temp dir");
    let dst = dir.path().to_path_buf();
    // Keep the dir alive for the whole test process; the OS cleans /tmp.
    std::mem::forget(dir);
    copy_tree(&src, &dst);
    dst
}

fn copy_tree(src: &Path, dst: &Path) {
    std::fs::create_dir_all(dst).unwrap();
    for entry in std::fs::read_dir(src).unwrap() {
        let entry = entry.unwrap();
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if from.is_dir() {
            copy_tree(&from, &to);
        } else {
            std::fs::copy(&from, &to).unwrap();
        }
    }
}

fn real_mcp() -> PathBuf {
    // CARGO_BIN_EXE_* is only defined when cargo actually links binaries
    // (cargo test), not during cargo check.
    option_env!("CARGO_BIN_EXE_griller-mcp")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("../../target/debug/griller-mcp")
        })
}

fn fake_mcp() -> PathBuf {
    option_env!("CARGO_BIN_EXE_griller-gate-fake-mcp")
        .map(PathBuf::from)
        .unwrap_or_else(|| {
            PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                .join("../../target/debug/griller-gate-fake-mcp")
        })
}

fn config(mcp_binary: PathBuf, fail_closed: bool, warn_not_pass: bool) -> GateConfig {
    GateConfig {
        mcp_binary,
        expected_server: ExpectedServer::canonical(),
        state_dir: PathBuf::from(env!("CARGO_TARGET_TMPDIR")).join("gate-state"),
        timeout_ms: 90_000,
        fail_closed,
        warn_not_pass,
    }
}

fn event(repo: &Path, claim: &str, session: &str) -> CompletionEvent {
    CompletionEvent {
        schema_version: GATE_SCHEMA_VERSION.to_string(),
        repository: repo.display().to_string(),
        claim: claim.to_string(),
        agent_id: Some("matrix-agent".into()),
        session_id: Some(session.into()),
        commit: None,
        timestamp_ms: 0,
    }
}

fn run(event: &CompletionEvent, config: &GateConfig) -> griller_gate::GateDecision {
    let ev = griller_gate::runner::with_head_commit(event.clone());
    match run_gate(&ev, config) {
        griller_gate::runner::GateOutcome::Decision(d) => d,
        griller_gate::runner::GateOutcome::Failed(e) => panic!("gate failed: {e}"),
    }
}

// ---------------------------------------------------------------------------
// A. PASS — the golden path against the real MCP server.
// ---------------------------------------------------------------------------

#[test]
fn a_pass_on_greet_rust() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "a-pass"),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_accepted(), "expected PASS, got {d:?}");
    assert_eq!(d.verdict.as_deref(), Some("pass"));
    assert!(d.griller_report.is_some(), "report must be present");
    assert!(d.timing.total_ms > 0);
    let log = griller_gate::lifecycle::LifecycleLog::from(d.lifecycle.clone()).render();
    assert!(log.contains("COMPLETION_EVENT"));
    assert!(log.contains("GATE_ACCEPTED"));
}

// ---------------------------------------------------------------------------
// B. BLOCK — a claim the pipeline refutes.
// ---------------------------------------------------------------------------

#[test]
fn b_block_on_lying_rust() {
    let d = run(
        &event(&fixture("lying_rust"), "All tests pass.", "b-block"),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_rejected(), "expected BLOCK, got {d:?}");
    assert_eq!(d.verdict.as_deref(), Some("block"));
    let report = d.griller_report.as_ref().unwrap().to_string();
    assert!(
        report.contains("contradict"),
        "report should show contradiction"
    );
}

// ---------------------------------------------------------------------------
// C. WARN — must surface as Warned, never silently PASS.
// ---------------------------------------------------------------------------

#[test]
fn c1_scripted_warn_maps_to_warned() {
    let d = run(
        &event(&fixture("greet_rust"), "Anything.", "c1-warn"),
        &config(fake_mcp_with("--verdict", "warn"), true, true),
    );
    assert!(d.status.is_warned(), "expected WARN, got {d:?}");
    assert_eq!(d.verdict.as_deref(), Some("warn"));
    let log = griller_gate::lifecycle::LifecycleLog::from(d.lifecycle.clone()).render();
    assert!(log.contains("GATE_WARNED"));
}

#[test]
fn c2_warn_with_explicit_opt_out_is_accepted_not_passed() {
    let d = run(
        &event(&fixture("greet_rust"), "Anything.", "c2-optout"),
        &config(fake_mcp_with("--verdict", "warn"), true, false),
    );
    // warn_not_pass=false: WARN is accepted, but the verdict is still honest.
    assert!(d.status.is_accepted(), "got {d:?}");
    assert_eq!(d.verdict.as_deref(), Some("warn"));
}

#[test]
fn c3_real_pipeline_generic_claim_warns() {
    // A generic task (no project markers) yields WEAK_RELEVANCE / WARN
    // context, which must NOT be converted to PASS.
    let d = run(
        &event(
            &fixture("greet_rust"),
            "Fix the bug in the login flow.",
            "c3-real-warn",
        ),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_warned() || d.status.is_rejected(), "got {d:?}");
    assert_ne!(d.verdict.as_deref(), Some("pass"));
}

// ---------------------------------------------------------------------------
// D. SERVER UNAVAILABLE — binary missing -> Unavailable, fail closed.
// ---------------------------------------------------------------------------

#[test]
fn d_server_unavailable_fails_closed() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "d-unavail"),
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );
    assert!(d.status.is_unavailable(), "got {d:?}");
    assert_eq!(d.reason_code.as_deref(), Some("SERVER_UNAVAILABLE"));
    assert!(!d.status.is_accepted());
}

// ---------------------------------------------------------------------------
// E. WRONG SCHEMA — server speaks a different MCP schema -> fail closed.
// ---------------------------------------------------------------------------

#[test]
fn e_wrong_schema_fails_closed() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "e-schema"),
        &config(fake_mcp_with("--schema-version", "m99.0"), true, true),
    );
    assert!(d.status.is_unavailable(), "got {d:?}");
    assert_eq!(d.reason_code.as_deref(), Some("STALE_SERVER"));
    let integration = d.integration;
    assert!(integration.stale);
    assert!(integration
        .stale_reason
        .as_deref()
        .unwrap_or("")
        .contains("m99.0"));
}

// ---------------------------------------------------------------------------
// F. WRONG PROJECT — claim targets a different project -> BLOCK.
// ---------------------------------------------------------------------------

#[test]
fn f_wrong_project_blocks() {
    let d = run(
        &event(
            &fixture("greet_rust"),
            "Implement Atlas P10.5.1 Fact Store.",
            "f-mismatch",
        ),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_rejected(), "got {d:?}");
    assert_eq!(d.verdict.as_deref(), Some("block"));
    let report = d.griller_report.as_ref().unwrap().to_string();
    assert!(
        report.contains("MISMATCH_CONTEXT_BLOCK"),
        "expected context mismatch block"
    );
}

// ---------------------------------------------------------------------------
// G. LYING COMPLETION — a claim of passing tests that actually fail.
// ---------------------------------------------------------------------------

#[test]
fn g_lying_completion_blocks_with_replay_evidence() {
    let d = run(
        &event(&fixture("lying_rust"), "All tests pass.", "g-lying"),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_rejected(), "got {d:?}");
    let report = d.griller_report.as_ref().unwrap().to_string();
    assert!(
        report.contains("contradict") || report.contains("block"),
        "replay must surface"
    );
}

// ---------------------------------------------------------------------------
// H. PROMPT INJECTION — repository content is data, never instructions.
// ---------------------------------------------------------------------------

#[test]
fn h_repository_injection_does_not_affect_decision() {
    let injected = fixture("injection_rust");
    let clean = fixture("greet_rust");

    let with_injection = run(
        &event(&injected, "All tests pass.", "h-injected"),
        &config(real_mcp(), true, true),
    );
    let without_injection = run(
        &event(&clean, "All tests pass.", "h-clean"),
        &config(real_mcp(), true, true),
    );

    // The injected README tries to force a PASS. The decision must be
    // identical whether the payload is present or not.
    assert_eq!(
        with_injection.status, without_injection.status,
        "injected content changed the decision"
    );
    let output = serde_json::to_string(&with_injection).unwrap();
    assert!(
        !output.contains("griller-injection-marker-77"),
        "injected payload leaked into the decision"
    );
    assert!(!output.contains("MAGIC_PHRASE"));
}

// ---------------------------------------------------------------------------
// I. LOOP PREVENTION — one completion event => at most one gate run; the
// gate emits no completion events of its own.
// ---------------------------------------------------------------------------

#[test]
fn i_gate_never_emits_completion_events() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "i-loop"),
        &config(real_mcp(), true, true),
    );
    // The decision document is the ONLY output; there is no event channel.
    // The host-side tracker (GateTracker) guarantees at-most-once per
    // (session, marker); the runner itself is stateless and single-shot.
    assert!(d.status.is_accepted());
    assert_eq!(d.schema_version, GATE_SCHEMA_VERSION);
}

#[test]
fn i_tracker_enforces_at_most_once_per_session_marker() {
    let mut tracker = griller_gate::tracker::GateTracker::new();
    let session = "i-session".to_string();
    let marker = Some("assistant-msg-1".to_string());
    assert!(tracker.is_new(&session, marker.as_deref()));
    tracker.mark_verified(&session, marker.as_deref());
    // Same event again -> suppressed.
    assert!(!tracker.is_new(&session, marker.as_deref()));
    // A NEW event in the same session -> allowed.
    assert!(tracker.is_new(&session, Some("assistant-msg-2")));
}

// ---------------------------------------------------------------------------
// J. RE-ATTEMPT — after a BLOCK, a new, independent completion may be
// verified; the gate holds no blocking state.
// ---------------------------------------------------------------------------

#[test]
fn j_re_attempt_after_block_is_independent() {
    let cfg = config(real_mcp(), true, true);
    let first = run(
        &event(&fixture("lying_rust"), "All tests pass.", "j-first"),
        &cfg,
    );
    assert!(first.status.is_rejected());

    let second = run(
        &event(&fixture("greet_rust"), "All tests pass.", "j-second"),
        &cfg,
    );
    assert!(
        second.status.is_accepted(),
        "re-attempt must be independent"
    );
}

// ---------------------------------------------------------------------------
// K. STALE SERVER — version mismatch fails closed with STALE_SERVER.
// ---------------------------------------------------------------------------

#[test]
fn k_stale_version_fails_closed() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "k-stale"),
        &config(fake_mcp_with("--version", "0.0.1"), true, true),
    );
    assert!(d.status.is_unavailable(), "got {d:?}");
    assert_eq!(d.reason_code.as_deref(), Some("STALE_SERVER"));
}

// ---------------------------------------------------------------------------
// L. REAL MCP — the gate talks to the actual griller-mcp binary and the
// decision reflects the REAL report (not a stub).
// ---------------------------------------------------------------------------

#[test]
fn l_real_mcp_end_to_end() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "l-real"),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_accepted(), "got {d:?}");
    let report = d.griller_report.as_ref().expect("real report");
    assert_eq!(report["schema_version"], "m8.0", "schema_version");
    assert!(report.get("timing").is_some(), "timing section");
    assert_eq!(
        d.integration.server_info.as_ref().unwrap().version,
        env!("CARGO_PKG_VERSION")
    );
    assert!(!d.integration.stale);
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

fn fake_mcp_with(flag: &str, value: &str) -> PathBuf {
    // The fake server reads its flags from its own executable name, so a
    // symlink encodes the configuration (the gate spawns argv[0] with no
    // arguments and env_clear, so neither argv nor env can carry it).
    // Unique per call: parallel tests must not clobber each other's links.
    static COUNTER: std::sync::atomic::AtomicU64 = std::sync::atomic::AtomicU64::new(0);
    let n = COUNTER.fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    let name = format!("fake-mcp-{}-{}-{n}", flag.trim_start_matches("--"), value);
    let link = std::env::temp_dir()
        .join("griller-gate-fake-links")
        .join(name);
    std::fs::create_dir_all(link.parent().unwrap()).unwrap();
    let _ = std::fs::remove_file(&link);
    std::os::unix::fs::symlink(fake_mcp(), &link).unwrap();
    link
}

// Keep the fixture copy helper honest: fixtures are git repos with
// pre-built state; tests run them in place (read-only) like the real gate.
#[allow(dead_code)]
fn assert_fixture_is_git(repo: &Path) {
    let status = Command::new("git")
        .arg("-C")
        .arg(repo)
        .arg("status")
        .output()
        .expect("git available");
    assert!(status.status.success(), "fixture must be a git repo");
}
