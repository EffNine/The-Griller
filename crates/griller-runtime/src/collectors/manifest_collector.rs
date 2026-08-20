//! Collector for manifest files.

use std::path::Path;

use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind, ManifestContent};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};

use crate::error::RuntimeError;
use crate::manifest::find_manifests;
use crate::worktree::Worktree;

use super::Collector;

/// Collects one `manifest` evidence record per detected manifest file.
///
/// Works for git and non-git directories alike.
pub struct ManifestCollector;

impl Collector for ManifestCollector {
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError> {
        let info = Worktree::discover(repo)?;
        let root = info.root;

        let manifests = find_manifests(&root)?;
        let mut evidence = Vec::new();

        for manifest in manifests {
            let full = root.join(&manifest.path);
            let raw = std::fs::read_to_string(&full)?;

            let provenance = Provenance {
                source: EvidenceSource::Manifest,
                capture_method: CaptureMethod::FileRead,
                command: None,
                working_directory: Some(root.clone()),
                environment_fingerprint: None,
                tool: None,
                reproducibility: griller_core::provenance::Reproducibility::Deterministic,
                directly_observed: true,
                depends_on_agent_claim: false,
            };

            evidence.push(Evidence::new(
                EvidenceKind::Manifest,
                provenance,
                EvidenceContent::Manifest(ManifestContent {
                    manifest_path: manifest.path,
                    format: manifest.format,
                    raw,
                }),
                griller_core::evidence::now_secs(),
            ));
        }

        Ok(evidence)
    }
}
