//! Evidence store tests: save/load, query by id/hash, canonical roundtrips.

mod common;

use griller_core::evidence::{Evidence, EvidenceKind};
use griller_core::hash::canonical_string;
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};
use griller_runtime::store::{EvidenceStore, JsonStore};

fn sample_evidence(tag: &str) -> Evidence {
    let provenance = Provenance::environment(
        EvidenceSource::Repository,
        CaptureMethod::DirectoryScan,
        Some("/tmp/repo".into()),
    );
    let content = griller_core::evidence::EvidenceContent::RepositoryState(
        griller_core::evidence::RepositoryStateContent {
            context: griller_core::context::ProjectContext {
                repository_root: "/tmp/repo".into(),
                identity: None,
                remotes: vec![],
                head: None,
                branch: None,
                working_tree_state: griller_core::context::WorkingTreeState::Unknown,
                project_name: Some(tag.to_string()),
                languages: vec![],
                package_managers: vec![],
                manifest_paths: vec![],
                has_readme: false,
                vocabulary: vec![],
            },
        },
    );
    Evidence::new(
        EvidenceKind::RepositoryState,
        provenance,
        content,
        1_700_000_000,
    )
}

#[test]
fn save_and_load_roundtrip() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = JsonStore::new(dir.path()).unwrap();
    let evidence = sample_evidence("roundtrip");
    store.save(&evidence).unwrap();

    let loaded = store
        .get_by_id(evidence.id.as_str())
        .unwrap()
        .expect("must exist");
    assert_eq!(loaded, evidence);
    assert_eq!(loaded.content_hash, evidence.content_hash);
}

#[test]
fn canonical_serialization_is_stable_across_save_load() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = JsonStore::new(dir.path()).unwrap();
    let evidence = sample_evidence("canonical");
    let original = canonical_string(&evidence).unwrap();
    store.save(&evidence).unwrap();

    let loaded = store.get_by_id(evidence.id.as_str()).unwrap().unwrap();
    assert_eq!(canonical_string(&loaded).unwrap(), original);
}

#[test]
fn load_all_returns_sorted_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = JsonStore::new(dir.path()).unwrap();
    let a = sample_evidence("a");
    let b = sample_evidence("b");
    store.save(&a).unwrap();
    store.save(&b).unwrap();

    let all = store.load_all().unwrap();
    assert_eq!(all.len(), 2);
    let ids: Vec<String> = all.iter().map(|e| e.id.to_string()).collect();
    let mut sorted = ids.clone();
    sorted.sort();
    assert_eq!(ids, sorted);
}

#[test]
fn query_by_hash_finds_matching_records() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = JsonStore::new(dir.path()).unwrap();
    let evidence = sample_evidence("by-hash");
    store.save(&evidence).unwrap();

    let found = store.get_by_hash(&evidence.content_hash).unwrap();
    assert_eq!(found.len(), 1);
    assert_eq!(found[0], evidence);
}

#[test]
fn missing_id_returns_none() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStore::new(dir.path()).unwrap();
    let result = store.get_by_id("deadbeef".repeat(8).as_str()).unwrap();
    assert!(result.is_none());
}

#[test]
fn empty_store_loads_empty() {
    let dir = tempfile::tempdir().unwrap();
    let store = JsonStore::new(dir.path()).unwrap();
    assert!(store.load_all().unwrap().is_empty());
}

#[test]
fn save_overwrites_same_id() {
    let dir = tempfile::tempdir().unwrap();
    let mut store = JsonStore::new(dir.path()).unwrap();
    let evidence = sample_evidence("overwrite");
    store.save(&evidence).unwrap();
    store.save(&evidence).unwrap();
    assert_eq!(store.load_all().unwrap().len(), 1);
}
