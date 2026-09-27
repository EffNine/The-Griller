//! Bounded summary extraction from a `griller.check_completion` report.
//!
//! The gate carries the complete structured report verbatim; this module
//! derives a small, human-readable `findings` list from it for the decision
//! document. Parsing is defensive: a report that does not match the expected
//! shape still yields a decision (with a malformed-report finding) rather
//! than crashing the gate.

use serde::{Deserialize, Serialize};
use serde_json::Value;

/// One bounded finding in a gate decision.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Finding {
    pub key: String,
    pub detail: String,
}

/// Maximum number of findings carried in a decision.
const MAX_FINDINGS: usize = 12;

/// The subset of the report the gate needs to map to a decision.
#[derive(Debug, Clone)]
pub struct ReportSummary {
    pub verdict: String,
    pub reason_code: String,
    pub required_action: String,
    pub required_action_message: String,
    pub findings: Vec<Finding>,
}

/// Parse a `check_completion` report document into a summary.
pub fn summarize_report(report: &Value) -> ReportSummary {
    let verdict = string_at(report, "/verdict").unwrap_or_else(|| "unknown".to_string());
    let reason_code = string_at(report, "/reason_code").unwrap_or_default();
    let required_action = string_at(report, "/required_action").unwrap_or_default();
    let required_action_message = string_at(report, "/required_action_message").unwrap_or_default();

    let mut findings: Vec<Finding> = Vec::new();
    if let Some(context_status) = string_at(report, "/context/status") {
        findings.push(Finding {
            key: "context_status".into(),
            detail: context_status,
        });
    }
    if let Some(context_reason) = string_at(report, "/context/reason") {
        if !context_reason.is_empty() {
            findings.push(Finding {
                key: "context_reason".into(),
                detail: context_reason,
            });
        }
    }
    if let Some(initial) = string_at(report, "/assessment/initial/status") {
        findings.push(Finding {
            key: "initial_status".into(),
            detail: initial,
        });
    }
    if let Some(final_status) = string_at(report, "/assessment/final/status") {
        findings.push(Finding {
            key: "final_status".into(),
            detail: final_status,
        });
    }
    if let Some(coverage) = report.pointer("/attacks/coverage") {
        if let Some(refuted) = coverage.get("refuted").and_then(Value::as_u64) {
            findings.push(Finding {
                key: "attacks_refuted".into(),
                detail: refuted.to_string(),
            });
        }
        if let Some(executed) = coverage.get("executed").and_then(Value::as_u64) {
            findings.push(Finding {
                key: "attacks_executed".into(),
                detail: executed.to_string(),
            });
        }
    }
    if let Some(rejected) = report
        .pointer("/attacks/rejected")
        .and_then(Value::as_array)
    {
        if !rejected.is_empty() {
            findings.push(Finding {
                key: "attacks_rejected".into(),
                detail: format!("{} attack(s) could not be executed", rejected.len()),
            });
        }
    }
    // Dirty working tree: the record must explain why nothing was verified,
    // because replay would have inspected `HEAD` rather than the working tree.
    if report.pointer("/repository/dirty").and_then(Value::as_bool) == Some(true) {
        let detail = string_at(report, "/repository/refusal_reason")
            .filter(|r| !r.is_empty())
            .unwrap_or_else(|| {
                "working tree was dirty; replay would have inspected HEAD".to_string()
            });
        findings.push(Finding {
            key: "repository_dirty".into(),
            detail,
        });
    }
    if let Some(ran) = report.pointer("/self_grill/ran").and_then(Value::as_bool) {
        if ran {
            if let Some(refuted) = report
                .pointer("/self_grill/refuted")
                .and_then(Value::as_bool)
            {
                findings.push(Finding {
                    key: "self_grill".into(),
                    detail: if refuted {
                        "self-grill found a structural inconsistency".into()
                    } else {
                        "self-grill passed".into()
                    },
                });
            }
        } else if let Some(skipped) = string_at(report, "/self_grill/skipped_reason") {
            findings.push(Finding {
                key: "self_grill_skipped".into(),
                detail: skipped,
            });
        }
    }
    if findings.is_empty() {
        findings.push(Finding {
            key: "report_shape".into(),
            detail: "report did not match the expected check_completion shape".into(),
        });
    }
    findings.truncate(MAX_FINDINGS);

    ReportSummary {
        verdict,
        reason_code,
        required_action,
        required_action_message,
        findings,
    }
}

fn string_at(value: &Value, pointer: &str) -> Option<String> {
    value
        .pointer(pointer)
        .and_then(Value::as_str)
        .map(str::to_string)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_report(verdict: &str) -> Value {
        serde_json::json!({
            "schema_version": "m8.0",
            "verdict": verdict,
            "reason_code": "EVIDENCE_MATCHED",
            "required_action": "none",
            "required_action_message": "",
            "context": { "status": "matched", "reason": "task matches repo" },
            "assessment": { "initial": { "status": "verified" }, "final": { "status": "verified" } },
            "attacks": { "coverage": { "refuted": 0, "executed": 1 }, "rejected": [] },
            "self_grill": { "ran": true, "refuted": false }
        })
    }

    #[test]
    fn summarizes_pass_report() {
        let summary = summarize_report(&sample_report("pass"));
        assert_eq!(summary.verdict, "pass");
        assert_eq!(summary.reason_code, "EVIDENCE_MATCHED");
        assert!(summary
            .findings
            .iter()
            .any(|f| f.key == "self_grill" && f.detail == "self-grill passed"));
    }

    #[test]
    fn summarizes_block_report_with_self_grill_refutation() {
        let mut report = sample_report("block");
        report["reason_code"] = "SELF_GRILL_REFUTED".into();
        report["self_grill"]["refuted"] = true.into();
        let summary = summarize_report(&report);
        assert_eq!(summary.verdict, "block");
        assert!(summary
            .findings
            .iter()
            .any(|f| f.key == "self_grill" && f.detail.contains("inconsistency")));
    }

    #[test]
    fn malformed_report_is_defensive() {
        let summary = summarize_report(&serde_json::json!({"weird": true}));
        assert_eq!(summary.verdict, "unknown");
        assert!(summary.findings.iter().any(|f| f.key == "report_shape"));
    }

    #[test]
    fn findings_are_bounded() {
        let mut report = sample_report("pass");
        report["attacks"]["rejected"] = serde_json::json!(["a", "b", "c"]);
        let summary = summarize_report(&report);
        assert!(summary.findings.len() <= MAX_FINDINGS);
    }
}
