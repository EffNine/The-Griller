//! Core-level tests: evidence identity (content hash) and serialization
//! reproducibility.
//!
//! EVIDENCE IDENTITY describes WHAT was observed. PROVENANCE describes
//! WHERE/HOW/WHEN it was observed. Identity must not depend on timestamp,
//! working directory, or environment fingerprint.

use griller_core::command::CommandSpec;
use griller_core::command::Enforcement;
use griller_core::evidence::{CommandOutputContent, Evidence, EvidenceContent, EvidenceKind};
use griller_core::hash::canonical_string;
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};
use griller_core::redaction::RedactionReport;

fn command_output(stdout: &str) -> CommandOutputContent {
    CommandOutputContent {
        spec: CommandSpec::new("echo").with_args([stdout]),
        stdout: stdout.to_string(),
        stderr: String::new(),
        exit_code: Some(0),
        status: griller_core::command::ExecutionStatus::Completed,
        enforcement: Enforcement::default(),
        redaction: RedactionReport::default(),
        duration_ms: 1,
    }
}

/// Build evidence for the same observed content with a fully customised
/// provenance (environment fingerprint, working directory, timestamp).
fn evidence_with(
    stdout: &str,
    env_fingerprint: Option<&str>,
    working_dir: &str,
    captured_at: i64,
) -> Evidence {
    let mut provenance = Provenance::environment(
        EvidenceSource::Command,
        CaptureMethod::ProcessExecution,
        Some(working_dir.into()),
    );
    provenance.environment_fingerprint = env_fingerprint.map(String::from);
    Evidence::new(
        EvidenceKind::CommandOutput,
        provenance,
        EvidenceContent::CommandOutput(command_output(stdout)),
        captured_at,
    )
}

#[test]
fn same_evidence_different_timestamp_same_hash() {
    let a = evidence_with("hello", Some("fp-a"), "/a", 1_000);
    let b = evidence_with("hello", Some("fp-a"), "/a", 9_999_999);
    assert_eq!(a.content_hash, b.content_hash);
    assert_ne!(a.captured_at, b.captured_at);
}

#[test]
fn same_evidence_different_working_directory_same_hash() {
    let a = evidence_with("hello", Some("fp-a"), "/checkout/one", 1);
    let b = evidence_with("hello", Some("fp-a"), "/checkout/two", 1);
    assert_eq!(a.content_hash, b.content_hash);
    assert_ne!(
        a.provenance.working_directory,
        b.provenance.working_directory
    );
}

#[test]
fn same_evidence_different_environment_fingerprint_same_hash() {
    let a = evidence_with("hello", Some("fingerprint-aaa"), "/a", 1);
    let b = evidence_with("hello", Some("fingerprint-bbb"), "/b", 1);
    assert_eq!(a.content_hash, b.content_hash);
    // Fingerprints remain available as provenance.
    assert_eq!(
        a.provenance.environment_fingerprint.as_deref(),
        Some("fingerprint-aaa")
    );
    assert_eq!(
        b.provenance.environment_fingerprint.as_deref(),
        Some("fingerprint-bbb")
    );
}

#[test]
fn same_state_same_command_same_result_equivalent_identity() {
    // The full hardening-1 scenario in one test.
    let a = evidence_with("identical result", Some("env-X"), "/home/user/repo", 100);
    let b = evidence_with("identical result", Some("env-Y"), "/tmp/elsewhere", 200);
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.id, b.id);
}

#[test]
fn different_observed_output_different_hash() {
    let a = evidence_with("output one", Some("fp"), "/a", 1);
    let b = evidence_with("output two", Some("fp"), "/a", 1);
    assert_ne!(a.content_hash, b.content_hash);
}

#[test]
fn different_exit_code_different_hash() {
    let ok = command_output("out");
    let mut fail = command_output("out");
    fail.exit_code = Some(1);
    let a = Evidence::new(
        EvidenceKind::CommandOutput,
        Provenance::environment(
            EvidenceSource::Command,
            CaptureMethod::ProcessExecution,
            Some("/a".into()),
        ),
        EvidenceContent::CommandOutput(ok),
        1,
    );
    let b = Evidence::new(
        EvidenceKind::CommandOutput,
        Provenance::environment(
            EvidenceSource::Command,
            CaptureMethod::ProcessExecution,
            Some("/a".into()),
        ),
        EvidenceContent::CommandOutput(fail),
        1,
    );
    assert_ne!(a.content_hash, b.content_hash);
}

#[test]
fn hash_is_reproducible_across_construction() {
    let a = evidence_with("same output", Some("fp"), "/a", 1_000);
    let b = evidence_with("same output", Some("fp"), "/a", 1_000);
    assert_eq!(a.content_hash, b.content_hash);
    assert_eq!(a.id, b.id);
    assert_eq!(a, b);
}

#[test]
fn canonical_serialization_is_stable() {
    let evidence = evidence_with("serialize me", Some("fp"), "/a", 1);
    let json = serde_json::to_string(&evidence).unwrap();
    let reparsed: Evidence = serde_json::from_str(&json).unwrap();
    assert_eq!(
        canonical_string(&evidence).unwrap(),
        canonical_string(&reparsed).unwrap()
    );
    assert_eq!(evidence, reparsed);
}
