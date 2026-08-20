//! Deterministic name normalization.
//!
//! Names — project names, package/crate names, repository names, task
//! entities — are compared after normalization. The rules are deliberately
//! **conservative**: they collapse obvious casing/separator variants and
//! nothing more. There is no stemmer, no phonetic matching, and no fuzzy
//! similarity, so unrelated projects cannot collide.
//!
//! ## Documented rules
//!
//! `normalize_name` maps:
//!
//! ```text
//! codebro | CodeBro | code-bro | code_bro  -> codebro
//! the-griller | The Griller | the_griller  -> thegriller
//! ```
//!
//! 1. Unicode-normalize lower-casing.
//! 2. Strip every non-alphanumeric character.
//!
//! Two names compare equal if and only if their normalized forms are equal.
//! A leading "the" is **not** stripped: `thegriller` and `griller` stay
//! distinct rather than risk colliding unrelated projects.

use std::path::Path;

/// Normalize a project/package/repository/entity name for comparison.
pub fn normalize_name(value: &str) -> String {
    value
        .chars()
        .filter(|c| c.is_alphanumeric())
        .flat_map(|c| c.to_lowercase())
        .collect()
}

/// Normalize a relative path for existence comparison.
///
/// * separators are flattened to `/`
/// * a leading `./` is stripped
/// * trailing `/` (directory references) are stripped
/// * case is preserved (paths are case-sensitive)
pub fn normalize_path(value: &str) -> String {
    let value = value.replace('\\', "/");
    let value = value.trim_start_matches("./");
    let value = value.trim_matches('/');
    value.to_string()
}

/// The basename of a path (last component), without extension.
pub fn basename_stem(path: &str) -> String {
    let normalized = normalize_path(path);
    let name = normalized.rsplit('/').next().unwrap_or(&normalized);
    match name.rsplit_once('.') {
        Some((stem, _)) if !stem.is_empty() => stem.to_string(),
        _ => name.to_string(),
    }
}

/// Extract the repository name from a git remote URL.
///
/// Handles both `https://host/owner/repo.git` and scp-like
/// `git@host:owner/repo.git` forms. Returns the raw last path component with
/// a trailing `.git` removed. A URL without a path (`https://example.com`)
/// yields `None` — the hostname is not a repository name.
pub fn repo_name_from_remote(url: &str) -> Option<String> {
    let trimmed = url.trim_end_matches('/');
    let path_part = if trimmed.contains("://") {
        let after_scheme = trimmed.split_once("://").map(|x| x.1).unwrap_or("");
        // A bare host with no path has no repository name.
        let i = after_scheme.find('/')?;
        &after_scheme[i + 1..]
    } else {
        // scp-like syntax: everything after the last ':' is the path.
        trimmed
            .rsplit(':')
            .next()
            .unwrap_or(trimmed)
            .trim_start_matches('/')
    };
    let last = path_part.rsplit('/').next().unwrap_or("");
    if last.is_empty() {
        return None;
    }
    let name = last.strip_suffix(".git").unwrap_or(last);
    if name.is_empty() {
        None
    } else {
        Some(name.to_string())
    }
}

/// A relative path is available for safe construction of display strings.
///
/// This helper is used only to convert user-supplied path references for
/// evidence receipts; it never touches the filesystem.
pub fn display_path(path: &Path) -> String {
    path.to_string_lossy().into_owned()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalization_collapses_obvious_variants() {
        let expected = "codebro".to_string();
        for variant in ["codebro", "CodeBro", "code-bro", "code_bro", "CODEBRO"] {
            assert_eq!(normalize_name(variant), expected, "variant {variant}");
        }
        let griller = "thegriller".to_string();
        for variant in ["the-griller", "The Griller", "the_griller", "thegriller"] {
            assert_eq!(normalize_name(variant), griller, "variant {variant}");
        }
    }

    #[test]
    fn normalization_does_not_merge_unrelated_names() {
        assert_ne!(normalize_name("the-griller"), normalize_name("griller"));
        assert_ne!(normalize_name("griller-core"), normalize_name("griller"));
        assert_ne!(normalize_name("atlas"), normalize_name("atlas-fact-store"));
    }

    #[test]
    fn path_normalization_handles_syntax_variants() {
        assert_eq!(normalize_path("./src/main.rs"), "src/main.rs");
        assert_eq!(normalize_path("src/main.rs/"), "src/main.rs");
        assert_eq!(normalize_path(r"src\main.rs"), "src/main.rs");
        assert_eq!(
            normalize_path("crates/griller-core/"),
            "crates/griller-core"
        );
    }

    #[test]
    fn basename_stem_extracts_last_component() {
        assert_eq!(basename_stem("src/main.rs"), "main");
        assert_eq!(basename_stem("crates/griller-core"), "griller-core");
        assert_eq!(basename_stem("fact_store.rs"), "fact_store");
    }

    #[test]
    fn remote_name_extraction_handles_urls_and_scp() {
        assert_eq!(
            repo_name_from_remote("https://github.com/owner/codebro.git"),
            Some("codebro".to_string())
        );
        assert_eq!(
            repo_name_from_remote("git@github.com:owner/atlas.git"),
            Some("atlas".to_string())
        );
        assert_eq!(
            repo_name_from_remote("https://example.com/team/the-griller"),
            Some("the-griller".to_string())
        );
        assert_eq!(
            repo_name_from_remote("https://example.com/only-host"),
            Some("only-host".to_string())
        );
        assert_eq!(repo_name_from_remote("https://example.com"), None);
    }
}
