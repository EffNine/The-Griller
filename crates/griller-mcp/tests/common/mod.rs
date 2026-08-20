//! Shared helpers for griller-mcp integration tests.

#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

/// The workspace root (three levels up from this crate).
pub fn workspace_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .join("..")
        .join("..")
}

/// The static fixtures directory.
pub fn fixtures_dir() -> PathBuf {
    workspace_root().join("tests").join("fixtures")
}

/// Copy a static fixture directory into a fresh temporary directory.
pub fn fixture_copy(name: &str) -> tempfile::TempDir {
    let source = fixtures_dir().join(name);
    let temp = tempfile::tempdir().expect("create temp dir");
    copy_dir_recursive(&source, temp.path());
    temp
}

/// Copy a fixture into a temp dir and initialize it as a git repository with
/// one commit, so worktree isolation works for replay attacks.
pub fn fixture_git_repo(name: &str) -> tempfile::TempDir {
    let temp = fixture_copy(name);
    git(temp.path(), &["init", "-b", "main"]);
    git(
        temp.path(),
        &["config", "user.email", "griller-test@example.com"],
    );
    git(temp.path(), &["config", "user.name", "Griller Test"]);
    git(temp.path(), &["add", "-A"]);
    git(temp.path(), &["commit", "-m", "fixture"]);
    temp
}

/// Run git for test setup; panics on failure.
pub fn git(dir: &Path, args: &[&str]) -> String {
    let output = Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .expect("run git");
    assert!(
        output.status.success(),
        "git {args:?} failed in {:?}: {}",
        dir,
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8_lossy(&output.stdout).trim().to_string()
}

fn copy_dir_recursive(source: &Path, dest: &Path) {
    for entry in std::fs::read_dir(source).expect("read fixture dir") {
        let entry = entry.expect("read dir entry");
        let target = dest.join(entry.file_name());
        if entry.file_type().expect("file type").is_dir() {
            std::fs::create_dir_all(&target).expect("create dir");
            copy_dir_recursive(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).expect("copy file");
        }
    }
}
