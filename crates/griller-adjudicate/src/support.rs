//! The SupportMatrix: the full relationship between a claim and every piece of
//! evidence considered.
//!
//! The matrix is deterministic: it is ordered by evidence id, de-duplicated,
//! and every entry is produced by the same rules from the same inputs. Two
//! assessments of the same claim against the same evidence set always produce
//! an identical matrix.

use griller_core::evidence::EvidenceId;
use serde::{Deserialize, Serialize};

use crate::predicate::Proposition;
use crate::relationship::{EvidenceMatch, EvidenceRelationship};

/// A deterministic collection of evidence-to-claim relationships.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SupportMatrix {
    /// The canonical proposition the matrix is about.
    pub proposition: Proposition,
    /// One entry per considered evidence record, ordered by evidence id and
    /// de-duplicated.
    pub matches: Vec<EvidenceMatch>,
}

impl SupportMatrix {
    /// Build a matrix, normalizing ordering (sort by evidence id) and removing
    /// duplicate records.
    pub fn new(proposition: Proposition, mut matches: Vec<EvidenceMatch>) -> Self {
        matches.sort_by(|a, b| a.evidence_id.as_str().cmp(b.evidence_id.as_str()));
        matches.dedup_by(|a, b| a.evidence_id == b.evidence_id);
        Self {
            proposition,
            matches,
        }
    }

    /// Evidence that supports the claim.
    pub fn supports(&self) -> Vec<&EvidenceMatch> {
        self.filtered(EvidenceRelationship::Supports)
    }

    /// Evidence that contradicts the claim.
    pub fn contradicts(&self) -> Vec<&EvidenceMatch> {
        self.filtered(EvidenceRelationship::Contradicts)
    }

    /// Evidence that does not pertain to the claim.
    pub fn irrelevant(&self) -> Vec<&EvidenceMatch> {
        self.filtered(EvidenceRelationship::Irrelevant)
    }

    /// Evidence that is relevant but insufficient to adjudicate the claim.
    pub fn insufficient(&self) -> Vec<&EvidenceMatch> {
        self.filtered(EvidenceRelationship::Insufficient)
    }

    /// Evidence that is relevant but could not be determined by the environment.
    pub fn unverifiable(&self) -> Vec<&EvidenceMatch> {
        self.filtered(EvidenceRelationship::Unverifiable)
    }

    /// Evidence that is relevant to the claim in any way.
    pub fn relevant(&self) -> Vec<&EvidenceMatch> {
        self.matches.iter().filter(|m| m.relevant).collect()
    }

    fn filtered(&self, relationship: EvidenceRelationship) -> Vec<&EvidenceMatch> {
        self.matches
            .iter()
            .filter(|m| m.relationship == relationship)
            .collect()
    }

    /// The ids of all evidence that supports the claim, sorted.
    pub fn supporting_ids(&self) -> Vec<EvidenceId> {
        self.supports()
            .iter()
            .map(|m| m.evidence_id.clone())
            .collect()
    }

    /// The ids of all evidence that contradicts the claim, sorted.
    pub fn contradicting_ids(&self) -> Vec<EvidenceId> {
        self.contradicts()
            .iter()
            .map(|m| m.evidence_id.clone())
            .collect()
    }
}
