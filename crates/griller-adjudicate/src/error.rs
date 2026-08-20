//! Error types for the adjudication pipeline.

use std::fmt;

/// Errors produced during claim normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationError {
    /// Untrusted LLM normalization output could not be parsed.
    ///
    /// The output was rejected at the LLM boundary. Rejection is safe: the
    /// caller may fall back to deterministic normalization. Rejection never
    /// assigns an epistemic status.
    InvalidLlmOutput(String),
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NormalizationError::InvalidLlmOutput(msg) => write!(f, "invalid LLM output: {msg}"),
        }
    }
}

impl std::error::Error for NormalizationError {}

/// Errors produced by the adjudication pipeline.
///
/// Matching, contradiction checking and status assignment are total functions
/// over their inputs; the only fallible stage is normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum AdjudicationError {
    /// Claim normalization failed.
    Normalization(NormalizationError),
}

impl fmt::Display for AdjudicationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            AdjudicationError::Normalization(err) => write!(f, "normalization error: {err}"),
        }
    }
}

impl std::error::Error for AdjudicationError {}

impl From<NormalizationError> for AdjudicationError {
    fn from(err: NormalizationError) -> Self {
        AdjudicationError::Normalization(err)
    }
}
