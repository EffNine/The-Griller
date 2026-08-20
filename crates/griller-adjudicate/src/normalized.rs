//! The normalized claim form and its quality signals.

use serde::{Deserialize, Serialize};

use crate::claim::{ClaimKind, ClaimSource, ClaimTarget};
use crate::predicate::{Polarity, Predicate, Proposition};
use crate::requirement::RequirementProvenance;

/// How the normalized claim was produced.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationMode {
    /// Deterministic rule-based normalization (no LLM involved).
    Deterministic,
    /// An LLM proposed claim structure; the output passed the LLM boundary.
    Llm,
}

/// Structural quality of the normalization.
///
/// This is a structural signal about how much of the claim could be turned
/// into machine structure. It is **not** a probability and it is **not** a
/// truth-confidence; it never influences epistemic status directly.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum NormalizationQuality {
    /// Subject, predicate and target scope were all determined.
    Complete,
    /// A predicate was determined but the target could not be resolved.
    Partial,
    /// No predicate could be determined; matching treats the claim as opaque.
    Unrecognized,
}

/// The result of normalizing a claim.
///
/// The verbatim original text is **always** preserved in [`NormalizedClaim::original`];
/// it is never thrown away, regardless of how the structure was produced.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct NormalizedClaim {
    /// The verbatim original claim text.
    pub original: String,
    /// Who made the claim.
    pub source: ClaimSource,
    /// Resolved kind.
    pub kind: ClaimKind,
    /// Resolved target.
    pub target: ClaimTarget,
    /// The canonical proposition (matching key).
    pub proposition: Proposition,
    /// Structured predicates (the proposition is the canonicalisation of these).
    pub predicates: Vec<Predicate>,
    /// Requirement provenance, when the claim is (or carries) a requirement.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_provenance: Option<RequirementProvenance>,
    /// How the structure was produced.
    pub mode: NormalizationMode,
    /// Structural quality signal.
    pub quality: NormalizationQuality,
}

impl NormalizedClaim {
    /// Build a normalized claim, deriving the predicates from the proposition.
    ///
    /// The parameter count is fixed and explicit: every field is semantically
    /// distinct, so the constructor is kept flat rather than hidden behind a
    /// builder.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        original: String,
        source: ClaimSource,
        kind: ClaimKind,
        target: ClaimTarget,
        proposition: Proposition,
        requirement_provenance: Option<RequirementProvenance>,
        mode: NormalizationMode,
        quality: NormalizationQuality,
    ) -> Self {
        let predicates = vec![proposition.to_predicate()];
        Self {
            original,
            source,
            kind,
            target,
            proposition,
            predicates,
            requirement_provenance,
            mode,
            quality,
        }
    }
}

/// Parse a polarity token string produced by an LLM.
pub fn parse_polarity(value: &str) -> Option<Polarity> {
    match value.trim().to_lowercase().as_str() {
        "positive" | "true" | "affirmed" => Some(Polarity::Positive),
        "negative" | "false" | "negated" => Some(Polarity::Negative),
        _ => None,
    }
}
