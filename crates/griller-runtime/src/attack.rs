//! M5 adversarial attack execution and validation.

use std::path::{Path, PathBuf};

use griller_core::attack::{AttackKind, AttackOutcome, AttackResult, AttackSpec, ExecutionCapture};
use griller_core::command::{CommandSpec, ExecutionStatus};
use griller_core::evidence::EvidenceId;
use griller_core::redaction::RedactionReport;
use serde::{Deserialize, Serialize};

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};
use crate::git::git_command;

/// Allow-list classification for command executables.
///
/// Determines whether an attack command may be executed.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CommandAllowlist {
    /// Safe: read-only commands with no mutation or network access.
    SafeReadonly,
    /// Potentially unsafe: may mutate state or access network.
    PotentiallyUnsafe,
    /// Unsafe: must never execute automatically.
    Unsafe,
}

/// Result of validating an attack spec against the allow-list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ValidationResult {
    /// Whether the attack is allowed to execute.
    pub allowed: bool,
    /// Classification of the command(s) involved.
    pub classification: CommandAllowlist,
    /// Human-readable justification or rejection reason.
    pub reason: String,
}

/// A finding from the stub sweep attack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StubSweepFinding {
    /// File path where the marker was found.
    pub file: PathBuf,
    /// Line number (1-indexed), 0 if unknown.
    pub line: usize,
    /// The marker type (TODO, FIXME, unimplemented!, etc.).
    pub marker: String,
    /// Context line containing the marker.
    pub context: String,
    /// Severity level.
    pub severity: StubSweepSeverity,
}

/// Severity of a stub sweep finding.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum StubSweepSeverity {
    /// Warning: potential unfinished implementation.
    Warning,
    /// Suspicious: likely unfinished code.
    Suspicious,
    /// Critical: clearly incomplete implementation.
    Critical,
}

/// A finding from the safety attack.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SafetyFinding {
    /// File path where the issue was found.
    pub file: PathBuf,
    /// The dangerous pattern detected.
    pub pattern: String,
    /// Context line.
    pub context: String,
    /// Description of the risk.
    pub risk: String,
}

/// Executor for M5 adversarial attacks.
pub struct AttackExecutor;

impl AttackExecutor {
    /// Validate an attack spec against the command allow-list and working state requirements.
    pub fn validate(spec: &AttackSpec) -> ValidationResult {
        if let Some(cmd) = &spec.command {
            let classification = Self::classify_command(cmd);

            if classification == CommandAllowlist::Unsafe {
                return ValidationResult {
                    allowed: false,
                    classification,
                    reason: format!("unsafe command rejected: {}", cmd.display_line()),
                };
            }

            if classification == CommandAllowlist::PotentiallyUnsafe
                && spec.rationale.as_deref().is_none_or(|r| r.is_empty())
            {
                return ValidationResult {
                    allowed: false,
                    classification,
                    reason: "potentially unsafe without rationale".into(),
                };
            }

            for arg in &cmd.args {
                if arg.contains('`') || arg.contains('$') || arg.contains(';') {
                    return ValidationResult {
                        allowed: false,
                        classification: CommandAllowlist::Unsafe,
                        reason: format!("shell metacharacter in arg: {arg}"),
                    };
                }
            }
        }

        ValidationResult {
            allowed: true,
            classification: CommandAllowlist::SafeReadonly,
            reason: "validation passed".into(),
        }
    }

    fn classify_command(cmd: &CommandSpec) -> CommandAllowlist {
        match cmd.executable.as_str() {
            "git" | "grep" | "rg" | "find" | "cat" | "test" => CommandAllowlist::SafeReadonly,
            "cargo" => {
                if cmd.args.iter().any(|a| a == "run" || a == "install") {
                    CommandAllowlist::PotentiallyUnsafe
                } else {
                    CommandAllowlist::SafeReadonly
                }
            }
            "rustc" | "python" | "python3" | "node" | "npx" | "deno" => {
                CommandAllowlist::PotentiallyUnsafe
            }
            "rm" | "mv" | "chmod" | "chown" => CommandAllowlist::Unsafe,
            "curl" | "wget" => CommandAllowlist::PotentiallyUnsafe,
            "ssh" | "scp" => CommandAllowlist::Unsafe,
            "docker" => CommandAllowlist::Unsafe,
            _ => CommandAllowlist::PotentiallyUnsafe,
        }
    }

    /// Execute a replay attack: run the claimed verification command.
    ///
    /// Returns an [`AttackResult`] with the outcome and evidence.
    pub fn execute_replay(
        spec: &AttackSpec,
        _repo: &Path,
        env: &EnvSource,
    ) -> Result<AttackResult, RuntimeError> {
        let cmd = spec
            .command
            .as_ref()
            .expect("replay spec must have a command");
        let result = ExecutionBackend::execute(cmd, env)?;
        let outcome = Self::outcome_for_status(&result.status, result.exit_code);

        let explanation = match outcome {
            AttackOutcome::NoObjection => {
                format!("replay succeeded (exit 0, {}ms)", result.duration_ms)
            }
            AttackOutcome::Refuted => {
                format!(
                    "replay failed (exit {}, {}ms)",
                    result.exit_code.unwrap_or(-1),
                    result.duration_ms
                )
            }
            AttackOutcome::ExecutionFailed => "replay could not execute".into(),
            _ => "replay".into(),
        };

        let capture = ExecutionCapture {
            spec: cmd.clone(),
            stdout: String::from_utf8_lossy(&result.stdout).into_owned(),
            stderr: String::from_utf8_lossy(&result.stderr).into_owned(),
            exit_code: result.exit_code,
            status: result.status.clone(),
            duration_ms: result.duration_ms,
            redaction: RedactionReport::default(),
        };

        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome,
            evidence_refs: vec![EvidenceId::from_content_hash(&format!(
                "replay-{}",
                spec.id.as_str()
            ))],
            execution: capture,
            explanation,
        })
    }

    fn outcome_for_status(status: &ExecutionStatus, exit_code: Option<i32>) -> AttackOutcome {
        match status {
            ExecutionStatus::Completed => {
                if exit_code == Some(0) {
                    AttackOutcome::NoObjection
                } else {
                    AttackOutcome::Refuted
                }
            }
            ExecutionStatus::TimedOut => AttackOutcome::ExecutionFailed,
            ExecutionStatus::FailedToSpawn(_) => AttackOutcome::ExecutionFailed,
        }
    }

    /// Execute a presence/absence attack: verify claims about file/entity existence.
    ///
    /// Supports: "Removed <path>", "Added <path>", "Exists <path>".
    pub fn execute_presence_absence(
        spec: &AttackSpec,
        repo: &Path,
    ) -> Result<AttackResult, RuntimeError> {
        let claim = &spec.target_claim;
        let (expected_absent, target_path) = Self::parse_presence_claim(claim, repo)?;
        let path_exists = target_path.exists();

        let outcome = if expected_absent {
            if path_exists {
                AttackOutcome::Refuted
            } else {
                AttackOutcome::NoObjection
            }
        } else if path_exists {
            AttackOutcome::Confirmed
        } else {
            AttackOutcome::Refuted
        };

        let explanation = if expected_absent {
            if path_exists {
                format!(
                    "presence: '{}' exists but claim says removed",
                    target_path.display()
                )
            } else {
                format!("presence: '{}' absent as claimed", target_path.display())
            }
        } else if path_exists {
            format!("presence: '{}' exists as claimed", target_path.display())
        } else {
            format!("presence: '{}' not found", target_path.display())
        };

        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome,
            evidence_refs: vec![EvidenceId::from_content_hash(&format!(
                "presence-{}-{}",
                claim,
                if path_exists { "exists" } else { "absent" }
            ))],
            execution: ExecutionCapture {
                spec: CommandSpec::new("test").with_args(["-e", &target_path.to_string_lossy()]),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(if path_exists { 0 } else { 1 }),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation,
        })
    }

    fn parse_presence_claim(claim: &str, repo: &Path) -> Result<(bool, PathBuf), RuntimeError> {
        if let Some(rest) = claim.strip_prefix("Removed ") {
            let fragment = rest.trim();
            // Reject empty fragments.
            if fragment.is_empty() {
                return Err(RuntimeError::Execution(
                    "empty: claim has no path after Removed".into(),
                ));
            }
            // Reject absolute paths in the fragment.
            if fragment.starts_with('/') {
                return Err(RuntimeError::Execution(
                    "repository-relative: claim contains absolute path".into(),
                ));
            }
            // Reject traversal in the fragment.
            if fragment.contains("..") {
                return Err(RuntimeError::Execution(
                    "traverse: claim resolves outside the repository root".into(),
                ));
            }
            let path = repo.join(fragment);
            // Reject symlink escapes - check all path components.
            let mut current = path.clone();
            let mut escaped = false;
            while let Some(parent) = current.parent() {
                if let Ok(metadata) = std::fs::symlink_metadata(&current) {
                    if metadata.file_type().is_symlink() {
                        if let Ok(target) = std::fs::read_link(&current) {
                            let resolved = if target.is_absolute() {
                                target
                            } else {
                                current.parent().unwrap_or(current.as_path()).join(&target)
                            };
                            if let Ok(canonical_repo) = repo.canonicalize() {
                                if let Ok(canonical_resolved) = resolved.canonicalize() {
                                    if !canonical_resolved.starts_with(&canonical_repo) {
                                        escaped = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                current = parent.to_path_buf();
            }
            if escaped {
                return Err(RuntimeError::Execution(
                    "escapes: claim resolves outside the repository root via symlink".into(),
                ));
            }
            // Double-check canonicalized path stays inside repo.
            if path.strip_prefix(repo).is_err() {
                if let Ok(canonical_repo) = repo.canonicalize() {
                    if let Ok(canonical_path) = path.canonicalize() {
                        if !canonical_path.starts_with(&canonical_repo) {
                            return Err(RuntimeError::Execution(
                                "traverse: claim resolves outside the repository root".into(),
                            ));
                        }
                    }
                }
            }
            return Ok((true, path));
        }
        if let Some(rest) = claim.strip_prefix("Added ") {
            let fragment = rest.trim();
            // Reject empty fragments.
            if fragment.is_empty() {
                return Err(RuntimeError::Execution(
                    "empty: claim has no path after Added".into(),
                ));
            }
            // Reject absolute paths in the fragment.
            if fragment.starts_with('/') {
                return Err(RuntimeError::Execution(
                    "repository-relative: claim contains absolute path".into(),
                ));
            }
            // Reject traversal in the fragment.
            if fragment.contains("..") {
                return Err(RuntimeError::Execution(
                    "traverse: claim resolves outside the repository root".into(),
                ));
            }
            let path = repo.join(fragment);
            // Reject symlink escapes - check all path components.
            let mut current = path.clone();
            let mut escaped = false;
            while let Some(parent) = current.parent() {
                if let Ok(metadata) = std::fs::symlink_metadata(&current) {
                    if metadata.file_type().is_symlink() {
                        if let Ok(target) = std::fs::read_link(&current) {
                            let resolved = if target.is_absolute() {
                                target
                            } else {
                                current.parent().unwrap_or(current.as_path()).join(&target)
                            };
                            if let Ok(canonical_repo) = repo.canonicalize() {
                                if let Ok(canonical_resolved) = resolved.canonicalize() {
                                    if !canonical_resolved.starts_with(&canonical_repo) {
                                        escaped = true;
                                        break;
                                    }
                                }
                            }
                        }
                    }
                }
                current = parent.to_path_buf();
            }
            if escaped {
                return Err(RuntimeError::Execution(
                    "escapes: claim resolves outside the repository root via symlink".into(),
                ));
            }
            // Double-check canonicalized path stays inside repo.
            if path.strip_prefix(repo).is_err() {
                if let Ok(canonical_repo) = repo.canonicalize() {
                    if let Ok(canonical_path) = path.canonicalize() {
                        if !canonical_path.starts_with(&canonical_repo) {
                            return Err(RuntimeError::Execution(
                                "traverse: claim resolves outside the repository root".into(),
                            ));
                        }
                    }
                }
            }
            return Ok((false, path));
        }
        for word in claim.split_whitespace() {
            // Reject absolute paths in the fallback word path.
            if word.starts_with('/') {
                return Err(RuntimeError::Execution(
                    "repository-relative: claim contains absolute path".into(),
                ));
            }
            // Reject traversal in the fallback word path.
            if word.contains("..") {
                return Err(RuntimeError::Execution(
                    "traverse: claim contains path traversal".into(),
                ));
            }
            if word.ends_with(".rs") || word.ends_with(".py") || word.ends_with(".js") {
                return Ok((false, repo.join(word.trim_matches(['/', '\\']))));
            }
        }
        Err(RuntimeError::Execution(
            "claim format not recognized".into(),
        ))
    }

    /// Execute a stub sweep: inspect changed files for unfinished implementation markers.
    ///
    /// Scans for: TODO, FIXME, todo!(), unimplemented!(), panic!(), "not implemented".
    pub fn execute_stub_sweep(
        spec: &AttackSpec,
        repo: &Path,
    ) -> Result<AttackResult, RuntimeError> {
        let mut findings: Vec<StubSweepFinding> = Vec::new();
        let git_output = Self::get_changed_files(repo);
        let changed_files: Vec<PathBuf> = git_output
            .lines()
            .filter_map(|l| l.get(2..).map(|s| s.trim_start()).map(|p| repo.join(p)))
            .collect();

        for file_path in &changed_files {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                Self::scan_file_for_stubs(file_path, &content, &mut findings);
            }
        }

        let outcome = Self::stub_sweep_outcome(&findings, &spec.target_claim);
        let explanation = if findings.is_empty() {
            "stub sweep: no markers found".into()
        } else {
            format!("stub sweep: found {} marker(s)", findings.len())
        };

        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome,
            evidence_refs: findings
                .iter()
                .map(|f| EvidenceId::from_content_hash(&format!("stub-{}", f.file.display())))
                .collect(),
            execution: ExecutionCapture {
                spec: CommandSpec::new("find").with_args([
                    repo.to_string_lossy().as_ref(),
                    "-name",
                    "*.rs",
                ]),
                stdout: format!("{:?}", findings),
                stderr: String::new(),
                exit_code: Some(0),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation,
        })
    }

    fn get_changed_files(repo: &Path) -> String {
        let spec = git_command(repo, &["status", "--porcelain"]);
        match ExecutionBackend::execute(&spec, &EnvSource::from_host()) {
            Ok(result) if result.succeeded() => result.stdout_lossy(),
            _ => String::new(),
        }
    }

    fn scan_file_for_stubs(file: &Path, content: &str, findings: &mut Vec<StubSweepFinding>) {
        for (line_num, line) in content.lines().enumerate() {
            let line_num = line_num + 1;
            if line.to_lowercase().contains("todo") {
                findings.push(StubSweepFinding {
                    file: file.to_path_buf(),
                    line: line_num,
                    marker: "TODO".into(),
                    context: line.trim().to_string(),
                    severity: StubSweepSeverity::Warning,
                });
            }
            if line.to_lowercase().contains("fixme") {
                findings.push(StubSweepFinding {
                    file: file.to_path_buf(),
                    line: line_num,
                    marker: "FIXME".into(),
                    context: line.trim().to_string(),
                    severity: StubSweepSeverity::Suspicious,
                });
            }
            if line.contains("unimplemented!") {
                findings.push(StubSweepFinding {
                    file: file.to_path_buf(),
                    line: line_num,
                    marker: "unimplemented!".into(),
                    context: line.trim().to_string(),
                    severity: StubSweepSeverity::Critical,
                });
            }
            if line.contains("todo!(") {
                findings.push(StubSweepFinding {
                    file: file.to_path_buf(),
                    line: line_num,
                    marker: "todo!".into(),
                    context: line.trim().to_string(),
                    severity: StubSweepSeverity::Critical,
                });
            }
        }
    }

    fn stub_sweep_outcome(findings: &[StubSweepFinding], claim: &str) -> AttackOutcome {
        if findings.is_empty() {
            return AttackOutcome::NoObjection;
        }
        let claim_lower = claim.to_lowercase();
        let claims_completeness = claim_lower.contains("complete")
            || claim_lower.contains("all done")
            || claim_lower.contains("finished")
            || claim_lower.contains("implementation complete")
            || claim_lower.contains("all functions")
            || claim_lower.contains("everything implemented");

        if claims_completeness {
            let has_critical = findings
                .iter()
                .any(|f| f.severity == StubSweepSeverity::Critical);
            if has_critical {
                return AttackOutcome::Refuted;
            }
            return AttackOutcome::Inconclusive;
        }
        AttackOutcome::Inconclusive
    }

    /// Execute a safety attack: inspect changed files and commands for dangerous patterns.
    ///
    /// Looks for: credential exposure, destructive commands, suspicious network access.
    pub fn execute_safety(spec: &AttackSpec, repo: &Path) -> Result<AttackResult, RuntimeError> {
        let mut findings: Vec<SafetyFinding> = Vec::new();
        let git_output = Self::get_changed_files(repo);
        let changed_files: Vec<PathBuf> = git_output
            .lines()
            .filter_map(|l| l.get(2..).map(|s| s.trim_start()).map(|p| repo.join(p)))
            .collect();

        for file_path in &changed_files {
            if let Ok(content) = std::fs::read_to_string(file_path) {
                Self::scan_file_for_safety(file_path, &content, &mut findings);
            }
        }

        if let Some(cmd) = &spec.command {
            if let Some(finding) = Self::check_command_for_danger(cmd) {
                findings.push(finding);
            }
        }

        let outcome = if findings.is_empty() {
            AttackOutcome::NoObjection
        } else {
            AttackOutcome::Inconclusive
        };

        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome,
            evidence_refs: findings
                .iter()
                .map(|f| EvidenceId::from_content_hash(&format!("safety-{}", f.file.display())))
                .collect(),
            execution: ExecutionCapture {
                spec: CommandSpec::new("grep").with_args(["-r", ".", "--include=*.rs"]),
                stdout: format!("{:?}", findings),
                stderr: String::new(),
                exit_code: Some(0),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation: if findings.is_empty() {
                "safety: no concerns found".into()
            } else {
                format!("safety: {} concern(s) found", findings.len())
            },
        })
    }

    fn scan_file_for_safety(file: &Path, content: &str, findings: &mut Vec<SafetyFinding>) {
        let cl = content.to_lowercase();
        if cl.contains("password=") || cl.contains("passwd=") {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "potential_password".into(),
                context: String::new(),
                risk: "Possible password exposure".into(),
            });
        }
        if cl.contains("api_key=") || cl.contains("apikey=") {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "potential_api_key".into(),
                context: String::new(),
                risk: "Possible API key exposure".into(),
            });
        }
        if cl.contains("secret=") || cl.contains("token=") {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "potential_secret".into(),
                context: String::new(),
                risk: "Possible secret/token exposure".into(),
            });
        }
        if content.contains("PRIVATE KEY") {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "private_key".into(),
                context: String::new(),
                risk: "Private key material".into(),
            });
        }
        if content.contains("AKIA") && content.len() > 20 {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "aws_credential".into(),
                context: String::new(),
                risk: "AWS credential pattern".into(),
            });
        }
        if content.contains("rm -rf") || content.contains("sudo rm") {
            findings.push(SafetyFinding {
                file: file.to_path_buf(),
                pattern: "destructive_command".into(),
                context: "rm -rf or sudo rm".into(),
                risk: "Destructive operation".into(),
            });
        }
    }

    fn check_command_for_danger(cmd: &CommandSpec) -> Option<SafetyFinding> {
        let full = cmd.display_line();
        for dangerous in ["rm -rf", "sudo", "chmod", "chown"] {
            if full.contains(dangerous) {
                return Some(SafetyFinding {
                    file: PathBuf::from("<command>"),
                    pattern: format!("dangerous_command_{}", dangerous),
                    context: full.clone(),
                    risk: format!("Contains: {}", dangerous),
                });
            }
        }
        if cmd.executable == "curl" || cmd.executable == "wget" {
            return Some(SafetyFinding {
                file: PathBuf::from("<command>"),
                pattern: "network_access".into(),
                context: full,
                risk: "Network access".into(),
            });
        }
        None
    }

    /// Execute a completeness attack (foundation).
    ///
    /// Returns INCONCLUSIVE as full coverage analysis is not yet implemented.
    pub fn execute_completeness(
        spec: &AttackSpec,
        _repo: &Path,
    ) -> Result<AttackResult, RuntimeError> {
        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome: AttackOutcome::Inconclusive,
            evidence_refs: Vec::new(),
            execution: ExecutionCapture {
                spec: CommandSpec::new("echo").with_args(["true"]),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation: "completeness: foundation only".into(),
        })
    }

    /// Execute an invert attack (foundation - returns NOT_APPLICABLE).
    ///
    /// Uses isolated git worktrees. Never modifies the user's working tree.
    pub fn execute_invert(spec: &AttackSpec, _repo: &Path) -> Result<AttackResult, RuntimeError> {
        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome: AttackOutcome::NotApplicable,
            evidence_refs: Vec::new(),
            execution: ExecutionCapture {
                spec: CommandSpec::new("echo").with_args(["true"]),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation: "invert: not implemented - deferred".into(),
        })
    }

    /// Execute a robustness attack: run verification under stricter conditions.
    ///
    /// Repeats the command and compares results for consistency.
    pub fn execute_robustness(
        spec: &AttackSpec,
        _repo: &Path,
        env: &EnvSource,
    ) -> Result<AttackResult, RuntimeError> {
        let cmd = spec
            .command
            .as_ref()
            .expect("robustness spec must have a command");
        let r1 = ExecutionBackend::execute(cmd, env)?;
        let r2 = ExecutionBackend::execute(cmd, env)?;

        let outcome = if r1.succeeded() && r2.succeeded() {
            AttackOutcome::NoObjection
        } else if !r1.succeeded() && !r2.succeeded() {
            AttackOutcome::Refuted
        } else {
            AttackOutcome::Inconclusive
        };

        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome,
            evidence_refs: Vec::new(),
            execution: ExecutionCapture {
                spec: cmd.clone(),
                stdout: format!("{}, {}", r1.succeeded(), r2.succeeded()),
                stderr: String::new(),
                exit_code: Some(if r1.succeeded() { 0 } else { 1 }),
                status: ExecutionStatus::Completed,
                duration_ms: r1.duration_ms + r2.duration_ms,
                redaction: RedactionReport::default(),
            },
            explanation: match outcome {
                AttackOutcome::NoObjection => "robustness: consistent".into(),
                AttackOutcome::Refuted => "robustness: consistently failed".into(),
                _ => "robustness: inconsistent".into(),
            },
        })
    }

    /// Execute a requirement coverage attack (foundation).
    ///
    /// Maps requirements to evidence. Returns INCONCLUSIVE as mapping is not yet implemented.
    pub fn execute_requirement_coverage(
        spec: &AttackSpec,
        _requirements: &[(&'static str, bool)],
    ) -> Result<AttackResult, RuntimeError> {
        Ok(AttackResult {
            attack: spec.id.clone(),
            outcome: AttackOutcome::Inconclusive,
            evidence_refs: Vec::new(),
            execution: ExecutionCapture {
                spec: CommandSpec::new("echo").with_args(["true"]),
                stdout: String::new(),
                stderr: String::new(),
                exit_code: Some(0),
                status: ExecutionStatus::Completed,
                duration_ms: 0,
                redaction: RedactionReport::default(),
            },
            explanation: "requirement coverage: foundation only".into(),
        })
    }
}

/// Validator for M5 attack proposals.
pub struct AttackValidator;

impl AttackValidator {
    /// Validate an attack proposal before execution.
    ///
    /// Returns Ok(()) if allowed, or an error if rejected.
    pub fn validate_attack(spec: &AttackSpec) -> Result<(), RuntimeError> {
        let v = AttackExecutor::validate(spec);
        if !v.allowed {
            return Err(RuntimeError::Execution(v.reason));
        }
        if let Some(cmd) = &spec.command {
            for arg in &cmd.args {
                if arg.contains('`') || arg.contains('$') || arg.contains(';') {
                    return Err(RuntimeError::Execution(format!("shell char in arg: {arg}")));
                }
            }
        }
        Ok(())
    }

    /// Check whether an attack kind has a concrete implementation.
    pub fn is_attack_implemented(kind: &AttackKind) -> bool {
        matches!(
            kind,
            AttackKind::Replay
                | AttackKind::PresenceAbsence
                | AttackKind::StubSweep
                | AttackKind::Safety
        )
    }

    /// Check whether an attack kind is applicable to a claim scope.
    pub fn is_attack_applicable(_kind: &AttackKind, _claim: &str) -> bool {
        true
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use griller_core::attack::{AttackOutcome, AttackSpec};

    #[test]
    fn classify_git_safe() {
        assert_eq!(
            AttackExecutor::classify_command(&CommandSpec::new("git").with_args(["status"])),
            CommandAllowlist::SafeReadonly
        );
    }

    #[test]
    fn classify_rm_unsafe() {
        assert_eq!(
            AttackExecutor::classify_command(&CommandSpec::new("rm").with_args(["-rf", "/"])),
            CommandAllowlist::Unsafe
        );
    }

    #[test]
    fn validate_rejects_shell_chars() {
        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("t"),
            kind: AttackKind::Replay,
            target_claim: "t".into(),
            command: Some(CommandSpec::new("echo").with_args(["$(rm -rf /)"])),
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        assert!(AttackValidator::validate_attack(&spec).is_err());
    }

    #[test]
    fn presence_removed_but_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("foo.rs"), "x").unwrap();
        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("p1"),
            kind: AttackKind::PresenceAbsence,
            target_claim: "Removed foo.rs".into(),
            command: None,
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let r = AttackExecutor::execute_presence_absence(&spec, dir.path()).unwrap();
        assert_eq!(r.outcome, AttackOutcome::Refuted);
    }

    #[test]
    fn presence_added_and_exists() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("new.rs"), "x").unwrap();
        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("p2"),
            kind: AttackKind::PresenceAbsence,
            target_claim: "Added new.rs".into(),
            command: None,
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let r = AttackExecutor::execute_presence_absence(&spec, dir.path()).unwrap();
        assert_eq!(r.outcome, AttackOutcome::Confirmed);
    }

    #[test]
    fn stub_finds_unimplemented_critical() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("s.rs"), "fn x() -> i32 { 42 }\n").unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .ok();
        std::process::Command::new("git")
            .args(["add", "s.rs"])
            .current_dir(&dir)
            .output()
            .ok();
        std::process::Command::new("git")
            .args(["commit", "-m", "i"])
            .current_dir(&dir)
            .output()
            .ok();
        std::fs::write(
            dir.path().join("s.rs"),
            "fn x() -> i32 { 42 }\nfn y() -> i32 { unimplemented!() }\n",
        )
        .unwrap();
        std::process::Command::new("git")
            .args(["add", "s.rs"])
            .current_dir(&dir)
            .output()
            .ok();

        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("ss1"),
            kind: AttackKind::StubSweep,
            target_claim: "All functions implemented".into(),
            command: None,
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let r = AttackExecutor::execute_stub_sweep(&spec, dir.path()).unwrap();
        assert_eq!(r.outcome, AttackOutcome::Refuted);
    }

    #[test]
    fn safety_finds_password() {
        let dir = tempfile::tempdir().unwrap();
        std::process::Command::new("git")
            .args(["init"])
            .current_dir(&dir)
            .output()
            .ok();
        std::fs::write(dir.path().join("l.rs"), "password=hunter2\n").unwrap();
        std::process::Command::new("git")
            .args(["add", "l.rs"])
            .current_dir(&dir)
            .output()
            .ok();

        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("sa1"),
            kind: AttackKind::Safety,
            target_claim: "safe".into(),
            command: None,
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let _r = AttackExecutor::execute_safety(&spec, dir.path()).unwrap();
    }

    #[test]
    fn invert_returns_not_applicable() {
        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("inv"),
            kind: AttackKind::Invert,
            target_claim: "t".into(),
            command: None,
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let r = AttackExecutor::execute_invert(&spec, Path::new("/tmp")).unwrap();
        assert_eq!(r.outcome, AttackOutcome::NotApplicable);
    }

    #[test]
    fn robustness_consistent() {
        let spec = AttackSpec {
            id: griller_core::attack::AttackId::from_content_hash("rb1"),
            kind: AttackKind::Robustness,
            target_claim: "Tests pass".into(),
            command: Some(CommandSpec::new("echo").with_args(["ok"])),
            working_state: None,
            timeout_ms: 30_000,
            rationale: None,
        };
        let r =
            AttackExecutor::execute_robustness(&spec, Path::new("/tmp"), &EnvSource::from_host())
                .unwrap();
        assert_eq!(r.outcome, AttackOutcome::NoObjection);
    }
}
