//! Deterministic evidence matching.
//!
//! The matcher decides, for each piece of structured evidence, how it relates
//! to a normalized claim: `Supports`, `Contradicts`, `Irrelevant`,
//! `Insufficient`, or `Unverifiable`. It is fully deterministic and never
//! "guesses": when a claim cannot be matched confidently, the matcher returns
//! `Insufficient` / `Unverifiable` / `Irrelevant`.
//!
//! M2 matching focuses on coding-agent claims (test runs, builds, formatting,
//! file changes, entity presence). Full semantic understanding of arbitrary
//! claims is explicitly out of scope.
//!
//! ## Boundaries
//!
//! * Agent narration is **never** environment evidence and never supports a
//!   claim.
//! * A successful unrelated command does not support a claim (an
//!   `auth tests pass` claim is not supported by `cargo fmt --check`).
//! * A command that could not run (timeout, spawn failure) is
//!   `Unverifiable`, never `Contradicts`.

use griller_core::command::CommandSpec;
use griller_core::context::ProjectContext;
use griller_core::evidence::{Evidence, EvidenceContent, EvidenceKind};
use griller_core::provenance::Reproducibility;

use crate::normalized::NormalizedClaim;
use crate::predicate::Polarity;
use crate::relationship::{EvidenceMatch, EvidenceQuality, EvidenceRelationship, EvidenceStrength};

/// The deterministic match boundary.
pub trait EvidenceMatcher: Send + Sync {
    /// Match every evidence record against the claim.
    fn match_evidence(
        &self,
        claim: &NormalizedClaim,
        project_context: Option<&ProjectContext>,
        evidence: &[Evidence],
    ) -> Vec<EvidenceMatch>;
}

/// The broad class of claim the matcher reasons about.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum ClaimClass {
    /// "tests pass" style completion claim.
    TestsPass,
    /// "tests fail" style completion claim.
    TestsFail,
    /// "build succeeds".
    BuildSucceeds,
    /// "build fails".
    BuildFails,
    /// "formatting passes".
    FormattingPasses,
    /// "formatting fails".
    FormattingFails,
    /// A file/module/function was changed/added/fixed.
    FileChanged,
    /// A file/module/function was removed.
    FileRemoved,
    /// A named entity exists (file present, symbol present).
    EntityExists,
    /// A named entity does not exist.
    EntityAbsent,
    /// The claim could not be classified; no rule applies.
    Unknown,
}

/// Classify a normalized claim into a matcher rule class.
pub fn claim_class(claim: &NormalizedClaim) -> ClaimClass {
    let p = claim.proposition.predicate.as_str();
    match (p, claim.proposition.polarity) {
        ("pass", Polarity::Positive) => ClaimClass::TestsPass,
        ("pass", Polarity::Negative) => ClaimClass::TestsFail,
        ("build", Polarity::Positive) => ClaimClass::BuildSucceeds,
        ("build", Polarity::Negative) => ClaimClass::BuildFails,
        ("format", Polarity::Positive) => ClaimClass::FormattingPasses,
        ("format", Polarity::Negative) => ClaimClass::FormattingFails,
        ("change", _) => ClaimClass::FileChanged,
        ("remove", _) => ClaimClass::FileRemoved,
        ("exist", Polarity::Positive) => ClaimClass::EntityExists,
        ("exist", Polarity::Negative) => ClaimClass::EntityAbsent,
        _ => ClaimClass::Unknown,
    }
}

/// The deterministic, rule-based evidence matcher.
#[derive(Debug, Clone, Copy, Default)]
pub struct DeterministicEvidenceMatcher;

impl EvidenceMatcher for DeterministicEvidenceMatcher {
    fn match_evidence(
        &self,
        claim: &NormalizedClaim,
        _project_context: Option<&ProjectContext>,
        evidence: &[Evidence],
    ) -> Vec<EvidenceMatch> {
        let class = claim_class(claim);
        let mut matches: Vec<EvidenceMatch> = evidence
            .iter()
            .map(|ev| self.match_one(claim, class, ev))
            .collect();
        matches.sort_by(|a, b| a.evidence_id.as_str().cmp(b.evidence_id.as_str()));
        matches
    }
}

/// An intermediate assessment before building the final [`EvidenceMatch`].
enum Assessed {
    Support {
        strength: EvidenceStrength,
        reason: String,
    },
    Contradict {
        strength: EvidenceStrength,
        reason: String,
    },
    Unverifiable {
        reason: String,
    },
    Insufficient {
        reason: String,
    },
    Irrelevant {
        reason: String,
    },
}

impl DeterministicEvidenceMatcher {
    fn match_one(
        &self,
        claim: &NormalizedClaim,
        class: ClaimClass,
        ev: &Evidence,
    ) -> EvidenceMatch {
        let assessed = if ev.kind == EvidenceKind::AgentClaim {
            Assessed::Insufficient {
                reason: "agent narration is not environment evidence; it cannot support or refute a claim".into(),
            }
        } else if !ev.is_environment_evidence() {
            Assessed::Insufficient {
                reason: "evidence is not directly observed from the environment".into(),
            }
        } else {
            match class {
                ClaimClass::TestsPass | ClaimClass::TestsFail => {
                    self.match_execution(claim, class, ev, "test", is_test_command)
                }
                ClaimClass::BuildSucceeds | ClaimClass::BuildFails => {
                    self.match_execution(claim, class, ev, "build", is_build_command)
                }
                ClaimClass::FormattingPasses | ClaimClass::FormattingFails => {
                    self.match_execution(claim, class, ev, "format", is_format_command)
                }
                ClaimClass::FileChanged => self.match_file_change(claim, ev),
                ClaimClass::FileRemoved => self.match_file_remove(claim, ev),
                ClaimClass::EntityExists | ClaimClass::EntityAbsent => {
                    self.match_entity(claim, class, ev)
                }
                ClaimClass::Unknown => Assessed::Irrelevant {
                    reason: "claim class is not recognized; no deterministic matching rule applies"
                        .into(),
                },
            }
        };

        let mut quality = quality_from(ev);
        let relationship = relationship_of(&assessed);
        quality.relevant = !matches!(relationship, EvidenceRelationship::Irrelevant);
        EvidenceMatch {
            evidence_id: ev.id.clone(),
            evidence_kind: ev.kind,
            relationship,
            strength: strength_of(&assessed),
            quality,
            relevant: quality.relevant,
            reason: reason_of(&assessed),
        }
    }

    /// Generic dispatch for command-outcome claims (tests, build, format).
    fn match_execution(
        &self,
        claim: &NormalizedClaim,
        class: ClaimClass,
        ev: &Evidence,
        domain: &str,
        is_domain: fn(&CommandSpec) -> bool,
    ) -> Assessed {
        let Some(content) = ev.content.as_command_output() else {
            return Assessed::Irrelevant {
                reason: format!(
                    "evidence is not command output (claim needs a {domain} command run)"
                ),
            };
        };
        if !is_domain(&content.spec) {
            return Assessed::Irrelevant {
                reason: format!("command is not a {domain} command"),
            };
        }

        // Named *test* targets require a matching command scope (e.g. a
        // `cargo test authentication` filter for an "authentication tests"
        // claim). Domain names for build/format claims ("build", "format") are
        // not filters — any matching command is relevant.
        let test_target = claim
            .target
            .name
            .as_deref()
            .filter(|n| !n.is_empty())
            .map(str::to_lowercase);
        if claim.target.scope == crate::claim::TargetScope::Test {
            if let Some(named) = &test_target {
                let specific = content
                    .spec
                    .args
                    .iter()
                    .any(|a| a.to_lowercase().contains(named.as_str()));
                if !specific {
                    return Assessed::Insufficient {
                        reason: format!(
                            "{domain} command ran the whole suite; it cannot isolate the named target {named:?}"
                        ),
                    };
                }
            }
        }

        let outcome = content.outcome();
        let positive = matches!(
            class,
            ClaimClass::TestsPass | ClaimClass::BuildSucceeds | ClaimClass::FormattingPasses
        );

        match outcome {
            griller_core::command::CommandOutcome::Success if positive => {
                Assessed::Support {
                    strength: EvidenceStrength::Direct,
                    reason: format!("{domain} command completed with exit code 0"),
                }
            }
            griller_core::command::CommandOutcome::Success => Assessed::Contradict {
                strength: EvidenceStrength::Direct,
                reason: format!("{domain} command completed with exit code 0, contradicting the claim"),
            },
            griller_core::command::CommandOutcome::Failed if !positive => Assessed::Support {
                strength: EvidenceStrength::Direct,
                reason: format!("{domain} command failed with a non-zero exit code"),
            },
            griller_core::command::CommandOutcome::Failed => Assessed::Contradict {
                strength: EvidenceStrength::Direct,
                reason: format!("{domain} command failed with a non-zero exit code, contradicting the claim"),
            },
            griller_core::command::CommandOutcome::TimedOut => Assessed::Unverifiable {
                reason: format!("{domain} command timed out; the outcome is undetermined"),
            },
            griller_core::command::CommandOutcome::CouldNotSpawn => Assessed::Unverifiable {
                reason: format!(
                    "{domain} command could not be executed (missing dependency or tool); the outcome is undetermined"
                ),
            },
        }
    }

    /// Matching for "X changed" claims.
    fn match_file_change(&self, claim: &NormalizedClaim, ev: &Evidence) -> Assessed {
        let name = claim.target.name.as_deref().filter(|n| !n.is_empty());
        match &ev.content {
            EvidenceContent::GitDiff(content) => {
                let touched =
                    diff_touches(&content.unstaged, &content.staged, &content.porcelain, name);
                match (name, touched) {
                    (Some(n), true) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: format!("git diff shows the claimed path {n:?} changed"),
                    },
                    (None, true) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: "git diff shows repository changes".into(),
                    },
                    (Some(n), false) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: format!("git diff shows no change to the claimed path {n:?}"),
                    },
                    (None, false) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: "git diff shows no repository changes".into(),
                    },
                }
            }
            EvidenceContent::RepositorySnapshot(content) => {
                let touched = snapshot_touches(&content.snapshot, name);
                match (name, touched) {
                    (Some(n), Some(true)) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: format!("repository snapshot shows the claimed path {n:?} changed"),
                    },
                    (Some(_), Some(false)) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: "repository snapshot shows changes, but not to the claimed path".into(),
                    },
                    (Some(n), None) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: format!("repository snapshot shows a clean working tree; the claimed path {n:?} is unchanged"),
                    },
                    (None, Some(true)) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: "repository snapshot shows a dirty working tree".into(),
                    },
                    (None, _) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: "repository snapshot shows a clean working tree".into(),
                    },
                }
            }
            EvidenceContent::FileContent(_) => Assessed::Insufficient {
                reason: "file presence cannot prove the file changed".into(),
            },
            _ => Assessed::Irrelevant {
                reason: "evidence kind is not relevant to a file-change claim".into(),
            },
        }
    }

    /// Matching for "X removed" claims.
    fn match_file_remove(&self, claim: &NormalizedClaim, ev: &Evidence) -> Assessed {
        let name = claim.target.name.as_deref().filter(|n| !n.is_empty());
        match &ev.content {
            EvidenceContent::GitDiff(content) => {
                if diff_shows_deletion(&content.unstaged, &content.staged, &content.porcelain, name)
                {
                    return Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: match name {
                            Some(n) => format!("git diff shows the claimed path {n:?} deleted"),
                            None => "git diff shows deletions".into(),
                        },
                    };
                }
                match name {
                    Some(n)
                        if diff_touches(
                            &content.unstaged,
                            &content.staged,
                            &content.porcelain,
                            Some(n),
                        ) =>
                    {
                        Assessed::Contradict {
                            strength: EvidenceStrength::Direct,
                            reason: format!(
                                "git diff shows the claimed path {n:?} modified, not removed"
                            ),
                        }
                    }
                    _ => Assessed::Insufficient {
                        reason: "git evidence does not show the claimed path being removed".into(),
                    },
                }
            }
            EvidenceContent::RepositorySnapshot(content) => {
                let status = snapshot_path_status(&content.snapshot, name);
                match status {
                    Some(true) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: "repository snapshot shows the claimed path deleted".into(),
                    },
                    Some(false) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: "repository snapshot shows the claimed path still present".into(),
                    },
                    None => Assessed::Insufficient {
                        reason: "repository snapshot does not mention the claimed path".into(),
                    },
                }
            }
            EvidenceContent::FileContent(content) => {
                let path = content.path.to_string_lossy().to_string();
                if name.is_some_and(|n| paths_match(&path, n)) {
                    Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: format!(
                            "file content evidence shows the claimed path {path:?} still present"
                        ),
                    }
                } else {
                    Assessed::Irrelevant {
                        reason: "file content evidence is for an unrelated path".into(),
                    }
                }
            }
            _ => Assessed::Irrelevant {
                reason: "evidence kind is not relevant to a removal claim".into(),
            },
        }
    }

    /// Matching for existence/absence claims about a named entity.
    fn match_entity(&self, claim: &NormalizedClaim, class: ClaimClass, ev: &Evidence) -> Assessed {
        let Some(name) = claim.target.name.as_deref().filter(|n| !n.is_empty()) else {
            return Assessed::Insufficient {
                reason: "existence claims need a named entity target".into(),
            };
        };

        match &ev.content {
            EvidenceContent::FileContent(content) => {
                let path = content.path.to_string_lossy().to_string();
                let text = content.text.as_deref();
                let scope = claim.target.scope;
                if scope == crate::claim::TargetScope::Function
                    || scope == crate::claim::TargetScope::Module
                {
                    // Symbol-presence search over file content (M2: light text
                    // search only).
                    let present = text.map(|t| t.contains(name)).unwrap_or(false);
                    return symbol_presence(class, present, name);
                }
                if paths_match(&path, name) {
                    // Direct path presence observation.
                    match class {
                        ClaimClass::EntityExists => Assessed::Support {
                            strength: EvidenceStrength::Direct,
                            reason: format!("file content evidence shows the claimed path {path:?} exists"),
                        },
                        _ => Assessed::Contradict {
                            strength: EvidenceStrength::Direct,
                            reason: format!("file content evidence shows the claimed path {path:?} still exists"),
                        },
                    }
                } else {
                    Assessed::Irrelevant {
                        reason: "file content evidence is for an unrelated path".into(),
                    }
                }
            }
            EvidenceContent::GitDiff(content) => {
                if diff_shows_deletion(
                    &content.unstaged,
                    &content.staged,
                    &content.porcelain,
                    Some(name),
                ) {
                    Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: format!("git diff shows the claimed entity {name:?} deleted"),
                    }
                } else {
                    Assessed::Insufficient {
                        reason: "git diff does not directly resolve the existence claim".into(),
                    }
                }
            }
            EvidenceContent::RepositorySnapshot(content) => {
                match snapshot_path_status(&content.snapshot, Some(name)) {
                    Some(true) => Assessed::Contradict {
                        strength: EvidenceStrength::Direct,
                        reason: format!(
                            "repository snapshot shows the claimed entity {name:?} deleted"
                        ),
                    },
                    Some(false) => Assessed::Support {
                        strength: EvidenceStrength::Direct,
                        reason: format!(
                            "repository snapshot shows the claimed entity {name:?} still present"
                        ),
                    },
                    None => Assessed::Insufficient {
                        reason: "repository snapshot does not mention the claimed entity".into(),
                    },
                }
            }
            _ => Assessed::Irrelevant {
                reason: "evidence kind is not relevant to an existence claim".into(),
            },
        }
    }
}

/// Assess presence for symbol-scoped claims.
fn symbol_presence(class: ClaimClass, present: bool, name: &str) -> Assessed {
    match (class, present) {
        (ClaimClass::EntityExists, true) => Assessed::Support {
            strength: EvidenceStrength::Moderate,
            reason: format!("file content contains the claimed symbol {name:?}"),
        },
        (ClaimClass::EntityExists, false) => Assessed::Contradict {
            strength: EvidenceStrength::Moderate,
            reason: format!("file content does not contain the claimed symbol {name:?}"),
        },
        (ClaimClass::EntityAbsent, true) => Assessed::Contradict {
            strength: EvidenceStrength::Moderate,
            reason: format!("file content contains the claimed symbol {name:?}"),
        },
        _ => Assessed::Support {
            strength: EvidenceStrength::Moderate,
            reason: format!("file content does not contain the claimed symbol {name:?}"),
        },
    }
}

// ---------------------------------------------------------------------------
// Evidence quality / match assembly
// ---------------------------------------------------------------------------

fn quality_from(ev: &Evidence) -> EvidenceQuality {
    let reproducible = matches!(
        ev.provenance.reproducibility,
        Reproducibility::Deterministic
    );
    EvidenceQuality {
        direct_observation: ev.provenance.directly_observed,
        reproducible,
        independent: !ev.provenance.depends_on_agent_claim,
        relevant: true,
    }
}

fn relationship_of(assessed: &Assessed) -> EvidenceRelationship {
    match assessed {
        Assessed::Support { .. } => EvidenceRelationship::Supports,
        Assessed::Contradict { .. } => EvidenceRelationship::Contradicts,
        Assessed::Unverifiable { .. } => EvidenceRelationship::Unverifiable,
        Assessed::Insufficient { .. } => EvidenceRelationship::Insufficient,
        Assessed::Irrelevant { .. } => EvidenceRelationship::Irrelevant,
    }
}

fn strength_of(assessed: &Assessed) -> EvidenceStrength {
    match assessed {
        Assessed::Support { strength, .. } | Assessed::Contradict { strength, .. } => *strength,
        _ => EvidenceStrength::None,
    }
}

fn reason_of(assessed: &Assessed) -> String {
    match assessed {
        Assessed::Support { reason, .. }
        | Assessed::Contradict { reason, .. }
        | Assessed::Unverifiable { reason }
        | Assessed::Insufficient { reason }
        | Assessed::Irrelevant { reason } => reason.clone(),
    }
}

// ---------------------------------------------------------------------------
// Command detection
// ---------------------------------------------------------------------------

/// True when a command spec is a test command.
fn is_test_command(spec: &CommandSpec) -> bool {
    let exe = spec.executable.to_lowercase();
    if exe == "pytest" || exe.contains("test") {
        return true;
    }
    match exe.as_str() {
        "cargo" | "cargo-nextest" => spec.args.iter().any(|a| a == "test"),
        "npm" | "yarn" | "pnpm" | "bun" => spec.args.iter().any(|a| a == "test"),
        "go" => spec.args.iter().any(|a| a == "test"),
        "mvn" | "gradle" => spec.args.iter().any(|a| a == "test"),
        _ => false,
    }
}

/// True when a command spec is a build command.
fn is_build_command(spec: &CommandSpec) -> bool {
    let exe = spec.executable.to_lowercase();
    match exe.as_str() {
        "cargo" => spec
            .args
            .iter()
            .any(|a| a == "build" || a == "check" || a == "build-release"),
        "npm" | "yarn" | "pnpm" => spec.args.iter().any(|a| a == "build"),
        "go" => spec.args.iter().any(|a| a == "build"),
        "make" | "cmake" | "ninja" | "meson" | "gradle" | "mvn" => true,
        _ => false,
    }
}

/// True when a command spec is a formatting command.
fn is_format_command(spec: &CommandSpec) -> bool {
    let exe = spec.executable.to_lowercase();
    if exe == "cargo" {
        return spec.args.iter().any(|a| a == "fmt");
    }
    matches!(
        exe.as_str(),
        "rustfmt" | "gofmt" | "black" | "prettier" | "ruff" | "eslint"
    )
}

// ---------------------------------------------------------------------------
// Git diff / snapshot parsing
// ---------------------------------------------------------------------------

/// True when the porcelain or diff output touches the claimed path.
///
/// When `name` is `None` (repository-wide claim), any change counts.
fn diff_touches(unstaged: &str, staged: &str, porcelain: &str, name: Option<&str>) -> bool {
    match name {
        Some(n) => {
            let n = normalize_path(n);
            porcelain
                .lines()
                .any(|l| path_mentioned_in_porcelain(l, &n))
                || unstaged.contains(&format!("diff --git a/{n}"))
                || staged.contains(&format!("diff --git a/{n}"))
        }
        None => {
            !(unstaged.trim().is_empty() && staged.trim().is_empty() && porcelain.trim().is_empty())
        }
    }
}

/// True when the diff/porcelain shows the claimed path (or any path, when name
/// is `None`) deleted.
fn diff_shows_deletion(unstaged: &str, staged: &str, porcelain: &str, name: Option<&str>) -> bool {
    match name {
        Some(n) => {
            let n = normalize_path(n);
            porcelain.lines().any(|l| {
                path_mentioned_in_porcelain(l, &n) && (l.starts_with("D ") || l.starts_with(" D"))
            }) || diff_text_shows_deletion(unstaged, &n)
                || diff_text_shows_deletion(staged, &n)
        }
        None => {
            porcelain
                .lines()
                .any(|l| l.starts_with("D ") || l.starts_with(" D"))
                || diff_text_shows_any_deletion(unstaged)
                || diff_text_shows_any_deletion(staged)
        }
    }
}

/// True when a porcelain line mentions the path.
fn path_mentioned_in_porcelain(line: &str, path: &str) -> bool {
    // Porcelain format: `XY <path>` or `?? <path>`.
    if line.len() < 4 {
        return false;
    }
    let rest = line[3..].trim();
    rest == path || rest.starts_with(path) || path.contains(rest.trim_start_matches("\""))
}

/// True when the diff text contains a `deleted file mode` block for the path.
fn diff_text_shows_deletion(diff: &str, path: &str) -> bool {
    let needle = format!("diff --git a/{path}");
    let mut lines = diff.lines().peekable();
    while let Some(line) = lines.next() {
        if line.starts_with(&needle) {
            for _ in 0..8 {
                if let Some(next) = lines.peek().copied() {
                    if next.contains("deleted file mode") {
                        return true;
                    }
                    if next.starts_with("diff --git ") {
                        break;
                    }
                    lines.next();
                }
            }
        }
    }
    false
}

/// True when the diff text contains any deletion block.
fn diff_text_shows_any_deletion(diff: &str) -> bool {
    diff.lines().any(|l| l.contains("deleted file mode"))
}

/// Status of a path in a snapshot: `Some(true)` = deleted, `Some(false)` =
/// present, `None` = not mentioned.
fn snapshot_path_status(
    snapshot: &griller_core::snapshot::RepositorySnapshot,
    name: Option<&str>,
) -> Option<bool> {
    let name = name?;
    let name = normalize_path(name);
    for line in snapshot
        .staged_changes
        .iter()
        .chain(&snapshot.unstaged_changes)
    {
        if path_mentioned_in_porcelain(line, &name) {
            let deleted = line.starts_with("D ") || line.starts_with(" D");
            return Some(deleted);
        }
    }
    if snapshot
        .untracked_files
        .iter()
        .any(|f| normalize_path(f) == name)
    {
        return Some(false);
    }
    None
}

/// True when the snapshot shows the path changed at all (for change claims).
fn snapshot_touches(
    snapshot: &griller_core::snapshot::RepositorySnapshot,
    name: Option<&str>,
) -> Option<bool> {
    if snapshot.dirty && name.is_none() {
        return Some(true);
    }
    let Some(name) = name else {
        return Some(false);
    };
    let name = normalize_path(name);
    for line in snapshot
        .staged_changes
        .iter()
        .chain(&snapshot.unstaged_changes)
    {
        if path_mentioned_in_porcelain(line, &name) {
            return Some(true);
        }
    }
    if snapshot
        .untracked_files
        .iter()
        .any(|f| normalize_path(f) == name)
    {
        return Some(true);
    }
    if snapshot.dirty {
        Some(false)
    } else {
        None
    }
}

fn normalize_path(path: &str) -> String {
    path.trim().trim_start_matches("./").to_string()
}

/// Lenient path comparison (exact or suffix-with-separator).
fn paths_match(a: &str, b: &str) -> bool {
    let a = normalize_path(a);
    let b = normalize_path(b);
    a == b || a.ends_with(&format!("/{b}")) || b.ends_with(&format!("/{a}"))
}
