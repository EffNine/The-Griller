//! Evidence provenance: *where* evidence came from and *how* it was captured.
//!
//! Provenance deliberately avoids a numeric "trust ladder". Instead it records
//! structured facts, most importantly whether the evidence was **directly
//! observed** from the environment versus derived from an **agent claim**.
//!
//! Invariant: an agent claim is never silently upgraded into environment
//! evidence. Agent-derived evidence is always `source = Agent` and
//! `directly_observed = false`.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};
use serde_json::json;

use crate::command::CommandSpec;

/// Where the evidence came from.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum EvidenceSource {
    /// Evidence produced by running a git command.
    Git,
    /// Evidence produced by reading from the filesystem.
    Filesystem,
    /// Evidence produced by reading a build/package manifest.
    Manifest,
    /// Evidence produced by executing an arbitrary command.
    Command,
    /// Evidence describing the repository as a whole.
    Repository,
    /// Evidence that originated from an agent's claim, not the environment.
    Agent,
}

/// The mechanism used to capture the evidence.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum CaptureMethod {
    /// Captured by invoking git.
    GitInvocation,
    /// Captured by reading a file.
    FileRead,
    /// Captured by scanning a directory.
    DirectoryScan,
    /// Captured by running a child process and capturing its output.
    ProcessExecution,
    /// Captured by declaration (agent statements, synthetic data).
    Declared,
}

/// Whether re-capturing the evidence under identical conditions is expected to
/// reproduce it.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Reproducibility {
    /// Expected to reproduce under identical conditions.
    Deterministic,
    /// Not expected to reproduce (e.g. agent prose, wall-clock timings).
    NonDeterministic,
    /// Unknown.
    Unknown,
}

/// Structured provenance facts for one evidence record.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Provenance {
    /// Primary source of the evidence.
    pub source: EvidenceSource,
    /// Mechanism used to capture it.
    pub capture_method: CaptureMethod,
    /// The command specification that produced it, if applicable.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub command: Option<CommandSpec>,
    /// Working directory the evidence was captured from.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub working_directory: Option<PathBuf>,
    /// A hash over the environment actually passed to the command.
    ///
    /// This is a fingerprint, not a dump; it never contains raw values.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub environment_fingerprint: Option<String>,
    /// Tool and version used to capture the evidence, e.g. `git 2.53.0`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tool: Option<String>,
    /// Expected reproducibility of the capture.
    pub reproducibility: Reproducibility,
    /// True when the environment directly demonstrated the content.
    pub directly_observed: bool,
    /// True when this evidence depends on an agent claim.
    pub depends_on_agent_claim: bool,
}

impl Provenance {
    /// Provenance for directly observed environment evidence.
    ///
    /// `directly_observed` is forced to `true` and
    /// `depends_on_agent_claim` to `false`.
    pub fn environment(
        source: EvidenceSource,
        capture_method: CaptureMethod,
        working_directory: Option<PathBuf>,
    ) -> Self {
        Self {
            source,
            capture_method,
            command: None,
            working_directory,
            environment_fingerprint: None,
            tool: None,
            reproducibility: Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        }
    }

    /// The hash-relevant view of this provenance.
    ///
    /// `working_directory` is intentionally excluded: it is a location, not a
    /// property of the captured state. `environment_fingerprint` is also
    /// excluded: evidence identity describes *what* was observed, and the
    /// environment is *where/how* it was observed. The fingerprint remains in
    /// the stored [`Provenance`].
    pub(crate) fn hash_view(&self) -> serde_json::Value {
        let command = self.command.as_ref().map(|c| {
            json!({
                "executable": c.executable,
                "args": c.args,
                "timeout_ms": c.timeout_ms,
                "environment_policy": c.environment_policy,
                "network_policy": c.network_policy,
            })
        });
        json!({
            "source": self.source,
            "capture_method": self.capture_method,
            "command": command,
            "tool": self.tool,
            "reproducibility": self.reproducibility,
            "directly_observed": self.directly_observed,
            "depends_on_agent_claim": self.depends_on_agent_claim,
        })
    }

    /// Provenance for agent-derived evidence.
    ///
    /// This is the *only* constructor that yields `source = Agent`;
    /// `directly_observed` is forced to `false` so agent claims can never be
    /// upgraded into environment evidence.
    pub fn agent() -> Self {
        Self {
            source: EvidenceSource::Agent,
            capture_method: CaptureMethod::Declared,
            command: None,
            working_directory: None,
            environment_fingerprint: None,
            tool: None,
            reproducibility: Reproducibility::NonDeterministic,
            directly_observed: false,
            depends_on_agent_claim: true,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn agent_provenance_cannot_be_directly_observed() {
        let p = Provenance::agent();
        assert_eq!(p.source, EvidenceSource::Agent);
        assert!(!p.directly_observed);
        assert!(p.depends_on_agent_claim);
    }

    #[test]
    fn environment_provenance_is_direct() {
        let p = Provenance::environment(
            EvidenceSource::Git,
            CaptureMethod::GitInvocation,
            Some("/tmp/repo".into()),
        );
        assert!(p.directly_observed);
        assert!(!p.depends_on_agent_claim);
    }
}
