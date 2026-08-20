//! Deterministic attack domain model.
//!
//! These types are pure data: content-addressed, serializable, and
//! I/O-free. Execution logic lives in `griller-runtime`.
//!
//! ## Architectural discipline
//!
//! * No filesystem I/O, no process execution, no network access.
//! * Never directly assigns PASS/WARN/BLOCK — those belong to M3.
//! * Outcomes are descriptive, not operational verdicts.
//! * All fields are serializable and hash-reproducible.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::command::{CommandSpec, ExecutionStatus};
use crate::evidence::EvidenceId;
use crate::redaction::RedactionReport;

/// Captures the essential execution result in a self-contained form that
/// `griller-core` can own without depending on `griller-runtime`.
///
/// This is a simplified version of the full execution result; the runtime crate
/// provides the complete `griller_runtime::execution::ExecutionResult` with
/// enforcement metadata and environment fingerprints.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ExecutionCapture {
    /// The command that was executed.
    pub spec: CommandSpec,
    /// Captured stdout, lossily decoded as UTF-8.
    pub stdout: String,
    /// Captured stderr, lossily decoded as UTF-8.
    pub stderr: String,
    /// Process exit code (`None` when the process was killed or failed to spawn).
    pub exit_code: Option<i32>,
    /// Terminal execution status.
    pub status: ExecutionStatus,
    /// Wall-clock duration of the execution in milliseconds.
    pub duration_ms: u64,
    /// What the redaction layer did to stdout/stderr. **Not hashed** (it is
    /// deterministically derivable from the redacted content).
    pub redaction: RedactionReport,
}

/// A stable, content-addressed identifier for an [`AttackSpec`] or
/// [`AttackResult`]. Derived from the BLAKE3 hash of the canonical
/// representation, so the same spec always yields the same id.
///
/// Content-addressed: equal to `content_hash` for all records produced by
/// [`AttackSpec`] construction from the same inputs.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct AttackId(String);

impl AttackId {
    /// Build an id from a content hash.
    pub fn from_content_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }

    /// The underlying hex string (64 BLAKE3 chars).
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for AttackId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// The kind of adversarial attack.
///
/// Deterministic rule-based kinds. LLM-injected kinds are rejected at the
/// validator boundary. Additional kinds may be added when justified, but they
/// must be deterministic and serializable.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackKind {
    /// Replay the claimed verification command.
    Replay,
    /// Inspect coverage: changed files, relevant tests, manifests, requirement
    /// coverage, obvious missing verification.
    Completeness,
    /// Verify presence/absence claims (e.g. "module X was removed").
    PresenceAbsence,
    /// Safe inversion: does reversing the claimed change destroy the claimed
    /// behaviour?
    Invert,
    /// Run verification under stricter conditions (clean env, --locked, etc.).
    Robustness,
    /// Map requirements to evidence.
    RequirementCoverage,
    /// Inspect changed files for obvious unfinished implementation markers.
    StubSweep,
    /// Inspect changed files and commands for obvious dangerous behaviour.
    Safety,
}

impl std::fmt::Display for AttackKind {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Structured specification for an adversarial attack.
///
/// The primary representation is `kind` + `command` (when applicable); the
/// shell is never involved. Arguments are always passed verbatim. The `id`
/// is content-addressed: the same spec always yields the same id.
///
/// `working_state` describes the repository state the attack operates against.
/// `timeout_ms` defaults to 30 seconds. `rationale` is optional human-readable
/// justification for why this attack was chosen.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackSpec {
    /// Stable, content-addressed identifier.
    pub id: AttackId,
    /// The type of attack.
    pub kind: AttackKind,
    /// Human-readable claim this attack targets.
    pub target_claim: String,
    /// Optional command to execute (replay attacks). `None` for
    /// completeness/presence/invert/stub-sweep/safety.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandSpec>,
    /// Working state description (base commit, worktree snapshot, etc.).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_state: Option<PathBuf>,
    /// Timeout in milliseconds. `0` means use default (30s).
    #[serde(default = "default_timeout_ms")]
    pub timeout_ms: u64,
    /// Rationale for why this attack was chosen.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub rationale: Option<String>,
}

fn default_timeout_ms() -> u64 {
    30_000
}

/// The outcome of executing an [`AttackSpec`].
///
/// REFUTED: attack found evidence against the claim.
/// CONFIRMED: attack produced direct supporting evidence.
/// INCONCLUSIVE: attack ran but could not determine the result.
/// NO_OBJECTION: attack executed successfully and found no legitimate objection.
/// NOT_APPLICABLE: attack does not apply to this claim.
/// EXECUTION_FAILED: attack could not execute (missing tool, timeout, etc.).
///
/// EXECUTION_FAILED != REFUTED. A failed attack is not proof the claim is false.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackOutcome {
    /// REFUTED: attack found evidence against the claim.
    Refuted,
    /// CONFIRMED: attack produced direct supporting evidence.
    Confirmed,
    /// INCONCLUSIVE: attack ran but could not determine the result.
    Inconclusive,
    /// NO_OBJECTION: attack executed successfully and found no legitimate objection.
    NoObjection,
    /// NOT_APPLICABLE: attack does not apply to this claim.
    NotApplicable,
    /// EXECUTION_FAILED: attack could not execute (missing tool, timeout, etc.).
    ExecutionFailed,
}

impl std::fmt::Display for AttackOutcome {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

impl AttackOutcome {
    /// True when the outcome is REFUTED or CONFIRMED (a determinate result).
    pub fn is_determinate(self) -> bool {
        matches!(self, AttackOutcome::Refuted | AttackOutcome::Confirmed)
    }

    /// True when the outcome is NO_OBJECTION or CONFIRMED (supporting evidence).
    pub fn is_supporting(self) -> bool {
        matches!(self, AttackOutcome::NoObjection | AttackOutcome::Confirmed)
    }

    /// True when the outcome means the claim cannot be trusted.
    pub fn is_refuted(self) -> bool {
        matches!(self, AttackOutcome::Refuted)
    }

    /// True when the outcome means the claim is supported by the attack.
    pub fn is_confirmed(self) -> bool {
        matches!(self, AttackOutcome::Confirmed)
    }

    /// True when the outcome is INCONCLUSIVE or EXECUTION_FAILED.
    pub fn is_undetermined(self) -> bool {
        matches!(
            self,
            AttackOutcome::Inconclusive
                | AttackOutcome::ExecutionFailed
                | AttackOutcome::NotApplicable
        )
    }
}

/// Working state for an attack spec.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AttackWorkingState {
    /// Run against the repository as-is (base HEAD).
    AsIs,
    /// Run inside an isolated temporary worktree.
    Worktree,
    /// Run against a fresh clone.
    Clone,
}

impl std::fmt::Display for AttackWorkingState {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// Structured result of executing an [`AttackSpec`].
///
/// The explanation is descriptive: it does not assign operational verdicts
/// (PASS/WARN/BLOCK) — those belong to M3.
///
/// ## Outcome semantics
///
/// * `REFUTED` — the attack found evidence contradicting the claim.
/// * `CONFIRMED` — the attack produced direct supporting evidence.
/// * `NO_OBJECTION` — the attack executed successfully with no legitimate
///   objection found. This is positive adversarial evidence.
/// * `INCONCLUSIVE` — the attack ran but could not determine the result
///   (e.g. insufficient coverage, ambiguous environment).
/// * `NOT_APPLICABLE` — the attack does not apply to this claim type.
/// * `EXECUTION_FAILED` — the attack could not execute (missing tool,
///   timeout, infrastructure failure). This is NOT the same as REFUTED.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackResult {
    /// The attack that was executed.
    pub attack: AttackId,
    /// The outcome of the execution.
    pub outcome: AttackOutcome,
    /// Evidence references that support the outcome.
    pub evidence_refs: Vec<EvidenceId>,
    /// Execution capture (self-contained, griller-core owned).
    pub execution: ExecutionCapture,
    /// Descriptive explanation of the result.
    pub explanation: String,
}

impl AttackResult {
    /// True when the outcome is REFUTED or CONFIRMED (a determinate result).
    pub fn is_determinate(self) -> bool {
        matches!(
            self.outcome,
            AttackOutcome::Refuted | AttackOutcome::Confirmed
        )
    }

    /// True when the outcome is NO_OBJECTION or CONFIRMED (supporting evidence).
    pub fn is_supporting(self) -> bool {
        matches!(
            self.outcome,
            AttackOutcome::NoObjection | AttackOutcome::Confirmed
        )
    }

    /// True when the outcome means the claim cannot be trusted.
    pub fn is_refuted(self) -> bool {
        matches!(self.outcome, AttackOutcome::Refuted)
    }

    /// True when the outcome means the claim is supported by the attack.
    pub fn is_confirmed(self) -> bool {
        matches!(self.outcome, AttackOutcome::Confirmed)
    }

    /// True when the outcome is INCONCLUSIVE or EXECUTION_FAILED.
    pub fn is_undetermined(self) -> bool {
        matches!(
            self.outcome,
            AttackOutcome::Inconclusive
                | AttackOutcome::ExecutionFailed
                | AttackOutcome::NotApplicable
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandSpec;

    #[test]
    fn attack_id_from_content_hash() {
        let id = AttackId::from_content_hash("abc123");
        assert_eq!(id.as_str(), "abc123");
    }

    #[test]
    fn attack_outcome_determinate() {
        assert!(AttackOutcome::Refuted.is_determinate());
        assert!(AttackOutcome::Confirmed.is_determinate());
        assert!(!AttackOutcome::Inconclusive.is_determinate());
        assert!(!AttackOutcome::NoObjection.is_determinate());
        assert!(!AttackOutcome::ExecutionFailed.is_determinate());
        assert!(!AttackOutcome::NotApplicable.is_determinate());
    }

    #[test]
    fn attack_outcome_supporting() {
        assert!(AttackOutcome::NoObjection.is_supporting());
        assert!(AttackOutcome::Confirmed.is_supporting());
        assert!(!AttackOutcome::Refuted.is_supporting());
        assert!(!AttackOutcome::Inconclusive.is_supporting());
        assert!(!AttackOutcome::ExecutionFailed.is_supporting());
        assert!(!AttackOutcome::NotApplicable.is_supporting());
    }

    #[test]
    fn attack_outcome_refuted() {
        assert!(AttackOutcome::Refuted.is_refuted());
        assert!(!AttackOutcome::Confirmed.is_refuted());
    }

    #[test]
    fn attack_outcome_confirmed() {
        assert!(AttackOutcome::Confirmed.is_confirmed());
        assert!(!AttackOutcome::Refuted.is_confirmed());
    }

    #[test]
    fn attack_outcome_is_undetermined() {
        assert!(AttackOutcome::Inconclusive.is_undetermined());
        assert!(AttackOutcome::ExecutionFailed.is_undetermined());
        assert!(AttackOutcome::NotApplicable.is_undetermined());
        assert!(!AttackOutcome::Refuted.is_undetermined());
        assert!(!AttackOutcome::Confirmed.is_undetermined());
    }

    #[test]
    fn attack_spec_roundtrip() {
        let spec = AttackSpec {
            id: AttackId::from_content_hash("test-spec"),
            kind: AttackKind::Replay,
            target_claim: "Tests pass".into(),
            command: Some(CommandSpec::new("cargo").with_args(["test"])),
            working_state: Some(PathBuf::from("/tmp/repo")),
            timeout_ms: 60_000,
            rationale: Some("replay claimed test suite".into()),
        };
        let json = serde_json::to_string(&spec).unwrap();
        let back: AttackSpec = serde_json::from_str(&json).unwrap();
        assert_eq!(back.kind, AttackKind::Replay);
        assert_eq!(back.target_claim, "Tests pass");
        assert_eq!(back.timeout_ms, 60_000);
    }

    #[test]
    fn attack_result_roundtrip() {
        let capture = ExecutionCapture {
            spec: CommandSpec::new("cargo").with_args(["test"]),
            stdout: "test ok\n".into(),
            stderr: String::new(),
            exit_code: Some(0),
            status: ExecutionStatus::Completed,
            duration_ms: 1200,
            redaction: RedactionReport::default(),
        };
        let result = AttackResult {
            attack: AttackId::from_content_hash("result-1"),
            outcome: AttackOutcome::NoObjection,
            evidence_refs: Vec::new(),
            execution: capture,
            explanation: "replay succeeded".into(),
        };
        let json = serde_json::to_string(&result).unwrap();
        let back: AttackResult = serde_json::from_str(&json).unwrap();
        assert_eq!(back.outcome, AttackOutcome::NoObjection);
        assert_eq!(back.explanation, "replay succeeded");
        assert_eq!(back.attack.as_str(), "result-1");
        assert_eq!(back.execution.spec.display_line(), "cargo test");
    }

    #[test]
    fn attack_kind_roundtrip() {
        let json = serde_json::to_string(&AttackKind::Replay).unwrap();
        let back: AttackKind = serde_json::from_str(&json).unwrap();
        assert_eq!(back, AttackKind::Replay);
    }

    #[test]
    fn attack_outcome_roundtrip() {
        let outcome = AttackOutcome::Refuted;
        let json = serde_json::to_string(&outcome).unwrap();
        let back: AttackOutcome = serde_json::from_str(&json).unwrap();
        assert_eq!(outcome, back);
    }
}
