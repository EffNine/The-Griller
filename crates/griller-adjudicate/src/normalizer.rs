//! Claim normalization.
//!
//! Normalization turns a natural-language claim into structured predicates. It
//! **proposes** claim structure; it is never allowed to propose an epistemic
//! status or a verdict.
//!
//! Two modes are supported:
//!
//! * **Deterministic** — [`DeterministicNormalizer`], rule-based, works without
//!   any LLM and always succeeds.
//! * **LLM-backed** — injected through the [`ClaimNormalizer`] trait. LLM
//!   output passes through the strict boundary in [`crate::llm`]; anything that
//!   is not claim structure is rejected or ignored. See
//!   [`FallbackNormalizer`] for safe degradation.
//!
//! The original verbatim claim text is preserved on every [`NormalizedClaim`].

use crate::claim::{ClaimInput, ClaimKind, ClaimTarget, TargetScope};
use crate::claim::{CountSpec, NumericAssertion};
use crate::error::NormalizationError;
use crate::normalized::{NormalizationMode, NormalizationQuality, NormalizedClaim};
use crate::predicate::{Polarity, Proposition};

/// The normalization boundary.
///
/// A normalizer consumes a raw [`ClaimInput`] and proposes a
/// [`NormalizedClaim`]. Implementations must not invent epistemic status or
/// verdicts: the [`NormalizedClaim`] type cannot represent them.
pub trait ClaimNormalizer: Send + Sync {
    /// Normalize the claim input.
    fn normalize(&self, input: &ClaimInput) -> Result<NormalizedClaim, NormalizationError>;
}

// ---------------------------------------------------------------------------
// Deterministic normalizer
// ---------------------------------------------------------------------------

/// Deterministic, rule-based claim normalizer.
///
/// Never requires an LLM and never fails: at worst it returns a claim with
/// `NormalizationQuality::Unrecognized`, which the matcher treats as opaque.
#[derive(Debug, Clone, Default)]
pub struct DeterministicNormalizer;

impl ClaimNormalizer for DeterministicNormalizer {
    fn normalize(&self, input: &ClaimInput) -> Result<NormalizedClaim, NormalizationError> {
        Ok(normalize_deterministic(input))
    }
}

/// The rule-based normalization function.
///
/// This is a pure function: same input, same result, always.
pub fn normalize_deterministic(input: &ClaimInput) -> NormalizedClaim {
    let text = input.text.trim().to_string();

    let kind = input.kind.unwrap_or_else(|| infer_kind(&text));
    let target = input.target.clone().unwrap_or_else(|| infer_target(&text));

    let tokens = tokenize(&text);
    let proposition = build_proposition(&text, &tokens, &target);
    let quality = quality_of(&proposition, &target);
    let numeric_assertions = detect_numeric_assertions(&text);

    NormalizedClaim::new(
        text,
        input.source,
        kind,
        target,
        proposition,
        input.requirement_provenance,
        NormalizationMode::Deterministic,
        quality,
        numeric_assertions,
    )
}

// ---------------------------------------------------------------------------
// Vocabulary
// ---------------------------------------------------------------------------

/// Stop words ignored when extracting subjects and objects.
const STOP_WORDS: &[&str] = &[
    "the", "this", "that", "these", "those", "i", "we", "my", "our", "you", "your", "it", "its",
    "to", "for", "of", "and", "or", "in", "on", "at", "with", "from", "about", "has", "have",
    "had", "been", "be", "was", "were", "is", "are", "a", "an", "as", "by", "all", "any", "some",
    "there", "they", "their", "them", "should", "can", "will", "would", "now",
];

/// Predicate verbs and their canonical form.
const PRED_PASS: &[&str] = &[
    "pass",
    "passes",
    "passing",
    "passed",
    "succeed",
    "succeeds",
    "succeeding",
    "succeeded",
    "green",
];
const PRED_FAIL: &[&str] = &["fail", "fails", "failing", "failed", "broken"];
const PRED_BUILD: &[&str] = &[
    "build",
    "builds",
    "building",
    "built",
    "compile",
    "compiles",
    "compiling",
    "compiled",
];
const PRED_FORMAT: &[&str] = &["format", "formats", "formatting", "formatted", "fmt"];
const PRED_CHANGE: &[&str] = &[
    "change",
    "changes",
    "changed",
    "modify",
    "modifies",
    "modified",
    "update",
    "updates",
    "updated",
    "edit",
    "edits",
    "edited",
    "fix",
    "fixes",
    "fixed",
    "implement",
    "implements",
    "implemented",
    "add",
    "adds",
    "added",
    "create",
    "creates",
    "created",
    "write",
    "wrote",
    "written",
];
const PRED_REMOVE: &[&str] = &[
    "remove", "removes", "removed", "delete", "deletes", "deleted", "drop", "drops", "dropped",
];
const PRED_EXIST: &[&str] = &[
    "exist", "exists", "existed", "present", "remains", "contain", "contains",
];

/// Words that negate a proposition when they precede the verb.
const NEGATION: &[&str] = &[
    "not", "no", "never", "don't", "dont", "doesn't", "doesnt", "didn't", "didnt", "won't", "wont",
    "cannot", "can't", "cant",
];

/// Kind inference markers.
const REQUIREMENT_MARKERS: &[&str] = &["should", "must", "needs", "needed", "required", "ought"];
const CAUSAL_MARKERS: &[&str] = &[
    "causes",
    "caused",
    "because",
    "due to",
    "leads to",
    "results in",
];
const DESIGN_MARKERS: &[&str] = &["design", "designed", "architecture"];

/// Scope marker words.
const TEST_MARKERS: &[&str] = &["test", "tests", "testing", "suite"];
const BUILD_MARKERS: &[&str] = &["build", "builds", "compiles", "compiling", "compiled"];
const FORMAT_MARKERS: &[&str] = &["format", "formats", "formatting", "fmt"];
const FUNCTION_MARKERS: &[&str] = &["function", "functions", "method", "methods"];
const MODULE_MARKERS: &[&str] = &[
    "module", "modules", "crate", "crates", "package", "packages",
];
const COMMAND_MARKERS: &[&str] = &["command", "commands", "build command"];
const ARTIFACT_MARKERS: &[&str] = &["artifact", "artifacts", "binary", "binaries"];

/// Quantifier/scope words that describe the *breadth* of a claim ("all
/// workspace tests pass"), not a named isolation target. A whole-suite run
/// supports such claims, so they must never become named test filters.
const SCOPE_QUANTIFIERS: &[&str] = &[
    "whole",
    "entire",
    "full",
    "workspace",
    "workspaces",
    "repository",
    "repo",
];

// ---------------------------------------------------------------------------
// Tokenization
// ---------------------------------------------------------------------------

/// Tokenize a claim on non-alphanumeric boundaries, keeping apostrophes so
/// contractions like `doesn't` survive as a single token.
fn tokenize(text: &str) -> Vec<String> {
    text.split(|c: char| !c.is_alphanumeric() && c != '\'')
        .map(str::to_lowercase)
        .filter(|t| !t.is_empty())
        .collect()
}

fn is_stop_word(token: &str) -> bool {
    STOP_WORDS.contains(&token)
}

fn is_negation(token: &str) -> bool {
    NEGATION.contains(&token)
}

fn is_predicate_word(token: &str) -> bool {
    PRED_PASS.contains(&token)
        || PRED_FAIL.contains(&token)
        || PRED_BUILD.contains(&token)
        || PRED_FORMAT.contains(&token)
        || PRED_CHANGE.contains(&token)
        || PRED_REMOVE.contains(&token)
        || PRED_EXIST.contains(&token)
}

fn contains_any(text: &str, markers: &[&str]) -> bool {
    markers.iter().any(|m| text.contains(m))
}

// ---------------------------------------------------------------------------
// Kind inference
// ---------------------------------------------------------------------------

/// Infer a claim kind from surface markers.
fn infer_kind(text: &str) -> ClaimKind {
    let lower = text.to_lowercase();
    if contains_any(&lower, REQUIREMENT_MARKERS) {
        ClaimKind::Requirement
    } else if contains_any(&lower, CAUSAL_MARKERS) {
        ClaimKind::Causal
    } else if contains_any(&lower, DESIGN_MARKERS) {
        ClaimKind::Design
    } else if contains_any(&lower, PRED_EXIST) {
        ClaimKind::Fact
    } else if has_completion_predicate(&lower) {
        ClaimKind::Completion
    } else {
        ClaimKind::Fact
    }
}

fn has_completion_predicate(text: &str) -> bool {
    let tokens = tokenize(text);
    tokens.iter().any(|t| is_predicate_word(t))
}

// ---------------------------------------------------------------------------
// Target inference
// ---------------------------------------------------------------------------

/// Try to extract a path-like token (carries a dotted file extension).
///
/// A bare slash is not enough: prose compounds like "pass/fail" or
/// "timeout/double-execution" contain `/` but are not paths. Treating them as
/// file targets made clean-git-diff evidence *contradict* honest claims. A
/// token counts as a path only when some segment has an extension
/// (`src/lib.rs`, `README.md`).
fn extract_path_like(text: &str) -> Option<String> {
    for word in text.split_whitespace() {
        let cleaned = word.trim_matches(|c: char| {
            !c.is_alphanumeric() && c != '/' && c != '.' && c != '_' && c != '-' && c != '\\'
        });
        // A sentence-ending period is punctuation, not part of the path.
        let cleaned = cleaned.trim_end_matches('.');
        let has_dot_ext = cleaned
            .rsplit_once('.')
            .map(|(stem, ext)| {
                !stem.is_empty()
                    && !ext.is_empty()
                    && ext.len() <= 6
                    && ext.chars().all(|c| c.is_ascii_alphanumeric())
                    && !ext.chars().all(|c| c.is_ascii_digit())
            })
            .unwrap_or(false);
        if has_dot_ext {
            return Some(cleaned.to_string());
        }
    }
    None
}

/// Infer a target scope from the claim text.
fn infer_target(text: &str) -> ClaimTarget {
    let lower = text.to_lowercase();

    if let Some(path) = extract_path_like(text) {
        return ClaimTarget::new(TargetScope::File, Some(path));
    }

    let tokens = tokenize(&lower);

    if let Some((idx, _)) = tokens
        .iter()
        .enumerate()
        .find(|(_, t)| TEST_MARKERS.contains(&t.as_str()))
    {
        // An enumerated verification list ("cargo fmt, cargo check, cargo
        // test — all exit 0") describes the repository's whole local
        // verification, not one named filter; a suite replay decides it.
        let enumerates_other_domains = tokens
            .iter()
            .any(|t| BUILD_MARKERS.contains(&t.as_str()) || FORMAT_MARKERS.contains(&t.as_str()));
        let name = if enumerates_other_domains {
            None
        } else {
            descriptor_before(&tokens, idx)
        };
        return ClaimTarget::new(TargetScope::Test, name);
    }
    if tokens.iter().any(|t| BUILD_MARKERS.contains(&t.as_str())) {
        return ClaimTarget::new(TargetScope::Command, Some("build".into()));
    }
    if tokens.iter().any(|t| FORMAT_MARKERS.contains(&t.as_str())) {
        return ClaimTarget::new(TargetScope::Command, Some("format".into()));
    }
    if let Some((idx, _)) = tokens
        .iter()
        .enumerate()
        .find(|(_, t)| FUNCTION_MARKERS.contains(&t.as_str()))
    {
        let name = descriptor_around(&tokens, idx);
        return ClaimTarget::new(TargetScope::Function, name);
    }
    if let Some((idx, _)) = tokens
        .iter()
        .enumerate()
        .find(|(_, t)| MODULE_MARKERS.contains(&t.as_str()))
    {
        let name = descriptor_around(&tokens, idx);
        return ClaimTarget::new(TargetScope::Module, name);
    }
    if tokens.iter().any(|t| COMMAND_MARKERS.contains(&t.as_str())) {
        return ClaimTarget::new(TargetScope::Command, None);
    }
    if tokens
        .iter()
        .any(|t| ARTIFACT_MARKERS.contains(&t.as_str()))
    {
        return ClaimTarget::new(TargetScope::Artifact, None);
    }
    if tokens.iter().any(|t| t == "file") {
        return ClaimTarget::new(TargetScope::File, None);
    }

    ClaimTarget::repository()
}

/// Content words in the three-token window before `idx` (stop words and
/// scope markers excluded).
///
/// The narrow window keeps the descriptor adjacent to its scope marker;
/// anything farther back is narration ("Fixed the ... bugs"), not the name
/// of a test filter.
fn descriptor_before(tokens: &[String], idx: usize) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    let start = idx.saturating_sub(3);
    for t in tokens[start..idx].iter() {
        if is_stop_word(t)
            || is_predicate_word(t)
            || is_negation(t)
            || SCOPE_QUANTIFIERS.contains(&t.as_str())
        {
            continue;
        }
        if TEST_MARKERS.contains(&t.as_str())
            || FUNCTION_MARKERS.contains(&t.as_str())
            || MODULE_MARKERS.contains(&t.as_str())
        {
            continue;
        }
        parts.push(t.clone());
        if parts.len() >= 4 {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("_"))
    }
}

/// Descriptor around a scope marker: words before it, else the word right
/// after it (e.g. "function login exists" -> "login").
fn descriptor_around(tokens: &[String], idx: usize) -> Option<String> {
    if let Some(before) = descriptor_before(tokens, idx) {
        return Some(before);
    }
    tokens
        .get(idx + 1)
        .filter(|t| !is_stop_word(t) && !is_predicate_word(t) && !is_scope_marker(t))
        .cloned()
}

// ---------------------------------------------------------------------------
// Proposition building
// ---------------------------------------------------------------------------

/// Build the canonical proposition from tokens + resolved target.
fn build_proposition(_text: &str, tokens: &[String], target: &ClaimTarget) -> Proposition {
    match extract_verb(tokens) {
        Some((canonical, polarity)) => {
            let verb_pos = tokens
                .iter()
                .position(|t| is_predicate_word(t))
                .unwrap_or(0);
            let subject = extract_subject(tokens, verb_pos, target);
            let object = extract_object(tokens, verb_pos);
            Proposition {
                subject,
                predicate: canonical,
                polarity,
                object,
            }
        }
        None => Proposition {
            subject: fallback_subject(target),
            predicate: "unknown".into(),
            polarity: Polarity::Positive,
            object: None,
        },
    }
}

/// Find the canonical predicate verb and its polarity.
///
/// Narration verbs ("fixed", "removed") never outrank verification verbs: a
/// completion claim that both narrates work and asserts an outcome is judged
/// on the outcome — the part environment evidence can actually decide.
/// Otherwise the earliest verb in the claim wins. Exception: an enumerated
/// verification list ("cargo fmt, cargo check, cargo test — all exit 0")
/// asserts one suite-level outcome, so the test-outcome verb is canonical and
/// the suite replay decides it.
fn extract_verb(tokens: &[String]) -> Option<(String, Polarity)> {
    let is_narration =
        |t: &str| PRED_EXIST.contains(&t) || PRED_CHANGE.contains(&t) || PRED_REMOVE.contains(&t);
    let is_verification = |t: &str| {
        PRED_PASS.contains(&t)
            || PRED_FAIL.contains(&t)
            || PRED_BUILD.contains(&t)
            || PRED_FORMAT.contains(&t)
    };
    let is_test_outcome = |t: &str| PRED_PASS.contains(&t) || PRED_FAIL.contains(&t);

    let has_test_marker = tokens.iter().any(|t| TEST_MARKERS.contains(&t.as_str()));
    let enumerates_other_domain = tokens
        .iter()
        .any(|t| BUILD_MARKERS.contains(&t.as_str()) || FORMAT_MARKERS.contains(&t.as_str()));
    if has_test_marker && enumerates_other_domain {
        return scan_verb(tokens, &is_test_outcome).or_else(|| scan_verb(tokens, &is_verification));
    }

    scan_verb(tokens, &is_verification).or_else(|| scan_verb(tokens, &is_narration))
}

/// Left-to-right scan returning the first token accepted by `filter` and
/// recognized as a predicate verb.
fn scan_verb(tokens: &[String], filter: &dyn Fn(&str) -> bool) -> Option<(String, Polarity)> {
    for (i, t) in tokens.iter().enumerate() {
        if !filter(t) {
            continue;
        }
        let negated = has_negation_before(tokens, i);
        if PRED_PASS.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("pass".into(), polarity));
        }
        if PRED_FAIL.contains(&t.as_str()) {
            // "fail" canonicalises to "pass" with negative polarity, unless the
            // domain is build/format, in which case negate that domain verb.
            let domain = if tokens.iter().any(|w| BUILD_MARKERS.contains(&w.as_str())) {
                "build"
            } else if tokens.iter().any(|w| FORMAT_MARKERS.contains(&w.as_str())) {
                "format"
            } else {
                "pass"
            };
            let polarity = if negated {
                Polarity::Positive
            } else {
                Polarity::Negative
            };
            return Some((domain.into(), polarity));
        }
        if PRED_BUILD.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("build".into(), polarity));
        }
        if PRED_FORMAT.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("format".into(), polarity));
        }
        if PRED_REMOVE.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("remove".into(), polarity));
        }
        if PRED_CHANGE.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("change".into(), polarity));
        }
        if PRED_EXIST.contains(&t.as_str()) {
            let polarity = if negated {
                Polarity::Negative
            } else {
                Polarity::Positive
            };
            return Some(("exist".into(), polarity));
        }
    }
    None
}

/// True when a negation word appears within the two tokens before `idx`.
fn has_negation_before(tokens: &[String], idx: usize) -> bool {
    let start = idx.saturating_sub(2);
    tokens[start..idx].iter().any(|t| is_negation(t))
}

/// Content words before the verb, excluding stop words and scope markers.
fn extract_subject(tokens: &[String], verb_idx: usize, target: &ClaimTarget) -> String {
    let mut parts: Vec<String> = Vec::new();
    for t in tokens[..verb_idx].iter() {
        if is_stop_word(t) || is_predicate_word(t) || is_negation(t) {
            continue;
        }
        if is_scope_marker(t) {
            continue;
        }
        parts.push(t.clone());
    }
    let joined = parts.join("_");
    if joined.is_empty() {
        fallback_subject(target)
    } else {
        joined
    }
}

fn is_scope_marker(t: &str) -> bool {
    TEST_MARKERS.contains(&t)
        || BUILD_MARKERS.contains(&t)
        || FORMAT_MARKERS.contains(&t)
        || FUNCTION_MARKERS.contains(&t)
        || MODULE_MARKERS.contains(&t)
        || COMMAND_MARKERS.contains(&t)
        || ARTIFACT_MARKERS.contains(&t)
        || t == "file"
}

/// Content words after the verb, excluding stop words and scope markers.
fn extract_object(tokens: &[String], verb_idx: usize) -> Option<String> {
    let mut parts: Vec<String> = Vec::new();
    for t in tokens[verb_idx + 1..].iter() {
        if is_stop_word(t) || is_predicate_word(t) || is_negation(t) {
            continue;
        }
        if is_scope_marker(t) {
            continue;
        }
        parts.push(t.clone());
        if parts.len() >= 4 {
            break;
        }
    }
    if parts.is_empty() {
        None
    } else {
        Some(parts.join("_"))
    }
}

/// Default subject when no content words precede the verb.
fn fallback_subject(target: &ClaimTarget) -> String {
    match (&target.scope, &target.name) {
        (TargetScope::Test, Some(name)) => name.clone(),
        (TargetScope::Test, None) => "tests".into(),
        (TargetScope::Command, Some(name)) => name.clone(),
        (TargetScope::Command, None) => "command".into(),
        (TargetScope::File, Some(name)) => name.to_lowercase().replace(['/', '.'], "_"),
        (TargetScope::File, None) => "file".into(),
        (TargetScope::Function, Some(name)) => name.clone(),
        (TargetScope::Module, Some(name)) => name.clone(),
        (TargetScope::Artifact, Some(name)) => name.clone(),
        _ => "repository".into(),
    }
}

fn quality_of(proposition: &Proposition, target: &ClaimTarget) -> NormalizationQuality {
    if proposition.predicate == "unknown" {
        return NormalizationQuality::Unrecognized;
    }
    if (target.scope == TargetScope::Unknown || target.scope == TargetScope::Repository)
        && target.name.is_none()
    {
        return NormalizationQuality::Partial;
    }
    if proposition.subject.is_empty() {
        return NormalizationQuality::Partial;
    }
    NormalizationQuality::Complete
}

// ---------------------------------------------------------------------------
// Numeric assertion detection
// ---------------------------------------------------------------------------

/// Detect numeric completion assertions from claim text.
///
/// Returns `None` when no numeric assertion is recognized, preserving existing
/// behavior for all non-numeric claims. Only creates assertions when the
/// surrounding syntax clearly expresses a test/result count.
///
/// Patterns that produce assertions:
///   - "<number> tests pass."                     → Exact(n)
///   - "at least <number> tests pass."            → AtLeast(n)
///   - "<number> tests pass with <number> failures" → Exact(n), failed=Some(m)
///   - "<number> tests pass, <number> skipped"   → Exact(n), skipped=Some(m)
///   - "<number> <scope> tests pass."             → Exact(n), qualifier=Some(scope)
///
/// Patterns that must NOT produce assertions (false positives avoided):
///   - version numbers: "Release v0.2.0 is complete."
///   - issue numbers: "Fixed issue #154."
///   - line numbers: "Fixed at lines 154-160."
///   - arbitrary numeric prose
fn detect_numeric_assertions(text: &str) -> Vec<NumericAssertion> {
    let lower = text.to_lowercase();
    let tokens: Vec<&str> = lower
        .split(|c: char| !c.is_alphanumeric() && c != '\'')
        .filter(|t| !t.is_empty())
        .collect();

    let mut assertions = Vec::new();

    // Helper: try to parse a u64 from a token string
    let parse_u64 = |s: &str| -> Option<u64> { s.parse::<u64>().ok() };

    // Helper: check if the token immediately before the word at `tests_idx`
    // is a valid number (not a version, issue, line number, or port number).
    // Returns None when there is no preceding token.
    let is_valid_number_before_tests = |tests_idx: usize| -> Option<u64> {
        let num_idx = tests_idx.checked_sub(1)?;
        let t = tokens.get(num_idx)?;
        // Excluded: starts with 'v' (version)
        if t.starts_with('v') {
            return None;
        }
        // Excluded: preceded by '#' (issue number)
        if num_idx > 0 {
            let prev = tokens.get(num_idx - 1).unwrap_or(&"");
            if prev == &"#" {
                return None;
            }
        }
        // Excluded: preceded by "lines" (line number)
        if num_idx > 1 {
            let prev2 = tokens.get(num_idx - 2).unwrap_or(&"");
            if prev2 == &"lines" {
                return None;
            }
        }
        // Excluded: preceded by "port" (port number)
        if num_idx > 0 {
            let prev = tokens.get(num_idx - 1).unwrap_or(&"");
            if prev == &"port" {
                return None;
            }
        }
        parse_u64(t)
    };

    // Helper: check if token at position i is a scope marker word
    let is_scope_marker = |i: usize| -> bool {
        let t = tokens.get(i).unwrap_or(&"");
        matches!(
            *t,
            "rust"
                | "cargo"
                | "node"
                | "npm"
                | "pytest"
                | "python"
                | "go"
                | "test"
                | "tests"
                | "plugin"
                | "extraction"
        )
    };

    // Pattern: "at least <number> tests pass"
    if contains_any(&lower, &["at least"]) {
        if let Some(at_least_pos) = lower.find("at least") {
            let after = &lower[at_least_pos + "at least".len()..];
            let mut rest: Vec<&str> = after
                .split(|c: char| !c.is_alphanumeric() && c != '\'')
                .filter(|t| !t.is_empty())
                .collect();
            // Expect: number, "tests", "pass"
            if let Some(num_str) = rest.first().and_then(|t| parse_u64(t)) {
                rest.remove(0);
                if rest.len() >= 2 && rest[0] == "tests" && rest[1] == "pass" {
                    assertions.push(NumericAssertion {
                        expected_passed: CountSpec::AtLeast(num_str),
                        expected_failed: None,
                        expected_skipped: None,
                        scope_qualifier: None,
                    });
                }
            }
        }
    }

    // Pattern: "<number> tests pass" or "<number> <scope> tests pass"
    for i in 0..tokens.len() {
        // Check for "tests pass" at positions i, i+1
        if i + 1 < tokens.len() && tokens[i] == "tests" && tokens[i + 1] == "pass" {
            // Number must be immediately before "tests" at position i-1
            if let Some(count) = is_valid_number_before_tests(i) {
                let mut scope: Option<String> = None;
                // Check for scope qualifier before the number (at i-2, i-3, etc.)
                if i >= 2 && is_scope_marker(i - 2) {
                    // Ensure it's not preceded by "at" which would make it an exclusion
                    if i < 3 || tokens[i - 3] != "at" {
                        scope = Some(tokens[i - 2].to_string());
                    }
                }
                assertions.push(NumericAssertion {
                    expected_passed: CountSpec::Exact(count),
                    expected_failed: None,
                    expected_skipped: None,
                    scope_qualifier: scope,
                });
            }
            break; // Only match first "tests pass" occurrence
        }
        // Check for "test pass" (singular) at positions i, i+1
        if i + 1 < tokens.len() && tokens[i] == "test" && tokens[i + 1] == "pass" {
            if let Some(count) = is_valid_number_before_tests(i) {
                assertions.push(NumericAssertion {
                    expected_passed: CountSpec::Exact(count),
                    expected_failed: None,
                    expected_skipped: None,
                    scope_qualifier: None,
                });
            }
            break;
        }
    }

    // Pattern: "<number> tests pass with <number> failures"
    for i in 0..tokens.len() {
        // Check for "tests pass with" at positions i, i+1, i+2
        if i + 3 < tokens.len()
            && tokens[i] == "tests"
            && tokens[i + 1] == "pass"
            && tokens[i + 2] == "with"
        {
            if let Some(count) = is_valid_number_before_tests(i) {
                if i + 5 < tokens.len() && tokens[i + 3] == "failures" {
                    if let Some(fail_count) = parse_u64(tokens[i + 4]) {
                        let mut scope: Option<String> = None;
                        if i > 0 {
                            let potential_scope = tokens[i - 1].to_string();
                            if is_scope_marker(i - 1) && i > 1 && tokens[i - 2] != "at" {
                                scope = Some(potential_scope);
                            }
                        }
                        assertions.push(NumericAssertion {
                            expected_passed: CountSpec::Exact(count),
                            expected_failed: Some(fail_count),
                            expected_skipped: None,
                            scope_qualifier: scope,
                        });
                    }
                }
            }
            break;
        }
        // Check for "test pass with"
        if i + 3 < tokens.len()
            && tokens[i] == "test"
            && tokens[i + 1] == "pass"
            && tokens[i + 2] == "with"
        {
            if let Some(count) = is_valid_number_before_tests(i) {
                if i + 5 < tokens.len() && tokens[i + 3] == "failures" {
                    if let Some(fail_count) = parse_u64(tokens[i + 4]) {
                        assertions.push(NumericAssertion {
                            expected_passed: CountSpec::Exact(count),
                            expected_failed: Some(fail_count),
                            expected_skipped: None,
                            scope_qualifier: None,
                        });
                    }
                }
            }
            break;
        }
    }

    // Pattern: "<number> tests pass, <number> skipped"
    for i in 0..tokens.len() {
        // Check for "tests pass," at positions i, i+1, i+2 (comma at i+2)
        if i + 3 < tokens.len()
            && tokens[i] == "tests"
            && tokens[i + 1] == "pass"
            && tokens[i + 2] == ","
        {
            if let Some(count) = is_valid_number_before_tests(i) {
                if i + 5 < tokens.len() && tokens[i + 3] == "skipped" {
                    if let Some(skip_count) = parse_u64(tokens[i + 4]) {
                        let mut scope: Option<String> = None;
                        if i > 0 {
                            let potential_scope = tokens[i - 1].to_string();
                            if is_scope_marker(i - 1) && i > 1 && tokens[i - 2] != "at" {
                                scope = Some(potential_scope);
                            }
                        }
                        assertions.push(NumericAssertion {
                            expected_passed: CountSpec::Exact(count),
                            expected_failed: None,
                            expected_skipped: Some(skip_count),
                            scope_qualifier: scope,
                        });
                    }
                }
            }
            break;
        }
        // Check for "test pass,"
        if i + 3 < tokens.len()
            && tokens[i] == "test"
            && tokens[i + 1] == "pass"
            && tokens[i + 2] == ","
        {
            if let Some(count) = is_valid_number_before_tests(i) {
                if i + 5 < tokens.len() && tokens[i + 3] == "skipped" {
                    if let Some(skip_count) = parse_u64(tokens[i + 4]) {
                        assertions.push(NumericAssertion {
                            expected_passed: CountSpec::Exact(count),
                            expected_failed: None,
                            expected_skipped: Some(skip_count),
                            scope_qualifier: None,
                        });
                    }
                }
            }
            break;
        }
    }

    assertions
}

/// A normalizer that prefers an injected (e.g. LLM-backed) normalizer and
/// falls back to deterministic normalization whenever the injected one fails.
///
/// This guarantees that malformed or malicious LLM output can never abort the
/// pipeline: it degrades to the deterministic path, and it can never produce
/// an epistemic status.
pub struct FallbackNormalizer {
    inner: Option<Box<dyn ClaimNormalizer>>,
    deterministic: DeterministicNormalizer,
}

impl FallbackNormalizer {
    /// Wrap an optional injected normalizer.
    pub fn over(inner: Option<Box<dyn ClaimNormalizer>>) -> Self {
        Self {
            inner,
            deterministic: DeterministicNormalizer,
        }
    }
}

impl ClaimNormalizer for FallbackNormalizer {
    fn normalize(&self, input: &ClaimInput) -> Result<NormalizedClaim, NormalizationError> {
        match &self.inner {
            Some(inner) => match inner.normalize(input) {
                Ok(nc) => Ok(nc),
                Err(_) => self.deterministic.normalize(input),
            },
            None => self.deterministic.normalize(input),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::claim::{ClaimInput, ClaimSource};
    use crate::predicate::Polarity;

    fn normalize(text: &str) -> NormalizedClaim {
        normalize_deterministic(&ClaimInput::new(text, ClaimSource::User))
    }

    #[test]
    fn original_text_is_preserved() {
        let text = "I fixed the authentication timeout.";
        let nc = normalize(text);
        assert_eq!(nc.original, text);
        assert_eq!(nc.proposition.predicate, "change");
        assert_eq!(
            nc.proposition.object.as_deref(),
            Some("authentication_timeout")
        );
        assert_eq!(nc.mode, NormalizationMode::Deterministic);
    }

    #[test]
    fn tests_pass_normalizes_to_pass() {
        let nc = normalize("Tests pass.");
        assert_eq!(nc.proposition.subject, "tests");
        assert_eq!(nc.proposition.predicate, "pass");
        assert_eq!(nc.proposition.polarity, Polarity::Positive);
        assert_eq!(nc.target.scope, TargetScope::Test);
        assert_eq!(nc.kind, ClaimKind::Completion);
    }

    #[test]
    fn tests_fail_negates_pass() {
        let nc = normalize("Tests fail.");
        assert_eq!(nc.proposition.predicate, "pass");
        assert_eq!(nc.proposition.polarity, Polarity::Negative);
    }

    #[test]
    fn named_test_target_captures_descriptor() {
        let nc = normalize("Authentication tests pass.");
        assert_eq!(nc.proposition.subject, "authentication");
        assert_eq!(nc.target.scope, TargetScope::Test);
        assert_eq!(nc.target.name.as_deref(), Some("authentication"));
    }

    #[test]
    fn build_claim() {
        let nc = normalize("The project builds successfully.");
        assert_eq!(nc.proposition.predicate, "build");
        assert_eq!(nc.proposition.subject, "project");
        assert_eq!(nc.target.scope, TargetScope::Command);
    }

    #[test]
    fn formatting_claim() {
        let nc = normalize("Project formatting passes.");
        assert_eq!(nc.proposition.predicate, "format");
        assert_eq!(nc.proposition.polarity, Polarity::Positive);
    }

    #[test]
    fn file_removal_claim_extracts_path() {
        let nc = normalize("The file src/foo.rs was removed.");
        assert_eq!(nc.proposition.predicate, "remove");
        assert_eq!(nc.target.scope, TargetScope::File);
        assert_eq!(nc.target.name.as_deref(), Some("src/foo.rs"));
    }

    #[test]
    fn requirement_kind_inference() {
        let nc = normalize("The system should keep authentication compatible.");
        assert_eq!(nc.kind, ClaimKind::Requirement);
    }

    #[test]
    fn unrecognized_claim_is_flagged() {
        let nc = normalize("Hello there, friend.");
        assert_eq!(nc.proposition.predicate, "unknown");
        assert_eq!(nc.quality, NormalizationQuality::Unrecognized);
    }

    #[test]
    fn explicit_kind_and_target_win() {
        let input = ClaimInput::new("authentication still works", ClaimSource::Agent)
            .with_kind(ClaimKind::Completion)
            .with_target(ClaimTarget::new(
                TargetScope::Test,
                Some("authentication".into()),
            ));
        let nc = normalize_deterministic(&input);
        assert_eq!(nc.kind, ClaimKind::Completion);
        assert_eq!(nc.target.scope, TargetScope::Test);
        assert_eq!(nc.target.name.as_deref(), Some("authentication"));
    }

    #[test]
    fn proposition_key_is_stable() {
        let a = normalize("Tests pass.");
        let b = normalize("tests pass");
        assert_eq!(
            a.proposition.normalized_key(),
            b.proposition.normalized_key()
        );
        assert_eq!(a.proposition, b.proposition);
    }
}
