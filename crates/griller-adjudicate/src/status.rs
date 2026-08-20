//! Epistemic status semantics and deterministic assignment.
//!
//! The epistemic status is what M2 can legitimately say about the truth of a
//! claim given the evidence. It is **not** an operational verdict: M3 maps
//! `EpistemicStatus + ClaimKind + Severity + Policy` to PASS/WARN/BLOCK, and no
//! M2 code assigns a verdict.

use griller_core::evidence::EvidenceId;
use serde::{Deserialize, Serialize};

use crate::contradiction::{Contradiction, ContradictionKind};
use crate::normalized::NormalizedClaim;
use crate::relationship::{EvidenceMatch, EvidenceRelationship, EvidenceStrength};
use crate::support::SupportMatrix;

/// What the evidence allows us to say about a claim.
///
/// `#[non_exhaustive]`: downstream crates (e.g. the M3 policy table) must
/// handle future statuses explicitly and must never silently trust an
/// unlisted status.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum EpistemicStatus {
    /// The evidence provides sufficiently direct grounds to say the claim is
    /// false. **Not** a synonym for failed collection, missing evidence,
    /// timeout, or unavailable tools.
    False,
    /// The claim is meaningful and testable, but no sufficient supporting
    /// evidence exists.
    Unsupported,
    /// Evidence exists but is conflicting, incomplete, ambiguous, or
    /// insufficient to resolve the claim cleanly.
    Uncertain,
    /// Relevant direct evidence conflicts with the claim.
    Contradicted,
    /// Strong direct evidence supports the claim and there is no known
    /// contradictory evidence.
    Verified,
    /// Reserved for the M5 adversarial attack pipeline. **Unreachable from M2
    /// adjudication**; it is never produced by the M2 status rules.
    AdversariallyVerified,
    /// The available environment/evidence cannot meaningfully determine the
    /// claim (tool missing, execution unavailable, timeout).
    Unverifiable,
}

/// A deterministic reason for an assigned status.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reason {
    /// A structural contradiction was detected.
    ContradictionDetected {
        /// The kind of structural conflict.
        kind: ContradictionKind,
    },
    /// Direct supporting evidence exists.
    DirectSupport(EvidenceId),
    /// Direct contradicting evidence exists.
    DirectContradiction(EvidenceId),
    /// Only moderate (indirect) support exists; the claim is unresolved.
    ModerateSupportOnly,
    /// Only moderate (indirect) contradiction exists; the claim is unresolved.
    ModerateContradictionOnly,
    /// A presence claim was decisively refuted by the environment.
    PresenceRefuted(EvidenceId),
    /// Relevant evidence both supports and contradicts the claim.
    MixedSupportsAndContradicts,
    /// No evidence was provided.
    NoRelevantEvidence,
    /// Only agent narration is available.
    AgentNarrationOnly,
    /// Only irrelevant evidence was provided.
    IrrelevantEvidenceOnly,
    /// A relevant execution could not be performed.
    RelevantExecutionUnavailable(EvidenceId),
}

/// A note explaining why a piece of evidence could not resolve the claim.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct InsufficiencyNote {
    /// The evidence record id.
    pub evidence_id: EvidenceId,
    /// Whether it was `Insufficient` or `Unverifiable`.
    pub relationship: EvidenceRelationship,
    /// Deterministic explanation.
    pub reason: String,
}

/// The outcome of deterministic status assignment.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StatusDecision {
    /// The assigned status.
    pub status: EpistemicStatus,
    /// Deterministic reasons.
    pub reasons: Vec<Reason>,
    /// Ids of supporting evidence, sorted.
    pub supporting_evidence: Vec<EvidenceId>,
    /// Ids of contradicting evidence, sorted.
    pub contradicting_evidence: Vec<EvidenceId>,
    /// Notes on insufficient / unverifiable evidence.
    pub insufficient: Vec<InsufficiencyNote>,
}

/// Assign an epistemic status deterministically from a support matrix and the
/// detected contradictions.
///
/// ## Documented rules
///
/// 1. Any detected structural contradiction => `CONTRADICTED`.
/// 2. Supporting **and** contradicting direct evidence coexist =>
///    `CONTRADICTED` (a direct contradiction wins over partial support).
/// 3. Only direct contradictory evidence:
///    - a presence claim whose refutation is decisive (all contradictions
///      direct, no unverifiable/insufficient noise) => `FALSE`;
///    - otherwise => `CONTRADICTED`.
/// 4. Only supporting evidence: direct support => `VERIFIED`; only moderate
///    (indirect) support => `UNCERTAIN`.
/// 5. No support and no contradiction:
///    - relevant executions that could not run => `UNVERIFIABLE`;
///    - otherwise => `UNSUPPORTED`.
///
/// `FALSE` is intentionally narrow in M2: most "false-sounding" outcomes are
/// `CONTRADICTED` (see the golden test for "tests pass" + failing run).
pub fn assign_status(
    matrix: &SupportMatrix,
    contradictions: &[Contradiction],
    claim: &NormalizedClaim,
) -> StatusDecision {
    let supporting = matrix.supporting_ids();
    let contradicting = matrix.contradicting_ids();
    let insufficient = insufficiency_notes(matrix);

    if !contradictions.is_empty() {
        let reasons: Vec<Reason> = contradictions
            .iter()
            .map(|c| Reason::ContradictionDetected { kind: c.kind })
            .collect();
        return StatusDecision {
            status: EpistemicStatus::Contradicted,
            reasons,
            supporting_evidence: supporting,
            contradicting_evidence: contradicting,
            insufficient,
        };
    }

    let supports = matrix.supports();
    let contradicts = matrix.contradicts();
    let unverifiable = matrix.unverifiable();
    let insufficient_matches = matrix.insufficient();

    match (supports.is_empty(), contradicts.is_empty()) {
        // Both support and contradiction exist (rule 2).
        (false, false) => StatusDecision {
            status: EpistemicStatus::Contradicted,
            reasons: vec![Reason::MixedSupportsAndContradicts],
            supporting_evidence: supporting,
            contradicting_evidence: contradicting,
            insufficient,
        },
        // Only contradiction exists (rule 3).
        (true, false) => {
            let has_direct = contradicts.iter().any(is_direct);
            let decisive_presence_refutation = is_presence_claim(claim)
                && contradicts.iter().all(is_direct)
                && unverifiable.is_empty()
                && insufficient_matches.is_empty();
            if decisive_presence_refutation {
                let first = contradicts[0].evidence_id.clone();
                StatusDecision {
                    status: EpistemicStatus::False,
                    reasons: vec![Reason::PresenceRefuted(first)],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else if has_direct {
                let first = contradicts[0].evidence_id.clone();
                StatusDecision {
                    status: EpistemicStatus::Contradicted,
                    reasons: vec![Reason::DirectContradiction(first)],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else {
                StatusDecision {
                    status: EpistemicStatus::Uncertain,
                    reasons: vec![Reason::ModerateContradictionOnly],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            }
        }
        // Only support exists (rule 4).
        (false, true) => {
            if supports.iter().any(is_direct) {
                let first = supports[0].evidence_id.clone();
                StatusDecision {
                    status: EpistemicStatus::Verified,
                    reasons: vec![Reason::DirectSupport(first)],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else {
                StatusDecision {
                    status: EpistemicStatus::Uncertain,
                    reasons: vec![Reason::ModerateSupportOnly],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            }
        }
        // Nothing supports and nothing contradicts (rule 5).
        (true, true) => {
            if !unverifiable.is_empty() {
                let reasons: Vec<Reason> = unverifiable
                    .iter()
                    .map(|m| Reason::RelevantExecutionUnavailable(m.evidence_id.clone()))
                    .collect();
                StatusDecision {
                    status: EpistemicStatus::Unverifiable,
                    reasons,
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else if !insufficient_matches.is_empty() {
                let agent_only = insufficient_matches.iter().all(|m| m.depends_on_agent());
                let reasons = if agent_only {
                    vec![Reason::AgentNarrationOnly]
                } else {
                    vec![Reason::NoRelevantEvidence]
                };
                StatusDecision {
                    status: EpistemicStatus::Unsupported,
                    reasons,
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else if !matrix.matches.is_empty()
                && matrix
                    .matches
                    .iter()
                    .all(|m| m.relationship == EvidenceRelationship::Irrelevant)
            {
                StatusDecision {
                    status: EpistemicStatus::Unsupported,
                    reasons: vec![Reason::IrrelevantEvidenceOnly],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            } else {
                StatusDecision {
                    status: EpistemicStatus::Unsupported,
                    reasons: vec![Reason::NoRelevantEvidence],
                    supporting_evidence: supporting,
                    contradicting_evidence: contradicting,
                    insufficient,
                }
            }
        }
    }
}

fn is_direct(m: &&EvidenceMatch) -> bool {
    matches!(
        m.strength,
        EvidenceStrength::Direct | EvidenceStrength::Strong
    )
}

fn is_presence_claim(claim: &NormalizedClaim) -> bool {
    matches!(
        crate::matcher::claim_class(claim),
        crate::matcher::ClaimClass::EntityExists | crate::matcher::ClaimClass::EntityAbsent
    )
}

fn insufficiency_notes(matrix: &SupportMatrix) -> Vec<InsufficiencyNote> {
    let mut notes: Vec<InsufficiencyNote> = matrix
        .matches
        .iter()
        .filter(|m| {
            matches!(
                m.relationship,
                EvidenceRelationship::Insufficient | EvidenceRelationship::Unverifiable
            )
        })
        .map(|m| InsufficiencyNote {
            evidence_id: m.evidence_id.clone(),
            relationship: m.relationship,
            reason: m.reason.clone(),
        })
        .collect();
    notes.sort_by(|a, b| a.evidence_id.as_str().cmp(b.evidence_id.as_str()));
    notes
}
