//! Input validation for MCP tool arguments.
//!
//! Every tool validates its inputs at the boundary. Bounds are intentionally
//! small: the Griller is a verification layer, not a data sink.

use std::path::PathBuf;

use crate::error::{McpError, McpErrorKind};

/// Maximum length of free-text inputs (claims, tasks, reasons).
pub const MAX_TEXT_LEN: usize = 10_000;
/// Maximum number of attack kinds a caller may request in one call.
pub const MAX_ATTACKS: usize = 20;
/// Default per-attack timeout in milliseconds.
pub const DEFAULT_ATTACK_TIMEOUT_MS: u64 = 30_000;
/// Maximum per-attack timeout in milliseconds.
pub const MAX_TIMEOUT_MS: u64 = 120_000;
/// Maximum number of evidence/context records echoed in a response.
pub const MAX_EVIDENCE_SNIPPETS: usize = 10;

/// Validate a repository path.
pub fn validate_repo_path(path: &str) -> Result<PathBuf, McpError> {
    if path.is_empty() {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: "repo path is empty".into(),
        });
    }
    if path.len() > 2_000 {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!("repo path too long: {} chars", path.len()),
        });
    }
    let p = std::path::Path::new(path);
    if !p.exists() {
        return Err(McpError {
            kind: McpErrorKind::RepositoryNotFound,
            message: format!("repository not found: {}", path),
        });
    }
    Ok(p.canonicalize().unwrap_or_else(|_| p.to_path_buf()))
}

/// Validate a free-text input (non-empty, bounded length).
pub fn validate_text(text: &str, label: &str) -> Result<String, McpError> {
    let text = text.trim().to_string();
    if text.is_empty() {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!("{} is empty", label),
        });
    }
    if text.len() > MAX_TEXT_LEN {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!(
                "{} exceeds maximum length ({} > {})",
                label,
                text.len(),
                MAX_TEXT_LEN
            ),
        });
    }
    Ok(text)
}

/// Validate a timeout value (0 means default; capped at [`MAX_TIMEOUT_MS`]).
pub fn validate_timeout(ms: u64) -> Result<u64, McpError> {
    if ms == 0 {
        return Ok(DEFAULT_ATTACK_TIMEOUT_MS);
    }
    if ms > MAX_TIMEOUT_MS {
        return Err(McpError {
            kind: McpErrorKind::InvalidRequest,
            message: format!("timeout exceeds maximum ({ms}ms > {MAX_TIMEOUT_MS}ms)"),
        });
    }
    Ok(ms)
}
