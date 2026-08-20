//! The deterministic [`ProjectIdentity`] model.
//!
//! M4 derives structured identity signals from the repository **itself** —
//! no embeddings, no vector search, and no LLM ever decides project
//! identity. Collection (the I/O) happens in `griller-runtime`; this module
//! defines the pure, serializable shape.
//!
//! Names are stored in their raw (human-readable) form so receipts and JSON
//! output stay legible. Comparison happens on the normalized forms (see
//! [`crate::name::normalize_name`]).

use serde::{Deserialize, Serialize};

use crate::name::{normalize_name, repo_name_from_remote};

/// Deterministic project identity signals derived from a repository.
///
/// Every collection is deduplicated and sorted so that the same repository in
/// the same state always produces the same identity (determinism invariant).
#[derive(Debug, Clone, PartialEq, Eq, Default, Serialize, Deserialize)]
pub struct ProjectIdentity {
    /// Repository root directory name (the strongest always-present signal).
    pub root_dir_name: String,
    /// Project names found in manifests / README / directory, raw form.
    pub project_names: Vec<String>,
    /// Package / crate / module names found in manifests, raw form.
    pub package_names: Vec<String>,
    /// Repository names extracted from git remotes, raw form.
    pub repository_names: Vec<String>,
    /// Raw git remote URLs (sorted, deduplicated).
    pub remotes: Vec<String>,
    /// Detected programming languages (lowercase, sorted).
    pub languages: Vec<String>,
    /// Detected package/build managers (lowercase, sorted).
    pub package_managers: Vec<String>,
    /// Manifest file paths relative to the repository root (sorted).
    pub manifest_paths: Vec<String>,
    /// Top-level directory/file names (sorted, `.git`-style entries excluded).
    pub top_level_entries: Vec<String>,
    /// Important module/file paths relative to the root (sorted).
    pub important_paths: Vec<String>,
    /// All relative file paths in the repository (sorted; scan excludes
    /// generated/vendor directories). Used for deterministic path existence.
    pub all_paths: Vec<String>,
    /// Normalized identifiers extracted from source files (sorted).
    pub symbols: Vec<String>,
    /// First `# ` heading of the root README, when present.
    pub readme_title: Option<String>,
    /// Deterministic vocabulary signals (README + manifests), sorted.
    pub vocabulary: Vec<String>,
}

impl ProjectIdentity {
    /// All identity names (project/package/repository names), normalized.
    ///
    /// This is the set an explicit task entity must match to count as a
    /// repository identity match.
    pub fn identity_names(&self) -> Vec<String> {
        let mut names = Vec::new();
        names.push(normalize_name(&self.root_dir_name));
        names.extend(self.project_names.iter().map(|n| normalize_name(n)));
        names.extend(self.package_names.iter().map(|n| normalize_name(n)));
        names.extend(self.repository_names.iter().map(|n| normalize_name(n)));
        names.sort();
        names.dedup();
        names
    }

    /// The primary display identity for receipts (e.g. mismatch receipts name
    /// the repository the task was "handed").
    pub fn primary_identity(&self) -> String {
        self.project_names
            .first()
            .cloned()
            .or_else(|| self.root_dir_name.clone().into())
            .unwrap_or_else(|| "repository".to_string())
    }

    /// All presence signals a task entity could match: package names,
    /// top-level entries, important-path stems, symbols, vocabulary,
    /// readme title, languages and managers. Normalized.
    pub fn presence_terms(&self) -> Vec<String> {
        let mut terms = Vec::new();
        for name in &self.project_names {
            terms.push(normalize_name(name));
        }
        for name in &self.package_names {
            terms.push(normalize_name(name));
        }
        for entry in &self.top_level_entries {
            terms.push(normalize_name(entry));
        }
        for path in &self.important_paths {
            terms.push(normalize_name(&crate::name::basename_stem(path)));
            terms.push(normalize_name(path));
        }
        terms.extend(self.symbols.iter().cloned());
        terms.extend(self.vocabulary.iter().cloned());
        if let Some(title) = &self.readme_title {
            terms.push(normalize_name(title));
        }
        terms.extend(self.languages.iter().map(|l| normalize_name(l)));
        terms.extend(self.package_managers.iter().map(|m| normalize_name(m)));
        terms.sort();
        terms.dedup();
        terms
    }

    /// The normalized set of technology terms (languages + package managers).
    pub fn technology_terms(&self) -> Vec<String> {
        let mut terms: Vec<String> = self
            .languages
            .iter()
            .chain(self.package_managers.iter())
            .map(|t| normalize_name(t))
            .collect();
        terms.sort();
        terms.dedup();
        terms
    }

    /// The normalized set of vocabulary terms.
    pub fn vocabulary_terms(&self) -> Vec<String> {
        let mut terms: Vec<String> = self.vocabulary.iter().map(|t| normalize_name(t)).collect();
        terms.sort();
        terms.dedup();
        terms
    }

    /// True when the given name (raw or normalized) matches any repository
    /// identity name. The argument is normalized for comparison, so
    /// `codebro`, `CodeBro` and `code-bro` all match.
    pub fn matches_identity(&self, name: &str) -> bool {
        let normalized = normalize_name(name);
        self.identity_names().iter().any(|n| n == &normalized)
    }

    /// True when the normalized path exists in the repository's path index.
    ///
    /// A directory reference matches when it is a prefix of a known file path.
    pub fn path_exists(&self, normalized_path: &str) -> Option<String> {
        if self.all_paths.iter().any(|p| p == normalized_path) {
            return Some(normalized_path.to_string());
        }
        let prefix = format!("{normalized_path}/");
        self.all_paths
            .iter()
            .find(|p| p.starts_with(&prefix))
            .cloned()
    }
}

/// Extract the repository name from a remote URL (re-exported for the runtime
/// collector and callers that build identities).
pub fn repository_name_from_remote(url: &str) -> Option<String> {
    repo_name_from_remote(url)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_identity() -> ProjectIdentity {
        ProjectIdentity {
            root_dir_name: "codebro".into(),
            project_names: vec!["CodeBro".into()],
            package_names: vec!["griller-core".into(), "griller-mcp".into()],
            repository_names: vec!["codebro".into()],
            remotes: vec!["https://github.com/owner/codebro.git".into()],
            languages: vec!["rust".into()],
            package_managers: vec!["cargo".into()],
            manifest_paths: vec!["Cargo.toml".into()],
            top_level_entries: vec!["crates".into(), "src".into()],
            important_paths: vec!["src/lib.rs".into(), "crates/griller-mcp/src/lib.rs".into()],
            all_paths: vec![
                "Cargo.toml".into(),
                "crates/griller-mcp/src/lib.rs".into(),
                "src/lib.rs".into(),
            ],
            symbols: vec!["grill".into(), "mcp".into()],
            readme_title: Some("CodeBro".into()),
            vocabulary: vec!["claim".into(), "evidence".into()],
        }
    }

    #[test]
    fn identity_names_are_normalized_and_deduped() {
        let names = sample_identity().identity_names();
        assert!(names.contains(&"codebro".to_string()));
        assert!(names.contains(&"grillercore".to_string()));
        assert_eq!(names.iter().filter(|n| *n == "codebro").count(), 1);
    }

    #[test]
    fn matches_identity_is_case_and_separator_insensitive() {
        let id = sample_identity();
        assert!(id.matches_identity("codebro"));
        assert!(id.matches_identity("CodeBro"));
        assert!(id.matches_identity("griller-core"));
        assert!(id.matches_identity("code-bro"));
        assert!(!id.matches_identity("atlas"));
    }

    #[test]
    fn path_exists_matches_files_and_directory_prefixes() {
        let id = sample_identity();
        assert_eq!(id.path_exists("src/lib.rs"), Some("src/lib.rs".into()));
        assert_eq!(
            id.path_exists("crates/griller-mcp"),
            Some("crates/griller-mcp/src/lib.rs".into())
        );
        assert_eq!(id.path_exists("src/does_not_exist.rs"), None);
    }

    #[test]
    fn primary_identity_prefers_project_name() {
        assert_eq!(sample_identity().primary_identity(), "CodeBro");
    }
}
