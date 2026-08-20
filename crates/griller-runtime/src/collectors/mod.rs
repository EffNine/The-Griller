//! Evidence collectors.
//!
//! Collectors turn a repository (or a command) into structured [`Evidence`].
//! They never produce prose reports as their primary output, and they treat
//! all repository content and process output as untrusted data.

mod command_execution;
mod file_content;
mod git_diff;
mod manifest_collector;
mod repository_state;
mod snapshot;

pub use command_execution::CommandExecutionCollector;
pub use file_content::FileContentCollector;
pub use git_diff::GitDiffCollector;
pub use manifest_collector::ManifestCollector;
pub use repository_state::{CollectorEnvironment, RepositoryStateCollector};
pub use snapshot::SnapshotCollector;

use std::path::Path;

use griller_core::evidence::Evidence;

use crate::error::RuntimeError;

/// A collector that produces structured evidence from a repository path.
pub trait Collector {
    /// Collect evidence for the repository at `repo`.
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError>;
}
