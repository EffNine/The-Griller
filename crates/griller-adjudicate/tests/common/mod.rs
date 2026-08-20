//! Shared helpers for griller-adjudicate integration tests.
//!
//! These helpers build synthetic [`Evidence`] records directly from
//! `griller-core` value types. No filesystem, no processes, no LLM: the
//! adjudication pipeline is pure, so all tests are hermetic and deterministic.

#![allow(dead_code)]

use std::path::PathBuf;

use griller_core::command::{CommandSpec, ExecutionStatus};
use griller_core::evidence::{
    CommandOutputContent, Evidence, EvidenceContent, EvidenceKind, FileContentContent,
    GitDiffContent, RepositorySnapshotContent,
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

/// A successful test-command run (`cargo test`, exit 0).
pub fn test_ok() -> Evidence {
    command(
        "cargo",
        &["test"],
        "running 1 test\nok\n",
        "",
        Some(0),
        ExecutionStatus::Completed,
    )
}

/// A failing test-command run (`cargo test`, exit 1).
pub fn test_fail() -> Evidence {
    command(
        "cargo",
        &["test"],
        "",
        "test failed\n",
        Some(1),
        ExecutionStatus::Completed,
    )
}

/// A test command that could not be executed (missing dependency/tool).
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

/// A timed-out test command.
pub fn test_timeout() -> Evidence {
    command("cargo", &["test"], "", "", None, ExecutionStatus::TimedOut)
}

/// A successful build command (`cargo build`, exit 0).
pub fn build_ok() -> Evidence {
    command(
        "cargo",
        &["build"],
        "Finished\n",
        "",
        Some(0),
        ExecutionStatus::Completed,
    )
}

/// A failing build command.
pub fn build_fail() -> Evidence {
    command(
        "cargo",
        &["build"],
        "",
        "error[E0425]\n",
        Some(101),
        ExecutionStatus::Completed,
    )
}

/// A successful formatting check (`cargo fmt --check`, exit 0).
pub fn fmt_ok() -> Evidence {
    command(
        "cargo",
        &["fmt", "--check"],
        "",
        "",
        Some(0),
        ExecutionStatus::Completed,
    )
}

/// An unrelated successful command (e.g. a lint run).
pub fn lint_ok() -> Evidence {
    command(
        "cargo",
        &["clippy"],
        "",
        "",
        Some(0),
        ExecutionStatus::Completed,
    )
}

/// An agent-claim evidence record (never environment evidence).
pub fn agent_claim(text: &str) -> Evidence {
    Evidence::from_agent_claim(text, 1)
}

/// A git-diff evidence record.
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

/// A repository-snapshot evidence record.
pub fn snapshot(staged: Vec<String>, unstaged: Vec<String>, untracked: Vec<String>) -> Evidence {
    let dirty = !(staged.is_empty() && unstaged.is_empty() && untracked.is_empty());
    let snapshot = RepositorySnapshot {
        head: Some("deadbeef".into()),
        branch: Some("main".into()),
        staged_changes: staged,
        unstaged_changes: unstaged,
        untracked_files: untracked,
        dirty,
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
        EvidenceContent::RepositorySnapshot(RepositorySnapshotContent { snapshot }),
        1,
    )
}

/// A file-content evidence record.
pub fn file_content(path: &str, text: &str) -> Evidence {
    let provenance = Provenance {
        source: EvidenceSource::Filesystem,
        capture_method: CaptureMethod::FileRead,
        command: None,
        working_directory: Some(PathBuf::from("/tmp/repo")),
        environment_fingerprint: None,
        tool: None,
        reproducibility: Reproducibility::Deterministic,
        directly_observed: true,
        depends_on_agent_claim: false,
    };
    Evidence::new(
        EvidenceKind::FileContent,
        provenance,
        EvidenceContent::FileContent(FileContentContent {
            path: PathBuf::from(path),
            size: text.len() as u64,
            bytes: text.as_bytes().to_vec(),
            text: Some(text.to_string()),
        }),
        1,
    )
}

/// A git diff showing deletion of the given path.
pub fn deletion_diff(path: &str) -> Evidence {
    let unstaged = format!(
        "diff --git a/{path} b/{path}\ndeleted file mode 100644\nindex 1111111..0000000\n--- a/{path}\n+++ /dev/null\n@@ -1 +0,0 @@\n-old content\n"
    );
    let porcelain = format!(" D {path}");
    git_diff(&unstaged, "", &porcelain)
}
