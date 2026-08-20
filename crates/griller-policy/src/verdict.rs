//! The operational verdict model.
//!
//! PASS / WARN / BLOCK are **operational decisions**, not truth claims. They
//! describe what the system should do with an already-adjudicated claim.

use serde::{Deserialize, Serialize};

/// An operational verdict.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Verdict {
    /// The claim is accepted; no blocking action is required.
    Pass,
    /// The claim is not accepted as-is; proceed only with caution / stronger
    /// evidence.
    Warn,
    /// The claim must not be accepted; a required action must happen first.
    Block,
}

impl Verdict {
    /// The CLI process exit code for this verdict.
    ///
    /// `0 = PASS`, `1 = WARN`, `2 = BLOCK`. A separate error code (`3`) is
    /// used by the CLI for execution/system failures so that BLOCK is never
    /// conflated with a system failure.
    pub fn exit_code(self) -> i32 {
        match self {
            Verdict::Pass => 0,
            Verdict::Warn => 1,
            Verdict::Block => 2,
        }
    }

    /// True for the permissive verdict.
    pub fn is_pass(self) -> bool {
        matches!(self, Verdict::Pass)
    }

    /// True for the blocking verdict.
    pub fn is_block(self) -> bool {
        matches!(self, Verdict::Block)
    }
}

impl std::fmt::Display for Verdict {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{self:?}")
    }
}
