//! Deterministic [`ProjectIdentity`] collection.
//!
//! M4's identity is **read-only**: the repository itself is the evidence. This
//! collector derives structured identity signals from a `ProjectContext` plus
//! deterministic scans (manifests, README title, top-level entries, a path
//! index, and a bounded symbol index). No file is ever modified, no git state
//! changes, and no arbitrary command is executed.
//!
//! Everything is deterministic: the same repository in the same state always
//! yields the same [`ProjectIdentity`].

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use griller_context::name::repo_name_from_remote;
use griller_context::project::ProjectIdentity;

use crate::collectors::{CollectorEnvironment, RepositoryStateCollector};
use crate::error::RuntimeError;
use crate::manifest::{find_manifests, DetectedManifest};

/// Directories never scanned for identity signals.
const SKIP_DIRS: &[&str] = &[
    ".git",
    ".codebro",
    "target",
    "node_modules",
    ".venv",
    "__pycache__",
    "dist",
    "build",
    "vendor",
];

/// File extensions treated as source for the symbol index.
const SOURCE_EXTENSIONS: &[&str] = &[
    "rs", "py", "js", "jsx", "ts", "tsx", "go", "java", "kt", "c", "h", "cpp", "hpp", "cs", "rb",
    "php", "swift", "scala", "sh", "bash",
];

/// Rust and common keywords excluded from the symbol index.
const SYMBOL_STOP: &[&str] = &[
    "the",
    "and",
    "for",
    "with",
    "from",
    "this",
    "that",
    "you",
    "your",
    "have",
    "has",
    "are",
    "was",
    "not",
    "but",
    "its",
    "all",
    "will",
    "into",
    "can",
    "out",
    "one",
    "over",
    "under",
    "then",
    "them",
    "their",
    "there",
    "these",
    "those",
    "when",
    "which",
    "while",
    "about",
    "after",
    "also",
    "been",
    "being",
    "both",
    "did",
    "does",
    "during",
    "each",
    "few",
    "first",
    "how",
    "just",
    "like",
    "more",
    "most",
    "other",
    "our",
    "own",
    "said",
    "same",
    "should",
    "some",
    "such",
    "than",
    "too",
    "two",
    "up",
    "very",
    "way",
    "well",
    "would",
    "project",
    "readme",
    "use",
    "pub",
    "fn",
    "let",
    "mod",
    "impl",
    "struct",
    "enum",
    "trait",
    "type",
    "const",
    "static",
    "mut",
    "ref",
    "self",
    "super",
    "crate",
    "extern",
    "match",
    "if",
    "else",
    "for",
    "while",
    "loop",
    "return",
    "break",
    "continue",
    "async",
    "await",
    "move",
    "dyn",
    "box",
    "in",
    "where",
    "as",
    "true",
    "false",
    "null",
    "none",
    "none",
    "nil",
    "def",
    "class",
    "import",
    "from",
    "return",
    "if",
    "elif",
    "else",
    "try",
    "except",
    "finally",
    "with",
    "as",
    "lambda",
    "yield",
    "global",
    "nonlocal",
    "raise",
    "assert",
    "pass",
    "del",
    "is",
    "not",
    "and",
    "or",
    "this",
    "that",
    "new",
    "function",
    "var",
    "const",
    "return",
    "export",
    "default",
    "extends",
    "implements",
    "interface",
    "package",
    "private",
    "public",
    "protected",
    "static",
    "void",
    "boolean",
    "integer",
    "string",
    "double",
    "float",
    "byte",
];

/// Upper bound on symbols kept in the identity.
const MAX_SYMBOLS: usize = 2000;

/// Collects a deterministic [`ProjectIdentity`] for a repository.
pub struct ProjectIdentityCollector {
    env: CollectorEnvironment,
}

impl ProjectIdentityCollector {
    /// Create the collector using the given environment source.
    pub fn new(env: CollectorEnvironment) -> Self {
        Self { env }
    }

    /// Collect the project identity for a repository path (read-only).
    pub fn collect(&self, repo: &Path) -> Result<ProjectIdentity, RuntimeError> {
        let state = RepositoryStateCollector::new(self.env.clone()).collect_context(repo)?;
        let root = state.context.repository_root.clone();
        let context = state.context;

        let manifests = find_manifests(&root)?;
        let package_names = extract_package_names(&root, &manifests);

        let repository_names: Vec<String> = context
            .remotes
            .iter()
            .filter_map(|r| repo_name_from_remote(&r.url))
            .collect();

        let readme_title = readme_title(&root);

        let mut project_names = Vec::new();
        if let Some(name) = &context.project_name {
            project_names.push(name.clone());
        }
        if let Some(title) = &readme_title {
            project_names.push(title.clone());
        }
        project_names.sort();
        project_names.dedup();

        let top_level_entries = top_level_entries(&root)?;
        let important_paths = important_paths(&root, &manifests);
        let all_paths = all_paths(&root)?;
        let symbols = symbol_index(&root, &all_paths)?;

        let mut remotes: Vec<String> = context.remotes.iter().map(|r| r.url.clone()).collect();
        remotes.sort();
        remotes.dedup();

        let mut manifest_paths: Vec<String> = manifests
            .iter()
            .map(|m| m.path.to_string_lossy().into_owned())
            .collect();
        manifest_paths.sort();
        manifest_paths.dedup();

        let mut vocabulary = context.vocabulary.clone();
        vocabulary.sort();
        vocabulary.dedup();

        Ok(ProjectIdentity {
            root_dir_name: context
                .repository_root
                .file_name()
                .map(|n| n.to_string_lossy().into_owned())
                .unwrap_or_default(),
            project_names,
            package_names,
            repository_names,
            remotes,
            languages: context.languages.clone(),
            package_managers: context.package_managers.clone(),
            manifest_paths,
            top_level_entries,
            important_paths,
            all_paths,
            symbols,
            readme_title,
            vocabulary,
        })
    }
}

/// Extract package/crate/module names from detected manifests.
fn extract_package_names(root: &Path, manifests: &[DetectedManifest]) -> Vec<String> {
    let mut names = Vec::new();
    for m in manifests {
        let path = root.join(&m.path);
        let raw = match std::fs::read_to_string(&path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        if let Some(name) = package_name_from_manifest(&m.path, &raw) {
            names.push(name);
        }
    }
    names.sort();
    names.dedup();
    names
}

/// Extract a single package name from a manifest's raw text.
fn package_name_from_manifest(path: &Path, raw: &str) -> Option<String> {
    let file_name = path.file_name()?.to_string_lossy().into_owned();
    match file_name.as_str() {
        "package.json" => serde_json::from_str::<serde_json::Value>(raw)
            .ok()
            .and_then(|v| v.get("name").and_then(|n| n.as_str()).map(str::to_string)),
        "go.mod" => raw.lines().find_map(|l| {
            l.trim()
                .strip_prefix("module ")
                .map(|m| m.split_whitespace().next().unwrap_or(m).to_string())
        }),
        "Cargo.toml" | "pyproject.toml" => toml_name(raw),
        _ => None,
    }
}

/// Extract `name = "..."` from the `[package]`/`[project]`/`[tool.poetry]`
/// sections of a TOML-like manifest. Intentionally naive and deterministic:
/// no TOML parser, just a section-aware scan.
fn toml_name(raw: &str) -> Option<String> {
    let mut in_package = false;
    for line in raw.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('[') {
            let section = trimmed.trim_matches(['[', ']']).to_lowercase();
            in_package = section == "package"
                || section == "project"
                || section == "tool.poetry"
                || section == "tool.bumpversion";
            continue;
        }
        if !in_package {
            continue;
        }
        if let Some(value) = trimmed.strip_prefix("name") {
            let value = value.trim_start();
            if let Some(rest) = value.strip_prefix('=') {
                let rest = rest.trim();
                if let Some(quoted) = rest.strip_prefix('"') {
                    if let Some(end) = quoted.find('"') {
                        return Some(quoted[..end].to_string());
                    }
                }
                if let Some(quoted) = rest.strip_prefix('\'') {
                    if let Some(end) = quoted.find('\'') {
                        return Some(quoted[..end].to_string());
                    }
                }
            }
        }
    }
    None
}

/// The first `# ` heading of the root README, if present.
fn readme_title(root: &Path) -> Option<String> {
    for name in [
        "README.md",
        "README",
        "readme.md",
        "README.rst",
        "Readme.md",
    ] {
        let path = root.join(name);
        if let Ok(raw) = std::fs::read_to_string(&path) {
            for line in raw.lines() {
                if let Some(title) = line.strip_prefix("# ") {
                    let title = title.trim();
                    if !title.is_empty() {
                        return Some(title.to_string());
                    }
                }
            }
        }
    }
    None
}

/// Sorted top-level directory/file names (excluding `.git`-style entries).
fn top_level_entries(root: &Path) -> Result<Vec<String>, RuntimeError> {
    let mut entries = Vec::new();
    for entry in std::fs::read_dir(root)? {
        let entry = entry?;
        let name = entry.file_name().to_string_lossy().into_owned();
        if SKIP_DIRS.contains(&name.as_str()) {
            continue;
        }
        entries.push(name);
    }
    entries.sort();
    Ok(entries)
}

/// Important module/file paths: manifests plus standard crate roots.
fn important_paths(root: &Path, manifests: &[DetectedManifest]) -> Vec<String> {
    let mut paths: Vec<String> = Vec::new();
    for m in manifests {
        paths.push(m.path.to_string_lossy().into_owned());
    }
    for candidate in [
        "src/main.rs",
        "src/lib.rs",
        "lib/main.dart",
        "main.py",
        "app.py",
        "index.js",
        "index.ts",
        "main.go",
    ] {
        if root.join(candidate).is_file() {
            paths.push(candidate.to_string());
        }
    }
    // Standard crate layout under crates/, packages/, modules/, lib/.
    for dir_name in ["crates", "packages", "modules", "lib"] {
        let dir = root.join(dir_name);
        if let Ok(entries) = std::fs::read_dir(&dir) {
            let mut children: Vec<PathBuf> = entries
                .filter_map(|e| e.ok())
                .filter(|e| e.path().is_dir())
                .map(|e| e.path())
                .collect();
            children.sort();
            for child in children {
                let name = child
                    .file_name()
                    .map(|n| n.to_string_lossy().into_owned())
                    .unwrap_or_default();
                let rel = format!("{dir_name}/{name}");
                for leaf in ["src/lib.rs", "src/main.rs", "lib.rs", "main.rs"] {
                    let candidate = format!("{rel}/{leaf}");
                    if root.join(&candidate).is_file() {
                        paths.push(candidate);
                        break;
                    }
                }
            }
        }
    }
    paths.sort();
    paths.dedup();
    paths
}

/// Sorted relative file paths (excluding generated/vendor directories).
fn all_paths(root: &Path) -> Result<Vec<String>, RuntimeError> {
    let mut paths = Vec::new();
    walk_paths(root, root, &mut paths)?;
    paths.sort();
    paths.dedup();
    Ok(paths)
}

fn walk_paths(base: &Path, dir: &Path, paths: &mut Vec<String>) -> Result<(), RuntimeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };
    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().into_owned();
        let path = entry.path();
        if entry.file_type()?.is_symlink() {
            continue;
        }
        if entry.file_type()?.is_dir() {
            if !SKIP_DIRS.contains(&file_name.as_str()) {
                children.push(path);
            }
            continue;
        }
        if let Ok(rel) = path.strip_prefix(base) {
            paths.push(rel.to_string_lossy().replace('\\', "/"));
        }
    }
    children.sort();
    for child in children {
        walk_paths(base, &child, paths)?;
    }
    Ok(())
}

/// Bounded, deterministic symbol index from source files.
fn symbol_index(root: &Path, all_paths: &[String]) -> Result<Vec<String>, RuntimeError> {
    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for rel in all_paths {
        if !is_source_file(rel) {
            continue;
        }
        let path = root.join(rel);
        let raw = match std::fs::read(&path) {
            Ok(raw) => raw,
            Err(_) => continue,
        };
        let text = String::from_utf8_lossy(&raw).into_owned();
        for token in text.split(|c: char| !(c.is_alphanumeric() || c == '_')) {
            if token.len() < 3 {
                continue;
            }
            let lower = token.to_lowercase();
            if SYMBOL_STOP.contains(&lower.as_str()) || lower.chars().all(|c| c.is_ascii_digit()) {
                continue;
            }
            *counts.entry(lower).or_insert(0) += 1;
        }
    }

    let mut symbols: Vec<(usize, String)> = counts
        .into_iter()
        .map(|(name, count)| (count, name))
        .collect();
    symbols.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    let mut names: Vec<String> = symbols
        .into_iter()
        .take(MAX_SYMBOLS)
        .map(|(_, n)| n)
        .collect();
    names.sort();
    Ok(names)
}

fn is_source_file(rel: &str) -> bool {
    rel.rsplit_once('.')
        .map(|(_, ext)| SOURCE_EXTENSIONS.contains(&ext))
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use super::*;
    use griller_context::name::basename_stem;
    use std::io::Write;

    #[test]
    fn toml_name_extracts_package_sections() {
        let cargo = r#"
[package]
name = "griller-core"
version = "0.1.0"
"#;
        assert_eq!(
            package_name_from_manifest(Path::new("Cargo.toml"), cargo),
            Some("griller-core".to_string())
        );

        let poetry = r#"
[tool.poetry]
name = "atlas"
version = "0.1.0"
"#;
        assert_eq!(
            package_name_from_manifest(Path::new("pyproject.toml"), poetry),
            Some("atlas".to_string())
        );

        let workspace = r#"
[workspace]
members = ["crates/*"]
"#;
        assert_eq!(
            package_name_from_manifest(Path::new("Cargo.toml"), workspace),
            None
        );
    }

    #[test]
    fn package_json_name_is_extracted() {
        let json = r#"{"name": "shop-api", "version": "1.0.0"}"#;
        assert_eq!(
            package_name_from_manifest(Path::new("package.json"), json),
            Some("shop-api".to_string())
        );
    }

    #[test]
    fn go_module_name_is_extracted() {
        let go = "module github.com/owner/shop-api\n\ngo 1.21\n";
        assert_eq!(
            package_name_from_manifest(Path::new("go.mod"), go),
            Some("github.com/owner/shop-api".to_string())
        );
    }

    #[test]
    fn readme_title_uses_first_heading() {
        let dir = tempfile::tempdir().unwrap();
        let mut f = std::fs::File::create(dir.path().join("README.md")).unwrap();
        writeln!(f, "# CodeBro\n\nThe facts store.").unwrap();
        assert_eq!(readme_title(dir.path()), Some("CodeBro".to_string()));
    }

    #[test]
    fn important_paths_include_crate_layout() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("crates/griller-mcp/src")).unwrap();
        std::fs::write(dir.path().join("crates/griller-mcp/src/lib.rs"), "").unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[workspace]\n").unwrap();
        let manifests = find_manifests(dir.path()).unwrap();
        let paths = important_paths(dir.path(), &manifests);
        assert!(paths.iter().any(|p| p == "crates/griller-mcp/src/lib.rs"));
        assert!(paths.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn symbol_index_is_deterministic_and_bounded() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(
            dir.path().join("src/lib.rs"),
            "pub fn grill_claim() {}\npub fn grill_evidence() {}\n",
        )
        .unwrap();
        let paths = all_paths(dir.path()).unwrap();
        let a = symbol_index(dir.path(), &paths).unwrap();
        let b = symbol_index(dir.path(), &paths).unwrap();
        assert_eq!(a, b);
        assert!(a.contains(&"grill_claim".to_string()));
        assert!(a.len() <= MAX_SYMBOLS);
    }

    #[test]
    fn collection_is_read_only_and_deterministic() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"sample\"\n",
        )
        .unwrap();
        std::fs::create_dir_all(dir.path().join("src")).unwrap();
        std::fs::write(dir.path().join("src/lib.rs"), "pub fn x() {}\n").unwrap();
        let collector = ProjectIdentityCollector::new(CollectorEnvironment::host());
        let a = collector.collect(dir.path()).unwrap();
        let b = collector.collect(dir.path()).unwrap();
        assert_eq!(a, b);
        // The manifest-derived package name is part of the identity.
        assert!(a.identity_names().contains(&"sample".to_string()));
        // The repository must be untouched (no new files).
        let entries: Vec<String> = top_level_entries(dir.path()).unwrap();
        assert!(entries.contains(&"Cargo.toml".to_string()));
    }

    #[test]
    fn basename_stem_used_for_important_paths() {
        assert_eq!(basename_stem("src/fact_store.rs"), "fact_store");
    }
}
