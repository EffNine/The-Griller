//! Gate lifecycle records for observability.
//!
//! Every gate run emits a small, deterministic sequence of stages:
//!
//! ```text
//! COMPLETION_EVENT
//!   -> GRILLER_GATE_STARTED
//!   -> GRILLER_CONNECTED
//!   -> SERVER_CHECKED          (stale-server verification result)
//!   -> CHECK_COMPLETION
//!   -> GRILLER_RESULT
//!   -> GATE_ACCEPTED | GATE_WARNED | GATE_BLOCKED | GATE_UNAVAILABLE
//! ```

use serde::{Deserialize, Serialize};

/// A single lifecycle stage of one gate run.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum LifecycleStage {
    /// A completion boundary was observed by the host.
    CompletionEvent,
    /// The gate began processing the event.
    GrillerGateStarted,
    /// The gate connected to the MCP server.
    GrillerConnected,
    /// Stale-server verification completed (success or failure).
    ServerChecked,
    /// `griller.check_completion` was invoked.
    CheckCompletion,
    /// The Griller result was received and parsed.
    GrillerResult,
    /// Final gate status: accepted.
    GateAccepted,
    /// Final gate status: warned (not converted to PASS).
    GateWarned,
    /// Final gate status: blocked.
    GateBlocked,
    /// Final gate status: verification unavailable (fail-closed).
    GateUnavailable,
    /// Final gate status: the host extracted no completion assertion, so there
    /// was nothing to verify. Not a rejection and not a failure.
    GateNoCompletionAssertion,
}

impl LifecycleStage {
    /// Stable machine-readable label.
    pub fn label(self) -> &'static str {
        match self {
            LifecycleStage::CompletionEvent => "COMPLETION_EVENT",
            LifecycleStage::GrillerGateStarted => "GRILLER_GATE_STARTED",
            LifecycleStage::GrillerConnected => "GRILLER_CONNECTED",
            LifecycleStage::ServerChecked => "SERVER_CHECKED",
            LifecycleStage::CheckCompletion => "CHECK_COMPLETION",
            LifecycleStage::GrillerResult => "GRILLER_RESULT",
            LifecycleStage::GateAccepted => "GATE_ACCEPTED",
            LifecycleStage::GateWarned => "GATE_WARNED",
            LifecycleStage::GateBlocked => "GATE_BLOCKED",
            LifecycleStage::GateUnavailable => "GATE_UNAVAILABLE",
            LifecycleStage::GateNoCompletionAssertion => "GATE_NO_COMPLETION_ASSERTION",
        }
    }
}

/// One lifecycle record: a stage plus a wall-clock timestamp (ms).
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub struct LifecycleRecord {
    pub stage: LifecycleStage,
    pub at_ms: u64,
}

impl LifecycleRecord {
    pub fn new(stage: LifecycleStage, at_ms: u64) -> Self {
        Self { stage, at_ms }
    }
}

/// A mutable, append-only lifecycle log for one run.
#[derive(Debug, Default, Clone)]
pub struct LifecycleLog {
    records: Vec<LifecycleRecord>,
    started_ms: u64,
}

impl LifecycleLog {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn now_ms() -> u64 {
        use std::time::{SystemTime, UNIX_EPOCH};
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map(|d| d.as_millis() as u64)
            .unwrap_or(0)
    }

    pub fn record(&mut self, stage: LifecycleStage) {
        let at = Self::now_ms();
        if self.records.is_empty() {
            self.started_ms = at;
        }
        self.records.push(LifecycleRecord::new(stage, at));
    }

    /// Relative elapsed time of each record from the run start (for logs).
    pub fn elapsed(&self, rec: &LifecycleRecord) -> u64 {
        rec.at_ms.saturating_sub(self.started_ms)
    }

    pub fn records(&self) -> &[LifecycleRecord] {
        &self.records
    }

    pub fn started_ms(&self) -> u64 {
        self.started_ms
    }

    pub fn into_vec(self) -> Vec<LifecycleRecord> {
        self.records
    }

    /// A concise single-line summary for host logs.
    pub fn render(&self) -> String {
        self.records
            .iter()
            .map(|r| format!("{}@+{}ms", r.stage.label(), self.elapsed(r)))
            .collect::<Vec<_>>()
            .join(" -> ")
    }
}

impl From<LifecycleLog> for Vec<LifecycleRecord> {
    fn from(value: LifecycleLog) -> Self {
        value.into_vec()
    }
}

impl From<Vec<LifecycleRecord>> for LifecycleLog {
    fn from(records: Vec<LifecycleRecord>) -> Self {
        let started_ms = records.first().map(|r| r.at_ms).unwrap_or(0);
        LifecycleLog {
            records,
            started_ms,
        }
    }
}
