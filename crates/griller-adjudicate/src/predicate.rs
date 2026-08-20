//! Structured predicates and the canonical proposition form.
//!
//! A natural-language claim normalizes to a [`Predicate`] list and a canonical
//! [`Proposition`] (subject + canonical verb + polarity + object). The
//! proposition is the deterministic key used by evidence matching.

use serde::{Deserialize, Serialize};

use crate::claim::ClaimTarget;

/// Whether a proposition is asserted positively or negatively.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Polarity {
    /// Asserted as true ("tests pass").
    Positive,
    /// Asserted as false ("tests do not pass").
    Negative,
}

impl Polarity {
    /// True for the positive assertion.
    pub fn is_positive(self) -> bool {
        matches!(self, Polarity::Positive)
    }
}

/// A single structured predicate extracted from a claim.
///
/// The verb is the canonical form (see `griller_adjudicate::normalizer`), not
/// the surface word. Polarity captures negation.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Predicate {
    /// Canonical verb, e.g. `pass`, `build`, `remove`, `exist`.
    pub verb: String,
    /// Polarity of the assertion.
    pub polarity: Polarity,
    /// Optional object of the verb ("the authentication timeout").
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

/// The canonical proposition form of a claim.
///
/// This is the deterministic key used by the matcher. Same claim text through
/// the same normalizer yields the same proposition.
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct Proposition {
    /// Canonical subject, e.g. `authentication` or `tests`.
    pub subject: String,
    /// Canonical verb, e.g. `pass`, `build`, `format`, `change`, `remove`,
    /// `exist`.
    pub predicate: String,
    /// Polarity of the assertion.
    pub polarity: Polarity,
    /// Optional object.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub object: Option<String>,
}

impl Proposition {
    /// The verb as a standalone predicate.
    pub fn to_predicate(&self) -> Predicate {
        Predicate {
            verb: self.predicate.clone(),
            polarity: self.polarity,
            object: self.object.clone(),
        }
    }

    /// A short canonical rendering, e.g. `tests_pass`.
    pub fn normalized_key(&self) -> String {
        let mut parts = vec![self.subject.to_lowercase().replace(' ', "_")];
        parts.push(self.predicate.to_lowercase().replace(' ', "_"));
        if let Some(object) = &self.object {
            parts.push(object.to_lowercase().replace(' ', "_"));
        }
        let mut key = parts.join("_");
        if !self.polarity.is_positive() {
            key.push_str("_not");
        }
        key
    }
}

/// Extract the proposition from a claim target if it is named (helper for
/// reason strings and diagnostics).
pub fn target_summary(target: &ClaimTarget) -> String {
    match (&target.scope, &target.name) {
        (scope, Some(name)) => format!("{scope:?}({name})"),
        (scope, None) => format!("{scope:?}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn normalized_key_reflects_polarity() {
        let pos = Proposition {
            subject: "tests".into(),
            predicate: "pass".into(),
            polarity: Polarity::Positive,
            object: None,
        };
        let neg = Proposition {
            polarity: Polarity::Negative,
            ..pos.clone()
        };
        assert_eq!(pos.normalized_key(), "tests_pass");
        assert_eq!(neg.normalized_key(), "tests_pass_not");
        assert_ne!(pos.normalized_key(), neg.normalized_key());
    }
}
