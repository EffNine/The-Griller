//! Git worktree isolation.
//!
//! The Griller must never mutate the user's working tree during verification.
//! [`Worktree`] provides a reusable, disposable copy of a repository's
//! committed state:
//!
//! * [`Worktree::discover`] resolves a path to a repository root and its
//!   current `HEAD`.
//! * [`Worktree::create`] makes a detached temporary worktree (or, for
//!   commit-less repositories, a temporary clone) and registers it in the
//!   source repository's git metadata.
//! * Cleanup is automatic on `Drop` and explicit via [`Worktree::remove`];
//!   the registered worktree is removed with `git worktree remove --force`
//!   before the temporary directory is deleted, so no stale registrations
//!   survive.
//!
//! Everything created here is scratch. Commands that verify a claim run inside
//! this worktree, never in the user's tree.

use std::path::{Path, PathBuf};

use griller_core::command::ExecutionStatus;

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};
use crate::git::git_command;

/// Facts about a discovered repository.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RepoInfo {
    /// Canonical repository root (git top-level, or the canonicalized path for
    /// non-git directories).
    pub root: PathBuf,
    /// Whether the path resolves to a git repository.
    pub is_git: bool,
    /// Current `HEAD` commit hash when the repository has commits.
    pub head: Option<String>,
}

/// The kind of isolation backing a [`Worktree`].
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum WorktreeKind {
    /// Backed by `git worktree add`.
    GitWorktree,
    /// Backed by `git clone` (used for commit-less repositories).
    Clone,
}

/// A disposable, isolated copy of a repository's committed state.
#[derive(Debug)]
pub struct Worktree {
    repo: RepoInfo,
    path: PathBuf,
    kind: WorktreeKind,
    temp: Option<tempfile::TempDir>,
}

impl Worktree {
    /// Discover repository facts for a path.
    ///
    /// Non-git directories are reported as such (`is_git = false`) rather than
    /// failing, so collectors can degrade gracefully.
    pub fn discover(path: &Path) -> Result<RepoInfo, RuntimeError> {
        if !path.exists() {
            return Err(RuntimeError::NotFound(path.display().to_string()));
        }
        let canonical = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
        let env = EnvSource::from_host();

        if !Self::git_ok(&canonical, &["rev-parse", "--is-inside-work-tree"], &env) {
            return Ok(RepoInfo {
                root: canonical,
                is_git: false,
                head: None,
            });
        }

        let root = match Self::git_ok_output(&canonical, &["rev-parse", "--show-toplevel"], &env) {
            Some(root) => PathBuf::from(root),
            None => canonical,
        };
        let head = Self::git_ok_output(&root, &["rev-parse", "HEAD"], &env);

        Ok(RepoInfo {
            root,
            is_git: true,
            head,
        })
    }

    /// Create an isolated worktree for the repository at `repo_path`.
    ///
    /// Errors if `repo_path` is not inside a git repository.
    pub fn create(repo_path: &Path) -> Result<Self, RuntimeError> {
        let info = Self::discover(repo_path)?;
        if !info.is_git {
            return Err(RuntimeError::Git(format!(
                "not a git repository: {}",
                repo_path.display()
            )));
        }
        let temp = tempfile::tempdir()?;
        let env = EnvSource::from_host();

        let (path, kind) = if info.head.is_some() {
            let wt_path = temp.path().join("worktree");
            let spec = git_command(
                &info.root,
                &["worktree", "add", "--detach", &path_arg(&wt_path)],
            );
            let result = ExecutionBackend::execute(&spec, &env)?;
            if result.status != ExecutionStatus::Completed || result.exit_code != Some(0) {
                return Err(RuntimeError::Git(format!(
                    "git worktree add failed: {}",
                    result.stderr_lossy().trim()
                )));
            }
            (wt_path, WorktreeKind::GitWorktree)
        } else {
            let dst_path = temp.path().join("clone");
            let spec = git_command(
                &info.root,
                &["clone", &path_arg(&info.root), &path_arg(&dst_path)],
            );
            let result = ExecutionBackend::execute(&spec, &env)?;
            if result.status != ExecutionStatus::Completed || result.exit_code != Some(0) {
                return Err(RuntimeError::Git(format!(
                    "git clone for isolation failed: {}",
                    result.stderr_lossy().trim()
                )));
            }
            (dst_path, WorktreeKind::Clone)
        };

        Ok(Self {
            repo: info,
            path,
            kind,
            temp: Some(temp),
        })
    }

    /// The repository facts this worktree was created from.
    pub fn repo(&self) -> &RepoInfo {
        &self.repo
    }

    /// The path of the isolated worktree.
    pub fn path(&self) -> &Path {
        &self.path
    }

    /// Explicitly remove the worktree and its registration.
    ///
    /// Consumes the worktree; cleanup is idempotent even if `Drop` runs again.
    pub fn remove(mut self) -> Result<(), RuntimeError> {
        self.cleanup()
    }

    fn cleanup(&mut self) -> Result<(), RuntimeError> {
        if self.kind == WorktreeKind::GitWorktree {
            let env = EnvSource::from_host();
            let spec = git_command(
                &self.repo.root,
                &["worktree", "remove", "--force", &path_arg(&self.path)],
            );
            // Best effort: the directory may already be gone.
            let _ = ExecutionBackend::execute(&spec, &env);
        }
        if let Some(temp) = self.temp.take() {
            temp.close()?;
        }
        Ok(())
    }

    fn git_ok(dir: &Path, args: &[&str], env: &EnvSource) -> bool {
        let spec = git_command(dir, args);
        matches!(
            ExecutionBackend::execute(&spec, env),
            Ok(result) if result.succeeded()
        )
    }

    fn git_ok_output(dir: &Path, args: &[&str], env: &EnvSource) -> Option<String> {
        let spec = git_command(dir, args);
        match ExecutionBackend::execute(&spec, env) {
            Ok(result) if result.succeeded() => Some(result.stdout_lossy().trim().to_string()),
            _ => None,
        }
    }
}

impl Drop for Worktree {
    fn drop(&mut self) {
        let _ = self.cleanup();
    }
}

fn path_arg(path: &Path) -> String {
    path.to_str().expect("temp path is valid UTF-8").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn discover_reports_non_git_dirs() {
        let dir = tempfile::tempdir().unwrap();
        let info = Worktree::discover(dir.path()).unwrap();
        assert!(!info.is_git);
    }
}
