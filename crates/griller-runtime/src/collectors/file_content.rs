//! Collector for a single file's contents.

use std::path::{Path, PathBuf};

use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind, FileContentContent};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};

use crate::error::RuntimeError;
use crate::worktree::Worktree;

use super::Collector;

/// Collects the contents of one file, addressed relative to the repository
/// root.
///
/// The resolved path is verified to stay inside the repository root, so a
/// relative path cannot escape via `..`.
pub struct FileContentCollector {
    /// Path relative to the repository root.
    pub relative_path: PathBuf,
}

impl Collector for FileContentCollector {
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError> {
        let info = Worktree::discover(repo)?;
        let root = info.root;

        let canonical_root = root.canonicalize().map_err(RuntimeError::Io)?;
        let full = root.join(&self.relative_path);
        let canonical_full = full.canonicalize().map_err(RuntimeError::Io)?;
        if !canonical_full.starts_with(&canonical_root) {
            return Err(RuntimeError::Execution(format!(
                "path escapes repository root: {}",
                self.relative_path.display()
            )));
        }

        let bytes = std::fs::read(&canonical_full)?;
        let size = bytes.len() as u64;
        let text = std::str::from_utf8(&bytes).ok().map(|s| s.to_string());

        let provenance = Provenance {
            source: EvidenceSource::Filesystem,
            capture_method: CaptureMethod::FileRead,
            command: None,
            working_directory: Some(root.clone()),
            environment_fingerprint: None,
            tool: None,
            reproducibility: griller_core::provenance::Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };

        let evidence = Evidence::new(
            EvidenceKind::FileContent,
            provenance,
            EvidenceContent::FileContent(FileContentContent {
                path: self.relative_path.clone(),
                size,
                bytes,
                text,
            }),
            griller_core::evidence::now_secs(),
        );

        Ok(vec![evidence])
    }
}
