//! Collector for repository state and project context.

use std::collections::BTreeSet;
use std::path::{Path, PathBuf};

use griller_core::context::{GitRemote, ProjectContext, WorkingTreeState};
use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind, RepositoryStateContent};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance, Reproducibility};

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};
use crate::git::{git_command, git_version};
use crate::manifest::{find_manifests, DetectedManifest};
use crate::vocabulary;
use crate::worktree::{RepoInfo, Worktree};

use super::Collector;

/// Environment source used to collect repository state.
///
/// Tests inject a controlled environment here; the CLI uses the host
/// environment.
#[derive(Debug, Clone)]
pub struct CollectorEnvironment {
    /// Environment source for git invocations.
    pub env: EnvSource,
}

impl CollectorEnvironment {
    /// Build a collector environment from the host environment.
    pub fn host() -> Self {
        Self {
            env: EnvSource::from_host(),
        }
    }
}

/// A collected project context together with the provenance facts needed to
/// build a `repository_state` evidence record.
#[derive(Debug)]
pub struct CollectedContext {
    /// The collected project context.
    pub context: ProjectContext,
    /// Repository discovery facts.
    pub repo: RepoInfo,
    /// Environment fingerprint of the primary git command, when available.
    pub environment_fingerprint: Option<String>,
    /// Git version string, when available.
    pub tool: Option<String>,
}

/// Collects a [`ProjectContext`] and emits a `repository_state` evidence
/// record.
pub struct RepositoryStateCollector {
    env: CollectorEnvironment,
}

impl RepositoryStateCollector {
    /// Create the collector using the given environment source.
    pub fn new(env: CollectorEnvironment) -> Self {
        Self { env }
    }

    /// Collect the project context for a repository path.
    pub fn collect_context(&self, repo: &Path) -> Result<CollectedContext, RuntimeError> {
        let info = Worktree::discover(repo)?;
        let root = info.root.clone();

        let mut remotes: Vec<GitRemote> = Vec::new();
        let mut branch: Option<String> = None;
        let mut working_tree_state = WorkingTreeState::Unknown;
        let mut environment_fingerprint: Option<String> = None;

        if info.is_git {
            if let Some(output) =
                ExecutionBackend::git_ok_output(&root, &["remote", "-v"], &self.env.env)?
            {
                let mut seen: BTreeSet<(String, String)> = BTreeSet::new();
                for line in output.lines() {
                    let mut parts = line.splitn(3, '\t');
                    let name = parts.next().unwrap_or_default().to_string();
                    let url = parts
                        .next()
                        .unwrap_or_default()
                        .split_whitespace()
                        .next()
                        .unwrap_or_default()
                        .to_string();
                    if !name.is_empty() && !url.is_empty() {
                        seen.insert((name, url));
                    }
                }
                remotes = seen
                    .into_iter()
                    .map(|(name, url)| GitRemote { name, url })
                    .collect();
            }

            branch = ExecutionBackend::git_ok_output(
                &root,
                &["symbolic-ref", "--short", "HEAD"],
                &self.env.env,
            )?;

            let status_spec = git_command(&root, &["status", "--porcelain"]);
            let status_result = ExecutionBackend::execute(&status_spec, &self.env.env)?;
            environment_fingerprint = Some(status_result.env_fingerprint.clone());
            let porcelain = status_result.stdout_lossy();
            working_tree_state = if porcelain.is_empty() {
                WorkingTreeState::Clean
            } else {
                WorkingTreeState::Dirty
            };
        }

        let manifests = find_manifests(&root)?;
        let manifest_paths: Vec<PathBuf> = manifests.iter().map(|m| m.path.clone()).collect();

        let mut languages: BTreeSet<String> = BTreeSet::new();
        let mut package_managers: BTreeSet<String> = BTreeSet::new();
        for m in &manifests {
            if let Some(lang) = m.language {
                languages.insert(lang.to_string());
            }
            if let Some(manager) = m.manager {
                package_managers.insert(manager.to_string());
            }
        }

        let project_name = detect_project_name(&root, &manifests);

        let has_readme = [
            "README.md",
            "README",
            "readme.md",
            "README.rst",
            "Readme.md",
        ]
        .iter()
        .any(|name| root.join(name).is_file());

        let vocabulary_signals = vocabulary::compute(&root, &manifest_paths)?;

        let identity = remotes
            .iter()
            .find(|r| r.name == "origin")
            .map(|r| r.url.clone());

        let tool = if info.is_git {
            git_version(&self.env.env)
        } else {
            None
        };

        Ok(CollectedContext {
            context: ProjectContext {
                repository_root: root,
                identity,
                remotes,
                head: info.head.clone(),
                branch,
                working_tree_state,
                project_name,
                languages: languages.into_iter().collect(),
                package_managers: package_managers.into_iter().collect(),
                manifest_paths,
                has_readme,
                vocabulary: vocabulary_signals,
            },
            repo: info,
            environment_fingerprint,
            tool,
        })
    }
}

/// Determine the project name: `package.json` `name` field if present, else
/// the repository directory name.
fn detect_project_name(root: &Path, manifests: &[DetectedManifest]) -> Option<String> {
    let pkg = manifests.iter().find(|m| {
        m.path
            .file_name()
            .map(|n| n == "package.json")
            .unwrap_or(false)
    });
    if let Some(pkg) = pkg {
        if let Ok(raw) = std::fs::read_to_string(root.join(&pkg.path)) {
            if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
                if let Some(name) = value.get("name").and_then(|n| n.as_str()) {
                    return Some(name.to_string());
                }
            }
        }
    }
    root.file_name().map(|n| n.to_string_lossy().to_string())
}

impl Collector for RepositoryStateCollector {
    fn collect(&self, repo: &Path) -> Result<Vec<Evidence>, RuntimeError> {
        let collected = self.collect_context(repo)?;

        let capture_method = if collected.repo.is_git {
            CaptureMethod::GitInvocation
        } else {
            CaptureMethod::DirectoryScan
        };

        let provenance = Provenance {
            source: EvidenceSource::Repository,
            capture_method,
            command: None,
            working_directory: Some(collected.context.repository_root.clone()),
            environment_fingerprint: collected.environment_fingerprint,
            tool: collected.tool,
            reproducibility: Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };

        let evidence = Evidence::new(
            EvidenceKind::RepositoryState,
            provenance,
            EvidenceContent::RepositoryState(RepositoryStateContent {
                context: collected.context,
            }),
            griller_core::evidence::now_secs(),
        );

        Ok(vec![evidence])
    }
}
