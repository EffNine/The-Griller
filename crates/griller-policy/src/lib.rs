//! `griller-policy` — M3 operational policy.
//!
//! M2 answered *"what can we legitimately say about a claim?"* (epistemic
//! status). M3 answers *"given that assessment, what should the system do?"*
//! (operational verdict).
//!
//! M3 converts an adjudicated [`ClaimAssessment`] into a
//! [`Finding`](finding::Finding) with an operational verdict:
//!
//! ```text
//! Claim -> M2 Adjudication -> EpistemicStatus -> M3 Policy
//!       -> Severity / Impact -> Operational Verdict -> PASS / WARN / BLOCK
//! ```
//!
//! ## Boundary
//!
//! * The policy layer is **fully deterministic** and pure: no filesystem, no
//!   network, no LLM, no subprocesses.
//! * LLM output must never directly produce an operational verdict. Policy
//!   consumes only typed, already-adjudicated values.
//! * `EpistemicStatus != Verdict`. M3 never re-derives truth; it decides what
//!   to do with an already-adjudicated claim.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod context;
pub mod decision;
pub mod diva;
pub mod finding;
pub mod policy;
pub mod severity;
pub mod table;
pub mod verdict;

pub use context::{
    context_reason_code, context_reason_message, context_required_action,
    context_required_action_message, context_rule_for, context_severity, context_verdict_for,
    ContextFinding,
};
pub use decision::{Decision, RequiredAction};
pub use diva::DivaRenderer;
pub use finding::{aggregate, Finding, OverallDecision};
pub use policy::{Policy, PolicyContext, PolicyInput, PolicyTable};
pub use severity::{ResolvedSeverity, Severity, SeverityResolver, SeveritySignals, SeveritySource};
pub use verdict::Verdict;
