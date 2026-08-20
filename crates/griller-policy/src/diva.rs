//! The Diva persona renderer.
//!
//! The renderer is **decorative only**. It consumes a decision/finding and
//! produces prose; it never produces a verdict. The canonical JSON decision
//! remains authoritative. The prose is deliberately minimal — no time is spent
//! polishing it.

use griller_adjudicate::status::EpistemicStatus;
use griller_context::evidence::ContextEvidenceKind;

use crate::context::ContextFinding;
use crate::decision::Decision;
use crate::finding::{Finding, OverallDecision};
use crate::verdict::Verdict;

/// Renders verdicts and findings in a fixed persona voice.
#[derive(Debug, Clone, Copy, Default)]
pub struct DivaRenderer;

impl DivaRenderer {
    /// A single line for a verdict, optionally tuned by the epistemic status.
    pub fn render_verdict(&self, verdict: Verdict, status: Option<EpistemicStatus>) -> String {
        match (verdict, status) {
            (Verdict::Pass, _) => "Fine. I checked the receipts. It survived.".to_string(),
            (Verdict::Warn, Some(EpistemicStatus::Unsupported)) => {
                "You made the claim. I found no receipts.".to_string()
            }
            (Verdict::Warn, _) => "Mm. I'm not convinced yet. Bring me evidence.".to_string(),
            (Verdict::Block, Some(EpistemicStatus::Unverifiable)) => {
                "I can't prove you're wrong. I also can't prove you're right. That's not a PASS."
                    .to_string()
            }
            (Verdict::Block, _) => {
                "Absolutely not. You're asking me to trust this without evidence.".to_string()
            }
        }
    }

    /// Render one finding as a two-line block (persona line + reason code).
    pub fn render_finding(&self, finding: &Finding) -> String {
        format!(
            "{}\n[{}] {}",
            self.render_verdict(finding.verdict, Some(finding.assessment.status)),
            finding.reason_code,
            finding.required_action_message,
        )
    }

    /// Render an overall decision.
    pub fn render_overall(&self, overall: &OverallDecision) -> String {
        let mut out = format!(
            "{} (overall verdict: {:?})",
            self.render_verdict(overall.verdict, None),
            overall.verdict
        );
        for finding in &overall.findings {
            out.push('\n');
            out.push_str(&self.render_finding(finding));
        }
        out
    }

    /// Render a single decision line (used when only the decision is needed).
    pub fn render_decision(&self, decision: &Decision) -> String {
        format!(
            "{} [{}] {}",
            self.render_verdict(decision.verdict, None),
            decision.reason_code,
            decision.required_action_message
        )
    }

    /// Render an M4 context finding.
    ///
    /// Decorative only: it consumes a finding and never produces a verdict or
    /// status. The canonical JSON finding remains authoritative.
    pub fn render_context_finding(&self, finding: &ContextFinding) -> String {
        match finding.assessment.primary_mismatch() {
            Some(mismatch) if mismatch.kind == ContextEvidenceKind::ExplicitProjectMismatch => {
                let asked = mismatch.subject.clone();
                let handed = mismatch
                    .repository_identity
                    .clone()
                    .unwrap_or_else(|| "this repo".to_string());
                format!(
                    "Wrong repo, darling. You asked me for {asked} and handed me {handed}. I'm not touching this.\n[{}] {}",
                    finding.reason_code, finding.required_action_message
                )
            }
            _ => format!(
                "{} [{}] {}",
                self.render_verdict(finding.verdict, None),
                finding.reason_code,
                finding.required_action_message
            ),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn renderer_only_consumes_verdicts() {
        let renderer = DivaRenderer;
        for (verdict, expected) in [
            (Verdict::Pass, "Fine. I checked the receipts. It survived."),
            (
                Verdict::Warn,
                "Mm. I'm not convinced yet. Bring me evidence.",
            ),
            (
                Verdict::Block,
                "Absolutely not. You're asking me to trust this without evidence.",
            ),
        ] {
            assert_eq!(renderer.render_verdict(verdict, None), expected);
        }
    }
}
