//! Deterministic vocabulary signals.
//!
//! Computes a small, deterministic set of vocabulary tokens from the README
//! and manifest files of a repository. Intentionally naive: no embeddings, no
//! stop-word model, just frequency counting with a fixed sort order so the
//! same repository always yields the same signals.
//!
//! ## Diagnostic only — NOT a project identity mechanism
//!
//! The current vocabulary signal is **diagnostic/debugging data only** and is
//! not a reliable project identity mechanism. It MUST NOT drive verdicts,
//! security decisions, execution permissions, or context-BLOCK decisions. The
//! proper wrong-project detector is a later-phase (M4) concern.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use crate::error::RuntimeError;

/// A small fixed list of English stop words. Not a model — just tokens too
/// common to be a useful project signal.
const STOP_WORDS: &[&str] = &[
    "the", "and", "for", "with", "from", "this", "that", "you", "your", "have", "has", "are",
    "was", "not", "but", "its", "all", "will", "into", "can", "out", "one", "over", "under",
    "then", "them", "their", "there", "these", "those", "when", "which", "while", "about", "after",
    "also", "been", "being", "both", "did", "does", "during", "each", "few", "first", "how",
    "just", "like", "more", "most", "other", "our", "own", "said", "same", "should", "some",
    "such", "than", "too", "two", "up", "very", "way", "well", "what", "why", "would", "project",
    "readme",
];

/// Number of vocabulary tokens to keep.
const TOP_N: usize = 20;

/// README filenames considered at the repository root.
const README_NAMES: &[&str] = &[
    "README.md",
    "README",
    "README.rst",
    "readme.md",
    "Readme.md",
];

/// Compute vocabulary signals for `root`.
///
/// Text sources: root-level READMEs plus the given manifest paths (relative to
/// `root`). Tokens are lowercased, split on non-alphanumeric boundaries,
/// filtered, counted, and sorted by (count desc, token asc).
pub fn compute(root: &Path, manifest_paths: &[PathBuf]) -> Result<Vec<String>, RuntimeError> {
    let mut text = String::new();

    for name in README_NAMES {
        let path = root.join(name);
        if path.is_file() {
            text.push_str(&std::fs::read_to_string(&path)?);
            text.push('\n');
        }
    }
    for rel in manifest_paths {
        let path = root.join(rel);
        if path.is_file() {
            text.push_str(&std::fs::read_to_string(&path)?);
            text.push('\n');
        }
    }

    let mut counts: BTreeMap<String, usize> = BTreeMap::new();
    for raw in text.split(|c: char| !c.is_alphanumeric()) {
        let word = raw.to_lowercase();
        if word.len() >= 3 && !STOP_WORDS.contains(&word.as_str()) {
            *counts.entry(word).or_insert(0) += 1;
        }
    }

    let mut words: Vec<(usize, String)> = counts.into_iter().map(|(w, c)| (c, w)).collect();
    words.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.cmp(&b.1)));
    Ok(words.into_iter().take(TOP_N).map(|(_, w)| w).collect())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn vocabulary_is_deterministic_and_sorted() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("README.md"),
            "The griller grills claims griller griller grilled evidence.",
        )
        .unwrap();
        let a = compute(dir.path(), &[]).unwrap();
        let b = compute(dir.path(), &[]).unwrap();
        assert_eq!(a, b);
        assert!(a.contains(&"griller".to_string()));
    }
}
