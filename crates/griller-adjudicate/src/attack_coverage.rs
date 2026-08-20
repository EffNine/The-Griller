//! Attack coverage tracking and aggregation.
//!
//! Tracks which attacks were applicable, executed, successful, inconclusive,
//! failed, or not applicable for a given claim. This is deterministic and
//! replayable: the same claim + same evidence + same attack spec corpus always
//! produces the same coverage report.
//!
//! ## Coverage model
//!
//! ```text
//! AttackCoverage {
//!     applicable:     total attacks that could apply to this claim,
//!     implemented:    how many of those are actually implemented,
//!     executed:       how many were actually run,
//!     successful:     how many produced supporting outcomes,
//!     refuted:        how many produced refuting outcomes,
//!     inconclusive:   how many produced INCONCLUSIVE,
//!     failed:         how many had EXECUTION_FAILED,
//!     not_applicable: how many were NOT_APPLICABLE,
//!     unimplemented:  how many applicable attacks are NOT_YET_IMPLEMENTED,
//! }
//! ```
//!
//! ## ADVERSARIALLY_VERIFIED criteria
//!
//! A claim is ADVERSARIALLY_VERIFIED ONLY when ALL of the following hold:
//!
//! 1. `successful > 0` — at least one attack produced supporting evidence
//! 2. `refuted == 0` — no attack found contradictory evidence
//! 3. `inconclusive == 0` — no attack produced an inconclusive result
//! 4. `failed == 0` — no attack failed to execute
//! 5. `unimplemented == 0` — no applicable attack remains unimplemented
//! 6. `applicable == executed` — all applicable attacks were executed
//!
//! A stubbed attack returning INCONCLUSIVE must NEVER count as successful
//! adversarial verification.

use serde::{Deserialize, Serialize};

use griller_core::attack::{AttackKind, AttackOutcome};

/// Structured coverage report for a claim's adversarial attack suite.
///
/// Every field counts attacks by outcome. This is deterministic: the same
/// claim + same attack spec corpus + same execution results always produce
/// the same coverage.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct AttackCoverage {
    /// Total number of attack kinds that could apply to this claim.
    pub applicable: usize,
    /// Number of applicable attacks that have a concrete implementation.
    pub implemented: usize,
    /// Number of attacks that were actually executed.
    pub executed: usize,
    /// Number of attacks that produced supporting outcomes (CONFIRMED/NO_OBJECTION).
    pub successful: usize,
    /// Number of attacks that produced refuting outcomes (REFUTED).
    pub refuted: usize,
    /// Number of attacks that produced an undetermined outcome (INCONCLUSIVE).
    pub inconclusive: usize,
    /// Number of attacks that failed to execute (EXECUTION_FAILED).
    pub failed: usize,
    /// Number of attacks that were not applicable to this claim.
    pub not_applicable: usize,
    /// Number of applicable attacks that are not yet implemented.
    ///
    /// An applicable but unimplemented attack prevents ADVERSARIALLY_VERIFIED
    /// because there is no adversarial scrutiny for that attack type.
    pub unimplemented: usize,
}

impl AttackCoverage {
    /// Create a new coverage report.
    ///
    /// Call [`Self::observe`] to add results from executed attacks,
    /// and [`Self::mark_unimplemented`] for applicable attacks that lack
    /// an implementation.
    pub fn new(applicable: usize) -> Self {
        Self {
            applicable,
            executed: 0,
            successful: 0,
            refuted: 0,
            inconclusive: 0,
            failed: 0,
            not_applicable: 0,
            implemented: applicable,
            unimplemented: 0,
        }
    }

    /// Observe the result of executing one attack spec.
    ///
    /// Increments the appropriate counter based on the outcome.
    pub fn observe(&mut self, _kind: AttackKind, outcome: AttackOutcome, was_applicable: bool) {
        if !was_applicable {
            self.not_applicable += 1;
            return;
        }

        self.executed += 1;

        match outcome {
            AttackOutcome::Refuted => {
                self.refuted += 1;
            }
            AttackOutcome::Confirmed | AttackOutcome::NoObjection => {
                self.successful += 1;
            }
            AttackOutcome::Inconclusive => {
                self.inconclusive += 1;
            }
            AttackOutcome::ExecutionFailed => {
                self.failed += 1;
            }
            AttackOutcome::NotApplicable => {
                // Defensive: should be caught by was_applicable guard above.
                self.not_applicable += 1;
            }
        }
    }

    /// Mark an applicable attack as not yet implemented.
    ///
    /// This increments `unimplemented` and decrements `implemented`.
    /// An applicable unimplemented attack prevents ADVERSARIALLY_VERIFIED.
    pub fn mark_unimplemented(&mut self) {
        self.unimplemented += 1;
        self.implemented -= 1;
    }

    /// True when at least one attack produced a supporting outcome.
    pub fn has_successful(&self) -> bool {
        self.successful > 0
    }

    /// True when at least one attack produced a refuting outcome.
    pub fn has_refuted(&self) -> bool {
        self.refuted > 0
    }

    /// True when at least one attack was inconclusive or failed.
    pub fn has_undetermined(&self) -> bool {
        self.inconclusive > 0 || self.failed > 0
    }

    /// True when no attacks were applicable or all were not applicable.
    pub fn is_empty_coverage(&self) -> bool {
        self.applicable == 0
    }

    /// True when every applicable attack has a concrete implementation.
    pub fn all_applicable_implemented(&self) -> bool {
        self.unimplemented == 0
    }

    /// True when every applicable attack was executed.
    pub fn all_applicable_executed(&self) -> bool {
        self.executed + self.not_applicable == self.applicable
    }

    /// True when all executed attacks produced determinate outcomes.
    pub fn all_determinate(&self) -> bool {
        let determined = self.successful + self.refuted;
        determined == self.executed
    }

    /// Check whether a given attack kind has a concrete implementation.
    ///
    /// Returns `true` if the attack is implemented and can produce real
    /// outcomes. Returns `false` for stubs/foundations.
    pub fn is_attack_implemented(kind: AttackKind) -> bool {
        matches!(
            kind,
            AttackKind::Replay
                | AttackKind::PresenceAbsence
                | AttackKind::StubSweep
                | AttackKind::Safety
        )
    }

    /// Determine whether an attack kind is applicable to a claim scope.
    ///
    /// Returns true when the attack kind can meaningfully target the given
    /// claim scope. This is used to compute the `applicable` count.
    pub fn is_attack_applicable(kind: &AttackKind, claim_scope: &str) -> bool {
        match kind {
            AttackKind::Replay => {
                // Replay is applicable when the claim mentions verification commands
                // (test, build, fmt, check, etc.)
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("test")
                    || scope_lower.contains("build")
                    || scope_lower.contains("fmt")
                    || scope_lower.contains("check")
                    || scope_lower.contains("pass")
                    || scope_lower.contains("fail")
            }
            AttackKind::Completeness => {
                // Completeness applies to completion and requirement claims
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("all") || scope_lower.contains("complete")
            }
            AttackKind::PresenceAbsence => {
                // Presence/absence applies to claims about file/entity existence
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("removed")
                    || scope_lower.contains("added")
                    || scope_lower.contains("exists")
                    || scope_lower.contains("absent")
                    || scope_lower.contains("file")
            }
            AttackKind::Invert => {
                // Invert is applicable when there's a concrete change to reverse
                // (limited by capability)
                false // foundation only; returns NOT_APPLICABLE for now
            }
            AttackKind::Robustness => {
                // Robustness is applicable when verification commands exist
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("repeat") || scope_lower.contains("consistent")
            }
            AttackKind::RequirementCoverage => {
                // Requirement coverage applies to requirement claims
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("requirement")
                    || scope_lower.contains("all requirements")
                    || scope_lower.contains("r1")
                    || scope_lower.contains("r2")
            }
            AttackKind::StubSweep => {
                // Stub sweep applies to claims about implementation completeness
                let scope_lower = claim_scope.to_lowercase();
                scope_lower.contains("complete")
                    || scope_lower.contains("implemented")
                    || scope_lower.contains("finished")
                    || scope_lower.contains("done")
            }
            AttackKind::Safety => {
                // Safety applies to all claims involving code changes
                true
            }
        }
    }

    /// Get the overall adversarial verification status based on coverage.
    ///
    /// Returns `Some(AttackOutcome::NoObjection)` when the coverage is
    /// sufficient to support `ADVERSARIALLY_VERIFIED`, or `None` otherwise.
    ///
    /// # ADVERSARIALLY_VERIFIED Criteria
    ///
    /// A claim is ADVERSARIALLY_VERIFIED ONLY when ALL of the following hold:
    ///
    /// 1. `successful > 0` — at least one attack produced supporting evidence
    /// 2. `refuted == 0` — no attack found contradictory evidence
    /// 3. `inconclusive == 0` — no attack produced an inconclusive result
    /// 4. `failed == 0` — no attack failed to execute
    /// 5. `unimplemented == 0` — no applicable attack remains unimplemented
    /// 6. `applicable == executed` — all applicable attacks were executed
    pub fn adversarial_status(&self) -> Option<AttackOutcome> {
        // Rule 1: Must have at least one successful attack
        if !self.has_successful() {
            return None;
        }

        // Rule 2: No refutations allowed
        if self.has_refuted() {
            return None;
        }

        // Rule 3: No inconclusive results allowed
        if self.inconclusive > 0 {
            return None;
        }

        // Rule 4: No execution failures allowed
        if self.failed > 0 {
            return None;
        }

        // Rule 5: No unimplemented applicable attacks allowed
        if !self.all_applicable_implemented() {
            return None;
        }

        // Rule 6: All applicable attacks must have been executed
        if !self.all_applicable_executed() {
            return None;
        }

        // All criteria satisfied
        Some(AttackOutcome::NoObjection)
    }

    /// Generate a human-readable summary of the attack coverage.
    pub fn summary(&self) -> String {
        format!(
            "applicable={}, implemented={}, executed={}, successful={}, refuted={}, inconclusive={}, failed={}, unimplemented={}",
            self.applicable,
            self.implemented,
            self.executed,
            self.successful,
            self.refuted,
            self.inconclusive,
            self.failed,
            self.unimplemented,
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use griller_core::attack::AttackOutcome;

    #[test]
    fn coverage_new_defaults() {
        let coverage = AttackCoverage::new(5);
        assert_eq!(coverage.applicable, 5);
        assert_eq!(coverage.implemented, 5);
        assert_eq!(coverage.unimplemented, 0);
        assert_eq!(coverage.executed, 0);
        assert_eq!(coverage.successful, 0);
        assert_eq!(coverage.refuted, 0);
        assert_eq!(coverage.inconclusive, 0);
        assert_eq!(coverage.failed, 0);
        assert_eq!(coverage.not_applicable, 0);
    }

    #[test]
    fn coverage_mark_unimplemented() {
        let mut coverage = AttackCoverage::new(3);
        coverage.mark_unimplemented();
        coverage.mark_unimplemented();

        assert_eq!(coverage.implemented, 1);
        assert_eq!(coverage.unimplemented, 2);
        assert!(!coverage.all_applicable_implemented());
    }

    #[test]
    fn coverage_observe_refuted() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Inconclusive, true);

        assert_eq!(coverage.successful, 1);
        assert_eq!(coverage.refuted, 1);
        assert_eq!(coverage.inconclusive, 1);
        assert_eq!(coverage.executed, 3);
    }

    #[test]
    fn coverage_observe_confirmed() {
        let mut coverage = AttackCoverage::new(2);
        coverage.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        assert_eq!(coverage.successful, 2);
        assert_eq!(coverage.executed, 2);
    }

    #[test]
    fn coverage_observe_not_applicable() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, false); // not applicable
        coverage.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Inconclusive, true);

        assert_eq!(coverage.not_applicable, 1);
        assert_eq!(coverage.executed, 2);
        assert_eq!(coverage.successful, 0);
        assert_eq!(coverage.refuted, 1);
        assert_eq!(coverage.inconclusive, 1);
    }

    #[test]
    fn coverage_has_successful() {
        let coverage = AttackCoverage::new(3);
        assert!(!coverage.has_successful());

        let mut c2 = AttackCoverage::new(3);
        c2.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);
        assert!(c2.has_successful());

        let mut c3 = AttackCoverage::new(3);
        c3.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        assert!(c3.has_successful());

        let mut c4 = AttackCoverage::new(3);
        c4.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        assert!(!c4.has_successful());
    }

    #[test]
    fn coverage_has_refuted() {
        let coverage = AttackCoverage::new(3);
        assert!(!coverage.has_refuted());

        let mut c2 = AttackCoverage::new(3);
        c2.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        assert!(c2.has_refuted());
    }

    #[test]
    fn coverage_has_undetermined() {
        let coverage = AttackCoverage::new(3);
        assert!(!coverage.has_undetermined());

        let mut c2 = AttackCoverage::new(3);
        c2.observe(AttackKind::Replay, AttackOutcome::Inconclusive, true);
        assert!(c2.has_undetermined());

        let mut c3 = AttackCoverage::new(3);
        c3.observe(AttackKind::Replay, AttackOutcome::ExecutionFailed, true);
        assert!(c3.has_undetermined());

        let mut c4 = AttackCoverage::new(3);
        c4.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        assert!(!c4.has_undetermined());
    }

    #[test]
    fn coverage_is_empty() {
        let coverage = AttackCoverage::new(0);
        assert!(coverage.is_empty_coverage());

        let coverage2 = AttackCoverage::new(5);
        assert!(!coverage2.is_empty_coverage());
    }

    #[test]
    fn coverage_all_determinate() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        assert!(coverage.all_determinate());
    }

    #[test]
    fn coverage_all_determinate_with_inconclusive() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::Refuted, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Inconclusive, true);

        assert!(!coverage.all_determinate());
    }

    #[test]
    fn coverage_adversarial_status_all_no_objection() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let status = coverage.adversarial_status();
        assert_eq!(status, Some(AttackOutcome::NoObjection));
    }

    #[test]
    fn coverage_adversarial_status_mixed_refuted() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Refuted, true);

        let status = coverage.adversarial_status();
        assert_eq!(status, None);
    }

    #[test]
    fn coverage_adversarial_status_undetermined() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::Inconclusive, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let status = coverage.adversarial_status();
        assert_eq!(status, None);
    }

    #[test]
    fn coverage_adversarial_status_with_failed() {
        let mut coverage = AttackCoverage::new(3);
        coverage.observe(AttackKind::Replay, AttackOutcome::ExecutionFailed, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let status = coverage.adversarial_status();
        assert_eq!(status, None);
    }

    #[test]
    fn coverage_adversarial_status_unimplemented_prevents_verification() {
        let mut coverage = AttackCoverage::new(3);
        coverage.mark_unimplemented(); // one applicable attack is unimplemented
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let status = coverage.adversarial_status();
        // Cannot be adversarially verified because one applicable attack is unimplemented
        assert_eq!(status, None);
    }

    #[test]
    fn coverage_adversarial_status_all_executed_required() {
        let mut coverage = AttackCoverage::new(3);
        // Only executed 2 out of 3 applicable attacks
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let status = coverage.adversarial_status();
        // Cannot be adversarially verified because not all applicable attacks were executed
        assert_eq!(status, None);
    }

    #[test]
    fn coverage_adversarial_status_with_confirmed_only() {
        let mut coverage = AttackCoverage::new(2);
        coverage.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::Confirmed, true);

        let status = coverage.adversarial_status();
        assert_eq!(status, Some(AttackOutcome::NoObjection));
    }

    #[test]
    fn coverage_is_attack_applicable() {
        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::Replay,
            "Tests pass"
        ));
        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::Replay,
            "Build succeeds"
        ));
        assert!(!AttackCoverage::is_attack_applicable(
            &AttackKind::Replay,
            "File foo.rs exists"
        ));

        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::Completeness,
            "All tests pass"
        ));
        assert!(!AttackCoverage::is_attack_applicable(
            &AttackKind::Completeness,
            "File removed"
        ));

        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::PresenceAbsence,
            "Removed foo.rs"
        ));
        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::PresenceAbsence,
            "Added foo.rs"
        ));

        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::StubSweep,
            "Implementation complete"
        ));
        assert!(AttackCoverage::is_attack_applicable(
            &AttackKind::Safety,
            "Any claim"
        ));
    }

    #[test]
    fn coverage_is_attack_implemented() {
        assert!(AttackCoverage::is_attack_implemented(AttackKind::Replay));
        assert!(AttackCoverage::is_attack_implemented(
            AttackKind::PresenceAbsence
        ));
        assert!(AttackCoverage::is_attack_implemented(AttackKind::StubSweep));
        assert!(AttackCoverage::is_attack_implemented(AttackKind::Safety));

        // These are currently stubs/foundations
        assert!(!AttackCoverage::is_attack_implemented(AttackKind::Invert));
        assert!(!AttackCoverage::is_attack_implemented(
            AttackKind::Robustness
        ));
        assert!(!AttackCoverage::is_attack_implemented(
            AttackKind::Completeness
        ));
        assert!(!AttackCoverage::is_attack_implemented(
            AttackKind::RequirementCoverage
        ));
    }

    #[test]
    fn coverage_roundtrip() {
        let coverage = AttackCoverage {
            applicable: 5,
            executed: 3,
            successful: 2,
            refuted: 1,
            inconclusive: 0,
            failed: 0,
            not_applicable: 1,
            implemented: 4,
            unimplemented: 1,
        };
        let json = serde_json::to_string(&coverage).unwrap();
        let back: AttackCoverage = serde_json::from_str(&json).unwrap();
        assert_eq!(coverage, back);
    }

    #[test]
    fn coverage_summary_is_readable() {
        let mut coverage = AttackCoverage::new(3);
        coverage.mark_unimplemented();
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);
        coverage.observe(AttackKind::Replay, AttackOutcome::NoObjection, true);

        let summary = coverage.summary();
        assert!(summary.contains("applicable=3"));
        assert!(summary.contains("unimplemented=1"));
        assert!(summary.contains("successful=2"));
    }
}
