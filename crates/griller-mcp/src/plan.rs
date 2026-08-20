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

use griller_core::attack::AttackKind;
use griller_core::command::{CommandSpec, EnvironmentPolicy, NetworkPolicy};
use griller_runtime::execution::default_env_allowlist;

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

/// Derive the safe replay command implied by a claim.
///
/// The command is derived from the claim text by the Griller itself and never
/// taken from the caller. Returns `None` when the claim does not imply a
/// verification command; in that case Replay is not applicable.
///
/// The command inherits only the allow-listed environment keys (so `cargo`
/// can be found via `PATH`) and has network access disabled.
fn safe_command(executable: &str, args: &[&str]) -> CommandSpec {
    CommandSpec::new(executable)
        .with_args(args.iter().copied())
        .with_environment_policy(EnvironmentPolicy::InheritAllowlist(default_env_allowlist()))
        .with_network_policy(NetworkPolicy::Disabled)
}

/// Derive the safe replay command implied by a claim.
///
/// The command is derived from the claim text by the Griller itself and never
/// taken from the caller. Returns `None` when the claim does not imply a
/// verification command; in that case Replay is not applicable.
pub fn derive_replay_command(claim: &str) -> Option<CommandSpec> {
    let lower = claim.to_lowercase();
    if lower.contains("test") || lower.contains("pass") || lower.contains("fail") {
        Some(safe_command("cargo", &["test"]))
    } else if lower.contains("clippy") || lower.contains("lint") {
        Some(safe_command(
            "cargo",
            &["clippy", "--all-targets", "--all-features"],
        ))
    } else if lower.contains("fmt") || lower.contains("format") {
        Some(safe_command("cargo", &["fmt", "--all", "--", "--check"]))
    } else if lower.contains("check") {
        Some(safe_command("cargo", &["check", "--workspace"]))
    } else if lower.contains("build") || lower.contains("compil") {
        Some(safe_command("cargo", &["build"]))
    } else {
        None
    }
}

/// Whether Replay can run for this claim (a safe command could be derived).
pub fn replay_applicable(claim: &str) -> bool {
    derive_replay_command(claim).is_some()
}

/// Compute the deterministic set of applicable implemented attacks for a claim.
///
/// Uses the M2 applicability rules; a claim scope that does not mention a
/// verification command (test/check/build/…) does not get Replay.
pub fn applicable_attacks(claim: &str) -> Vec<AttackKind> {
    let mut kinds: Vec<AttackKind> = IMPLEMENTED_ATTACKS
        .iter()
        .copied()
        .filter(|kind| {
            if *kind == AttackKind::Replay {
                return replay_applicable(claim);
            }
            griller_adjudicate::attack_coverage::AttackCoverage::is_attack_applicable(kind, claim)
        })
        .collect();
    kinds.sort_by_key(|k| attack_kind_name(*k));
    kinds
}
