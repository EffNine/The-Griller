//! Gate configuration and stale-server expectations.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::canonical;

/// Default per-request timeout applied to the MCP round-trips.
///
/// This is the **outer envelope** for a whole `check_completion` call, so it must not
/// be smaller than the inner per-attack replay budget it contains. It is therefore
/// derived from the single canonical constant rather than restated as its own
/// literal: a replay that legitimately needs the full budget must not be aborted by
/// the envelope while it is still legitimately running.
///
/// Deriving rather than duplicating means the two cannot drift apart again, which is
/// exactly how the previous hardcoded `120_000` came to sit below the 600 s replay
/// budget. See `PHASE_2_BASELINE.md` §2.
pub const DEFAULT_TIMEOUT_MS: u64 = griller_core::command::REPLAY_TIMEOUT_MS;

/// The gate's outer envelope may not be shorter than the canonical replay budget it
/// wraps. Asserted at compile time, so a future edit that reintroduces a smaller
/// literal fails the build instead of silently reintroducing the defect.
const _: () = assert!(DEFAULT_TIMEOUT_MS >= griller_core::command::REPLAY_TIMEOUT_MS);

/// The server properties the gate requires before invoking any tool.
///
/// The gate is built against the same workspace as `griller-mcp`, so the
/// expected values are the canonical constants plus the build-time version.
/// A mismatch means the running server is a different/older build — the gate
/// fails closed with an explicit integration error instead of guessing.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExpectedServer {
    pub service: String,
    pub protocol: String,
    pub schema_version: String,
    pub version: String,
    pub tools: Vec<String>,
}

impl ExpectedServer {
    /// The canonical expectations for this build of the gate.
    pub fn canonical() -> Self {
        Self {
            service: canonical::SERVICE.to_string(),
            protocol: canonical::PROTOCOL.to_string(),
            schema_version: canonical::MCP_SCHEMA_VERSION.to_string(),
            version: env!("CARGO_PKG_VERSION").to_string(),
            tools: canonical::TOOLS.iter().map(|s| s.to_string()).collect(),
        }
    }
}

/// Complete gate configuration. Loaded from a JSON file by the CLI.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateConfig {
    /// Absolute path of the `griller-mcp` binary to spawn.
    pub mcp_binary: PathBuf,
    /// Expected server identity for stale-server protection. Defaults to
    /// [`ExpectedServer::canonical`] when omitted.
    #[serde(default = "ExpectedServer::canonical")]
    pub expected_server: ExpectedServer,
    /// Directory where gate lifecycle logs and decision records are written.
    pub state_dir: PathBuf,
    /// Per-request timeout in milliseconds.
    #[serde(default = "default_timeout")]
    pub timeout_ms: u64,
    /// When true (default), any integration failure produces
    /// `GateStatus::Unavailable` instead of being silently accepted.
    #[serde(default = "default_true")]
    pub fail_closed: bool,
    /// When true (default), a Griller WARN verdict is not treated as PASS by
    /// the gate: it is surfaced as `GateStatus::Warned` and the completion is
    /// not accepted as verified.
    #[serde(default = "default_true")]
    pub warn_not_pass: bool,
}

fn default_timeout() -> u64 {
    DEFAULT_TIMEOUT_MS
}

fn default_true() -> bool {
    true
}

impl GateConfig {
    /// Validate that required fields are present and sane.
    pub fn validate(&self) -> Result<(), String> {
        if self.mcp_binary.as_os_str().is_empty() {
            return Err("gate config: mcp_binary must be set".into());
        }
        if !self.mcp_binary.is_absolute() {
            return Err(format!(
                "gate config: mcp_binary must be an absolute path (got {:?})",
                self.mcp_binary
            ));
        }
        if self.timeout_ms == 0 {
            return Err("gate config: timeout_ms must be > 0".into());
        }
        if self.expected_server.tools.is_empty() {
            return Err("gate config: expected_server.tools must not be empty".into());
        }
        Ok(())
    }
}
