//! Best-effort deterministic secret redaction.
//!
//! This is a **conservative baseline**, not a DLP system and not a guarantee.
//! It detects obvious credential patterns in process output and replaces them
//! so that raw secrets never enter persisted evidence.
//!
//! Properties:
//! * deterministic — the same input always produces the same output and report
//! * the replacement token and the [`RedactionReport`] never contain secret
//!   material
//! * non-secret output passes through unchanged
//!
//! Detection is intentionally conservative: a pattern must look strongly like
//! a credential. Do **not** claim complete secret detection.

use regex::Regex;
use serde::{Deserialize, Serialize};

/// Categories of secret patterns the redactor can recognise.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RedactionCategory {
    /// `Bearer <token>` header values.
    BearerToken,
    /// Common API-key prefix forms (`sk-…`, `ghp_…`, `xox…`, `api_key=…`).
    ApiKey,
    /// PEM `-----BEGIN … PRIVATE KEY-----` blocks.
    PrivateKey,
    /// `password=…`, `secret: …` style assignment patterns.
    CredentialAssignment,
    /// Well-known cloud credential formats (AWS `AKIA…`, Google `AIza…`).
    CloudCredential,
}

impl RedactionCategory {
    fn label(self) -> &'static str {
        match self {
            RedactionCategory::BearerToken => "bearer_token",
            RedactionCategory::ApiKey => "api_key",
            RedactionCategory::PrivateKey => "private_key",
            RedactionCategory::CredentialAssignment => "credential_assignment",
            RedactionCategory::CloudCredential => "cloud_credential",
        }
    }
}

/// A count of redactions in one category.
#[derive(Debug, Clone, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub struct CategoryCount {
    /// The category.
    pub category: RedactionCategory,
    /// Number of distinct redactions in this category.
    pub count: usize,
}

/// What the redactor did to a piece of text.
///
/// Contains counts and categories only — never the redacted secret material.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize, Deserialize)]
pub struct RedactionReport {
    /// Whether any redaction occurred.
    pub occurred: bool,
    /// Total number of distinct redactions.
    pub total: usize,
    /// Per-category counts, sorted by category for determinism.
    pub categories: Vec<CategoryCount>,
}

impl RedactionReport {
    /// Merge another report into this one (used when redacting stdout and
    /// stderr together).
    pub fn merge(&mut self, other: &RedactionReport) {
        self.occurred |= other.occurred;
        self.total += other.total;
        for other_count in &other.categories {
            match self
                .categories
                .iter_mut()
                .find(|c| c.category == other_count.category)
            {
                Some(existing) => existing.count += other_count.count,
                None => self.categories.push(other_count.clone()),
            }
        }
        self.categories.sort();
    }
}

/// A match region to redact.
#[derive(Debug, Clone, Copy)]
struct Region {
    start: usize,
    end: usize,
    category: RedactionCategory,
}

/// A detection rule.
struct Rule {
    category: RedactionCategory,
    regex: Regex,
}

impl Rule {
    fn new(category: RedactionCategory, pattern: &str) -> Self {
        Self {
            category,
            regex: Regex::new(pattern)
                .unwrap_or_else(|e| panic!("invalid redaction regex {pattern:?}: {e}")),
        }
    }
}

/// The fixed, ordered set of detection rules.
fn rules() -> Vec<Rule> {
    vec![
        Rule::new(
            RedactionCategory::BearerToken,
            r"(?i)\bbearer\s+[A-Za-z0-9._~+/=-]{8,}",
        ),
        Rule::new(
            RedactionCategory::ApiKey,
            r"\b(?:sk-|pk-)[A-Za-z0-9._-]{8,}|\bgh[pousr]_[A-Za-z0-9]{20,}|\bxox[baprs]-[A-Za-z0-9-]{8,}",
        ),
        Rule::new(
            RedactionCategory::PrivateKey,
            r"(?s)-----BEGIN [A-Z ]*PRIVATE KEY-----.*?-----END [A-Z ]*PRIVATE KEY-----",
        ),
        Rule::new(
            RedactionCategory::CredentialAssignment,
            r#"(?i)(?:password|passwd|secret|token|access_token|api[_-]?key|client_secret)\s*[:=]\s*["']?[A-Za-z0-9._!@#$%^&*+\-/]{6,}"#,
        ),
        Rule::new(
            RedactionCategory::CloudCredential,
            r"\bAKIA[0-9A-Z]{16}\b|\bAIza[0-9A-Za-z_\-]{35}\b",
        ),
    ]
}

/// Redact obvious credentials in `text`.
///
/// Returns the redacted text and a report of what changed. The report never
/// contains the redacted material.
pub fn redact(text: &str) -> (String, RedactionReport) {
    let mut regions: Vec<Region> = Vec::new();
    for rule in rules() {
        for m in rule.regex.find_iter(text) {
            regions.push(Region {
                start: m.start(),
                end: m.end(),
                category: rule.category,
            });
        }
    }
    if regions.is_empty() {
        return (text.to_string(), RedactionReport::default());
    }

    regions.sort_by_key(|r| (r.start, r.end));
    let mut merged: Vec<Region> = Vec::new();
    for region in regions {
        match merged.last_mut() {
            Some(prev) if region.start < prev.end => {
                // Overlap: extend the earlier region, keep its category.
                prev.end = prev.end.max(region.end);
            }
            _ => merged.push(region),
        }
    }

    let mut output = String::with_capacity(text.len());
    let mut cursor = 0;
    let mut per_category: Vec<(RedactionCategory, usize)> = Vec::new();
    for region in &merged {
        output.push_str(&text[cursor..region.start]);
        output.push_str("[REDACTED-");
        output.push_str(region.category.label());
        output.push(']');
        cursor = region.end;
        match per_category.iter_mut().find(|(c, _)| *c == region.category) {
            Some((_, count)) => *count += 1,
            None => per_category.push((region.category, 1)),
        }
    }
    output.push_str(&text[cursor..]);

    per_category.sort();
    let report = RedactionReport {
        occurred: true,
        total: merged.len(),
        categories: per_category
            .into_iter()
            .map(|(category, count)| CategoryCount { category, count })
            .collect(),
    };
    (output, report)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearer_token_is_redacted() {
        let (out, report) = redact("Authorization: Bearer abcdefgh1234567890 end");
        assert!(out.contains("[REDACTED-bearer_token]"));
        assert!(!out.contains("abcdefgh1234567890"));
        assert_eq!(report.total, 1);
        assert_eq!(
            report.categories[0].category,
            RedactionCategory::BearerToken
        );
    }

    #[test]
    fn api_key_prefixes_are_redacted() {
        let (out, report) = redact(
            "key sk-ant-1234567890abcdef value ghp_123456789012345678901234567890123456 tail",
        );
        assert!(!out.contains("sk-ant-1234567890abcdef"));
        assert!(!out.contains("ghp_123456789012345678901234567890123456"));
        assert_eq!(report.total, 2);
    }

    #[test]
    fn private_key_block_is_redacted() {
        let block = "-----BEGIN RSA PRIVATE KEY-----\nMIIEowIBAAKCAQEA\nfakebase64\n-----END RSA PRIVATE KEY-----";
        let (out, report) = redact(block);
        assert!(out.contains("[REDACTED-private_key]"));
        assert!(!out.contains("MIIEowIBAAKCAQEA"));
        assert!(!out.contains("-----BEGIN RSA PRIVATE KEY-----"));
        assert_eq!(report.categories[0].category, RedactionCategory::PrivateKey);
    }

    #[test]
    fn assignment_patterns_are_redacted() {
        let (out, report) = redact("DB_PASSWORD=hunter2-super-secret and token=abc123def456");
        assert!(!out.contains("hunter2-super-secret"));
        assert!(!out.contains("abc123def456"));
        assert!(out.contains("[REDACTED-credential_assignment]"));
        assert_eq!(report.total, 2);
    }

    #[test]
    fn cloud_credentials_are_redacted() {
        let (out, report) = redact("AKIAIOSFODNN7EXAMPLE secret");
        assert!(!out.contains("AKIAIOSFODNN7EXAMPLE"));
        assert_eq!(
            report.categories[0].category,
            RedactionCategory::CloudCredential
        );
        assert_eq!(report.total, 1);
    }

    #[test]
    fn non_secret_text_passes_unchanged() {
        let text = "the build passed with 42 tests and no warnings";
        let (out, report) = redact(text);
        assert_eq!(out, text);
        assert!(!report.occurred);
        assert_eq!(report.total, 0);
    }

    #[test]
    fn report_never_contains_secret_material() {
        let secret = "Bearer super-secret-token-value-1234567890";
        let (_out, report) = redact(secret);
        let json = serde_json::to_string(&report).unwrap();
        assert!(!json.contains("super-secret-token-value"));
        assert!(!json.contains("1234567890"));
        assert!(json.contains("bearer_token"));
    }

    #[test]
    fn redaction_is_deterministic() {
        let text = "a: Bearer abcdefgh1234567890, b: sk-ant-1234567890abcdef, c: password=hunter2-super-secret";
        let (a, ra) = redact(text);
        let (b, rb) = redact(text);
        assert_eq!(a, b);
        assert_eq!(ra, rb);
    }
}
