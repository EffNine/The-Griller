//! Phase 2: timeout teardown must not leave descendants behind.
//!
//! A `cargo test` replay spawns `rustc` and linker grandchildren. Signalling only
//! the direct child left those grandchildren running, reparented to init, where
//! they could keep writing into a worktree the caller is about to remove and
//! perturb a *later* verification. Since the evidence model is content-addressed,
//! that is a reproducibility defect, not just a resource leak.
//!
//! These tests use three independent liveness signals so that no single one is
//! load-bearing:
//!
//! 1. **PID liveness** — the descendant's PID is read from a file the script
//!    wrote, then checked against `/proc`. The OS is the authority here.
//! 2. **Tick file stability** — the descendant appends to a file forever. If it
//!    survives, the file keeps growing. Asserted over a bounded window.
//! 3. **`flock` release** — the descendant holds an exclusive lock for its whole
//!    life. If the test can take that lock, the holder is gone. This needs no
//!    sleeping at all and is the strongest of the three where `flock` exists.
//!
//! The readiness marker matters as much as the checks: it proves the descendant
//! was really alive when the timeout fired, so a passing test cannot be an
//! accident of the script never having started.

#![cfg(unix)]

mod common;

use std::path::{Path, PathBuf};
use std::process::Command;
use std::time::{Duration, Instant};

use griller_core::command::{CommandSpec, EnvironmentPolicy, ExecutionStatus, NetworkPolicy};
use griller_runtime::execution::ExecutionBackend;

use common::test_env_source;

/// The descendant: hold an exclusive lock and append to a file, forever.
///
/// `sleep` inside the loop is a true grandchild of the process the backend spawns,
/// which is what makes this a real process-tree test rather than a single-process
/// one.
const DESCENDANT: &str = r#"#!/bin/sh
exec 9>>"$1"
flock -x 9 2>/dev/null || true
while :; do
  echo tick >> "$2"
  sleep 0.05
done
"#;

/// The direct child: start the descendant, record its PID, announce readiness,
/// then stay alive until it is terminated.
const PARENT: &str = r#"#!/bin/sh
sh "$6" "$1" "$2" &
echo $! > "$3"
: > "$4"
wait
"#;

struct Fixture {
    dir: tempfile::TempDir,
}

impl Fixture {
    fn new() -> Self {
        let dir = tempfile::tempdir().expect("temp dir");
        let write = |name: &str, body: &str| {
            let p = dir.path().join(name);
            std::fs::write(&p, body).expect("write script");
            p
        };
        write("descendant.sh", DESCENDANT);
        write("parent.sh", PARENT);
        Self { dir }
    }

    fn path(&self, name: &str) -> PathBuf {
        self.dir.path().join(name)
    }

    /// Poll until `name` exists, or fail with a clear message.
    fn wait_for(&self, name: &str, what: &str) -> PathBuf {
        let p = self.path(name);
        let deadline = Instant::now() + Duration::from_secs(10);
        while Instant::now() < deadline {
            if p.exists() {
                return p;
            }
            std::thread::sleep(Duration::from_millis(10));
        }
        panic!("{what} ({name}) never appeared — the descendant never started");
    }

    fn tick_len(&self) -> u64 {
        std::fs::metadata(self.path("tick"))
            .map(|m| m.len())
            .unwrap_or(0)
    }
}

fn spec_for(script: &Path, args: &[String], timeout_ms: u64) -> CommandSpec {
    CommandSpec::new("sh")
        .with_args(
            std::iter::once(script.display().to_string())
                .chain(args.iter().cloned())
                .collect::<Vec<_>>(),
        )
        .with_timeout(timeout_ms)
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(vec![
            "PATH".to_string(),
            "HOME".to_string(),
            "LANG".to_string(),
        ]))
        .with_network_policy(NetworkPolicy::Disabled)
}

/// Run the parent script, forcing a timeout, and return
/// `(result, fixture, descendant_pid)`.
fn timed_out_run() -> (griller_runtime::execution::ExecutionResult, Fixture, u32) {
    let fx = Fixture::new();
    let pidfile = fx.path("pid");
    let ready = fx.path("ready");
    let tick = fx.path("tick");
    let lock = fx.path("lock");
    let descendant = fx.path("descendant.sh");
    let parent = fx.path("parent.sh");

    let args: Vec<String> = [&lock, &tick, &pidfile, &ready, &descendant]
        .iter()
        .take(5)
        .map(|p| p.display().to_string())
        .collect();
    // PARENT reads $1..$4 for the paths and $6 for the descendant script.
    let spec = spec_for(
        &parent,
        &[
            args[0].clone(),
            args[1].clone(),
            args[2].clone(),
            args[3].clone(),
            String::new(),
            args[4].clone(),
        ],
        1_500,
    );

    let result = ExecutionBackend::execute(&spec, &test_env_source(&[])).expect("execute");

    // The descendant must have started, otherwise the test proves nothing.
    fx.wait_for("ready", "descendant readiness marker");
    let pid: u32 = std::fs::read_to_string(&pidfile)
        .expect("read pid file")
        .trim()
        .parse()
        .expect("pid must parse");

    (result, fx, pid)
}

fn alive(pid: u32) -> bool {
    Path::new(&format!("/proc/{pid}")).exists()
}

fn wait_until_gone(pid: u32, timeout: Duration) -> bool {
    let deadline = Instant::now() + timeout;
    while Instant::now() < deadline {
        if !alive(pid) {
            return true;
        }
        std::thread::sleep(Duration::from_millis(20));
    }
    !alive(pid)
}

// ---------------------------------------------------------------------------
// The timeout itself stays fail-closed
// ---------------------------------------------------------------------------

#[test]
fn descendant_timeout_is_fail_closed() {
    let (result, _fx, _pid) = timed_out_run();

    // A timeout must never look like success. These three are the whole
    // fail-closed contract and are asserted before anything else.
    assert_eq!(result.status, ExecutionStatus::TimedOut);
    assert!(result.timed_out, "timed_out flag must be set");
    assert_eq!(result.exit_code, None, "a timeout must have no exit code");
    assert!(
        !result.succeeded(),
        "a timed-out execution must never report success"
    );
    assert!(
        !result.stdout_lossy().contains("test result: ok"),
        "no passing evidence may be produced by a timeout"
    );
}

/// The core Phase 2 regression: a descendant must not outlive the timeout.
#[test]
fn timeout_terminates_descendants_not_just_the_direct_child() {
    let (_result, _fx, pid) = timed_out_run();

    // Readiness was asserted inside `timed_out_run`, so the descendant was
    // definitely alive when the timeout fired.
    assert!(
        wait_until_gone(pid, Duration::from_secs(5)),
        "descendant {pid} survived the timeout; it must be terminated with the \
         process group"
    );
}

/// Corroborating signal: the descendant appends forever, so a surviving process
/// means the file keeps growing.
#[test]
fn descendant_stops_writing_after_timeout() {
    let (_result, fx, pid) = timed_out_run();

    assert!(
        wait_until_gone(pid, Duration::from_secs(5)),
        "descendant survived"
    );

    let before = fx.tick_len();
    std::thread::sleep(Duration::from_millis(400));
    let after = fx.tick_len();
    assert_eq!(
        before, after,
        "tick file kept growing after the timeout ({before} -> {after}); the \
         descendant is still running"
    );
}

/// Strongest signal where `flock` exists: the descendant holds an exclusive lock
/// for its whole life, so acquiring it proves the holder is gone. No sleeping.
#[test]
fn descendant_releases_its_lock_on_timeout() {
    if !has_flock() {
        eprintln!("skipping: flock(1) unavailable on this host");
        return;
    }
    let (_result, fx, _pid) = timed_out_run();
    let lock = fx.path("lock");

    // Give the kernel a moment to release the fd if the process is mid-exit.
    let deadline = Instant::now() + Duration::from_secs(5);
    loop {
        if try_lock(&lock) {
            return; // acquired => holder is dead
        }
        assert!(
            Instant::now() < deadline,
            "the descendant's exclusive lock is still held 5s after the timeout"
        );
        std::thread::sleep(Duration::from_millis(50));
    }
}

fn has_flock() -> bool {
    Command::new("sh")
        .args(["-c", "command -v flock >/dev/null 2>&1"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

/// Try to take an exclusive lock non-blockingly. Succeeding proves no live
/// process holds it.
fn try_lock(lock: &Path) -> bool {
    Command::new("flock")
        .args(["-x", "-n", &lock.display().to_string(), "true"])
        .status()
        .map(|s| s.success())
        .unwrap_or(false)
}

// ---------------------------------------------------------------------------
// Second-run contamination
// ---------------------------------------------------------------------------

/// The risk that actually matters: not "one PID disappeared", but "an orphan
/// from RUN A cannot perturb RUN B".
///
/// RUN A times out while holding a lock and writing to a file. RUN B then runs a
/// normal successful command **in the same directory** and must behave exactly as
/// it would in a clean environment.
#[test]
fn orphan_from_a_timed_out_run_cannot_perturb_the_next_run() {
    let (result_a, fx, pid) = timed_out_run();

    // RUN A left the filesystem in a contaminated-looking state: a lock file and a
    // partially written tick file both exist.
    assert!(
        fx.path("lock").exists(),
        "RUN A must have created the lock file"
    );
    assert!(
        fx.tick_len() > 0,
        "RUN A descendant must have written at least once"
    );
    assert!(
        wait_until_gone(pid, Duration::from_secs(5)),
        "descendant survived"
    );

    // RUN B: a normal successful command in the very same directory.
    let out = fx.path("b.out");
    let spec = CommandSpec::new("sh")
        .with_args(vec![
            "-c".to_string(),
            // Reads and writes in the contaminated directory, then succeeds.
            format!(
                "cat {} >/dev/null 2>&1; echo CLEAN_RUN; exit 0",
                out.display()
            ),
        ])
        .with_timeout(10_000)
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(vec![
            "PATH".to_string(),
            "HOME".to_string(),
            "LANG".to_string(),
        ]))
        .with_network_policy(NetworkPolicy::Disabled);

    let result_b = ExecutionBackend::execute(&spec, &test_env_source(&[])).expect("RUN B");

    // RUN B behaves exactly as a clean execution.
    assert_eq!(
        result_b.status,
        ExecutionStatus::Completed,
        "RUN B must complete"
    );
    assert!(!result_b.timed_out, "RUN B must not time out");
    assert_eq!(result_b.exit_code, Some(0), "RUN B must exit 0");
    assert!(result_b.succeeded(), "RUN B must succeed");
    assert_eq!(result_b.stdout_lossy().trim(), "CLEAN_RUN");

    // The orphan cannot hold a lock RUN B needs.
    if has_flock() {
        assert!(
            try_lock(&fx.path("lock")),
            "an orphan still holds the lock — RUN A's cleanup was incomplete"
        );
    }

    // The orphan cannot still be writing into the directory RUN B used.
    let before = fx.tick_len();
    std::thread::sleep(Duration::from_millis(400));
    assert_eq!(
        before,
        fx.tick_len(),
        "an orphan is still writing into the tree"
    );

    // And RUN A's own result was never upgraded into a pass.
    assert!(!result_a.succeeded());
    assert_eq!(result_a.exit_code, None);
}
