//! M4 identity collector integration tests.
//!
//! These exercise the real `ProjectIdentityCollector` against temp git
//! repositories: remote-derived identity, determinism, read-only guarantees,
//! and the full collector -> assessor -> policy pipeline.

mod common;

use common::{commit_all, git, init_repo};
use griller_context::assess::ContextAssessor;
use griller_context::status::ContextStatus;
use griller_policy::context::ContextFinding;
use griller_runtime::collectors::CollectorEnvironment;
use griller_runtime::identity::ProjectIdentityCollector;

fn collector() -> ProjectIdentityCollector {
    ProjectIdentityCollector::new(CollectorEnvironment::host())
}

/// Build a CodeBro-like git repository with a matching remote.
fn make_codebro_repo() -> tempfile::TempDir {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(
        dir.path().join("Cargo.toml"),
        "[package]\nname = \"codebro\"\nversion = \"0.1.0\"\n",
    )
    .unwrap();
    std::fs::write(
        dir.path().join("README.md"),
        "# CodeBro\n\nCodeBro records verified engineering facts and registers MCP tools.\n",
    )
    .unwrap();
    std::fs::create_dir_all(dir.path().join("src")).unwrap();
    std::fs::write(
        dir.path().join("src/lib.rs"),
        "pub fn grill() {}\npub fn register_tool() {}\n",
    )
    .unwrap();
    commit_all(dir.path(), "init");
    git(
        dir.path(),
        &[
            "remote",
            "add",
            "origin",
            "https://github.com/owner/codebro.git",
        ],
    );
    dir
}

#[test]
fn collector_derives_identity_from_manifest_remote_and_readme() {
    let repo = make_codebro_repo();
    let identity = collector().collect(repo.path()).unwrap();

    assert!(identity.identity_names().contains(&"codebro".to_string()));
    assert!(identity.repository_names.contains(&"codebro".to_string()));
    assert_eq!(identity.readme_title.as_deref(), Some("CodeBro"));
    assert!(identity.manifest_paths.contains(&"Cargo.toml".to_string()));
    assert!(identity.languages.contains(&"rust".to_string()));
    assert!(identity.package_managers.contains(&"cargo".to_string()));
}

#[test]
fn collector_identity_is_deterministic() {
    let repo = make_codebro_repo();
    let a = collector().collect(repo.path()).unwrap();
    let b = collector().collect(repo.path()).unwrap();
    assert_eq!(a, b);
}

#[test]
fn collector_is_read_only() {
    let repo = make_codebro_repo();
    let before = git(repo.path(), &["status", "--porcelain"]);
    let head_before = git(repo.path(), &["rev-parse", "HEAD"]);
    let _identity = collector().collect(repo.path()).unwrap();
    let after = git(repo.path(), &["status", "--porcelain"]);
    let head_after = git(repo.path(), &["rev-parse", "HEAD"]);
    assert_eq!(before, after, "git status must be unchanged");
    assert_eq!(head_before, head_after, "HEAD must be unchanged");
}

#[test]
fn full_pipeline_remote_match_passes_and_atlas_mismatch_blocks() {
    let repo = make_codebro_repo();
    let identity = collector().collect(repo.path()).unwrap();
    let assessor = ContextAssessor::deterministic();

    // F: the task references the repo; identity matches (manifest/remote/
    // project name — any strong identity signal satisfies the rule).
    let assessment = assessor
        .assess("Fix codebro remote sync.", &identity)
        .unwrap();
    assert_eq!(assessment.status, ContextStatus::Matched);
    assert!(assessment.positive_evidence.iter().any(|e| {
        matches!(
            e.kind,
            griller_context::evidence::ContextEvidenceKind::ManifestMatch
                | griller_context::evidence::ContextEvidenceKind::RemoteMatch
                | griller_context::evidence::ContextEvidenceKind::ExplicitProjectMatch
        )
    }));
    // The remote URL itself is part of the collected identity.
    assert!(identity.remotes.iter().any(|u| u.contains("codebro.git")));

    // P: the wrong-project task blocks end-to-end.
    let assessment = assessor
        .assess("Implement Atlas P10.5.1 Fact Store.", &identity)
        .unwrap();
    assert_eq!(assessment.status, ContextStatus::Mismatch);
    let finding = ContextFinding::from_assessment(assessment);
    assert!(finding.verdict.is_block());
    assert_eq!(finding.reason_code, "MISMATCH_CONTEXT_BLOCK");
}

#[test]
fn static_codebro_fixture_collects_identity() {
    let temp = common::fixture_copy("codebro");
    let identity = collector().collect(temp.path()).unwrap();
    assert!(identity.identity_names().contains(&"codebro".to_string()));
    assert!(identity.package_names.contains(&"griller-mcp".to_string()));
    assert_eq!(identity.readme_title.as_deref(), Some("CodeBro"));
}

#[test]
fn static_atlas_fixture_supports_positive_match() {
    let temp = common::fixture_copy("atlas");
    let identity = collector().collect(temp.path()).unwrap();
    assert!(identity.identity_names().contains(&"atlas".to_string()));
    assert!(identity
        .all_paths
        .contains(&"src/fact_store.rs".to_string()));

    let assessor = ContextAssessor::deterministic();
    let assessment = assessor
        .assess("Implement P10.5.1 Fact Store.", &identity)
        .unwrap();
    assert!(!assessment.is_mismatch());
    assert_eq!(assessment.status, ContextStatus::LikelyMatch);
}
