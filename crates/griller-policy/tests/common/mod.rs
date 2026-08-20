//! Shared helpers for griller-policy integration tests.
//!
//! These helpers adjudicate claims with the M2 `Adjudicator` (pure, no I/O)
//! and build synthetic evidence directly from `griller-core` types.

#![allow(dead_code)]

use std::path::PathBuf;

use griller_adjudicate::assess::{Adjudicator, ClaimAssessment};
use griller_adjudicate::claim::{ClaimInput, ClaimSource};
use griller_core::command::{CommandSpec, ExecutionStatus};
use griller_core::evidence::{
    CommandOutputContent, Evidence, EvidenceContent, EvidenceKind, GitDiffContent,
};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance, Reproducibility};
use griller_core::redaction::RedactionReport;
use griller_core::snapshot::RepositorySnapshot;

/// Build a command-output evidence record.
pub fn command(
    executable: &str,
    args: &[&str],
    stdout: &str,
    stderr: &str,
    exit_code: Option<i32>,
    status: ExecutionStatus,
) -> Evidence {
    let spec = CommandSpec::new(executable).with_args(args.iter().map(|s| s.to_string()));
    let provenance = Provenance {
        source: EvidenceSource::Command,
        capture_method: CaptureMethod::ProcessExecution,
        command: Some(spec.clone()),
        working_directory: Some(PathBuf::from("/tmp/repo")),
        environment_fingerprint: None,
        tool: None,
        reproducibility: Reproducibility::Deterministic,
        directly_observed: true,
        depends_on_agent_claim: false,
    };
    let content = CommandOutputContent {
        spec,
        stdout: stdout.to_string(),
        stderr: stderr.to_string(),
        exit_code,
        status,
        enforcement: griller_core::command::Enforcement::default(),
        redaction: RedactionReport::default(),
        duration_ms: 0,
    };
    Evidence::new(
        EvidenceKind::CommandOutput,
        provenance,
        EvidenceContent::CommandOutput(content),
        1,
    )
}

pub fn test_ok() -> Evidence {
    command(
        "cargo",
        &["test"],
        "ok\n",
        "",
        Some(0),
        ExecutionStatus::Completed,
    )
}

pub fn test_fail() -> Evidence {
    command(
        "cargo",
        &["test"],
        "",
        "failed\n",
        Some(1),
        ExecutionStatus::Completed,
    )
}

pub fn test_unavailable() -> Evidence {
    command(
        "cargo",
        &["test"],
        "",
        "cargo: not found",
        None,
        ExecutionStatus::FailedToSpawn("cargo".into()),
    )
}

pub fn agent_claim(text: &str) -> Evidence {
    Evidence::from_agent_claim(text, 1)
}

pub fn git_diff(unstaged: &str, staged: &str, porcelain: &str) -> Evidence {
    let provenance = Provenance {
        source: EvidenceSource::Git,
        capture_method: CaptureMethod::GitInvocation,
        command: None,
        working_directory: Some(PathBuf::from("/tmp/repo")),
        environment_fingerprint: None,
        tool: None,
        reproducibility: Reproducibility::Deterministic,
        directly_observed: true,
        depends_on_agent_claim: false,
    };
    Evidence::new(
        EvidenceKind::GitDiff,
        provenance,
        EvidenceContent::GitDiff(GitDiffContent {
            path: PathBuf::from("/tmp/repo"),
            unstaged: unstaged.to_string(),
            staged: staged.to_string(),
            porcelain: porcelain.to_string(),
        }),
        1,
    )
}

pub fn snapshot(staged: Vec<String>, unstaged: Vec<String>, untracked: Vec<String>) -> Evidence {
    let snapshot = RepositorySnapshot {
        head: Some("deadbeef".into()),
        branch: Some("main".into()),
        staged_changes: staged,
        unstaged_changes: unstaged,
        untracked_files: untracked,
        dirty: true,
    };
    let provenance = Provenance {
        source: EvidenceSource::Git,
        capture_method: CaptureMethod::GitInvocation,
        command: None,
        working_directory: Some(PathBuf::from("/tmp/repo")),
        environment_fingerprint: None,
        tool: None,
        reproducibility: Reproducibility::Deterministic,
        directly_observed: true,
        depends_on_agent_claim: false,
    };
    Evidence::new(
        EvidenceKind::RepositorySnapshot,
        provenance,
        EvidenceContent::RepositorySnapshot(griller_core::evidence::RepositorySnapshotContent {
            snapshot,
        }),
        1,
    )
}

/// Adjudicate a claim (source = agent) against evidence with the deterministic
/// M2 pipeline.
pub fn assess(text: &str, evidence: &[Evidence]) -> ClaimAssessment {
    Adjudicator::deterministic()
        .assess(&ClaimInput::new(text, ClaimSource::Agent), evidence, None)
        .unwrap()
}
