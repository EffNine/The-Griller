//! Read-only repository snapshot value type.
//!
//! A [`RepositorySnapshot`] records the *base* and *working* state of a
//! repository without mutating it:
//!
//! * **base state** — `HEAD` (committed repository)
//! * **working state** — staged changes, unstaged changes, untracked files
//!
//! It exists so a future completion verifier can distinguish "does `HEAD`
//! work?" from "does the agent's proposed change work?". No patch application
//! logic lives here.

use serde::{Deserialize, Serialize};

/// A read-only description of a repository's base + working state.
///
/// Collected by `griller_runtime::collectors::SnapshotCollector` from
/// `git status --porcelain`; never mutates the repository.
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct RepositorySnapshot {
    /// Current `HEAD` commit hash (base state), when the repository has commits.
    pub head: Option<String>,
    /// Current branch name when not detached.
    pub branch: Option<String>,
    /// Porcelain lines describing changes staged in the index.
    pub staged_changes: Vec<String>,
    /// Porcelain lines describing changes in the working tree, not staged.
    pub unstaged_changes: Vec<String>,
    /// Paths of untracked files (porcelain `??` entries).
    pub untracked_files: Vec<String>,
    /// True when the working state differs from `HEAD` in any way.
    pub dirty: bool,
}

impl RepositorySnapshot {
    /// True when there are no staged, unstaged, or untracked changes.
    pub fn is_clean(&self) -> bool {
        !self.dirty
    }
}
