//! `griller-context` — M4 context adjudication.
//!
//! M4 answers the single killer question of the original wrong-project use
//! case:
//!
//! > Does this task actually belong to this repository / project?
//!
//! The architecture is a **context adjudication** layer, not a verdict
//! engine:
//!
//! ```text
//! Task
//!   -> Task Identity          (normalization; LLM may propose entities)
//!   + Project Identity        (deterministic, from the repository itself)
//!   -> Positive + Negative Evidence (deterministic matcher)
//!   -> Context Assessment     (deterministic ContextStatus)
//!   -> M3 Policy              (verdicts live in griller-policy, never here)
//!   -> PASS / WARN / BLOCK
//! ```
//!
//! ## Boundaries
//!
//! * This crate is **pure**: no filesystem, no process execution, no git, no
//!   LLM client. It depends only on `griller-core`. Project identity
//!   *collection* (the I/O) lives in `griller-runtime`; verdicts live in
//!   `griller-policy`. See `docs/m4-context.md`.
//! * **No embeddings, no vector search, no similarity score as the primary
//!   mechanism.** Relevance is decided by explicit, deterministic matching
//!   rules over typed entities and structured repository signals.
//! * **The LLM is a proposer, never a decider.** An injected normalizer may
//!   propose candidate task entities. It can never propose a
//!   [`ContextStatus`], a severity, or a verdict. The deterministic matcher
//!   decides relevance and the deterministic status rules decide the status.
//! * **Absence is not mismatch.** A missing word, file or directory alone is
//!   never enough to declare [`ContextStatus::Mismatch`]. A BLOCK-worthy
//!   mismatch requires an explicit contradictory project identity (a task
//!   entity that names another project) *plus* supporting absence/relevance
//!   evidence.

#![forbid(unsafe_code)]
#![warn(missing_docs)]

pub mod assess;
pub mod error;
pub mod evidence;
pub mod llm;
pub mod matcher;
pub mod name;
pub mod normalizer;
pub mod project;
pub mod status;
pub mod task;

pub use assess::{assign_context_status, ContextAssessment, ContextAssessor};
pub use error::{ContextError, NormalizationError};
pub use evidence::{ContextEvidence, ContextEvidenceKind, EvidenceDirection, EvidenceStrength};
pub use llm::{parse_llm_task_proposal, LlmEntityProposal, LlmTaskNormalizer, LlmTaskProposal};
pub use matcher::{ContextMatcher, DeterministicContextMatcher};
pub use normalizer::{
    normalize_deterministic, DeterministicTaskNormalizer, FallbackTaskNormalizer,
    TaskContextNormalizer,
};
pub use project::ProjectIdentity;
pub use status::ContextStatus;
pub use task::{EntityKind, NormalizationMode, NormalizationQuality, TaskEntity, TaskIdentity};
