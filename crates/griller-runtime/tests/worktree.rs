//! Worktree isolation tests: discovery, isolation from the main tree,
//! cleanup on success, cleanup after failure, and commit-less fallback.

mod common;

use griller_core::command::{CommandSpec, EnvironmentPolicy, ExecutionStatus, NetworkPolicy};
use griller_runtime::execution::{EnvSource, ExecutionBackend};
use griller_runtime::worktree::Worktree;

use common::{git, git_try, init_repo, make_repo};

fn allow_env() -> EnvSource {
    EnvSource::from_host()
}

fn env_cmd(exe: &str, args: &[&str]) -> CommandSpec {
    CommandSpec::new(exe)
        .with_args(args.iter().map(|s| s.to_string()))
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(vec![
            "PATH".into(),
            "HOME".into(),
        ]))
        .with_network_policy(NetworkPolicy::Disabled)
}

#[test]
fn discover_finds_repository_root_and_head() {
    let repo = make_repo();
    let info = Worktree::discover(repo.path()).unwrap();
    assert!(info.is_git);
    assert!(info.head.is_some());
    let toplevel = git(repo.path(), &["rev-parse", "--show-toplevel"]);
    assert_eq!(info.root.to_str().unwrap(), toplevel);
}

#[test]
fn discover_reports_non_git_dir() {
    let dir = tempfile::tempdir().unwrap();
    let info = Worktree::discover(dir.path()).unwrap();
    assert!(!info.is_git);
    assert!(info.head.is_none());
}

#[test]
fn worktree_is_isolated_from_main_tree() {
    let repo = make_repo();
    let worktree = Worktree::create(repo.path()).unwrap();
    let wt_path = worktree.path().to_path_buf();
    assert_ne!(wt_path, repo.path().canonicalize().unwrap());
    assert!(wt_path.join(".git").exists());

    // Mutations inside the worktree must not reach the main repo.
    std::fs::write(wt_path.join("new-file.txt"), "scratch\n").unwrap();
    assert!(!repo.path().join("new-file.txt").exists());

    // Committed state matches HEAD.
    let main_head = git(repo.path(), &["rev-parse", "HEAD"]);
    let wt_head = git(&wt_path, &["rev-parse", "HEAD"]);
    assert_eq!(main_head, wt_head);
}

#[test]
fn worktree_cleanup_removes_path_and_registration() {
    let repo = make_repo();
    let wt_path;
    {
        let worktree = Worktree::create(repo.path()).unwrap();
        wt_path = worktree.path().to_path_buf();
    }
    assert!(!wt_path.exists(), "worktree path must be removed on drop");
    let listing = git(repo.path(), &["worktree", "list"]);
    let registrations = listing.lines().count();
    assert_eq!(
        registrations, 1,
        "stale worktree registration left behind:\n{listing}"
    );
}

#[test]
fn worktree_cleanup_after_failed_command() {
    let repo = make_repo();
    let wt_path;
    {
        let worktree = Worktree::create(repo.path()).unwrap();
        wt_path = worktree.path().to_path_buf();
        // Run a failing command inside the worktree.
        let spec = env_cmd("/bin/false", &[]);
        let result = ExecutionBackend::execute(&spec, &allow_env()).unwrap();
        assert_eq!(result.exit_code, Some(1));
    }
    assert!(!wt_path.exists());
    assert_eq!(git(repo.path(), &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn commitless_repo_falls_back_to_clone() {
    let dir = tempfile::tempdir().unwrap();
    init_repo(dir.path());
    std::fs::write(dir.path().join("untracked.txt"), "x\n").unwrap();
    // No commits yet.
    assert!(!git_try(dir.path(), &["rev-parse", "HEAD"]));

    let worktree = Worktree::create(dir.path()).unwrap();
    assert!(worktree.path().exists());
    let wt_path = worktree.path().to_path_buf();
    drop(worktree);
    assert!(!wt_path.exists());
}

#[test]
fn create_on_non_git_repo_errors() {
    let dir = tempfile::tempdir().unwrap();
    let err = Worktree::create(dir.path());
    assert!(err.is_err());
}

#[test]
fn explicit_remove_is_idempotent() {
    let repo = make_repo();
    let wt_path;
    {
        let worktree = Worktree::create(repo.path()).unwrap();
        wt_path = worktree.path().to_path_buf();
        worktree.remove().unwrap();
    }
    assert!(!wt_path.exists());
    assert_eq!(git(repo.path(), &["worktree", "list"]).lines().count(), 1);
}

#[test]
fn execution_status_is_completed_inside_worktree() {
    let repo = make_repo();
    let _worktree = Worktree::create(repo.path()).unwrap();
    let spec = env_cmd("true", &[]);
    let result = ExecutionBackend::execute(&spec, &allow_env()).unwrap();
    assert_eq!(result.status, ExecutionStatus::Completed);
    assert_eq!(result.exit_code, Some(0));
}
