//! Requirement provenance.
//!
//! Requirements MUST distinguish how they entered the system:
//!
//! * `DECLARED` — explicitly stated by the user or task.
//! * `DERIVED` — inferred by an LLM or by deterministic analysis.
//! * `OBSERVED` — an existing repository/system constraint observed from
//!   evidence.
//!
//! These categories MUST NOT be silently merged. A derived requirement must
//! never be treated as though the user explicitly requested it.

use serde::{Deserialize, Serialize};
use serde_json::json;

use griller_core::hash::{blake3_hex, canonical_bytes};

/// How a requirement entered the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum RequirementProvenance {
    /// Explicitly stated by the user or task.
    Declared,
    /// Inferred by an LLM or deterministic analysis.
    Derived,
    /// Observed as an existing repository/system constraint from evidence.
    Observed,
}

/// A single requirement with its provenance.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Requirement {
    /// Content-addressed identifier over (text, provenance).
    pub id: String,
    /// The requirement text.
    pub text: String,
    /// How the requirement entered the system.
    pub provenance: RequirementProvenance,
}

impl Requirement {
    /// Construct a requirement with a content-addressed id.
    pub fn new(text: impl Into<String>, provenance: RequirementProvenance) -> Self {
        let text = text.into();
        let payload = json!({ "text": text, "provenance": provenance });
        let id = blake3_hex(
            &canonical_bytes(&payload).expect("requirement serialization is infallible"),
        );
        Self {
            id,
            text,
            provenance,
        }
    }
}

/// Merge two requirement lists into a deterministic, provenance-preserving
/// list.
///
/// Requirements are deduplicated by **(text, provenance)**. Two requirements
/// with the same text but different provenance are **never** merged into one;
/// each provenance class is preserved. Ordering is deterministic (sorted by id).
///
/// This is the structural guarantee that DECLARED / DERIVED / OBSERVED are not
/// silently merged anywhere in the pipeline.
pub fn merge_requirements(left: &[Requirement], right: &[Requirement]) -> Vec<Requirement> {
    let mut all = left.to_vec();
    all.extend_from_slice(right);
    all.sort_by(|a, b| a.id.cmp(&b.id));
    all.dedup_by(|a, b| a.id == b.id);
    all
}

/// The set of distinct provenances present in a requirement list.
pub fn distinct_provenances(requirements: &[Requirement]) -> Vec<RequirementProvenance> {
    let mut out: Vec<RequirementProvenance> = requirements.iter().map(|r| r.provenance).collect();
    out.sort_by_key(|p| format!("{p:?}"));
    out.dedup();
    out
}

/// Filter a requirement list to a single provenance class.
///
/// Used to prove a derived requirement is never surfaced as declared: the
/// derived text only appears under `Derived`, never under `Declared`.
pub fn of_provenance(
    requirements: &[Requirement],
    provenance: RequirementProvenance,
) -> Vec<&Requirement> {
    requirements
        .iter()
        .filter(|r| r.provenance == provenance)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn same_text_different_provenance_is_never_merged() {
        let declared =
            Requirement::new("the build must stay green", RequirementProvenance::Declared);
        let derived = Requirement::new("the build must stay green", RequirementProvenance::Derived);
        let merged = merge_requirements(
            std::slice::from_ref(&declared),
            std::slice::from_ref(&derived),
        );
        assert_eq!(
            merged.len(),
            2,
            "distinct provenances must both be preserved"
        );
        assert_ne!(declared.id, derived.id);

        let declared_only = of_provenance(&merged, RequirementProvenance::Declared);
        let derived_only = of_provenance(&merged, RequirementProvenance::Derived);
        assert_eq!(declared_only.len(), 1);
        assert_eq!(derived_only.len(), 1);
        assert_eq!(declared_only[0].id, declared.id);
        // The derived requirement is never surfaced as declared.
        assert!(declared_only.iter().all(|r| r.id != derived.id));
    }

    #[test]
    fn identical_requirements_deduplicate() {
        let a = Requirement::new("keep tests green", RequirementProvenance::Declared);
        let b = Requirement::new("keep tests green", RequirementProvenance::Declared);
        let merged = merge_requirements(&[a], &[b]);
        assert_eq!(merged.len(), 1);
    }

    #[test]
    fn distinct_provenances_are_sorted_and_unique() {
        let reqs = vec![
            Requirement::new("x", RequirementProvenance::Derived),
            Requirement::new("y", RequirementProvenance::Declared),
            Requirement::new("z", RequirementProvenance::Observed),
            Requirement::new("x2", RequirementProvenance::Derived),
        ];
        assert_eq!(
            distinct_provenances(&reqs),
            vec![
                RequirementProvenance::Declared,
                RequirementProvenance::Derived,
                RequirementProvenance::Observed,
            ]
        );
    }
}
