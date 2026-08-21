//! The Griller automatic completion gate.
//!
//! The gate is an **integration/runtime boundary**, not part of the Griller's
//! decision core. It receives a [`CompletionEvent`] at a host completion
//! boundary, connects to the existing `griller-mcp` server, verifies the
//! server is the expected build (stale-server protection), and invokes the
//! existing `griller.check_completion` pipeline. The verdict of that pipeline
//! is translated into a [`GateDecision`] with an explicit integration status.
//!
//! The gate never re-implements verification: there is exactly one completion
//! verification pipeline (in `griller-mcp`), and CLI, MCP, and the gate all
//! reach it through the same server.

#![forbid(unsafe_code)]
#![allow(missing_docs)]

pub mod config;
pub mod error;
pub mod head;
pub mod health_check;
pub mod lifecycle;
pub mod mcp_client;
pub mod report;
pub mod runner;
pub mod tracker;

pub use config::{ExpectedServer, GateConfig, DEFAULT_TIMEOUT_MS};
pub use error::{GateError, GateErrorKind};
pub use lifecycle::{LifecycleRecord, LifecycleStage};
pub use report::{Finding, ReportSummary};
pub use runner::{run_gate, GateOutcome};
pub use tracker::GateTracker;

use serde::{Deserialize, Serialize};

/// Canonical schema version of the gate's own decision document.
///
/// This is distinct from the MCP `check_completion` schema version (`m8.0`):
/// the gate does not change the MCP schema.
pub const GATE_SCHEMA_VERSION: &str = "g1.0";

/// Canonical values the gate expects from a healthy Griller MCP server.
pub mod canonical {
    pub const SERVICE: &str = "the-griller";
    pub const PROTOCOL: &str = "2025-11-25";
    pub const MCP_SCHEMA_VERSION: &str = "m8.0";
    pub const TOOLS: [&str; 6] = [
        "griller.verify_claim",
        "griller.check_completion",
        "griller.check_context",
        "griller.run_attacks",
        "griller.self_grill",
        "griller.health",
    ];
}

/// An agent-agnostic completion boundary event.
///
/// The host (e.g. the OpenCode adapter) constructs this from its own notion of
/// "the agent reached a completion boundary". The gate itself does not decide
/// when a completion boundary occurs.
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct CompletionEvent {
    /// Canonical schema version of the event document.
    pub schema_version: String,
    /// Absolute path of the repository the work happened in.
    pub repository: String,
    /// The agent's completion claim (verbatim, truncated by the host).
    pub claim: String,
    /// Agent/session identifier if the host has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_id: Option<String>,
    /// Session identifier if the host has one.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub session_id: Option<String>,
    /// HEAD commit the work was performed against, when resolvable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub commit: Option<String>,
    /// Wall-clock timestamp of the event in milliseconds since the epoch.
    pub timestamp_ms: u64,
}

impl CompletionEvent {
    /// A stable identity of this event for loop prevention: the session (if
    /// any) plus the agent/session identifier. Two events with the same
    /// identity refer to the same completion boundary.
    pub fn identity(&self) -> String {
        format!(
            "{}|{}",
            self.session_id.as_deref().unwrap_or(""),
            self.agent_id.as_deref().unwrap_or("")
        )
    }
}

/// The operational status of the gate's decision.
///
/// Distinct from the Griller verdict: this describes what the *gate* decided
/// to do with the completion event, including integration failure.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum GateStatus {
    /// The Griller accepted the completion claim.
    Accepted,
    /// The Griller returned WARN; the completion was not converted to PASS.
    Warned,
    /// The Griller returned BLOCK; the completion must not be accepted.
    Blocked,
    /// The Griller could not run (unavailable, malformed, stale, timeout).
    /// Never converted to PASS. Fail-closed by default.
    Unavailable,
}

impl GateStatus {
    /// True when the gate allowed the completion to proceed.
    pub fn is_accepted(self) -> bool {
        matches!(self, GateStatus::Accepted)
    }

    /// True when the gate rejected the completion (warning or block).
    pub fn is_rejected(self) -> bool {
        matches!(self, GateStatus::Warned | GateStatus::Blocked)
    }

    /// True when the gate surfaced a warning (never a silent PASS).
    pub fn is_warned(self) -> bool {
        matches!(self, GateStatus::Warned)
    }

    /// True when verification itself was unavailable.
    pub fn is_unavailable(self) -> bool {
        matches!(self, GateStatus::Unavailable)
    }
}

impl std::fmt::Display for GateStatus {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}

/// What the integration verified about the running server.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IntegrationInfo {
    /// Whether the MCP connection was established.
    pub connected: bool,
    /// Server name/version reported by `initialize`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub server_info: Option<ServerInfo>,
    /// The parsed `griller.health` response, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub health: Option<HealthReport>,
    /// Whether the running server failed the stale-server verification.
    pub stale: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub stale_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ServerInfo {
    pub name: String,
    pub version: String,
}

/// The subset of `griller.health` the gate inspects for stale-server checks.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct HealthReport {
    pub service: String,
    pub version: String,
    pub protocol: String,
    pub schema_version: String,
    pub tools: Vec<String>,
}

/// Timing diagnostics for one gate run (milliseconds).
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GateTiming {
    pub connect_ms: u64,
    pub check_ms: u64,
    pub total_ms: u64,
}

/// The decision the gate produces for one completion event.
///
/// The raw `griller_report` carries the complete structured
/// `griller.check_completion` report; `findings` is a bounded, human-readable
/// summary derived from it.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GateDecision {
    pub schema_version: String,
    /// Gate-level status.
    pub status: GateStatus,
    /// The Griller verdict, when a report was produced.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub verdict: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub reason_code: Option<String>,
    pub findings: Vec<Finding>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_action: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub required_action_message: Option<String>,
    /// Summary of the event that triggered this run.
    pub event: EventSummary,
    pub integration: IntegrationInfo,
    /// The full structured `griller.check_completion` report, when available.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub griller_report: Option<serde_json::Value>,
    pub timing: GateTiming,
    pub lifecycle: Vec<LifecycleRecord>,
}

impl GateDecision {
    /// A short, single-line human summary suitable for the host.
    pub fn summary_line(&self) -> String {
        match self.status {
            GateStatus::Accepted => format!(
                "GATE_ACCEPTED verdict=pass reason={}",
                self.reason_code.as_deref().unwrap_or("")
            ),
            GateStatus::Warned => format!(
                "GATE_WARNED verdict=warn reason={}",
                self.reason_code.as_deref().unwrap_or("")
            ),
            GateStatus::Blocked => format!(
                "GATE_BLOCKED verdict=block reason={} action={}",
                self.reason_code.as_deref().unwrap_or(""),
                self.required_action.as_deref().unwrap_or("")
            ),
            GateStatus::Unavailable => "GATE_UNAVAILABLE".to_string(),
        }
    }
}

/// A bounded summary of the triggering event.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EventSummary {
    pub repository: String,
    pub agent_id: Option<String>,
    pub session_id: Option<String>,
    pub commit: Option<String>,
    pub timestamp_ms: u64,
}
