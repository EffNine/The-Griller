//! Deterministic attack planning for the MCP tools.
//!
//! Both `griller.run_attacks` and `griller.check_completion` need to turn a
//! claim into a small, deterministic, safe attack plan:
//!
//! * parse/validate user-requested attack kinds (reject unknown kinds)
//! * derive the safe replay command for a claim (never from the caller)
//! * compute which implemented attacks apply to a claim
//!
//! All decisions here are deterministic and read-only. Nothing executes.

use std::path::Path;

use griller_core::attack::AttackKind;
use griller_core::command::{CommandSpec, EnvironmentPolicy, NetworkPolicy};
use griller_runtime::execution::default_env_allowlist;
use griller_runtime::manifest::{find_manifests, DetectedManifest};

use crate::error::{McpError, McpErrorKind};
use crate::validate::MAX_ATTACKS;

/// Parse one attack-kind string into the canonical enum, or `None`.
pub fn parse_attack_kind(raw: &str) -> Option<AttackKind> {
    match raw {
        "replay" => Some(AttackKind::Replay),
        "completeness" => Some(AttackKind::Completeness),
        "presence_absence" => Some(AttackKind::PresenceAbsence),
        "invert" => Some(AttackKind::Invert),
        "robustness" => Some(AttackKind::Robustness),
        "requirement_coverage" => Some(AttackKind::RequirementCoverage),
        "stub_sweep" => Some(AttackKind::StubSweep),
        "safety" => Some(AttackKind::Safety),
        _ => None,
    }
}

/// Canonical wire name for an attack kind.
pub fn attack_kind_name(kind: AttackKind) -> &'static str {
    match kind {
        AttackKind::Replay => "replay",
        AttackKind::Completeness => "completeness",
        AttackKind::PresenceAbsence => "presence_absence",
        AttackKind::Invert => "invert",
        AttackKind::Robustness => "robustness",
        AttackKind::RequirementCoverage => "requirement_coverage",
        AttackKind::StubSweep => "stub_sweep",
        AttackKind::Safety => "safety",
    }
}

/// The attack kinds that currently have a real, deterministic implementation.
pub const IMPLEMENTED_ATTACKS: [AttackKind; 4] = [
    AttackKind::Replay,
    AttackKind::PresenceAbsence,
    AttackKind::StubSweep,
    AttackKind::Safety,
];

/// Validate and normalize a caller-supplied attack-kind list.
///
/// Rejects:
/// * an empty list
/// * a list larger than [`MAX_ATTACKS`]
/// * any kind that is not a known attack kind (unsupported attacks are an
///   INVALID_REQUEST, never silently dropped)
///
/// Returns kinds sorted by canonical name with duplicates removed, so the
/// same request always produces the same plan.
pub fn validate_attack_kinds(list: &[String]) -> Result<Vec<AttackKind>, McpError> {
    if list.is_empty() {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: "attack_kinds is empty".into(),
        });
    }
    if list.len() > MAX_ATTACKS {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!(
                "attack_kinds exceeds maximum length ({} > {})",
                list.len(),
                MAX_ATTACKS
            ),
        });
    }

    let mut kinds: Vec<AttackKind> = Vec::new();
    for raw in list {
        let kind = parse_attack_kind(raw).ok_or_else(|| McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!("unsupported attack kind: {raw}"),
        })?;
        if !kinds.contains(&kind) {
            kinds.push(kind);
        }
    }

    kinds.sort_by_key(|k| attack_kind_name(*k));
    Ok(kinds)
}

/// The claim keywords that signal a verification command may be applicable.
/// These are checked first; only when one matches do we consult the
/// repository to determine the actual command.
const VERIFICATION_KEYWORDS: &[&str] = &[
    "test", "pass", "fail", "clippy", "lint", "fmt", "format", "check", "build", "compil",
    "pytest", "jest", "mocha", "tape",
];

/// Whether the claim mentions a verification keyword.
fn claim_mentions_verification(claim: &str) -> bool {
    let lower = claim.to_lowercase();
    VERIFICATION_KEYWORDS.iter().any(|kw| lower.contains(kw))
}

/// Derive the safe replay command implied by a claim and a repository.
///
/// The command is derived from the **repository evidence** by the Griller
/// itself and never taken from the caller. The claim text is only used to
/// determine whether a verification command is in scope; the actual
/// executable and arguments come from manifest detection.
///
/// Returns `None` when:
/// * the claim does not mention a verification keyword, or
/// * the repository ecosystem is ambiguous / unknown, or
/// * no safe deterministic replay command exists.
///
/// The command inherits only the allow-listed environment keys and has
/// network access disabled.
pub fn derive_replay_command(claim: &str, repo: &Path) -> Option<CommandSpec> {
    if !claim_mentions_verification(claim) {
        return None;
    }
    ecosystem_command(repo).map(|cmd| cmd.with_network_policy(NetworkPolicy::Disabled))
}

/// Detect the repository ecosystem from manifests and return the canonical
/// test/verification command for that ecosystem.
///
/// Returns `None` when the ecosystem is ambiguous or unknown.
fn ecosystem_command(repo: &Path) -> Option<CommandSpec> {
    let manifests = match find_manifests(repo) {
        Ok(m) => m,
        Err(_) => return None,
    };

    // Prefer root-level manifests; skip vendor/target directories.
    let root_manifests: Vec<&DetectedManifest> = manifests
        .iter()
        .filter(|m| m.path.components().count() <= 2)
        .collect();

    if root_manifests.is_empty() {
        return None;
    }

    // Rust takes priority when a Cargo.toml is present at the root.
    if root_manifests
        .iter()
        .any(|m| m.format == griller_core::evidence::ManifestFormat::CargoToml)
    {
        return Some(safe_command("cargo", &["test"]));
    }

    // Go when go.mod is at the root.
    if root_manifests
        .iter()
        .any(|m| m.format == griller_core::evidence::ManifestFormat::GoMod)
    {
        return Some(safe_command("go", &["test", "./..."]));
    }

    // Node.js — detect package manager from lockfile / packageManager field.
    if root_manifests
        .iter()
        .any(|m| m.format == griller_core::evidence::ManifestFormat::PackageJson)
    {
        return node_test_command(repo, &manifests);
    }

    // Python — detect pytest from pyproject.toml.
    if root_manifests
        .iter()
        .any(|m| m.format == griller_core::evidence::ManifestFormat::PyProjectToml)
    {
        return python_test_command(repo);
    }

    None
}

/// Derive the Node.js test command from package.json and lockfile evidence.
fn node_test_command(_repo: &Path, _manifests: &[DetectedManifest]) -> Option<CommandSpec> {
    // Read package.json to find the test script and package manager.
    let pkg_path = _repo.join("package.json");
    let raw = match std::fs::read_to_string(&pkg_path) {
        Ok(r) => r,
        Err(_) => return None,
    };
    let value: serde_json::Value = match serde_json::from_str(&raw) {
        Ok(v) => v,
        Err(_) => return None,
    };

    let empty_map = serde_json::Map::new();
    let scripts = value
        .get("scripts")
        .and_then(|s| s.as_object())
        .unwrap_or(&empty_map);
    let test_script = scripts.get("test").and_then(|t| t.as_str());

    // Detect package manager from lockfiles / packageManager field.
    let manager = detect_node_manager(_repo);

    // Prefer an explicit test script; otherwise fall back to the manager's
    // default test command.
    if let Some(cmd) = test_script {
        // The test script must be a simple, deterministic command.
        // Reject scripts with shell metacharacters or known-unsafe patterns.
        if is_safe_test_script(cmd) {
            let args: Vec<&str> = cmd.split_whitespace().collect();
            if args.is_empty() {
                return None;
            }
            // Use the declared test script as executable + args.
            return Some(
                CommandSpec::new(args[0])
                    .with_args(args[1..].iter().copied())
                    .with_environment_policy(EnvironmentPolicy::InheritAllowlist(
                        default_env_allowlist(),
                    )),
            );
        }
    }

    // Fallback: use the detected package manager's test command.
    match manager {
        "pnpm" => Some(safe_command("pnpm", &["test"])),
        "yarn" => Some(safe_command("yarn", &["test"])),
        "bun" => Some(safe_command("bun", &["test"])),
        _ => Some(safe_command("npm", &["test"])),
    }
}

/// Detect the Node.js package manager from lockfiles and packageManager field.
fn detect_node_manager(repo: &Path) -> &'static str {
    // Check packageManager field first (most authoritative).
    let pkg_path = repo.join("package.json");
    if let Ok(raw) = std::fs::read_to_string(&pkg_path) {
        if let Ok(value) = serde_json::from_str::<serde_json::Value>(&raw) {
            if let Some(pm) = value.get("packageManager").and_then(|p| p.as_str()) {
                if pm.starts_with("pnpm") {
                    return "pnpm";
                }
                if pm.starts_with("yarn") {
                    return "yarn";
                }
                if pm.starts_with("bun") {
                    return "bun";
                }
                if pm.starts_with("npm") {
                    return "npm";
                }
            }
        }
    }
    // Fall back to lockfile presence.
    if repo.join("pnpm-lock.yaml").is_file() {
        return "pnpm";
    }
    if repo.join("yarn.lock").is_file() {
        return "yarn";
    }
    if repo.join("bun.lockb").is_file() {
        return "bun";
    }
    if repo.join("package-lock.json").is_file() {
        return "npm";
    }
    "npm"
}

/// Check whether a test script is safe to execute verbatim.
///
/// Rejects scripts containing shell metacharacters, variable expansion,
/// or known-unsafe patterns.
fn is_safe_test_script(script: &str) -> bool {
    // Must not contain shell metacharacters.
    if script.chars().any(|c| {
        matches!(
            c,
            '$' | '`' | ';' | '|' | '&' | '(' | ')' | '{' | '}' | '>' | '<'
        )
    }) {
        return false;
    }
    // Must not contain subshell patterns.
    if script.contains("$((") || script.contains("||") || script.contains("&&") {
        return false;
    }
    // Must look like a single command with simple args.
    let parts: Vec<&str> = script.split_whitespace().collect();
    if parts.is_empty() {
        return false;
    }
    // The executable must be a known safe binary.
    let exe = parts[0];
    matches!(
        exe,
        "node"
            | "npm"
            | "npx"
            | "yarn"
            | "pnpm"
            | "bun"
            | "jest"
            | "vitest"
            | "mocha"
            | "tape"
            | "ava"
            | "tsx"
            | "ts-node"
            | "pytest"
            | "python"
            | "python3"
    ) || exe.ends_with(".js")
        || exe.ends_with(".mjs")
        || exe.ends_with(".cjs")
}

/// Derive the Python test command from pyproject.toml evidence.
fn python_test_command(repo: &Path) -> Option<CommandSpec> {
    let pyproject_path = repo.join("pyproject.toml");
    let raw = match std::fs::read_to_string(&pyproject_path) {
        Ok(r) => r,
        Err(_) => return None,
    };

    // Look for [tool.pytest.ini_options] section to confirm pytest is configured.
    let has_pytest_section =
        raw.contains("[tool.pytest") || raw.contains("[tool.pytest") || raw.contains("pytest");

    // Also check for pytest in dependencies.
    let has_pytest_dep =
        raw.contains("pytest") || raw.contains("\"pytest\"") || raw.contains("'pytest'");

    if has_pytest_section || has_pytest_dep {
        return Some(safe_command("pytest", &["-x"]));
    }

    // Check for tox or unittest.
    if raw.contains("[tool.tox]") || raw.contains("tox") {
        return Some(safe_command("tox", &[]));
    }

    // Default to Python's built-in unittest if no test framework is declared.
    Some(safe_command("python3", &["-m", "unittest", "discover"]))
}

/// Build a safe command spec with the default allow-listed environment.
fn safe_command(executable: &str, args: &[&str]) -> CommandSpec {
    CommandSpec::new(executable)
        .with_args(args.iter().copied())
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(default_env_allowlist()))
}

/// Whether Replay can run for this claim (a safe command could be derived).
/// Uses the claim text alone (without repository context) for the
/// `run_attacks` tool, which may be called before repository scanning.
#[allow(dead_code)]
pub fn replay_applicable(claim: &str) -> bool {
    claim_mentions_verification(claim)
}

/// Whether Replay can run for this claim given the repository path.
/// This is the authoritative check used in the completion pipeline.
pub fn replay_applicable_with_repo(claim: &str, repo: &Path) -> bool {
    derive_replay_command(claim, repo).is_some()
}

/// Compute the deterministic set of applicable implemented attacks for a claim.
///
/// Uses the M2 applicability rules; a claim scope that does not mention a
/// verification command (test/check/build/…) does not get Replay.
pub fn applicable_attacks(claim: &str, repo: &Path) -> Vec<AttackKind> {
    let mut kinds: Vec<AttackKind> = IMPLEMENTED_ATTACKS
        .iter()
        .copied()
        .filter(|kind| {
            if *kind == AttackKind::Replay {
                return replay_applicable_with_repo(claim, repo);
            }
            griller_adjudicate::attack_coverage::AttackCoverage::is_attack_applicable(kind, claim)
        })
        .collect();
    kinds.sort_by_key(|k| attack_kind_name(*k));
    kinds
}

/// Legacy: derive a replay command from claim text alone (no repo).
/// Used by `run_attacks` when no repository context is available.
/// DEPRECATED — prefer `derive_replay_command(claim, repo)`.
#[allow(dead_code)]
#[deprecated(note = "use derive_replay_command(claim, repo) instead")]
pub fn derive_replay_command_legacy(claim: &str) -> Option<CommandSpec> {
    // This preserves backward compatibility for callers that don't have
    // a repo path. It falls back to Rust-centric heuristics.
    if !claim_mentions_verification(claim) {
        return None;
    }
    let lower = claim.to_lowercase();
    if lower.contains("clippy") || lower.contains("lint") {
        return Some(safe_command(
            "cargo",
            &["clippy", "--all-targets", "--all-features"],
        ));
    }
    if lower.contains("fmt") || lower.contains("format") {
        return Some(safe_command("cargo", &["fmt", "--all", "--", "--check"]));
    }
    if lower.contains("check") {
        return Some(safe_command("cargo", &["check", "--workspace"]));
    }
    if lower.contains("build") || lower.contains("compil") {
        return Some(safe_command("cargo", &["build"]));
    }
    if lower.contains("test") || lower.contains("pass") || lower.contains("fail") {
        return Some(safe_command("cargo", &["test"]));
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn claim_keywords_are_recognized() {
        assert!(claim_mentions_verification("All tests pass."));
        assert!(claim_mentions_verification("The build succeeded."));
        assert!(claim_mentions_verification(
            "Implemented with clippy clean."
        ));
        assert!(!claim_mentions_verification("Hello world."));
        assert!(!claim_mentions_verification("Just a description."));
    }

    #[test]
    fn rust_replay_is_cargo_test() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"sample\"\n",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "cargo");
        assert_eq!(cmd.args, ["test"]);
    }

    #[test]
    fn go_replay_is_go_test() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/foo\n\ngo 1.21\n",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "go");
        assert_eq!(cmd.args, ["test", "./..."]);
    }

    #[test]
    fn unknown_ecosystem_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("README.md"), "# My Project\n").unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path());
        assert!(cmd.is_none());
    }

    #[test]
    fn no_verification_keyword_returns_none() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"sample\"\n",
        )
        .unwrap();
        let cmd = derive_replay_command("Hello world.", dir.path());
        assert!(cmd.is_none());
    }

    #[test]
    fn rust_takes_priority_over_node() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("Cargo.toml"),
            "[package]\nname = \"sample\"\n",
        )
        .unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            "{\"name\": \"x\", \"scripts\": {\"test\": \"jest\"}}",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "cargo");
    }

    #[test]
    fn node_with_lockfile_uses_correct_manager() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{\"name\": \"x\"}").unwrap();
        std::fs::write(dir.path().join("yarn.lock"), "").unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "yarn");
    }

    #[test]
    fn node_with_pnpm_lockfile_uses_pnpm() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(dir.path().join("package.json"), "{\"name\": \"x\"}").unwrap();
        std::fs::write(dir.path().join("pnpm-lock.yaml"), "").unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "pnpm");
    }

    #[test]
    fn python_with_pytest_is_detected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[tool.pytest.ini_options]\n",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "pytest");
    }

    #[test]
    fn python_without_pytest_falls_back() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("pyproject.toml"),
            "[project]\nname = \"x\"\n",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert_eq!(cmd.executable, "python3");
    }

    #[test]
    fn safe_command_has_disabled_network() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("go.mod"),
            "module example.com/foo\n\ngo 1.21\n",
        )
        .unwrap();
        let cmd = derive_replay_command("All tests pass.", dir.path()).unwrap();
        assert!(matches!(cmd.network_policy, NetworkPolicy::Disabled));
    }

    #[test]
    fn node_test_script_with_shell_meta_is_rejected() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("package.json"),
            "{\"scripts\": {\"test\": \"node -e 'console.log(1)'\"}}",
        )
        .unwrap();
        // The script contains single quotes which is fine, but $ would be rejected.
        let _cmd = derive_replay_command("All tests pass.", dir.path());
        // With $ in script, should fall back to manager default.
        std::fs::write(
            dir.path().join("package.json"),
            "{\"scripts\": {\"test\": \"npm test $FAIL\"}}",
        )
        .unwrap();
        let cmd2 = derive_replay_command("All tests pass.", dir.path());
        // Should use npm fallback since script is unsafe.
        assert!(cmd2.is_some());
        assert_eq!(cmd2.unwrap().executable, "npm");
    }
}
