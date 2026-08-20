//! The M3 policy table.
//!
//! This is the **single auditable source of truth** for how
//! `(ClaimKind, EpistemicStatus, Severity)` maps to an operational verdict,
//! reason code and required action. A developer can answer "why BLOCK?" by
//! reading `rule_for` and `verdict_for` — no LLM involved.
//!
//! ## Shape
//!
//! Each (kind, status) pair maps to a [`Rule`]:
//!
//! * `AlwaysPass` — verified/adversarially-verified claims pass regardless of
//!   severity (M3 has no explicit "additional verification" policy).
//! * `AlwaysWarn` — low-stakes combinations never block.
//! * `BlockAt(threshold)` — `BLOCK` when `severity >= threshold`, otherwise
//!   `WARN`.
//! * `NoRule` — no explicit rule; the conservative default `WARN` applies.
//!
//! ## Rationale
//!
//! * `VERIFIED`/`ADVERSARIALLY_VERIFIED` ⇒ `PASS`. M2 only assigns `VERIFIED`
//!   with direct evidence and no contradiction, so no extra check is required
//!   in M3.
//! * `FALSE`/`CONTRADICTED` are the strongest evidence positions; they block
//!   from `MEDIUM` for impact-bearing kinds.
//! * `UNSUPPORTED`/`UNVERIFIABLE`/`UNCERTAIN` block only at `HIGH`+ for
//!   impact-bearing kinds: they are failures of evidence, not falsity, and a
//!   lower-impact claim may proceed with a warning.
//! * Facts and design claims are lower stakes: `FALSE`/`CONTRADICTED` block
//!   only at `HIGH`+ (design) or `MEDIUM`+ (fact), and evidence-failure
//!   statuses only warn.
//!
//! ## Conservative default
//!
//! A combination with no explicit rule **never passes silently**; it defaults
//! to `WARN` with reason `NO_POLICY_RULE_WARN` ("Policy has no specific rule
//! for this combination."). This prevents new claim kinds / statuses from
//! accidentally becoming trusted.

use griller_adjudicate::claim::ClaimKind;
use griller_adjudicate::status::EpistemicStatus;

use crate::decision::RequiredAction;
use crate::severity::Severity;
use crate::verdict::Verdict;

/// A policy rule for one (kind, status) pair.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Rule {
    /// Always `PASS`.
    AlwaysPass,
    /// Always `WARN`.
    AlwaysWarn,
    /// Always `BLOCK`.
    AlwaysBlock,
    /// `BLOCK` at/above the threshold severity, otherwise `WARN`.
    BlockAt(Severity),
    /// No explicit rule; conservative default `WARN`.
    NoRule,
}

/// The full policy matrix, one explicit arm per (kind, status) pair.
///
/// See `docs/m3-policy.md` for the tabular form.
pub fn rule_for(kind: ClaimKind, status: EpistemicStatus) -> Rule {
    use ClaimKind::{Causal, Completion, Design, Fact, Requirement};
    use EpistemicStatus::{
        AdversariallyVerified, Contradicted, False, Uncertain, Unsupported, Unverifiable, Verified,
    };

    match (kind, status) {
        // --- Completion -----------------------------------------------------
        (Completion, Verified) => Rule::AlwaysPass,
        (Completion, AdversariallyVerified) => Rule::AlwaysPass,
        (Completion, Unsupported) => Rule::BlockAt(Severity::High),
        (Completion, Unverifiable) => Rule::BlockAt(Severity::High),
        (Completion, Uncertain) => Rule::BlockAt(Severity::High),
        (Completion, Contradicted) => Rule::BlockAt(Severity::Medium),
        (Completion, False) => Rule::BlockAt(Severity::Medium),

        // --- Requirement ----------------------------------------------------
        (Requirement, Verified) => Rule::AlwaysPass,
        (Requirement, AdversariallyVerified) => Rule::AlwaysPass,
        (Requirement, Unsupported) => Rule::BlockAt(Severity::High),
        (Requirement, Unverifiable) => Rule::BlockAt(Severity::High),
        (Requirement, Uncertain) => Rule::BlockAt(Severity::High),
        (Requirement, Contradicted) => Rule::BlockAt(Severity::Medium),
        (Requirement, False) => Rule::BlockAt(Severity::Medium),

        // --- Causal ---------------------------------------------------------
        (Causal, Verified) => Rule::AlwaysPass,
        (Causal, AdversariallyVerified) => Rule::AlwaysPass,
        (Causal, Unsupported) => Rule::BlockAt(Severity::High),
        (Causal, Unverifiable) => Rule::BlockAt(Severity::High),
        (Causal, Uncertain) => Rule::BlockAt(Severity::High),
        (Causal, Contradicted) => Rule::BlockAt(Severity::Medium),
        (Causal, False) => Rule::BlockAt(Severity::Medium),

        // --- Fact -----------------------------------------------------------
        (Fact, Verified) => Rule::AlwaysPass,
        (Fact, AdversariallyVerified) => Rule::AlwaysPass,
        (Fact, Unsupported) => Rule::AlwaysWarn,
        (Fact, Unverifiable) => Rule::AlwaysWarn,
        (Fact, Uncertain) => Rule::AlwaysWarn,
        (Fact, Contradicted) => Rule::BlockAt(Severity::Medium),
        (Fact, False) => Rule::BlockAt(Severity::Medium),

        // --- Design ---------------------------------------------------------
        (Design, Verified) => Rule::AlwaysPass,
        (Design, AdversariallyVerified) => Rule::AlwaysPass,
        (Design, Unsupported) => Rule::BlockAt(Severity::High),
        (Design, Unverifiable) => Rule::AlwaysWarn,
        (Design, Uncertain) => Rule::AlwaysWarn,
        (Design, Contradicted) => Rule::BlockAt(Severity::High),
        (Design, False) => Rule::BlockAt(Severity::High),

        // Unknown/future combinations are never trusted silently.
        _ => Rule::NoRule,
    }
}

/// The verdict for a (kind, status, severity) triple.
pub fn verdict_for(kind: ClaimKind, status: EpistemicStatus, severity: Severity) -> Verdict {
    verdict_from_rule(rule_for(kind, status), severity)
}

/// The verdict a rule yields at a given severity.
///
/// `NoRule` never passes silently; it always warns.
pub fn verdict_from_rule(rule: Rule, severity: Severity) -> Verdict {
    match rule {
        Rule::AlwaysPass => Verdict::Pass,
        Rule::AlwaysWarn => Verdict::Warn,
        Rule::AlwaysBlock => Verdict::Block,
        Rule::BlockAt(threshold) => {
            if severity >= threshold {
                Verdict::Block
            } else {
                Verdict::Warn
            }
        }
        Rule::NoRule => Verdict::Warn,
    }
}

/// True when this (kind, status) pair has no explicit policy rule.
pub fn is_no_rule(kind: ClaimKind, status: EpistemicStatus) -> bool {
    matches!(rule_for(kind, status), Rule::NoRule)
}

/// Stable machine-readable reason code.
///
/// Format: `{STATUS}_{KIND}_{VERDICT}`, e.g. `UNSUPPORTED_COMPLETION_BLOCK`.
/// The no-rule default has the fixed code `NO_POLICY_RULE_WARN`.
pub fn reason_code(kind: ClaimKind, status: EpistemicStatus, verdict: Verdict) -> String {
    if is_no_rule(kind, status) {
        return "NO_POLICY_RULE_WARN".to_string();
    }
    format!(
        "{}_{}_{}",
        status_code(status),
        kind_code(kind),
        verdict_code(verdict)
    )
}

/// Deterministic human-readable reason. Never contains untrusted claim text.
pub fn reason_message(
    kind: ClaimKind,
    status: EpistemicStatus,
    severity: Severity,
    verdict: Verdict,
) -> String {
    if is_no_rule(kind, status) {
        return "Policy has no specific rule for this combination.".to_string();
    }
    let subject = kind_subject(kind);
    match status {
        EpistemicStatus::Verified => {
            format!("Direct environment evidence verifies the {subject}.")
        }
        EpistemicStatus::AdversariallyVerified => {
            format!("The {subject} passed adversarial verification.")
        }
        EpistemicStatus::Unsupported => match verdict {
            Verdict::Block => {
                format!("The {subject} lacks independently observed evidence at {severity} impact.")
            }
            _ => format!("The {subject} lacks independently observed evidence."),
        },
        EpistemicStatus::Unverifiable => match verdict {
            Verdict::Block => format!(
                "The {subject} cannot be accepted because verification could not be performed."
            ),
            _ => format!("The {subject} could not be verified; it is not accepted as true."),
        },
        EpistemicStatus::Uncertain => match verdict {
            Verdict::Block => {
                format!("The {subject} is uncertain at {severity} impact and cannot be accepted.")
            }
            _ => {
                format!("The {subject} is uncertain; evidence is insufficient or conflicting.")
            }
        },
        EpistemicStatus::Contradicted => {
            format!("Evidence conflicts with the {subject}.")
        }
        EpistemicStatus::False => {
            format!("Evidence directly establishes the {subject} is false.")
        }
        // Future statuses are guarded by is_no_rule above, so this arm is a
        // defensive fallback only.
        _ => format!("The {subject} has a status with no policy rule."),
    }
}

/// The required action for a status (independent of severity).
pub fn required_action_for(status: EpistemicStatus) -> RequiredAction {
    match status {
        EpistemicStatus::Verified | EpistemicStatus::AdversariallyVerified => RequiredAction::None,
        EpistemicStatus::Unsupported => RequiredAction::ProvideEvidence,
        EpistemicStatus::Unverifiable => RequiredAction::MakeVerificationAvailable,
        EpistemicStatus::Uncertain => RequiredAction::AcknowledgeUncertainty,
        EpistemicStatus::Contradicted => RequiredAction::ResolveConflict,
        EpistemicStatus::False => RequiredAction::CorrectClaim,
        // Future statuses default to "define a rule" — never silently trust.
        _ => RequiredAction::DefinePolicyRule,
    }
}

/// Human-readable explanation of the required action.
pub fn required_action_message(status: EpistemicStatus) -> String {
    match required_action_for(status) {
        RequiredAction::None => "No action required.".to_string(),
        RequiredAction::ProvideEvidence => {
            "Run the relevant verification, attach the resulting evidence, and re-grill."
                .to_string()
        }
        RequiredAction::MakeVerificationAvailable => {
            "Make the required execution or evidence mechanism available, then re-grill."
                .to_string()
        }
        RequiredAction::ResolveConflict => {
            "Resolve the conflicting implementation or evidence, then re-grill.".to_string()
        }
        RequiredAction::CorrectClaim => {
            "Correct the claim or the implementation.".to_string()
                + " Re-grill the corrected claim."
        }
        RequiredAction::AcknowledgeUncertainty => {
            "Provide stronger, unambiguous evidence or explicitly acknowledge the uncertainty."
                .to_string()
        }
        RequiredAction::DefinePolicyRule => {
            "Define an explicit policy rule for this combination before proceeding.".to_string()
        }
        RequiredAction::ConfirmProject => {
            "Stop before modifying the repository and confirm the intended project/repository."
                .to_string()
        }
    }
}

// ---------------------------------------------------------------------------
// Labels
// ---------------------------------------------------------------------------

fn status_code(status: EpistemicStatus) -> String {
    format!("{status:?}").to_uppercase()
}

fn kind_code(kind: ClaimKind) -> String {
    format!("{kind:?}").to_uppercase()
}

fn verdict_code(verdict: Verdict) -> String {
    format!("{verdict:?}").to_uppercase()
}

fn kind_subject(kind: ClaimKind) -> &'static str {
    match kind {
        ClaimKind::Completion => "completion claim",
        ClaimKind::Fact => "fact claim",
        ClaimKind::Causal => "causal claim",
        ClaimKind::Requirement => "requirement claim",
        ClaimKind::Design => "design claim",
        _ => "claim",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn no_rule_reason_code_is_fixed_and_stable() {
        let code = reason_code(
            ClaimKind::Completion,
            EpistemicStatus::Verified,
            Verdict::Pass,
        );
        // Normal path: stable format.
        assert_eq!(code, "VERIFIED_COMPLETION_PASS");
        // A hypothetical future combination would fall back to the fixed
        // NO_POLICY_RULE_WARN code (defensive; not reachable today because
        // every current combination has an explicit rule).
        let verdict = verdict_from_rule(Rule::NoRule, Severity::Critical);
        assert_eq!(verdict, Verdict::Warn);
    }

    #[test]
    fn block_at_threshold_respects_severity_boundary() {
        // BlockAt(High): Medium => WARN, High => BLOCK.
        let rule = Rule::BlockAt(Severity::High);
        assert_eq!(verdict_from_rule(rule, Severity::Medium), Verdict::Warn);
        assert_eq!(verdict_from_rule(rule, Severity::High), Verdict::Block);
        assert_eq!(verdict_from_rule(rule, Severity::Critical), Verdict::Block);
    }

    #[test]
    fn always_rules_ignore_severity() {
        assert_eq!(
            verdict_from_rule(Rule::AlwaysPass, Severity::Critical),
            Verdict::Pass
        );
        assert_eq!(
            verdict_from_rule(Rule::AlwaysWarn, Severity::Critical),
            Verdict::Warn
        );
        assert_eq!(
            verdict_from_rule(Rule::AlwaysBlock, Severity::Info),
            Verdict::Block
        );
    }
}
