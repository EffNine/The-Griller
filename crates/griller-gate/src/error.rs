//! Structured error model for the gate.

use std::fmt;

use serde::{Deserialize, Serialize};

/// Machine-readable error kind. Never conflated with a Griller verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateErrorKind {
    /// The gate configuration is invalid.
    InvalidConfig,
    /// The `griller-mcp` binary could not be spawned or the connection failed.
    ServerUnavailable,
    /// The server's health report did not match the expected build.
    StaleServer,
    /// The MCP response was malformed or unparseable.
    MalformedResponse,
    /// A request exceeded its timeout.
    Timeout,
    /// The tool call was rejected by the server (structured MCP error).
    ToolError,
    /// An unexpected internal error.
    Internal,
}

impl GateErrorKind {
    /// Stable machine-readable code.
    pub fn code(self) -> &'static str {
        match self {
            GateErrorKind::InvalidConfig => "INVALID_CONFIG",
            GateErrorKind::ServerUnavailable => "SERVER_UNAVAILABLE",
            GateErrorKind::StaleServer => "STALE_SERVER",
            GateErrorKind::MalformedResponse => "MALFORMED_RESPONSE",
            GateErrorKind::Timeout => "TIMEOUT",
            GateErrorKind::ToolError => "TOOL_ERROR",
            GateErrorKind::Internal => "INTERNAL_ERROR",
        }
    }
}

/// A gate error with a kind and a concise message. Never contains secrets.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateError {
    pub kind: GateErrorKind,
    pub message: String,
}

impl GateError {
    pub fn new(kind: GateErrorKind, message: impl Into<String>) -> Self {
        Self {
            kind,
            message: message.into(),
        }
    }
}

impl fmt::Display for GateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}: {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for GateError {}

pub type GateResult<T> = Result<T, GateError>;

/// Convenience constructors.
pub mod err {
    use super::*;

    pub fn invalid_config(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::InvalidConfig, msg)
    }

    pub fn server_unavailable(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::ServerUnavailable, msg)
    }

    pub fn stale_server(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::StaleServer, msg)
    }

    pub fn malformed(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::MalformedResponse, msg)
    }

    pub fn timeout(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::Timeout, msg)
    }

    pub fn tool(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::ToolError, msg)
    }

    pub fn internal(msg: impl Into<String>) -> GateError {
        GateError::new(GateErrorKind::Internal, msg)
    }
}
