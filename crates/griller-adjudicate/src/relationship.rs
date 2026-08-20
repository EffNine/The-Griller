//! Evidence-to-claim relationships and evidence assessment signals.
//!
//! These are structural assessments, deliberately *not* calibrated
//! probabilities. [`EvidenceStrength`] categories are ordered, documented
//! proximity measures; [`EvidenceQuality`] records provenance-derived facts.

use griller_core::evidence::{EvidenceId, EvidenceKind};
use serde::{Deserialize, Serialize};

/// How a piece of evidence relates to a claim.
///
/// These relationships are **not** collapsed into truth directly: the
/// adjudication layer decides the epistemic status from the full matrix.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceRelationship {
    /// The environment directly demonstrated something consistent with the claim.
    Supports,
    /// The environment directly demonstrated something conflicting with the claim.
    Contradicts,
    /// The evidence does not pertain to the claim (different domain).
    Irrelevant,
    /// The evidence pertains to the claim but is not sufficient to adjudicate
    /// it (e.g. agent narration, a broad run that cannot isolate a named target).
    Insufficient,
    /// The evidence pertains to the claim but the environment could not
    /// determine it (timeout, missing tool/dependency).
    Unverifiable,
}

/// Proximity of a piece of evidence to the claim's proposition.
///
/// **Not** a calibrated probability. The categories are a documented ordering
/// of how directly the environment demonstrated the proposition. In M2 only
/// `None`, `Moderate` and `Direct` are produced; `Weak` and `Strong` are
/// reserved for later, calibrated rules and are never emitted by the M2
/// matcher.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceStrength {
    /// No informational value.
    None,
    /// Weak proximity (reserved; not emitted by the M2 matcher).
    Weak,
    /// Relevant but indirect demonstration (e.g. symbol presence inferred by
    /// text search).
    Moderate,
    /// Strong proximity (reserved; not emitted by the M2 matcher).
    Strong,
    /// The environment directly demonstrated the proposition (completed exit
    /// code, direct presence observation).
    Direct,
}

/// Provenance-derived quality facts for one evidence match.
///
/// These are plain boolean facts, not scores.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct EvidenceQuality {
    /// Whether the environment directly observed the content.
    pub direct_observation: bool,
    /// Whether the capture is expected to reproduce deterministically.
    pub reproducible: bool,
    /// Whether the evidence is independent of agent narration.
    pub independent: bool,
    /// Whether the evidence pertains to the claim.
    pub relevant: bool,
}

/// The result of matching one evidence record against a claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct EvidenceMatch {
    /// The matched evidence record id.
    pub evidence_id: EvidenceId,
    /// Kind of the matched evidence.
    pub evidence_kind: EvidenceKind,
    /// Relationship to the claim.
    pub relationship: EvidenceRelationship,
    /// Proximity of the evidence to the proposition.
    pub strength: EvidenceStrength,
    /// Provenance-derived quality facts.
    pub quality: EvidenceQuality,
    /// Whether this evidence is relevant (not `Irrelevant`).
    pub relevant: bool,
    /// Human-readable, deterministic explanation of the match.
    pub reason: String,
}

impl EvidenceMatch {
    /// True when the evidence is agent narration or otherwise derived from an
    /// agent's claim and therefore never independent.
    pub fn depends_on_agent(&self) -> bool {
        !self.quality.independent
    }
}
