//! The claim model: what The Griller is asked to assess.
//!
//! A [`Claim`] is the structured record of a natural-language statement about
//! the codebase. It is *input data* for the adjudication pipeline. A claim is
//! never evidence, and a claim never carries an epistemic status: status is
//! always decided by the deterministic adjudication layer from evidence.

use serde::{Deserialize, Serialize};
use serde_json::json;

use griller_core::hash::{blake3_hex, canonical_bytes};

use crate::predicate::Predicate;
use crate::requirement::RequirementProvenance;

/// Stable identifier for a claim.
///
/// Content-addressed: equal to the BLAKE3 hash of the claim's content fields.
/// The id is computed in [`Claim::new`], so the same claim always has the same
/// id.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(transparent)]
pub struct ClaimId(String);

impl ClaimId {
    /// Build an id from a content hash.
    pub fn from_hash(hash: &str) -> Self {
        Self(hash.to_string())
    }

    /// The underlying hex string.
    pub fn as_str(&self) -> &str {
        &self.0
    }
}

impl std::fmt::Display for ClaimId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.0)
    }
}

/// Who made the claim.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ClaimSource {
    /// An autonomous coding agent.
    Agent,
    /// The human user / task author.
    User,
    /// Inferred by an LLM or by deterministic analysis.
    Derived,
    /// Directly observed system/repository behaviour.
    SystemObserved,
}

/// What kind of claim is being made.
///
/// The kinds are deliberately few; M2 does not invent kinds without evidence
/// that they are needed.
///
/// `#[non_exhaustive]`: downstream crates (e.g. the M3 policy table) must
/// handle future claim kinds explicitly and must never silently trust an
/// unlisted kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
#[non_exhaustive]
pub enum ClaimKind {
    /// A claim that some work was completed ("tests pass", "X was removed").
    Completion,
    /// A claim of fact about the repository ("module X is unused").
    Fact,
    /// A causal claim ("A causes B").
    Causal,
    /// A requirement claim ("the system should ...").
    Requirement,
    /// A design claim ("the design isolates X").
    Design,
}

/// The scope a claim's target resolves to.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum TargetScope {
    /// The repository as a whole.
    Repository,
    /// A file (name is a path relative to the repository root).
    File,
    /// A function or method (name is the symbol).
    Function,
    /// A module / crate / package (name is the module).
    Module,
    /// A command (name is the command or its purpose).
    Command,
    /// A test or test group (name is the descriptor, e.g. "authentication").
    Test,
    /// A build artifact / binary (name is the artifact).
    Artifact,
    /// The scope could not be determined deterministically.
    Unknown,
}

/// The subject a claim is about.
///
/// Structured enough to identify relevant scope: repository, file, function,
/// module, command, test, artifact.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimTarget {
    /// The kind of thing the claim is about.
    pub scope: TargetScope,
    /// Optional identifying name (file path, symbol name, test descriptor, ...).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
}

impl ClaimTarget {
    /// A target with the given scope and name.
    pub fn new(scope: TargetScope, name: Option<String>) -> Self {
        Self { scope, name }
    }

    /// A repository-wide target.
    pub fn repository() -> Self {
        Self::new(TargetScope::Repository, None)
    }

    /// True when the target resolves to a specific named entity.
    pub fn is_named(&self) -> bool {
        matches!(
            self.scope,
            TargetScope::File
                | TargetScope::Function
                | TargetScope::Module
                | TargetScope::Command
                | TargetScope::Test
                | TargetScope::Artifact
        ) && self.name.as_deref().is_some_and(|n| !n.is_empty())
    }

    /// True when the target is a named entity in the repository (file, symbol,
    /// module, artifact) as opposed to a command or test run.
    pub fn is_entity(&self) -> bool {
        matches!(
            self.scope,
            TargetScope::File | TargetScope::Function | TargetScope::Module | TargetScope::Artifact
        )
    }
}

/// A structured claim about the codebase.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Claim {
    /// Content-addressed identifier (BLAKE3 over the content fields).
    pub id: ClaimId,
    /// Who made the claim.
    pub source: ClaimSource,
    /// Kind of claim.
    pub kind: ClaimKind,
    /// The original, verbatim natural-language claim. Never dropped.
    pub text: String,
    /// The resolved target of the claim.
    pub target: ClaimTarget,
    /// Structured predicates extracted from the text.
    pub predicates: Vec<Predicate>,
    /// For requirement claims: how the requirement entered the system.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_provenance: Option<RequirementProvenance>,
}

impl Claim {
    /// Construct a claim, deriving a content-addressed id from its fields.
    pub fn new(
        source: ClaimSource,
        kind: ClaimKind,
        text: impl Into<String>,
        target: ClaimTarget,
        predicates: Vec<Predicate>,
        requirement_provenance: Option<RequirementProvenance>,
    ) -> Self {
        let text = text.into();
        let id = claim_content_hash(
            &source,
            &kind,
            &text,
            &target,
            &predicates,
            &requirement_provenance,
        );
        Self {
            id: ClaimId::from_hash(&id),
            source,
            kind,
            text,
            target,
            predicates,
            requirement_provenance,
        }
    }

    /// Re-derive the id a claim with these fields would have.
    ///
    /// Useful for callers that construct the parts first.
    pub fn id_for(
        source: ClaimSource,
        kind: ClaimKind,
        text: &str,
        target: &ClaimTarget,
        predicates: &[Predicate],
        requirement_provenance: Option<RequirementProvenance>,
    ) -> String {
        claim_content_hash(
            &source,
            &kind,
            text,
            target,
            predicates,
            &requirement_provenance,
        )
    }
}

fn claim_content_hash(
    source: &ClaimSource,
    kind: &ClaimKind,
    text: &str,
    target: &ClaimTarget,
    predicates: &[Predicate],
    requirement_provenance: &Option<RequirementProvenance>,
) -> String {
    let payload = json!({
        "source": source,
        "kind": kind,
        "text": text,
        "target": target,
        "predicates": predicates,
        "requirement_provenance": requirement_provenance,
    });
    blake3_hex(
        &canonical_bytes(&payload)
            .expect("claim content serialization is infallible for plain data"),
    )
}

/// The raw, pre-normalization input to the adjudication pipeline.
///
/// Only `text` and `source` are required. `kind`, `target` and
/// `requirement_provenance` may be supplied explicitly by the caller; when
/// absent they are inferred by the normalizer.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ClaimInput {
    /// The verbatim claim text.
    pub text: String,
    /// Who made the claim.
    pub source: ClaimSource,
    /// Explicit kind, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub kind: Option<ClaimKind>,
    /// Explicit target, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub target: Option<ClaimTarget>,
    /// Explicit requirement provenance, when the caller knows it.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub requirement_provenance: Option<RequirementProvenance>,
}

impl ClaimInput {
    /// Construct a claim input from text and a source.
    pub fn new(text: impl Into<String>, source: ClaimSource) -> Self {
        Self {
            text: text.into(),
            source,
            kind: None,
            target: None,
            requirement_provenance: None,
        }
    }

    /// Set the explicit kind.
    pub fn with_kind(mut self, kind: ClaimKind) -> Self {
        self.kind = Some(kind);
        self
    }

    /// Set the explicit target.
    pub fn with_target(mut self, target: ClaimTarget) -> Self {
        self.target = Some(target);
        self
    }

    /// Set the explicit requirement provenance.
    pub fn with_requirement_provenance(mut self, provenance: RequirementProvenance) -> Self {
        self.requirement_provenance = Some(provenance);
        self
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::predicate::Polarity;

    fn claim() -> Claim {
        Claim::new(
            ClaimSource::Agent,
            ClaimKind::Completion,
            "tests pass",
            ClaimTarget::new(TargetScope::Test, None),
            vec![Predicate {
                verb: "pass".into(),
                polarity: Polarity::Positive,
                object: None,
            }],
            None,
        )
    }

    #[test]
    fn claim_id_is_content_addressed() {
        let a = claim();
        let b = claim();
        assert_eq!(a.id, b.id);
        assert_eq!(a.id.as_str().len(), 64);
        // Same text but different source must produce a different id.
        let c = Claim::new(
            ClaimSource::User,
            a.kind,
            a.text.clone(),
            a.target.clone(),
            a.predicates.clone(),
            None,
        );
        assert_ne!(a.id, c.id);
    }

    #[test]
    fn claim_roundtrips_through_json() {
        let c = claim();
        let json = serde_json::to_string(&c).unwrap();
        let back: Claim = serde_json::from_str(&json).unwrap();
        assert_eq!(c, back);
    }
}
