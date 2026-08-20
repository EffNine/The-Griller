//! Structured error model for the MCP adapter.
//!
//! MCP tools return structured JSON errors (with an `is_error: true` result)
//! rather than protocol-level failures whenever the caller can do something
//! about it. System-level failures (transport, protocol) surface as JSON-RPC
//! errors at the transport layer.
//!
//! The mapping to the CLI exit-code scheme (PASS=0 / WARN=1 / BLOCK=2 /
//! SYSTEM ERROR=3) is documented in `docs/m8-integration.md`. At the MCP
//! boundary a system error is *never* reported as a verdict: a "Griller
//! crashed" outcome must not be read as "the claim is false".

use rust_mcp_sdk::schema::{
    schema_utils::CallToolError, CallToolResult, ContentBlock, TextContent,
};
use serde::{Deserialize, Serialize};

/// Structured error kind returned by MCP tools.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum McpErrorKind {
    /// The request was malformed or missing required fields.
    InvalidRequest,
    /// The repository path does not exist.
    RepositoryNotFound,
    /// The path exists but is not a repository.
    NotARepository,
    /// The command requested violates the allow-list.
    UnsafeCommand,
    /// The execution backend failed.
    ExecutionFailed,
    /// A timeout was exceeded.
    Timeout,
    /// The operation is not supported by this tool.
    UnsupportedOperation,
    /// An unexpected internal error occurred.
    InternalError,
}

impl McpErrorKind {
    /// Stable machine-readable code.
    pub fn code(&self) -> &str {
        match self {
            McpErrorKind::InvalidRequest => "INVALID_REQUEST",
            McpErrorKind::RepositoryNotFound => "REPOSITORY_NOT_FOUND",
            McpErrorKind::NotARepository => "NOT_A_REPOSITORY",
            McpErrorKind::UnsafeCommand => "UNSAFE_COMMAND",
            McpErrorKind::ExecutionFailed => "EXECUTION_FAILED",
            McpErrorKind::Timeout => "TIMEOUT",
            McpErrorKind::UnsupportedOperation => "UNSUPPORTED_OPERATION",
            McpErrorKind::InternalError => "INTERNAL_ERROR",
        }
    }
}

/// A structured MCP error result.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct McpError {
    pub kind: McpErrorKind,
    pub message: String,
}

impl std::fmt::Display for McpError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "[{}] {}", self.kind.code(), self.message)
    }
}

impl std::error::Error for McpError {}

impl From<McpError> for CallToolResult {
    fn from(err: McpError) -> Self {
        error_result(err.kind, &err.message)
    }
}

impl From<McpError> for CallToolError {
    fn from(err: McpError) -> Self {
        CallToolError::from_message(format!("[{}] {}", err.kind.code(), err.message))
    }
}

/// Build a structured error result (`is_error: true`).
pub fn error_result(kind: McpErrorKind, message: &str) -> CallToolResult {
    let err = McpError {
        kind: kind.clone(),
        message: message.to_string(),
    };
    let json = serde_json::to_string_pretty(&err).unwrap_or_default();
    CallToolResult {
        content: vec![ContentBlock::TextContent(TextContent::from(json))],
        is_error: Some(true),
        meta: None,
        structured_content: None,
    }
}
