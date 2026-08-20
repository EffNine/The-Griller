//! Evidence persistence.
//!
//! A minimal JSON evidence store. One canonical JSON file per evidence record
//! (`<store>/<id>.json`), written atomically (temp file + rename). No database,
//! no vector store, no network. The [`EvidenceStore`] trait keeps the storage
//! backend replaceable.

use std::fs;
use std::path::{Path, PathBuf};

use griller_core::evidence::Evidence;
use griller_core::hash::canonical_string;

use crate::error::RuntimeError;

/// Persistence backend for evidence records.
pub trait EvidenceStore {
    /// Persist an evidence record, overwriting any record with the same id.
    fn save(&mut self, evidence: &Evidence) -> Result<(), RuntimeError>;

    /// Load all records in deterministic (id-sorted) order.
    fn load_all(&self) -> Result<Vec<Evidence>, RuntimeError>;

    /// Load a single record by id.
    fn get_by_id(&self, id: &str) -> Result<Option<Evidence>, RuntimeError>;

    /// Load all records matching a content hash.
    fn get_by_hash(&self, hash: &str) -> Result<Vec<Evidence>, RuntimeError>;
}

/// A JSON-backed [`EvidenceStore`] rooted at a directory.
#[derive(Debug, Clone)]
pub struct JsonStore {
    dir: PathBuf,
}

impl JsonStore {
    /// Create (if necessary) and open a store at `dir`.
    pub fn new(dir: impl Into<PathBuf>) -> Result<Self, RuntimeError> {
        let dir = dir.into();
        fs::create_dir_all(&dir)?;
        Ok(Self { dir })
    }

    /// Path of the store directory.
    pub fn dir(&self) -> &Path {
        &self.dir
    }

    fn record_path(&self, id: &str) -> PathBuf {
        self.dir.join(format!("{id}.json"))
    }
}

impl EvidenceStore for JsonStore {
    fn save(&mut self, evidence: &Evidence) -> Result<(), RuntimeError> {
        let json = canonical_string(evidence)?;
        let path = self.record_path(evidence.id.as_str());
        let tmp = self.dir.join(format!("{}.tmp", evidence.id.as_str()));
        fs::write(&tmp, json.as_bytes())?;
        fs::rename(&tmp, &path)?;
        Ok(())
    }

    fn load_all(&self) -> Result<Vec<Evidence>, RuntimeError> {
        let mut paths: Vec<PathBuf> = fs::read_dir(&self.dir)?
            .filter_map(|entry| entry.ok().map(|e| e.path()))
            .filter(|p| p.extension().map(|e| e == "json").unwrap_or(false))
            .collect();
        paths.sort();

        let mut all = Vec::new();
        for path in paths {
            all.push(load_record(&path)?);
        }
        Ok(all)
    }

    fn get_by_id(&self, id: &str) -> Result<Option<Evidence>, RuntimeError> {
        let path = self.record_path(id);
        if !path.exists() {
            return Ok(None);
        }
        Ok(Some(load_record(&path)?))
    }

    fn get_by_hash(&self, hash: &str) -> Result<Vec<Evidence>, RuntimeError> {
        let mut matches = Vec::new();
        for evidence in self.load_all()? {
            if evidence.content_hash == hash {
                matches.push(evidence);
            }
        }
        Ok(matches)
    }
}

fn load_record(path: &Path) -> Result<Evidence, RuntimeError> {
    let raw = fs::read_to_string(path)?;
    let evidence: Evidence = serde_json::from_str(&raw)
        .map_err(|err| RuntimeError::Store(format!("{}: {err}", path.display())))?;
    Ok(evidence)
}
