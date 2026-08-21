//! Regression tests: presence/absence attack paths derived from agent claim
//! text must resolve strictly inside the target repository.
//!
//! A claim must never let an attack inspect `..` parents, absolute paths, or
//! symlink escapes. Rejected paths fail the attack (execution error), which is
//! never treated as refutation.

mod common;

use griller_core::attack::{AttackId, AttackKind, AttackOutcome, AttackSpec};
use griller_runtime::attack::AttackExecutor;

use common::make_repo;

fn presence_spec(id: &str, claim: &str) -> AttackSpec {
    AttackSpec {
        id: AttackId::from_content_hash(id),
        kind: AttackKind::PresenceAbsence,
        target_claim: claim.into(),
        command: None,
        working_state: None,
        timeout_ms: 30_000,
        rationale: None,
    }
}

// ---------------------------------------------------------------------------
// Normal (deterministic behavior preserved)
// ---------------------------------------------------------------------------

#[test]
fn normal_relative_path_resolves_inside_repo() {
    let repo = make_repo();
    std::fs::write(repo.path().join("foo.rs"), "x").unwrap();

    let r = AttackExecutor::execute_presence_absence(
        &presence_spec("t1", "Removed foo.rs"),
        repo.path(),
    )
    .unwrap();
    assert_eq!(r.outcome, AttackOutcome::Refuted);

    let r =
        AttackExecutor::execute_presence_absence(&presence_spec("t2", "Added foo.rs"), repo.path())
            .unwrap();
    assert_eq!(r.outcome, AttackOutcome::Confirmed);
}

#[test]
fn nested_relative_path_resolves_inside_repo() {
    let repo = make_repo();
    std::fs::create_dir_all(repo.path().join("src/lib")).unwrap();
    std::fs::write(repo.path().join("src/lib/mod.rs"), "x").unwrap();

    let r = AttackExecutor::execute_presence_absence(
        &presence_spec("t3", "Removed src/lib/mod.rs"),
        repo.path(),
    )
    .unwrap();
    assert_eq!(r.outcome, AttackOutcome::Refuted);

    // A missing target inside the repo is the normal "removed" case.
    let r = AttackExecutor::execute_presence_absence(
        &presence_spec("t4", "Removed src/lib/gone.rs"),
        repo.path(),
    )
    .unwrap();
    assert_eq!(r.outcome, AttackOutcome::NoObjection);
}

// ---------------------------------------------------------------------------
// Traversal / containment bypasses
// ---------------------------------------------------------------------------

#[test]
fn parent_traversal_is_rejected() {
    let repo = make_repo();
    std::fs::write(repo.path().parent().unwrap().join("outside.txt"), "secret").unwrap();

    let err = AttackExecutor::execute_presence_absence(
        &presence_spec("t5", "Added ../outside.txt"),
        repo.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("traverse"), "{err}");

    let err = AttackExecutor::execute_presence_absence(
        &presence_spec("t6", "Removed ../outside.txt"),
        repo.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("traverse"), "{err}");
}

#[test]
fn nested_traversal_is_rejected() {
    let repo = make_repo();
    let outside = repo.path().join("..").join("outside-dir");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.rs"), "secret").unwrap();

    let err = AttackExecutor::execute_presence_absence(
        &presence_spec("t7", "Added src/../../outside-dir/secret.rs"),
        repo.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("traverse"), "{err}");
}

#[test]
fn absolute_path_is_rejected() {
    let repo = make_repo();
    let absolute = std::env::temp_dir().join("griller-absolute-escape-test.rs");
    std::fs::write(&absolute, "x").unwrap();
    let claim = format!("Added {}", absolute.display());

    let err = AttackExecutor::execute_presence_absence(&presence_spec("t8", &claim), repo.path())
        .unwrap_err();
    assert!(err.to_string().contains("repository-relative"), "{err}");
}

#[test]
fn absolute_path_in_word_claim_is_rejected() {
    let repo = make_repo();
    let absolute = std::env::temp_dir().join("griller-absolute-word-test.rs");
    std::fs::write(&absolute, "x").unwrap();

    let claim = format!("The file {} was added", absolute.display());
    let err = AttackExecutor::execute_presence_absence(&presence_spec("t9", &claim), repo.path())
        .unwrap_err();
    assert!(err.to_string().contains("repository-relative"), "{err}");
}

#[cfg(unix)]
#[test]
fn symlink_escape_is_rejected() {
    use std::os::unix::fs::symlink;

    let repo = make_repo();
    let outside = repo.path().parent().unwrap().join("symlink-escape-target");
    std::fs::create_dir_all(&outside).unwrap();
    std::fs::write(outside.join("secret.rs"), "secret").unwrap();
    if symlink(&outside, repo.path().join("link")).is_err() {
        eprintln!("symlink creation unsupported; skipping");
        return;
    }

    // The final component itself is the escaping symlink.
    let err =
        AttackExecutor::execute_presence_absence(&presence_spec("t10", "Added link"), repo.path())
            .unwrap_err();
    assert!(err.to_string().contains("escapes"), "{err}");

    // A path through the escaping symlink.
    let err = AttackExecutor::execute_presence_absence(
        &presence_spec("t11", "Added link/secret.rs"),
        repo.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("escapes"), "{err}");
}

#[test]
fn empty_and_unrecognized_claims_are_rejected() {
    let repo = make_repo();
    let err =
        AttackExecutor::execute_presence_absence(&presence_spec("t12", "Removed "), repo.path())
            .unwrap_err();
    assert!(err.to_string().contains("empty"), "{err}");

    let err = AttackExecutor::execute_presence_absence(
        &presence_spec("t13", "Everything is fine"),
        repo.path(),
    )
    .unwrap_err();
    assert!(err.to_string().contains("not recognized"), "{err}");
}

#[test]
fn non_git_directory_still_enforces_containment() {
    // Containment must hold regardless of whether the target is a git repo.
    let dir = tempfile::tempdir().unwrap();
    std::fs::write(dir.path().join("inner.rs"), "x").unwrap();

    let r = AttackExecutor::execute_presence_absence(
        &presence_spec("t14", "Added inner.rs"),
        dir.path(),
    )
    .unwrap();
    assert_eq!(r.outcome, AttackOutcome::Confirmed);
}
