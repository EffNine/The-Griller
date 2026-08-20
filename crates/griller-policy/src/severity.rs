//! Severity / impact model.
//!
//! Severity describes the **consequence of accepting the claim or action**,
//! not how important the claimer sounds. Severity is resolved
//! deterministically (see [`SeverityResolver`]); an LLM can never propose a
//! severity because the resolver only reads structured signals.
//!
//! `DECLARED` severity is caller-provided (the operator/pipeline), `DERIVED`
//! severity is produced by the deterministic resolver. The two are never
//! silently merged, and an LLM-proposed severity is never accepted.

use serde::{Deserialize, Serialize};

use griller_adjudicate::claim::ClaimKind;

/// Severity of the consequence of accepting a claim/action.
///
/// Ordering: `Info < Low < Medium < High < Critical`.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Severity {
    /// Informational; negligible consequence.
    Info,
    /// Low consequence.
    Low,
    /// Medium consequence.
    Medium,
    /// High consequence.
    High,
    /// Critical consequence.
    Critical,
}

impl std::fmt::Display for Severity {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// How a severity value entered the system.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum SeveritySource {
    /// Explicitly provided by the caller (the operator / pipeline), not by an
    /// LLM.
    Declared,
    /// Deterministically derived by [`SeverityResolver`] from structured
    /// claim kind and signals.
    Derived,
}

/// Structured signals used to derive a conservative severity.
///
/// These are plain facts supplied by the pipeline, never free-form claims.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default, Serialize, Deserialize)]
pub struct SeveritySignals {
    /// Explicit caller-provided severity. When present it is authoritative
    /// (source = `declared`); it overrides all derivation below.
    pub declared: Option<Severity>,
    /// Structured context indicates the claim touches production behaviour.
    pub touches_production: bool,
    /// Structured context indicates the change is a simple/mechanical refactor.
    pub simple_refactor: bool,
}

/// A resolved severity plus how it was obtained.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ResolvedSeverity {
    /// The severity value.
    pub severity: Severity,
    /// How it was obtained.
    pub source: SeveritySource,
}

/// Deterministic severity resolver.
///
/// ## Documented rules
///
/// 1. A `declared` severity is authoritative (`declared` source).
/// 2. Otherwise, a simple refactor => `Low`.
/// 3. Otherwise, production-touching => `High`.
/// 4. Otherwise, derive by claim kind: `fact` / `design` => `Low`;
///    `completion` / `requirement` / `causal` => `Medium`.
///
/// `Critical` and `Info` are **never** auto-derived; they require an explicit
/// declaration. If the caller cannot supply structured context, the resolver
/// uses the conservative default (per claim kind) and does not invent facts.
#[derive(Debug, Clone, Copy, Default)]
pub struct SeverityResolver;

impl SeverityResolver {
    /// Resolve a severity from claim kind and structured signals.
    pub fn resolve(&self, kind: ClaimKind, signals: &SeveritySignals) -> ResolvedSeverity {
        if let Some(severity) = signals.declared {
            return ResolvedSeverity {
                severity,
                source: SeveritySource::Declared,
            };
        }
        let severity = if signals.simple_refactor {
            Severity::Low
        } else if signals.touches_production {
            Severity::High
        } else {
            match kind {
                ClaimKind::Fact | ClaimKind::Design => Severity::Low,
                ClaimKind::Completion | ClaimKind::Requirement | ClaimKind::Causal => {
                    Severity::Medium
                }
                // Future claim kinds default to the conservative mid-impact
                // severity; severity itself never invents facts.
                _ => Severity::Medium,
            }
        };
        ResolvedSeverity {
            severity,
            source: SeveritySource::Derived,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn declared_severity_wins() {
        let resolver = SeverityResolver;
        let resolved = resolver.resolve(
            ClaimKind::Completion,
            &SeveritySignals {
                declared: Some(Severity::Critical),
                touches_production: false,
                simple_refactor: false,
            },
        );
        assert_eq!(resolved.severity, Severity::Critical);
        assert_eq!(resolved.source, SeveritySource::Declared);
    }

    #[test]
    fn refactor_derives_low() {
        let resolved = SeverityResolver.resolve(
            ClaimKind::Completion,
            &SeveritySignals {
                declared: None,
                touches_production: false,
                simple_refactor: true,
            },
        );
        assert_eq!(resolved.severity, Severity::Low);
        assert_eq!(resolved.source, SeveritySource::Derived);
    }

    #[test]
    fn production_derives_high() {
        let resolved = SeverityResolver.resolve(
            ClaimKind::Completion,
            &SeveritySignals {
                declared: None,
                touches_production: true,
                simple_refactor: false,
            },
        );
        assert_eq!(resolved.severity, Severity::High);
        assert_eq!(resolved.source, SeveritySource::Derived);
    }

    #[test]
    fn completion_defaults_to_medium() {
        let resolved = SeverityResolver.resolve(ClaimKind::Completion, &SeveritySignals::default());
        assert_eq!(resolved.severity, Severity::Medium);
        assert_eq!(resolved.source, SeveritySource::Derived);
    }

    #[test]
    fn design_defaults_to_low() {
        let resolved = SeverityResolver.resolve(ClaimKind::Design, &SeveritySignals::default());
        assert_eq!(resolved.severity, Severity::Low);
        assert_eq!(resolved.source, SeveritySource::Derived);
    }

    #[test]
    fn critical_is_never_derived() {
        let resolved =
            SeverityResolver.resolve(ClaimKind::Requirement, &SeveritySignals::default());
        assert_ne!(resolved.severity, Severity::Critical);
        assert_eq!(resolved.source, SeveritySource::Derived);
    }
}
