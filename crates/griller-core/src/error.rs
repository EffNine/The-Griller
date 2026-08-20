//! Error type for `griller-core`.

use std::fmt;

/// Errors produced by pure core operations.
///
/// The core crate performs no I/O, so this only wraps serialization failures.
#[derive(Debug)]
pub enum CoreError {
    /// A `serde_json` serialization/deserialization failure.
    Serialization(serde_json::Error),
}

impl fmt::Display for CoreError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            CoreError::Serialization(err) => write!(f, "serialization error: {err}"),
        }
    }
}

impl std::error::Error for CoreError {
    fn source(&self) -> Option<&(dyn std::error::Error + 'static)> {
        match self {
            CoreError::Serialization(err) => Some(err),
        }
    }
}

impl From<serde_json::Error> for CoreError {
    fn from(err: serde_json::Error) -> Self {
        CoreError::Serialization(err)
    }
}
