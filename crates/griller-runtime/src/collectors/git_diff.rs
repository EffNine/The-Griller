//! Collector for git diffs.

use std::path::Path;

use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind, GitDiffContent};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};
use crate::git::git_command;
use crate::worktree::Worktree;

use super::Collector;

/// Collects `git diff` evidence (unstaged, staged, and porcelain status).
///
/// Requires a git repository; errors otherwise.
pub struct GitDiffCollector {
    env: EnvSource,
}

impl GitDiffCollector {
    /// Create the collector using the given environment source.
    pub fn new(env: EnvSource) -> Self {
        Self { env }
    }
}

impl Collector for GitDiffCollector {
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError> {
        let info = Worktree::discover(repo)?;
        if !info.is_git {
            return Err(RuntimeError::Git(format!(
                "not a git repository: {}",
                repo.display()
            )));
        }
        let root = info.root;

        let unstaged =
            ExecutionBackend::git_ok_output(&root, &["diff"], &self.env)?.unwrap_or_default();
        let staged = ExecutionBackend::git_ok_output(&root, &["diff", "--cached"], &self.env)?
            .unwrap_or_default();
        let porcelain =
            ExecutionBackend::git_ok_output(&root, &["status", "--porcelain"], &self.env)?
                .unwrap_or_default();

        let provenance = Provenance {
            source: EvidenceSource::Git,
            capture_method: CaptureMethod::GitInvocation,
            command: Some(git_command(&root, &["diff"])),
            working_directory: Some(root.clone()),
            environment_fingerprint: None,
            tool: None,
            reproducibility: griller_core::provenance::Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };

        let evidence = Evidence::new(
            EvidenceKind::GitDiff,
            provenance,
            EvidenceContent::GitDiff(GitDiffContent {
                path: root,
                unstaged,
                staged,
                porcelain,
            }),
            griller_core::evidence::now_secs(),
        );

        Ok(vec![evidence])
    }
}
