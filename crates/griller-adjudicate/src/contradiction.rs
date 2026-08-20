//! Deterministic structural contradiction checking.
//!
//! M2 detects contradictions *structurally*: the same command scope observed
//! with conflicting exit codes, or a claimed entity observed both present and
//! absent. Sophisticated semantic contradiction detection is deliberately out
//! of scope; the [`ContradictionChecker`] trait is the seam where it can be
//! added later.

use griller_core::evidence::{CommandOutputContent, Evidence, EvidenceId, EvidenceKind};
use serde::{Deserialize, Serialize};

use crate::matcher::claim_class;
use crate::normalized::NormalizedClaim;
use crate::relationship::{EvidenceMatch, EvidenceRelationship, EvidenceStrength};
use crate::support::SupportMatrix;

/// What kind of structural contradiction was detected.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ContradictionKind {
    /// Two executions of the same command scope produced conflicting exit
    /// codes (one success, one failure).
    ExitCodeConflict,
    /// A claimed entity was observed both present and absent.
    PresenceConflict,
    /// Relevant supporting and contradicting evidence coexist without a more
    /// specific structural explanation.
    PolarityConflict,
}

/// A detected contradiction between two evidence records.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Contradiction {
    /// The kind of conflict.
    pub kind: ContradictionKind,
    /// The id of the first conflicting record (deterministically ordered).
    pub left_id: EvidenceId,
    /// The id of the second conflicting record.
    pub right_id: EvidenceId,
    /// Deterministic, human-readable description.
    pub description: String,
}

/// The contradiction checker boundary. M2 ships a deterministic structural
/// implementation; a semantic checker can replace it later without touching
/// the rest of the pipeline.
pub trait ContradictionChecker: Send + Sync {
    /// Check a support matrix for contradictions, given the evidence records.
    fn check(
        &self,
        matrix: &SupportMatrix,
        claim: &NormalizedClaim,
        evidence: &[Evidence],
    ) -> Vec<Contradiction>;
}

/// The deterministic, structural contradiction checker.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicContradictionChecker;

impl ContradictionChecker for DeterministicContradictionChecker {
    fn check(
        &self,
        matrix: &SupportMatrix,
        claim: &NormalizedClaim,
        evidence: &[Evidence],
    ) -> Vec<Contradiction> {
        let mut conflicts: Vec<Contradiction> = Vec::new();

        // Rule A — same command scope, conflicting exit codes.
        if let Some(conflict) = exit_code_conflict(matrix, evidence) {
            conflicts.push(conflict);
        }

        // Rule B — entity presence observed both ways.
        if is_presence_class(claim) {
            if let Some(conflict) = presence_conflict(matrix) {
                conflicts.push(conflict);
            }
        }

        // Rule C — aggregate polarity conflict (e.g. different commands with
        // opposing outcomes, not caught by rule A).
        if conflicts.is_empty() && !matrix.supports().is_empty() && !matrix.contradicts().is_empty()
        {
            let contradicts = matrix.contradicts();
            let supports = matrix.supports();
            let left = contradicts[0].evidence_id.clone();
            let right = supports[0].evidence_id.clone();
            let (left, right) = ordered_pair(left, right);
            conflicts.push(Contradiction {
                kind: ContradictionKind::PolarityConflict,
                left_id: left,
                right_id: right,
                description: "relevant evidence both supports and contradicts the claim"
                    .to_string(),
            });
        }

        conflicts
    }
}

fn is_presence_class(claim: &NormalizedClaim) -> bool {
    matches!(
        claim_class(claim),
        crate::matcher::ClaimClass::EntityExists | crate::matcher::ClaimClass::EntityAbsent
    )
}

/// Normalize a pair of ids into a deterministic order.
fn ordered_pair(a: EvidenceId, b: EvidenceId) -> (EvidenceId, EvidenceId) {
    if a.as_str() <= b.as_str() {
        (a, b)
    } else {
        (b, a)
    }
}

/// Rule A: find the first same-command exit-code conflict.
fn exit_code_conflict(matrix: &SupportMatrix, evidence: &[Evidence]) -> Option<Contradiction> {
    let by_id: std::collections::HashMap<&str, &Evidence> =
        evidence.iter().map(|e| (e.id.as_str(), e)).collect();

    let mut groups: std::collections::BTreeMap<String, Vec<&EvidenceMatch>> =
        std::collections::BTreeMap::new();

    for m in matrix.matches.iter().filter(|m| {
        m.evidence_kind == EvidenceKind::CommandOutput
            && matches!(
                m.relationship,
                EvidenceRelationship::Supports | EvidenceRelationship::Contradicts
            )
    }) {
        let spec = by_id
            .get(m.evidence_id.as_str())
            .and_then(|e| e.content.as_command_output())
            .map(|c| normalized_command_line(&c.spec))
            .unwrap_or_default();
        if !spec.is_empty() {
            groups.entry(spec).or_default().push(m);
        }
    }

    for (command, matches) in groups {
        let mut successes: Vec<&EvidenceMatch> = Vec::new();
        let mut failures: Vec<&EvidenceMatch> = Vec::new();
        for m in &matches {
            let content = by_id
                .get(m.evidence_id.as_str())
                .and_then(|e| e.content.as_command_output());
            match content.map(CommandOutputContent::outcome) {
                Some(outcome) if outcome.is_success() => successes.push(m),
                Some(_) => failures.push(m),
                None => {}
            }
        }
        if !successes.is_empty() && !failures.is_empty() {
            let (left, right) = ordered_pair(
                failures[0].evidence_id.clone(),
                successes[0].evidence_id.clone(),
            );
            return Some(Contradiction {
                kind: ContradictionKind::ExitCodeConflict,
                left_id: left,
                right_id: right,
                description: format!(
                    "command `{command}` was observed both succeeding and failing"
                ),
            });
        }
    }
    None
}

/// Rule B: entity presence observed both ways.
fn presence_conflict(matrix: &SupportMatrix) -> Option<Contradiction> {
    let supports = matrix.supports();
    let contradicts = matrix.contradicts();
    if supports.is_empty() || contradicts.is_empty() {
        return None;
    }
    // Only a *direct* presence contradiction refutes presence; weaker signals
    // (moderate text-search) produce a polarity conflict via rule C instead.
    let direct_contradiction = contradicts
        .iter()
        .any(|m| m.strength == EvidenceStrength::Direct);
    if !direct_contradiction {
        return None;
    }
    let (left, right) = ordered_pair(
        contradicts[0].evidence_id.clone(),
        supports[0].evidence_id.clone(),
    );
    Some(Contradiction {
        kind: ContradictionKind::PresenceConflict,
        left_id: left,
        right_id: right,
        description: "the claimed entity was observed both present and absent".to_string(),
    })
}

/// Normalize a command spec to a deterministic command-scope key.
pub(crate) fn normalized_command_line(spec: &griller_core::command::CommandSpec) -> String {
    let mut line = spec.executable.clone();
    for arg in &spec.args {
        line.push(' ');
        line.push_str(arg);
    }
    line
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{ClaimInput, ClaimSource};
    use crate::matcher::{DeterministicEvidenceMatcher, EvidenceMatcher};
    use crate::normalizer::normalize_deterministic;
    use griller_core::command::{CommandSpec, Enforcement, ExecutionStatus};
    use griller_core::evidence::{CommandOutputContent, Evidence, EvidenceContent, EvidenceKind};
    use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};
    use griller_core::redaction::RedactionReport;

    fn test_command(
        args: &[&str],
        stdout: &str,
        stderr: &str,
        exit_code: Option<i32>,
        status: ExecutionStatus,
    ) -> Evidence {
        let spec = CommandSpec::new("cargo").with_args(args.iter().map(|s| s.to_string()));
        let provenance = Provenance {
            source: EvidenceSource::Command,
            capture_method: CaptureMethod::ProcessExecution,
            command: Some(spec.clone()),
            working_directory: None,
            environment_fingerprint: None,
            tool: None,
            reproducibility: griller_core::provenance::Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };
        let content = CommandOutputContent {
            spec,
            stdout: stdout.into(),
            stderr: stderr.into(),
            exit_code,
            status,
            enforcement: Enforcement::default(),
            redaction: RedactionReport::default(),
            duration_ms: 0,
        };
        Evidence::new(
            EvidenceKind::CommandOutput,
            provenance,
            EvidenceContent::CommandOutput(content),
            1,
        )
    }

    #[test]
    fn same_command_conflicting_exit_codes_is_a_conflict() {
        let claim = normalize_deterministic(&ClaimInput::new("tests pass", ClaimSource::User));
        let ok = test_command(&["test"], "ok\n", "", Some(0), ExecutionStatus::Completed);
        let fail = test_command(
            &["test"],
            "",
            "failed\n",
            Some(1),
            ExecutionStatus::Completed,
        );
        let evidence = vec![ok.clone(), fail.clone()];
        let matcher = DeterministicEvidenceMatcher;
        let matches = matcher.match_evidence(&claim, None, &evidence);
        let matrix = SupportMatrix::new(claim.proposition.clone(), matches);
        let checker = DeterministicContradictionChecker;
        let conflicts = checker.check(&matrix, &claim, &evidence);
        assert_eq!(conflicts.len(), 1);
        assert_eq!(conflicts[0].kind, ContradictionKind::ExitCodeConflict);
    }
}
