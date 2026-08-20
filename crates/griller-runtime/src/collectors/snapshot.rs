//! Collector for a read-only repository snapshot.
//!
//! Records the base state (`HEAD`) and working state (staged, unstaged,
//! untracked) of a repository without mutating it. Never stages, never writes,
//! never patches.

use std::path::Path;

use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind, RepositorySnapshotContent};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};
use griller_core::snapshot::RepositorySnapshot;

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};
use crate::worktree::Worktree;

use super::Collector;

/// Collects a [`RepositorySnapshot`] from `git status --porcelain`.
///
/// Requires a git repository; errors otherwise.
pub struct SnapshotCollector {
    env: EnvSource,
}

impl SnapshotCollector {
    /// Create the collector using the given environment source.
    pub fn new(env: EnvSource) -> Self {
        Self { env }
    }

    /// Collect the snapshot value for a repository path (read-only).
    pub fn collect_snapshot(&self, repo: &Path) -> Result<RepositorySnapshot, RuntimeError> {
        let info = Worktree::discover(repo)?;
        if !info.is_git {
            return Err(RuntimeError::Git(format!(
                "not a git repository: {}",
                repo.display()
            )));
        }
        let root = info.root;

        let head = info.head;
        let branch = ExecutionBackend::git_ok_output(
            &root,
            &["symbolic-ref", "--short", "HEAD"],
            &self.env,
        )?;

        let porcelain =
            ExecutionBackend::git_ok_output(&root, &["status", "--porcelain"], &self.env)?
                .unwrap_or_default();

        let mut staged_changes = Vec::new();
        let mut unstaged_changes = Vec::new();
        let mut untracked_files = Vec::new();

        for line in porcelain.lines() {
            if let Some(rest) = line.strip_prefix("?? ") {
                untracked_files.push(rest.to_string());
                continue;
            }
            if line.starts_with("!! ") {
                continue; // ignored files are not part of the working state
            }
            let mut chars = line.chars();
            let index = chars.next();
            let worktree = chars.next();
            if index.is_some_and(|c| c != ' ') {
                staged_changes.push(line.to_string());
            }
            if worktree.is_some_and(|c| c != ' ') {
                unstaged_changes.push(line.to_string());
            }
        }

        let dirty = !(staged_changes.is_empty()
            && unstaged_changes.is_empty()
            && untracked_files.is_empty());

        Ok(RepositorySnapshot {
            head,
            branch,
            staged_changes,
            unstaged_changes,
            untracked_files,
            dirty,
        })
    }
}

impl Collector for SnapshotCollector {
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError> {
        let snapshot = self.collect_snapshot(repo)?;
        let info = Worktree::discover(repo)?;

        let provenance = Provenance {
            source: EvidenceSource::Git,
            capture_method: CaptureMethod::GitInvocation,
            command: None,
            working_directory: Some(info.root),
            environment_fingerprint: None,
            tool: None,
            reproducibility: griller_core::provenance::Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };

        let evidence = Evidence::new(
            EvidenceKind::RepositorySnapshot,
            provenance,
            EvidenceContent::RepositorySnapshot(RepositorySnapshotContent { snapshot }),
            griller_core::evidence::now_secs(),
        );

        Ok(vec![evidence])
    }
}
