//! The decision produced by the policy layer.

use serde::{Deserialize, Serialize};

use crate::verdict::Verdict;

/// What must happen next for a decision to change.
///
/// The Griller **judges; it does not fix**. Required actions tell the caller
/// what to do; nothing is modified automatically.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequiredAction {
    /// No action required.
    None,
    /// Run the relevant verification and attach the resulting evidence.
    ProvideEvidence,
    /// Make the required execution / evidence mechanism available.
    MakeVerificationAvailable,
    /// Resolve conflicting implementation or evidence.
    ResolveConflict,
    /// Correct the claim or the implementation.
    CorrectClaim,
    /// Provide stronger, unambiguous evidence or explicitly acknowledge the
    /// uncertainty.
    AcknowledgeUncertainty,
    /// Define an explicit policy rule before proceeding.
    DefinePolicyRule,
    /// Stop and confirm the intended project/repository before proceeding
    /// (used by the M4 context bridge for mismatch/ambiguity).
    ConfirmProject,
}

impl RequiredAction {
    /// A stable machine-readable code for the action.
    pub fn code(self) -> &'static str {
        match self {
            RequiredAction::None => "NONE",
            RequiredAction::ProvideEvidence => "PROVIDE_EVIDENCE",
            RequiredAction::MakeVerificationAvailable => "MAKE_VERIFICATION_AVAILABLE",
            RequiredAction::ResolveConflict => "RESOLVE_CONFLICT",
            RequiredAction::CorrectClaim => "CORRECT_CLAIM",
            RequiredAction::AcknowledgeUncertainty => "ACKNOWLEDGE_UNCERTAINTY",
            RequiredAction::DefinePolicyRule => "DEFINE_POLICY_RULE",
            RequiredAction::ConfirmProject => "CONFIRM_PROJECT",
        }
    }
}

/// A single operational decision for one claim.
///
/// Fully machine-readable: downstream systems must consume `reason_code`,
/// `required_action` and `required_action_message`, never parse prose.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Decision {
    /// The operational verdict.
    pub verdict: Verdict,
    /// Stable, machine-readable reason code, e.g. `UNSUPPORTED_COMPLETION_BLOCK`.
    pub reason_code: String,
    /// Deterministic, human-readable reason. Never contains untrusted claim
    /// text.
    pub reason: String,
    /// The required action.
    pub required_action: RequiredAction,
    /// Human-readable explanation of the required action.
    pub required_action_message: String,
}
