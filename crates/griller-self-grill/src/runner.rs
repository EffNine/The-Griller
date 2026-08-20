//! The self-grill runner.
//!
//! Runs all applicable checks, aggregates findings into a result, and
//! preserves the original decision as an immutable snapshot.

use crate::checks::run_all_checks;
use crate::domain::*;

/// Runs a single pass of self-grill checks over an input.
///
/// This is NOT recursive: it runs exactly one pass and returns.
#[derive(Debug, Clone, Default)]
pub struct SelfGrillRunner;

impl SelfGrillRunner {
    /// Run the self-audit and return the result alongside the original
    /// decision snapshot.
    ///
    /// The original decision is never mutated.
    pub fn run(&self, input: &SelfGrillInput) -> SelfGrillResult {
        let findings = run_all_checks(input);

        // Build coverage from findings
        let mut coverage = SelfGrillCoverage::default();
        for f in &findings {
            coverage.mark_applicable();
            coverage.record(f.outcome);
        }

        // Compute overall outcome
        let outcome = SelfGrillResult::aggregate_outcome(&findings);

        SelfGrillResult {
            outcome,
            coverage,
            findings,
            issues: Vec::new(),
        }
    }

    /// Check whether the given input has any applicable checks.
    ///
    /// Returns false when the input is empty and nothing can be audited.
    pub fn is_applicable(input: &SelfGrillInput) -> bool {
        !input.evidence.is_empty()
            || input.context_assessment.is_some()
            || input.attack_coverage.is_some()
    }
}

/// Run self-grill and return the paired result.
pub fn self_grill(input: &SelfGrillInput) -> (OriginalDecision, SelfGrillResult) {
    let runner = SelfGrillRunner;
    let result = runner.run(input);
    let original = OriginalDecision::from_input(input, &input.decision);
    (original, result)
}
