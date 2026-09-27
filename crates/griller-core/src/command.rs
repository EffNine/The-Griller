//! Structured command specification and execution-status types.
//!
//! A [`CommandSpec`] describes a command to run as *data*: an executable plus
//! explicit arguments. There is deliberately **no** shell-string field. This
//! makes commands safe to allow-list, hash, and execute without an implicit
//! shell.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

/// Default command timeout in milliseconds (30 seconds).
pub const DEFAULT_TIMEOUT_MS: u64 = 30_000;

/// Timeout for derived replay commands (10 minutes).
///
/// Replays run the full verification suite in a cold, isolated worktree, so
/// they must compile every dependency from scratch. The default 30-second
/// timeout can never cover that; an honest "all tests pass" claim on any
/// non-trivial Rust repository would always time out into UNVERIFIABLE.
pub const REPLAY_TIMEOUT_MS: u64 = 600_000;

// ---------------------------------------------------------------------------
// Cross-process replay serialization
// ---------------------------------------------------------------------------

/// Cross-process exclusive lock serializing replay executions per repository.
///
/// Two simultaneous gate runs each spawn heavy verification commands
/// (`cargo test --workspace`, …). Test suites are not always parallel-safe
/// *across processes* — shared fixture directories under `CARGO_TARGET_TMPDIR`,
/// fixed ports, singleton state — so two concurrent replays of the same
/// repository can fail each other (exit != 0) and flip verdicts
/// non-deterministically. The lock file lives in the system temp directory,
/// keyed by the repository path, so every process replaying the same
/// repository queues on the same lock. `flock` semantics release the lock
/// even when a holder crashes.
#[derive(Debug)]
pub struct ReplayLock {
    /// Held open for the lifetime of the lock; closing the file releases it.
    _file: std::fs::File,
}

impl ReplayLock {
    /// Acquire the replay lock for `repo`, waiting up to `timeout`.
    ///
    /// Returns `None` when the lock could not be acquired within `timeout`;
    /// the caller then proceeds without serialization rather than failing the
    /// verification pipeline on lock contention alone.
    pub fn acquire(repo: &std::path::Path, timeout: std::time::Duration) -> Option<Self> {
        use fs2::FileExt;

        let key: String = repo
            .to_string_lossy()
            .chars()
            .map(|c| if c.is_alphanumeric() { c } else { '_' })
            .collect();
        let lock_path = std::env::temp_dir().join(format!("griller-replay-{key}.lock"));
        let file = std::fs::OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .ok()?;
        let start = std::time::Instant::now();
        loop {
            match file.try_lock_exclusive() {
                Ok(()) => return Some(Self { _file: file }),
                Err(_) if start.elapsed() < timeout => {
                    std::thread::sleep(std::time::Duration::from_millis(50));
                }
                Err(_) => return None,
            }
        }
    }
}

#[cfg(test)]
mod replay_lock_tests {
    use super::*;

    #[test]
    fn replay_lock_excludes_second_holder_and_releases() {
        let repo = std::env::temp_dir().join(format!("griller-lock-test-{}", std::process::id()));
        std::fs::create_dir_all(&repo).unwrap();

        let first = ReplayLock::acquire(&repo, std::time::Duration::from_millis(100))
            .expect("first acquire must succeed");
        let second = ReplayLock::acquire(&repo, std::time::Duration::from_millis(150));
        assert!(second.is_none(), "second holder must time out while held");

        drop(first);
        let third = ReplayLock::acquire(&repo, std::time::Duration::from_millis(1_000))
            .expect("acquire must succeed after release");
        drop(third);

        let _ = std::fs::remove_dir_all(&repo);
    }
}

/// How the child process environment is derived from the caller's
/// environment source (host environment or an injected map; see
/// `griller_runtime::execution::EnvSource`).
///
/// The default is [`EnvironmentPolicy::Clear`]: a child runs with an empty
/// environment unless the caller explicitly grants keys. Callers that need
/// `git` or other tools pass an explicit allow-list (see
/// `griller_runtime::execution::DEFAULT_ENV_ALLOWLIST`).
///
/// There is deliberately **no "inherit everything" mode**. The Griller never
/// silently inherits the full host environment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum EnvironmentPolicy {
    /// Run the child with an empty environment.
    #[default]
    Clear,
    /// Inherit only the listed environment keys from the environment source.
    InheritAllowlist(Vec<String>),
}

/// Network access policy for a command.
///
/// The default is [`NetworkPolicy::Disabled`]. In M0/M1 network isolation is a
/// *declared* constraint plus a guard against interactive prompts
/// (`GIT_TERMINAL_PROMPT=0`); true kernel-level network isolation is deferred
/// to later phases (see `docs/execution.md`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "snake_case")]
pub enum NetworkPolicy {
    /// Declared intent to allow network access.
    Enabled,
    /// Declared intent to disable network access.
    #[default]
    Disabled,
}

/// Whether an execution constraint was actually enforced by the backend.
///
/// This is the **enforced** side of the requested/enforced distinction:
///
/// ```text
/// DECLARED POLICY != ENFORCED POLICY
/// ```
///
/// A `CommandSpec` *requests* a policy (e.g. `network_policy =
/// Disabled`). The [`Enforcement`] record reports what the backend *actually
/// guaranteed*. In M1 the backend performs no kernel-level isolation, so the
/// report truthfully says `NotEnforced` even when a policy was requested.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EnforcementLevel {
    /// The constraint was requested but the backend could not enforce it.
    NotEnforced,
    /// The backend actually enforced the constraint at the OS level.
    Enforced,
}

/// What the backend actually enforced during an execution.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct Enforcement {
    /// Whether kernel/OS-level network isolation was actually applied.
    pub network_isolation: EnforcementLevel,
    /// Whether OS-level sandboxing (seccomp, namespaces, privilege drop) was
    /// actually applied.
    pub os_sandbox: EnforcementLevel,
}

impl Enforcement {
    /// M1 backend truth: nothing beyond normal process boundaries is enforced.
    pub fn m1_none() -> Self {
        Self {
            network_isolation: EnforcementLevel::NotEnforced,
            os_sandbox: EnforcementLevel::NotEnforced,
        }
    }
}

impl Default for Enforcement {
    fn default() -> Self {
        Self::m1_none()
    }
}

/// A structured command specification.
///
/// The primary representation is `executable` + `args`; the shell is never
/// involved. Arguments are always passed verbatim (no metacharacter
/// interpretation).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct CommandSpec {
    /// Executable to run, resolved via `PATH` or an absolute path.
    pub executable: String,
    /// Arguments passed verbatim to the executable, in order.
    #[serde(default)]
    pub args: Vec<String>,
    /// Working directory for the child. `None` inherits the backend's cwd.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    /// Timeout in milliseconds. `0` means no timeout.
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Environment derivation policy.
    #[serde(default)]
    pub environment_policy: EnvironmentPolicy,
    /// Network access policy.
    #[serde(default)]
    pub network_policy: NetworkPolicy,
}

impl CommandSpec {
    /// Create a command with default (safe) policies: empty environment,
    /// network disabled, 30s timeout.
    pub fn new(executable: impl Into<String>) -> Self {
        Self {
            executable: executable.into(),
            args: Vec::new(),
            working_directory: None,
            timeout_ms: DEFAULT_TIMEOUT_MS,
            environment_policy: EnvironmentPolicy::default(),
            network_policy: NetworkPolicy::default(),
        }
    }

    /// Set the argument list.
    pub fn with_args<I, S>(mut self, args: I) -> Self
    where
        I: IntoIterator<Item = S>,
        S: Into<String>,
    {
        self.args = args.into_iter().map(Into::into).collect();
        self
    }

    /// Set the working directory.
    pub fn with_working_directory(mut self, dir: impl Into<PathBuf>) -> Self {
        self.working_directory = Some(dir.into());
        self
    }

    /// Set the timeout in milliseconds. `0` disables the timeout.
    pub fn with_timeout(mut self, timeout_ms: u64) -> Self {
        self.timeout_ms = timeout_ms;
        self
    }

    /// Set the environment policy.
    pub fn with_environment_policy(mut self, policy: EnvironmentPolicy) -> Self {
        self.environment_policy = policy;
        self
    }

    /// Set the network policy.
    pub fn with_network_policy(mut self, policy: NetworkPolicy) -> Self {
        self.network_policy = policy;
        self
    }

    /// Render the executable and arguments as a display string.
    ///
    /// This is for humans and diagnostics only; it is never fed back to a
    /// shell.
    pub fn display_line(&self) -> String {
        let mut line = self.executable.clone();
        for arg in &self.args {
            line.push(' ');
            line.push_str(arg);
        }
        line
    }
}

fn default_timeout_ms() -> u64 {
    DEFAULT_TIMEOUT_MS
}

/// The terminal state of an executed process.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ExecutionStatus {
    /// The process ran to completion (exit code may be non-zero).
    Completed,
    /// The process was killed because it exceeded the spec timeout.
    TimedOut,
    /// The process could not be spawned at all (e.g. executable not found).
    FailedToSpawn(String),
}

/// A high-level interpretation of an executed command.
///
/// **Important:** `Failed`, `TimedOut` and `CouldNotSpawn` describe the
/// *collection* of evidence, not the truth of any claim. A failed evidence
/// collection is never automatically proof that a claim is false
/// (COMMAND_FAILED != CLAIM_FALSE).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandOutcome {
    /// Process completed with exit code 0.
    Success,
    /// Process completed with a non-zero exit code.
    Failed,
    /// Process exceeded its timeout and was killed.
    TimedOut,
    /// The process could not be started.
    CouldNotSpawn,
}

impl CommandOutcome {
    /// True when the command completed successfully.
    pub fn is_success(self) -> bool {
        matches!(self, CommandOutcome::Success)
    }
}

/// Derive a [`CommandOutcome`] from a status and exit code.
pub fn outcome_for(status: &ExecutionStatus, exit_code: Option<i32>) -> CommandOutcome {
    match status {
        ExecutionStatus::Completed => {
            if exit_code == Some(0) {
                CommandOutcome::Success
            } else {
                CommandOutcome::Failed
            }
        }
        ExecutionStatus::TimedOut => CommandOutcome::TimedOut,
        ExecutionStatus::FailedToSpawn(_) => CommandOutcome::CouldNotSpawn,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn command_spec_defaults_are_safe() {
        let spec = CommandSpec::new("echo");
        assert_eq!(spec.environment_policy, EnvironmentPolicy::Clear);
        assert_eq!(spec.network_policy, NetworkPolicy::Disabled);
        assert_eq!(spec.timeout_ms, DEFAULT_TIMEOUT_MS);
        assert!(spec.args.is_empty());
        assert!(spec.working_directory.is_none());
    }

    #[test]
    fn builder_roundtrip() {
        let spec = CommandSpec::new("git")
            .with_args(["status", "--porcelain"])
            .with_working_directory("/tmp/x")
            .with_timeout(1000);
        assert_eq!(spec.display_line(), "git status --porcelain");
        assert_eq!(spec.args, ["status", "--porcelain"]);
        assert_eq!(spec.timeout_ms, 1000);
    }

    #[test]
    fn outcome_derivation() {
        assert_eq!(
            outcome_for(&ExecutionStatus::Completed, Some(0)),
            CommandOutcome::Success
        );
        assert_eq!(
            outcome_for(&ExecutionStatus::Completed, Some(1)),
            CommandOutcome::Failed
        );
        assert_eq!(
            outcome_for(&ExecutionStatus::TimedOut, None),
            CommandOutcome::TimedOut
        );
        assert_eq!(
            outcome_for(&ExecutionStatus::FailedToSpawn("nope".into()), None),
            CommandOutcome::CouldNotSpawn
        );
    }
}
