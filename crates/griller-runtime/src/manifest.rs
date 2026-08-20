//! Deterministic manifest detection.
//!
//! Scans a repository for known build/package manifests, skipping vendor and
//! generated directories. Results are sorted for determinism.

use std::path::{Path, PathBuf};

use griller_core::evidence::ManifestFormat;

use crate::error::RuntimeError;

/// A detected manifest file.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct DetectedManifest {
    /// Path relative to the scan root.
    pub path: PathBuf,
    /// Detected format.
    pub format: ManifestFormat,
    /// Detected language (lowercase), if known.
    pub language: Option<&'static str>,
    /// Detected package/build manager (lowercase), if known.
    pub manager: Option<&'static str>,
}

/// Filenames recognised as manifests: `(filename, format, language, manager)`.
const KNOWN_MANIFESTS: &[(&str, ManifestFormat, &str, &str)] = &[
    ("Cargo.toml", ManifestFormat::CargoToml, "rust", "cargo"),
    (
        "package.json",
        ManifestFormat::PackageJson,
        "javascript",
        "npm",
    ),
    ("go.mod", ManifestFormat::GoMod, "go", "go"),
    (
        "pyproject.toml",
        ManifestFormat::PyProjectToml,
        "python",
        "pypi",
    ),
    ("pom.xml", ManifestFormat::PomXml, "java", "maven"),
];

/// Directories never scanned for manifests.
const SKIP_DIRS: &[&str] = &[
    ".git",
    "target",
    "node_modules",
    ".venv",
    "__pycache__",
    "dist",
    "build",
    "vendor",
];

/// Recursively find all known manifests under `root`, sorted by path.
///
/// The scan only descends into directories and never follows symlinks, so
/// output is deterministic for a given tree.
pub fn find_manifests(root: &Path) -> Result<Vec<DetectedManifest>, RuntimeError> {
    let mut found = Vec::new();
    walk(root, root, &mut found)?;
    found.sort_by(|a, b| a.path.cmp(&b.path));
    Ok(found)
}

fn walk(base: &Path, dir: &Path, found: &mut Vec<DetectedManifest>) -> Result<(), RuntimeError> {
    let entries = match std::fs::read_dir(dir) {
        Ok(entries) => entries,
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(()),
        Err(err) => return Err(err.into()),
    };

    let mut children: Vec<PathBuf> = Vec::new();
    for entry in entries {
        let entry = entry?;
        let file_name = entry.file_name().to_string_lossy().to_string();
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

        if let Some((_, format, language, manager)) = KNOWN_MANIFESTS
            .iter()
            .find(|(name, _, _, _)| *name == file_name)
        {
            let rel = path.strip_prefix(base).unwrap_or(&path).to_path_buf();
            found.push(DetectedManifest {
                path: rel,
                format: *format,
                language: Some(language),
                manager: Some(manager),
            });
        }
    }

    children.sort();
    for child in children {
        walk(base, &child, found)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn detects_and_sorts_manifests() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("Cargo.toml"), "[package]\n").unwrap();
        std::fs::write(dir.path().join("package.json"), "{}").unwrap();
        std::fs::create_dir_all(dir.path().join("target")).unwrap();
        std::fs::write(dir.path().join("target/Cargo.toml"), "ignored").unwrap();

        let found = find_manifests(dir.path()).unwrap();
        assert_eq!(found.len(), 2);
        assert_eq!(found[0].path, PathBuf::from("Cargo.toml"));
        assert_eq!(found[1].path, PathBuf::from("package.json"));
        assert_eq!(found[0].language, Some("rust"));
        assert_eq!(found[0].manager, Some("cargo"));
    }
}
