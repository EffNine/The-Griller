//! Runtime error type.

/// Errors produced by runtime operations.
#[derive(Debug, thiserror::Error)]
pub enum RuntimeError {
    /// A core (serialization/hashing) error.
    #[error("core error: {0}")]
    Core(#[from] griller_core::error::CoreError),
    /// An I/O error.
    #[error("io error: {0}")]
    Io(#[from] std::io::Error),
    /// A git-related failure.
    #[error("git error: {0}")]
    Git(String),
    /// An execution-related failure.
    #[error("execution error: {0}")]
    Execution(String),
    /// An evidence-store failure.
    #[error("store error: {0}")]
    Store(String),
    /// The path does not exist.
    #[error("path not found: {0}")]
    NotFound(String),
}
