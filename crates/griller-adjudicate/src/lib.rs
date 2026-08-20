//! `griller-adjudicate` — M2 claim adjudication.
//!
//! This crate answers the central M2 question:
//!
//! > Given a claim and the evidence available, what can we legitimately say
//! > about the truth/status of that claim?
//!
//! It does **not** answer "should we block the agent?" — that belongs to M3.
//! No code in this crate assigns PASS/WARN/BLOCK.
//!
//! ## Architectural principle
//!
//! **LLM proposes. Evidence disposes.**
//!
//! * An LLM (or the deterministic normalizer) may propose claim structure:
//!   predicates, targets, kinds. See [`normalizer`] and [`llm`].
//! * The deterministic adjudication layer decides epistemic status from
//!   evidence. See [`matcher`], [`contradiction`], [`status`].
//! * All LLM output is untrusted structured input. [`NormalizedClaim`] cannot
//!   even represent an epistemic status, so a normalizer can never inject a
//!   verdict.
//!
//! This crate is pure: it performs no I/O and depends only on `griller-core`.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assess;
pub mod attack_coverage;
pub mod claim;
pub mod contradiction;
pub mod error;
pub mod llm;
pub mod matcher;
pub mod normalized;
pub mod normalizer;
pub mod predicate;
pub mod relationship;
pub mod requirement;
pub mod status;
pub mod support;

pub use assess::{Adjudicator, ClaimAssessment};
pub use attack_coverage::AttackCoverage;
pub use claim::{Claim, ClaimId, ClaimInput, ClaimKind, ClaimSource, ClaimTarget, TargetScope};
pub use contradiction::{Contradiction, ContradictionChecker, ContradictionKind};
pub use error::{AdjudicationError, NormalizationError};
pub use llm::{LlmClaimProposal, LlmNormalizer};
pub use matcher::{claim_class, ClaimClass, DeterministicEvidenceMatcher, EvidenceMatcher};
pub use normalized::{parse_polarity, NormalizationMode, NormalizationQuality, NormalizedClaim};
pub use normalizer::{ClaimNormalizer, DeterministicNormalizer, FallbackNormalizer};
pub use predicate::{Polarity, Predicate, Proposition};
pub use relationship::{EvidenceMatch, EvidenceQuality, EvidenceRelationship, EvidenceStrength};
pub use requirement::{Requirement, RequirementProvenance};
pub use status::{EpistemicStatus, InsufficiencyNote, Reason};
pub use support::SupportMatrix;
