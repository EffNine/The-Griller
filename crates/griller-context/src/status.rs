//! Context status semantics.
//!
//! M4's status describes **context** — how well the task matches this
//! repository. It is deliberately distinct from M3's operational
//! PASS/WARN/BLOCK verdicts: M4 says *"this task appears strongly mismatched
//! with this repository"*; M3 decides *"that mismatch is BLOCK-worthy."*
//!
//! ## Statuses and their exact semantics
//!
//! | Status | Meaning | Typical policy |
//! | --- | --- | --- |
//! | `MATCHED` | Strong positive evidence (explicit project/manifest/remote identity match) with no explicit mismatch. The task demonstrably belongs. | PASS |
//! | `LIKELY_MATCH` | Substantial positive evidence (existing paths/modules/symbols, terminology overlap) without a definitive identity match. Probably belongs. | PASS |
//! | `AMBIGUOUS` | Conflicting signals of comparable strength: the task names a foreign project that has some supporting presence here, or it names both this repository and a foreign project. Cannot be resolved deterministically. | WARN |
//! | `WEAK_RELEVANCE` | Only weak token-level overlap; no strong positive or negative evidence. Not a match, but not a mismatch either. | WARN |
//! | `MISMATCH` | Explicit contradictory project identity: the task names a different project/repository with **no supporting presence** in this repository. The only status that can drive a BLOCK. | BLOCK |
//! | `INSUFFICIENT_CONTEXT` | The task (or repository) provides too little signal to judge (e.g. a generic task). Must never block. | WARN |
//!
//! ## The core invariant
//!
//! **Absence is not mismatch.** A missing word, file or directory alone is
//! never sufficient for [`ContextStatus::Mismatch`]. MISMATCH requires an
//! explicit contradictory project identity (a task entity naming another
//! project) plus supporting absence/relevance evidence. See
//! [`crate::assess::assign_context_status`].

use serde::{Deserialize, Serialize};

/// The deterministic context status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContextStatus {
    /// Strong positive evidence; the task demonstrably belongs.
    Matched,
    /// Substantial positive evidence; the task probably belongs.
    LikelyMatch,
    /// Conflicting signals of comparable strength.
    Ambiguous,
    /// Only weak token-level relevance.
    WeakRelevance,
    /// Explicit contradictory project identity.
    Mismatch,
    /// Too little signal to judge.
    InsufficientContext,
}

impl ContextStatus {
    /// Stable machine-readable code.
    pub fn code(self) -> &'static str {
        match self {
            ContextStatus::Matched => "matched",
            ContextStatus::LikelyMatch => "likely_match",
            ContextStatus::Ambiguous => "ambiguous",
            ContextStatus::WeakRelevance => "weak_relevance",
            ContextStatus::Mismatch => "mismatch",
            ContextStatus::InsufficientContext => "insufficient_context",
        }
    }

    /// True for the block-worthy status.
    pub fn is_mismatch(self) -> bool {
        matches!(self, ContextStatus::Mismatch)
    }
}

impl std::fmt::Display for ContextStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
