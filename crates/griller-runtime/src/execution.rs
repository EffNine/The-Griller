//! Deterministic, shell-free process execution with controlled environment.
//!
//! ## Safety model
//!
//! * Commands run directly via `executable` + explicit `args`; **no shell is
//!   ever invoked**, so shell metacharacters in arguments are data.
//! * The child environment is derived explicitly from an [`EnvSource`] via the
//!   spec's [`EnvironmentPolicy`]. The default policy is `Clear`; secrets in
//!   the host environment are not inherited unless explicitly allow-listed.
//! * A `NetworkPolicy::Disabled` spec gets `GIT_TERMINAL_PROMPT=0` so tools
//!   fail closed instead of blocking on interactive prompts. Kernel-level
//!   network isolation is deferred (see `docs/execution.md`).
//! * Timeouts are enforced by polling, then terminating the whole **process
//!   group** (see [`terminate_process_group`]) so that descendants spawned by the
//!   direct child — `rustc` under `cargo test` — cannot outlive the timeout and
//!   perturb a later verification. On non-Unix the group signal is unavailable and
//!   only the direct child is terminated.

use std::collections::BTreeMap;
use std::path::Path;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

#[cfg(unix)]
use std::os::unix::process::CommandExt;

use griller_core::command::{
    CommandSpec, Enforcement, EnvironmentPolicy, ExecutionStatus, NetworkPolicy,
};
use griller_core::hash::{blake3_hex, canonical_bytes};

use crate::error::RuntimeError;

/// Default set of environment keys allowed through when no explicit allow-list
/// is supplied. Conservative; excludes any credential-carrying variables.
pub const DEFAULT_ENV_ALLOWLIST: &[&str] = &["PATH", "HOME", "LANG", "TMPDIR", "USER"];

/// Returns the default environment allow-list as owned strings.
pub fn default_env_allowlist() -> Vec<String> {
    DEFAULT_ENV_ALLOWLIST
        .iter()
        .map(|s| s.to_string())
        .collect()
}

/// A source of environment variables for a child process.
///
/// Either the host environment ([`EnvSource::from_host`]) or an explicit map
/// (for tests and controlled setups).
#[derive(Debug, Clone, Default)]
pub struct EnvSource {
    vars: Vec<(String, String)>,
}

impl EnvSource {
    /// Snapshot the host process environment.
    pub fn from_host() -> Self {
        Self {
            vars: std::env::vars().collect(),
        }
    }

    /// Build an environment source from an iterator of `(key, value)` pairs.
    ///
    /// Named `from_pairs` to avoid confusion with `std::iter::FromIterator`.
    pub fn from_pairs<I>(iter: I) -> Self
    where
        I: IntoIterator<Item = (String, String)>,
    {
        Self {
            vars: iter.into_iter().collect(),
        }
    }

    /// Resolve the environment to pass to a child for the given policy.
    ///
    /// Results are sorted by key for determinism.
    pub fn resolve(&self, policy: &EnvironmentPolicy) -> Vec<(String, String)> {
        let mut resolved: Vec<(String, String)> = match policy {
            EnvironmentPolicy::Clear => Vec::new(),
            EnvironmentPolicy::InheritAllowlist(keys) => self
                .vars
                .iter()
                .filter(|(key, _)| keys.contains(key))
                .cloned()
                .collect(),
        };
        resolved.sort();
        resolved
    }
}

/// The structured result of executing a command.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExecutionResult {
    /// The specification that was executed.
    pub spec: CommandSpec,
    /// Terminal status of the process.
    pub status: ExecutionStatus,
    /// Exit code (`None` if the process was killed or failed to spawn).
    pub exit_code: Option<i32>,
    /// Captured stdout bytes.
    pub stdout: Vec<u8>,
    /// Captured stderr bytes.
    pub stderr: Vec<u8>,
    /// Wall-clock duration of the execution.
    pub duration_ms: u64,
    /// `true` when the process exceeded its timeout and was killed.
    pub timed_out: bool,
    /// What the backend actually enforced. In M1 this is always `Enforcement::m1_none()`
    /// — a requested policy is never misrepresented as an enforced one.
    pub enforcement: Enforcement,
    /// BLAKE3 fingerprint over the environment actually passed to the child
    /// (sorted `key=value` pairs). A fingerprint, never a raw dump.
    pub env_fingerprint: String,
    /// The sorted list of environment pairs actually passed to the child.
    pub passed_env: Vec<(String, String)>,
}

impl ExecutionResult {
    /// stdout as a lossy UTF-8 string. Untrusted data.
    pub fn stdout_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stdout).into_owned()
    }

    /// stderr as a lossy UTF-8 string. Untrusted data.
    pub fn stderr_lossy(&self) -> String {
        String::from_utf8_lossy(&self.stderr).into_owned()
    }

    /// True when the command ran to completion with exit code 0.
    pub fn succeeded(&self) -> bool {
        self.status == ExecutionStatus::Completed && self.exit_code == Some(0)
    }
}

/// The execution backend.
///
/// Stateless and deliberately boring: every call spawns a process directly
/// from the [`CommandSpec`]. There is no shell and no ambient state.
pub struct ExecutionBackend;

/// Put the child into its own process group so a timeout can tear down the
/// whole tree, not just the direct child.
///
/// On Unix this issues `setpgid(0, 0)` in the child between `fork` and `exec`, so
/// the child becomes a group leader whose PGID equals its own PID.
/// [`CommandExt::process_group`] is used rather than `Command::pre_exec` because it
/// is safe and cannot be used to smuggle in non-signal-safe code.
///
/// On non-Unix this is a no-op and teardown falls back to the direct child.
#[cfg(unix)]
fn isolate_process_group(cmd: &mut Command) {
    cmd.process_group(0);
}

#[cfg(not(unix))]
fn isolate_process_group(_cmd: &mut Command) {}

/// Terminate the child and every process it spawned.
///
/// [`std::process::Child::kill`] signals only the direct child, so a `cargo test`
/// replay leaves `rustc` and linker grandchildren running: they get reparented to
/// init, can keep writing into a worktree this process is about to remove, and can
/// perturb a *later* verification. Because the evidence model is content-addressed,
/// that is a reproducibility defect, not merely a resource leak.
///
/// On Unix the whole group is signalled with `SIGKILL`, matching the previous
/// direct-child semantics: no grace period, no cleanup hook, no new exit path. The
/// direct child is additionally killed as a fallback so a spawn where the group did
/// not take effect still cannot leave the child running. Reaping is unchanged — the
/// caller still waits on the direct child exactly once.
///
/// Termination is best-effort and **never** changes the reported status: a timeout
/// stays [`ExecutionStatus::TimedOut`] with `exit_code: None` either way, so this
/// cannot turn a timeout into a success.
#[cfg(unix)]
#[allow(
    unsafe_code,
    reason = "killpg(2) has no safe std equivalent; confined to this function. See crate-level note in lib.rs and PHASE_2_BASELINE.md §6"
)]
fn terminate_process_group(child: &mut Child) {
    let pid = child.id();
    if pid != 0 {
        // SAFETY: `killpg` is a direct async-signal-safe syscall wrapper taking a
        // process-group id and a signal number, with no preconditions beyond a
        // valid pgid; it cannot dereference memory or allocate. The child was
        // placed in its own group by `isolate_process_group` before `exec`, so its
        // PGID equals its PID and the id is a live group leader at this point
        // (the caller only reaches here while the child is still running). A group
        // that has already exited returns ESRCH, which is deliberately ignored:
        // the fallback below then reaps whatever remains, and neither outcome can
        // alter the TimedOut status the caller reports.
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);
        }
    }
    terminate_direct_child(child);
}

/// Non-Unix fallback: only the direct child can be signalled with a safe API.
#[cfg(not(unix))]
fn terminate_process_group(child: &mut Child) {
    terminate_direct_child(child);
}

/// Signal the direct child. Errors are ignored because the child may already be
/// dead, which is not an error condition here.
fn terminate_direct_child(child: &mut Child) {
    let _ = child.kill();
}

impl ExecutionBackend {
    /// Execute the given command against the environment source.
    ///
    /// Never returns an error for a failed process — a non-zero exit, timeout,
    /// or spawn failure is a normal, structured result. I/O errors while
    /// waiting on the process are the only error path.
    pub fn execute(spec: &CommandSpec, env: &EnvSource) -> Result<ExecutionResult, RuntimeError> {
        let passed_env = env.resolve(&spec.environment_policy);
        let env_map: BTreeMap<String, String> = passed_env.iter().cloned().collect();
        let env_fingerprint = blake3_hex(&canonical_bytes(&env_map)?);

        let started = Instant::now();

        let mut cmd = Command::new(&spec.executable);
        cmd.args(&spec.args);
        if let Some(dir) = &spec.working_directory {
            cmd.current_dir(dir);
        }
        cmd.env_clear();
        cmd.envs(passed_env.iter().cloned());
        if spec.network_policy == NetworkPolicy::Disabled {
            // Fail closed instead of blocking on interactive prompts.
            cmd.env("GIT_TERMINAL_PROMPT", "0");
        }
        cmd.stdin(Stdio::null());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        isolate_process_group(&mut cmd);

        let mut child = match cmd.spawn() {
            Ok(child) => child,
            Err(err) => {
                return Ok(ExecutionResult {
                    spec: spec.clone(),
                    status: ExecutionStatus::FailedToSpawn(err.to_string()),
                    exit_code: None,
                    stdout: Vec::new(),
                    stderr: Vec::new(),
                    duration_ms: started.elapsed().as_millis() as u64,
                    timed_out: false,
                    enforcement: Enforcement::m1_none(),
                    env_fingerprint,
                    passed_env,
                });
            }
        };

        let deadline =
            (spec.timeout_ms > 0).then(|| Instant::now() + Duration::from_millis(spec.timeout_ms));

        let mut timed_out = false;
        loop {
            if let Some(deadline) = deadline {
                if Instant::now() >= deadline {
                    terminate_process_group(&mut child);
                    timed_out = true;
                    break;
                }
            }
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {}
                Err(err) => return Err(RuntimeError::Io(err)),
            }
            std::thread::sleep(Duration::from_millis(5));
        }

        let output = child.wait_with_output().map_err(RuntimeError::Io)?;
        let duration_ms = started.elapsed().as_millis() as u64;

        let exit_code = if timed_out {
            None
        } else {
            output.status.code()
        };
        let status = if timed_out {
            ExecutionStatus::TimedOut
        } else {
            ExecutionStatus::Completed
        };

        Ok(ExecutionResult {
            spec: spec.clone(),
            status,
            exit_code,
            stdout: output.stdout,
            stderr: output.stderr,
            duration_ms,
            timed_out,
            enforcement: Enforcement::m1_none(),
            env_fingerprint,
            passed_env,
        })
    }

    /// Convenience: run a command in a working directory, expecting success,
    /// returning trimmed stdout if and only if it completed with exit code 0.
    pub fn git_ok_output(
        dir: &Path,
        args: &[&str],
        env: &EnvSource,
    ) -> Result<Option<String>, RuntimeError> {
        let spec = crate::git::git_command(dir, args);
        let result = Self::execute(&spec, env)?;
        if result.succeeded() {
            Ok(Some(result.stdout_lossy().trim_end().to_string()))
        } else {
            Ok(None)
        }
    }
}
