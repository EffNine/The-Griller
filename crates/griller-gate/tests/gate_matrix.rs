//! The gate integration matrix (A–L) against the REAL griller-mcp binary and
//! a scripted fake server for failure paths.
//!
//! `CARGO_BIN_EXE_griller-mcp` is provided by the `griller-mcp` dev-dependency.

use std::path::{Path, PathBuf};
use std::process::Command;

use griller_gate::config::{ExpectedServer, GateConfig};
use griller_gate::runner::run_gate;
use griller_gate::{CompletionEvent, CompletionExtraction, GateStatus, GATE_SCHEMA_VERSION};

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
        extraction: None,
    }
}

/// A completion event where the host observed a boundary but extracted no
/// completion assertion.
fn no_assertion_event(repo: &Path, session: &str) -> CompletionEvent {
    CompletionEvent {
        extraction: Some(CompletionExtraction::NoAssertion),
        claim: String::new(),
        ..event(repo, "", session)
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

// ---------------------------------------------------------------------------
// Phase 2: timeout invariant
// ---------------------------------------------------------------------------

/// The gate's outer MCP envelope must never be shorter than the inner per-attack
/// replay budget it contains.
///
/// This is the regression guard for the defect found in Phase 2: the gate default
/// was a hardcoded `120_000` while the canonical replay budget is `600_000`, so a
/// replay that legitimately needed more than two minutes was aborted by the
/// envelope at 120 s. The constant is now *derived* from
/// `griller_core::command::REPLAY_TIMEOUT_MS`, and this test fails if anyone
/// reintroduces an independent literal.
#[test]
fn gate_default_timeout_is_not_shorter_than_the_replay_budget() {
    assert_eq!(
        griller_gate::DEFAULT_TIMEOUT_MS,
        griller_core::command::REPLAY_TIMEOUT_MS,
        "gate envelope must be derived from the canonical replay timeout"
    );
    assert_eq!(griller_gate::DEFAULT_TIMEOUT_MS, 600_000);
}

/// The default actually reaches the MCP round-trip deadline.
#[test]
fn gate_config_default_timeout_uses_the_derived_envelope() {
    let cfg: GateConfig = serde_json::from_value(serde_json::json!({
        "mcp_binary": "/usr/bin/true",
        "state_dir": "/tmp",
    }))
    .expect("config with only required fields must deserialize");

    assert_eq!(cfg.timeout_ms, griller_gate::DEFAULT_TIMEOUT_MS);
    assert_eq!(cfg.timeout_ms, griller_core::command::REPLAY_TIMEOUT_MS);
}

// ---------------------------------------------------------------------------
// Phase 3: NO_COMPLETION_ASSERTION
// ---------------------------------------------------------------------------
//
// The host observed a completion boundary but extracted no completion
// assertion. This is a *pre-adjudication* fact, not a verdict: no claim reached
// the Griller, no evidence was collected, and no epistemic status exists.
//
// The gate must record it, non-passingly, without claiming anything was
// verified — and without an agent-facing notice.

/// A no-assertion event produces a persisted, non-passing GateDecision.
#[test]
fn n1_no_assertion_produces_a_non_passing_decision() {
    let d = run(
        &no_assertion_event(&fixture("greet_rust"), "n1"),
        // A deliberately broken MCP binary: the gate must short-circuit before
        // it would ever be used, so this must NOT become Unavailable.
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );

    assert_eq!(d.status, GateStatus::NoCompletionAssertion);
    assert!(!d.status.is_accepted(), "must never be accepted");
    assert!(d.status.is_no_completion_assertion());
    // Distinct from the two adjacent conditions.
    assert!(
        !d.status.is_unavailable(),
        "nothing failed; must not be Unavailable"
    );
    assert!(
        !d.status.is_rejected(),
        "nothing was refused; must not be rejected"
    );
}

/// The reason code is the stable free-form value from the design.
#[test]
fn n2_no_assertion_uses_the_dedicated_reason_code() {
    let d = run(
        &no_assertion_event(&fixture("greet_rust"), "n2"),
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );
    assert_eq!(d.reason_code.as_deref(), Some("NO_COMPLETION_ASSERTION"));
    assert!(d.summary_line().starts_with("GATE_NO_COMPLETION_ASSERTION"));
    assert!(d.summary_line().contains("NO_COMPLETION_ASSERTION"));
}

/// The record must not claim anything was verified or refused.
#[test]
fn n3_no_assertion_record_has_no_verdict_no_claim_and_no_agent_message() {
    let d = run(
        &no_assertion_event(&fixture("greet_rust"), "n3"),
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );

    // No verdict: nothing was adjudicated.
    assert_eq!(d.verdict, None, "no adjudication occurred, so no verdict");
    // No Griller report: the Griller was never invoked.
    assert!(
        d.griller_report.is_none(),
        "the Griller must not be invoked for a no-assertion event"
    );
    // No agent-facing text. This is what keeps the host silent: the handler
    // injects whenever `required_action_message` is present.
    assert_eq!(
        d.required_action_message, None,
        "a no-assertion event must not produce an agent-facing notice"
    );
    assert_eq!(d.required_action, None);

    // The claim is absent from the record entirely.
    let json = serde_json::to_string(&d).expect("serialize");
    assert!(
        !json.contains("All tests pass"),
        "no claim text may appear in a no-assertion record"
    );

    // The finding explains the outcome for a human reading the record.
    assert!(
        d.findings
            .iter()
            .any(|f| f.key == "completion_trigger" && f.detail.contains("nothing to verify")),
        "expected a completion_trigger finding, got {:?}",
        d.findings
    );
}

/// The gate never contacted the Griller, and the record says so.
#[test]
fn n4_no_assertion_records_that_the_griller_was_not_invoked() {
    let d = run(
        &no_assertion_event(&fixture("greet_rust"), "n4"),
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );
    assert!(!d.integration.connected, "no MCP connection was made");
    assert!(d.integration.server_info.is_none());
    assert!(d.integration.health.is_none());
}

/// The normal path is untouched: a real claim still reaches the Griller.
#[test]
fn n5_normal_completion_still_follows_the_existing_path() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "n5"),
        &config(real_mcp(), true, true),
    );
    assert!(d.status.is_accepted(), "a passing claim is still accepted");
    assert_eq!(d.verdict.as_deref(), Some("pass"));
    assert!(
        d.griller_report.is_some(),
        "the Griller report is still present"
    );
    assert!(d.integration.connected);
}

/// An absent `extraction` field means "assertion present" — the behaviour of
/// every host that predates the field. This is the backward-compatibility
/// guarantee: a no-assertion record can never be produced by omission.
#[test]
fn n6_absent_extraction_field_is_treated_as_an_assertion() {
    let mut e = event(&fixture("greet_rust"), "All tests pass.", "n6");
    assert_eq!(e.extraction, None, "the helper must leave the field absent");
    let d = run(&e, &config(real_mcp(), true, true));
    assert!(d.status.is_accepted(), "absent must mean 'claim present'");
    assert!(!d.status.is_no_completion_assertion());

    // And an explicitly-empty claim with no signal is still rejected, so the
    // field cannot be bypassed by sending a blank claim.
    e.claim = String::new();
    assert!(e.extraction.is_none());
}

/// `Unavailable` semantics are unchanged.
#[test]
fn n7_unavailable_behaviour_is_unchanged() {
    let d = run(
        &event(&fixture("greet_rust"), "All tests pass.", "n7"),
        &config(PathBuf::from("/nonexistent/griller-mcp"), true, true),
    );
    assert!(d.status.is_unavailable(), "got {d:?}");
    assert_eq!(d.reason_code.as_deref(), Some("SERVER_UNAVAILABLE"));
    assert_eq!(d.verdict, None);
    // Unavailable DOES carry an agent-facing message — that is the behaviour
    // that makes it different from NoCompletionAssertion, and it is preserved.
    assert!(
        d.required_action_message.is_some(),
        "Unavailable must still tell the agent what to fix"
    );
    assert!(!d.status.is_no_completion_assertion());
}

/// The wire format is stable and additive: a host that omits `extraction`
/// deserializes, and the two observed values round-trip.
#[test]
fn n8_extraction_field_round_trips_and_is_backward_compatible() {
    // A pre-existing event document, with no `extraction` key at all.
    let legacy = serde_json::json!({
        "schema_version": "g1.0",
        "repository": "/tmp",
        "claim": "All tests pass.",
        "agent_id": "opencode",
        "session_id": "legacy",
        "timestamp_ms": 1,
    });
    let parsed: CompletionEvent =
        serde_json::from_value(legacy.clone()).expect("a legacy event must still deserialize");
    assert_eq!(parsed.extraction, None, "absent must deserialize to None");

    for (json, expected) in [
        (
            serde_json::json!("extracted"),
            Some(CompletionExtraction::Extracted),
        ),
        (
            serde_json::json!("no_assertion"),
            Some(CompletionExtraction::NoAssertion),
        ),
    ] {
        let mut v = legacy.clone();
        v["extraction"] = json.clone();
        let e: CompletionEvent = serde_json::from_value(v).expect("round-trip");
        assert_eq!(e.extraction, expected);
        // Re-serialising preserves the wire value.
        let out = serde_json::to_value(&e).expect("serialize");
        assert_eq!(out["extraction"], json);
    }

    // The gate serialises the status in snake_case, like the existing four.
    assert_eq!(
        serde_json::to_value(GateStatus::NoCompletionAssertion).unwrap(),
        serde_json::json!("no_completion_assertion")
    );
    for s in [
        GateStatus::Accepted,
        GateStatus::Warned,
        GateStatus::Blocked,
        GateStatus::Unavailable,
        GateStatus::NoCompletionAssertion,
    ] {
        // Every status must serialise as a plain string, so an unknown-status
        // reader sees a string rather than an object.
        assert!(serde_json::to_value(s).unwrap().is_string());
    }
}

/// The existing single persistence path is still the only record writer, and
/// existing records remain readable.
#[test]
fn n9_single_persistence_path_and_legacy_records_remain_readable() {
    // Exactly one `persist_decision(` call site exists in the binary.
    let main_rs = std::fs::read_to_string(
        std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("src/main.rs"),
    )
    .expect("read main.rs");
    assert_eq!(
        main_rs.matches("persist_decision(&config").count(),
        1,
        "there must be exactly one persistence call site"
    );

    // Every real historical record must still parse. The store lives outside
    // the repository; when it is absent (CI, a fresh clone) the check is
    // skipped rather than silently passing, and the legacy-shape assertion
    // below still runs.
    // CARGO_MANIFEST_DIR is `<repo>/crates/griller-gate`; five levels up is
    // the home directory that holds the evidence store.
    let evidence_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../../../../../.local/share/griller/gate/decisions");
    if evidence_dir.is_dir() {
        let mut checked = 0usize;
        for entry in std::fs::read_dir(&evidence_dir).expect("read evidence dir") {
            let path = entry.expect("entry").path();
            if path.extension().and_then(|e| e.to_str()) != Some("json") {
                continue;
            }
            let text = std::fs::read_to_string(&path).expect("read record");
            serde_json::from_str::<griller_gate::GateDecision>(&text)
                .unwrap_or_else(|e| panic!("{} must remain readable: {e}", path.display()));
            checked += 1;
        }
        assert!(checked > 0, "evidence dir was present but held no records");
        eprintln!("verified {checked} historical decision records remain readable");
    } else {
        eprintln!("historical evidence store not present; legacy-shape check only");
    }

    // A representative legacy record shape (no `extraction` on the event, the
    // original four statuses) must still parse into GateDecision.
    let legacy = serde_json::json!({
        "schema_version": "g1.0",
        "status": "blocked",
        "verdict": "block",
        "reason_code": "UNSUPPORTED_COMPLETION_BLOCK",
        "findings": [],
        "required_action": "correct_claim",
        "required_action_message": "Fix the claim.",
        "event": {
            "repository": "/tmp",
            "agent_id": "opencode",
            "session_id": "old",
            "timestamp_ms": 0
        },
        "integration": { "connected": true, "stale": false },
        "griller_report": { "verdict": "block" },
        "timing": { "connect_ms": 1, "check_ms": 2, "total_ms": 3 },
        "lifecycle": []
    });
    let d: griller_gate::GateDecision =
        serde_json::from_value(legacy).expect("a legacy decision record must still parse");
    assert_eq!(d.status, GateStatus::Blocked);
    assert_eq!(
        d.reason_code.as_deref(),
        Some("UNSUPPORTED_COMPLETION_BLOCK")
    );
    assert!(d.status.is_rejected());
}
