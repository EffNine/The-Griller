//! Project context value types.
//!
//! [`ProjectContext`] is a deterministic, serializable description of a
//! repository. It is produced by `griller_runtime` collectors and designed to
//! be suitable for later context-mismatch analysis. No embeddings, no vectors,
//! no LLM.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// A git remote.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct GitRemote {
    /// Remote name, e.g. `origin`.
    pub name: String,
    /// Remote URL as reported by git.
    pub url: String,
}

/// Working-tree cleanliness state.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum WorkingTreeState {
    /// No tracked or untracked changes (as reported by `git status --porcelain`).
    Clean,
    /// At least one change relative to HEAD.
    Dirty,
    /// The directory is not a git repository, so the state is unknown.
    Unknown,
}

/// A deterministic description of a repository and its project signals.
///
/// All fields are plain data; collection happens in `griller-runtime`. The
/// same repository in the same state must produce the same context.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ProjectContext {
    /// Absolute path to the repository root (the git work-tree root for git
    /// repositories, otherwise the canonicalized given path).
    pub repository_root: PathBuf,
    /// Repository identity: the primary (`origin`) remote URL when present.
    pub identity: Option<String>,
    /// All configured git remotes (empty for non-git directories).
    pub remotes: Vec<GitRemote>,
    /// Current `HEAD` commit hash when the repository is git with commits.
    pub head: Option<String>,
    /// Current branch name (`symbolic-ref --short HEAD`) when not detached.
    pub branch: Option<String>,
    /// Dirty/clean/unknown working-tree state.
    pub working_tree_state: WorkingTreeState,
    /// Detected project name (from `package.json` `name`, else directory name).
    pub project_name: Option<String>,
    /// Detected programming language(s), sorted, derived from manifests.
    pub languages: Vec<String>,
    /// Detected package/build managers, sorted.
    pub package_managers: Vec<String>,
    /// Manifest file paths, relative to the repository root, sorted.
    pub manifest_paths: Vec<PathBuf>,
    /// Whether a README file is present at the root.
    pub has_readme: bool,
    /// Deterministic vocabulary signals (top frequent tokens from README and
    /// manifests). Intentionally naive; no embeddings.
    pub vocabulary: Vec<String>,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn context_is_plain_serializable_data() {
        let ctx = ProjectContext {
            repository_root: "/tmp/repo".into(),
            identity: Some("https://example.com/owner/repo".into()),
            remotes: vec![GitRemote {
                name: "origin".into(),
                url: "https://example.com/owner/repo".into(),
            }],
            head: Some("deadbeef".into()),
            branch: Some("main".into()),
            working_tree_state: WorkingTreeState::Clean,
            project_name: Some("repo".into()),
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            has_readme: true,
            vocabulary: vec!["griller".into()],
        };
        let json = serde_json::to_string(&ctx).unwrap();
        assert!(json.contains("working_tree_state"));
    }
}
