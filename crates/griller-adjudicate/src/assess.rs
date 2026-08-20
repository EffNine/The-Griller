//! The adjudication pipeline and its result type.
//!
//! The pipeline is deliberately small and inspectable:
//!
//! ```text
//! ClaimInput + Evidence[] + (optional ProjectContext)
//!     -> normalize          (ClaimNormalizer)
//!     -> match              (EvidenceMatcher -> SupportMatrix)
//!     -> contradict         (ContradictionChecker)
//!     -> assign status      (pure function -> EpistemicStatus)
//! ```
//!
//! Every stage is replaceable via a trait, and the default configuration is
//! fully deterministic and works without an LLM.

use griller_core::context::ProjectContext;
use griller_core::evidence::{Evidence, EvidenceId};
use serde::{Deserialize, Serialize};

use crate::claim::{Claim, ClaimInput};
use crate::contradiction::{
    Contradiction, ContradictionChecker, DeterministicContradictionChecker,
};
use crate::error::AdjudicationError;
use crate::matcher::{DeterministicEvidenceMatcher, EvidenceMatcher};
use crate::normalized::NormalizedClaim;
use crate::normalizer::{ClaimNormalizer, DeterministicNormalizer};
use crate::status::{assign_status, EpistemicStatus, InsufficiencyNote, Reason, StatusDecision};
use crate::support::SupportMatrix;

/// The complete, machine-readable result of assessing one claim.
///
/// Deterministic: the same claim + same evidence + same rules always produce
/// an identical [`ClaimAssessment`] (byte-for-byte identical canonical JSON).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimAssessment {
    /// The input claim (verbatim text preserved).
    pub claim: Claim,
    /// The normalized claim.
    pub normalized: NormalizedClaim,
    /// The full evidence relationship matrix.
    pub matrix: SupportMatrix,
    /// Detected structural contradictions.
    pub contradictions: Vec<Contradiction>,
    /// The assigned epistemic status.
    pub status: EpistemicStatus,
    /// Deterministic reasons for the status.
    pub reasons: Vec<Reason>,
    /// Supporting evidence ids (sorted).
    pub supporting_evidence: Vec<EvidenceId>,
    /// Contradicting evidence ids (sorted).
    pub contradicting_evidence: Vec<EvidenceId>,
    /// Notes on evidence that was insufficient or unverifiable.
    pub insufficient: Vec<InsufficiencyNote>,
}

/// The adjudication pipeline.
///
/// This is a thin wiring of three small, replaceable components — not an
/// "engine". The default configuration is deterministic and LLM-free.
pub struct Adjudicator {
    normalizer: Box<dyn ClaimNormalizer>,
    matcher: Box<dyn EvidenceMatcher>,
    contradiction_checker: Box<dyn ContradictionChecker>,
}

impl Default for Adjudicator {
    fn default() -> Self {
        Self::new(
            Box::new(DeterministicNormalizer),
            Box::new(DeterministicEvidenceMatcher),
            Box::new(DeterministicContradictionChecker),
        )
    }
}

impl Adjudicator {
    /// Build an adjudicator with explicit stage implementations.
    pub fn new(
        normalizer: Box<dyn ClaimNormalizer>,
        matcher: Box<dyn EvidenceMatcher>,
        contradiction_checker: Box<dyn ContradictionChecker>,
    ) -> Self {
        Self {
            normalizer,
            matcher,
            contradiction_checker,
        }
    }

    /// Replace the normalizer.
    pub fn with_normalizer(mut self, normalizer: Box<dyn ClaimNormalizer>) -> Self {
        self.normalizer = normalizer;
        self
    }

    /// Replace the matcher.
    pub fn with_matcher(mut self, matcher: Box<dyn EvidenceMatcher>) -> Self {
        self.matcher = matcher;
        self
    }

    /// Replace the contradiction checker.
    pub fn with_contradiction_checker(mut self, checker: Box<dyn ContradictionChecker>) -> Self {
        self.contradiction_checker = checker;
        self
    }

    /// The deterministic default adjudicator.
    pub fn deterministic() -> Self {
        Self::default()
    }

    /// Assess a claim against an evidence set.
    ///
    /// `project_context` is optional; the M2 matcher uses it only for reason
    /// labeling and never for status changes.
    pub fn assess(
        &self,
        input: &ClaimInput,
        evidence: &[Evidence],
        project_context: Option<&ProjectContext>,
    ) -> Result<ClaimAssessment, AdjudicationError> {
        let normalized = self.normalizer.normalize(input)?;

        let matches = self
            .matcher
            .match_evidence(&normalized, project_context, evidence);
        let matrix = SupportMatrix::new(normalized.proposition.clone(), matches);

        let contradictions = self
            .contradiction_checker
            .check(&matrix, &normalized, evidence);

        let decision: StatusDecision = assign_status(&matrix, &contradictions, &normalized);

        let claim = Claim::new(
            input.source,
            normalized.kind,
            normalized.original.clone(),
            normalized.target.clone(),
            normalized.predicates.clone(),
            input.requirement_provenance,
        );

        Ok(ClaimAssessment {
            claim,
            normalized,
            matrix,
            contradictions,
            status: decision.status,
            reasons: decision.reasons,
            supporting_evidence: decision.supporting_evidence,
            contradicting_evidence: decision.contradicting_evidence,
            insufficient: decision.insufficient,
        })
    }

    /// Normalize a claim without assessing it (useful for inspection).
    pub fn normalize(&self, input: &ClaimInput) -> Result<NormalizedClaim, AdjudicationError> {
        Ok(self.normalizer.normalize(input)?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::ClaimSource;

    #[test]
    fn adjudicator_defaults_are_deterministic_and_llm_free() {
        let a = Adjudicator::default();
        let b = Adjudicator::default();
        let input = ClaimInput::new("tests pass", ClaimSource::User);
        let out_a = a.assess(&input, &[], None).unwrap();
        let out_b = b.assess(&input, &[], None).unwrap();
        assert_eq!(out_a, out_b);
        assert_eq!(out_a.status, EpistemicStatus::Unsupported);
    }
}
