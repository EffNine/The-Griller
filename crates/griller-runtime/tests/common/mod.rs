//! Shared helpers for griller-runtime integration tests.
//!
//! These helpers create real git repositories and copy static fixtures; they
//! intentionally use `std::process::Command` for *setup only*. The code under
//! test always goes through the execution backend.

// Each integration-test binary compiles this module independently, so not
// every helper is used by every test file.
#![allow(dead_code)]

use std::path::{Path, PathBuf};
use std::process::Command;

use griller_runtime::execution::EnvSource;

/// The workspace root (three levels up from this file).
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

/// A controlled environment source with `PATH`, `HOME` and `LANG` set, plus
/// any extra pairs. Used so tests are hermetic and don't depend on the host
/// environment.
pub fn test_env_source(extra: &[(&str, &str)]) -> EnvSource {
    let mut vars: Vec<(String, String)> = vec![
        ("PATH".to_string(), "/usr/bin:/bin".to_string()),
        ("HOME".to_string(), "/tmp".to_string()),
        ("LANG".to_string(), "C.UTF-8".to_string()),
    ];
    for (k, v) in extra {
        vars.push(((*k).to_string(), (*v).to_string()));
    }
    EnvSource::from_pairs(vars)
}

/// Run git for test setup; returns trimmed stdout on success, panics on
/// failure.
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

/// Run git for test setup, returning whether it succeeded (no panic).
pub fn git_try(dir: &Path, args: &[&str]) -> bool {
    Command::new("git")
        .args(args)
        .current_dir(dir)
        .env("GIT_TERMINAL_PROMPT", "0")
        .output()
        .map(|o| o.status.success())
        .unwrap_or(false)
}

/// Initialise a git repository with a fixed branch name and local identity.
pub fn init_repo(dir: &Path) {
    git(dir, &["init", "-q", "-b", "main"]);
    git(dir, &["config", "user.email", "griller-test@example.com"]);
    git(dir, &["config", "user.name", "Griller Test"]);
}

/// Stage and commit everything in `dir`.
pub fn commit_all(dir: &Path, message: &str) {
    git(dir, &["add", "-A"]);
    git(dir, &["commit", "-q", "-m", message]);
}

/// Create a fresh temp git repository with one committed file.
pub fn make_repo() -> tempfile::TempDir {
    let repo = tempfile::tempdir().expect("create temp dir");
    init_repo(repo.path());
    std::fs::write(repo.path().join("hello.txt"), "hello from the griller\n").unwrap();
    commit_all(repo.path(), "initial commit");
    repo
}

/// Create a fresh temp bare repository (usable as a remote URL).
pub fn make_bare_repo() -> tempfile::TempDir {
    let bare = tempfile::tempdir().expect("create temp dir");
    let output = Command::new("git")
        .args(["init", "-q", "--bare"])
        .current_dir(bare.path())
        .output()
        .expect("run git init --bare");
    assert!(output.status.success(), "git init --bare failed");
    bare
}

fn copy_dir_recursive(source: &Path, dest: &Path) {
    std::fs::create_dir_all(dest).unwrap();
    for entry in std::fs::read_dir(source).unwrap() {
        let entry = entry.unwrap();
        let target = dest.join(entry.file_name());
        if entry.file_type().unwrap().is_dir() {
            copy_dir_recursive(&entry.path(), &target);
        } else {
            std::fs::copy(entry.path(), &target).unwrap();
        }
    }
}
