//! Error types for context adjudication.

use std::fmt;

/// Errors produced during task normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum NormalizationError {
    /// Untrusted LLM task-normalization output could not be parsed.
    ///
    /// The output was rejected at the LLM boundary. Rejection is safe: the
    /// caller may fall back to deterministic normalization. Rejection never
    /// assigns a context status or verdict.
    InvalidLlmOutput(String),
}

impl fmt::Display for NormalizationError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            NormalizationError::InvalidLlmOutput(msg) => {
                write!(f, "invalid LLM output: {msg}")
            }
        }
    }
}

impl std::error::Error for NormalizationError {}

/// Errors produced by the context pipeline.
///
/// Matching and status assignment are total functions over their inputs; the
/// only fallible stage is task normalization.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ContextError {
    /// Task normalization failed.
    Normalization(NormalizationError),
}

impl fmt::Display for ContextError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            ContextError::Normalization(err) => write!(f, "normalization error: {err}"),
        }
    }
}

impl std::error::Error for ContextError {}

impl From<NormalizationError> for ContextError {
    fn from(err: NormalizationError) -> Self {
        ContextError::Normalization(err)
    }
}
