//! `griller-self-grill` — M7 deterministic self-audit.
//!
//! The Griller must be able to grill its OWN reasoning/evidence pipeline.
//! This crate turns the limited `self_grill` stub into a real, deterministic
//! self-audit mechanism.
//!
//! ## Core invariant
//!
//! Self-grill **never** mutates the original decision. It produces a separate
//! audit result alongside an immutable snapshot of the original:
//!
//! ```text
//! OriginalDecision + SelfGrillResult
//! ```
//!
//! ## Architecture
//!
//! ```text
//! SelfGrillInput (claim + assessment + decision + context + attacks + evidence)
//!     -> run checks (SelfGrillRunner)
//!     -> SelfGrillResult (outcome + coverage + findings + issues)
//! ```
//!
//! ## Checks
//!
//! 1. evidence_completeness — every reference resolves, no duplicates masquerading as independent
//! 2. evidence_independence — no double-counting of same-source evidence
//! 3. trust_tier — tier-0 agent claims cannot independently create VERIFIED/ADVERSARIALLY_VERIFIED
//! 4. claim_scope — evidence must actually cover the claim scope
//! 5. context_consistency — re-checks M4; MISMATCH + PASS => REFUTED
//! 6. attack_coverage — ADVERSARIALLY_VERIFIED requires strict M5 conditions
//! 7. contradictory_evidence — search for structural contradictions
//! 8. execution_validity — provenance, exit codes, enforcement truth
//! 9. policy_consistency — recompute M3 policy from structured inputs
//! 10. reason_action_consistency — reason_code matches status+kind+verdict
//! 11. required_action_consistency — BLOCK => meaningful remediation exists
//! 12. evidence_hash_integrity — recompute and verify content hashes
//! 13. environment_reproducibility — provenance carries limitations correctly
//! 14. redaction_integrity — persisted output passed through redaction boundary
//! 15. context_claim_consistency — context identity and claim target agree
//! 16. attack_target_consistency — attacks actually targeted the claim
//!
//! ## Outcomes
//!
//! - `HOLDS`: original decision survives the self-audit
//! - `WEAKENED`: decision remains possible but confidence/evidence strength reduced
//! - `REFUTED`: self-audit finds a legitimate contradiction in the original assessment
//! - `INCONCLUSIVE`: self-audit could not determine whether the original is sound
//! - `NOT_APPLICABLE`: no meaningful self-audit could be performed
//!
//! ## Aggregation
//!
//! Priority: REFUTED > WEAKENED > INCONCLUSIVE > HOLDS > NOT_APPLICABLE
//! Individual results are preserved; the overall outcome is the worst hit.
//!
//! ## Non-recursion
//!
//! Self-grill runs exactly one pass. It does not call itself.
//!
//! ## LLM boundaries
//!
//! Self-grill is fully deterministic. No LLM is consulted for outcomes.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod checks;
pub mod domain;
pub mod runner;

pub use domain::*;
pub use runner::{self_grill, SelfGrillRunner};
