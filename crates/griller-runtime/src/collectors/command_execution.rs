//! Collector that executes a command and captures its output as evidence.

use std::path::Path;

use griller_core::command::CommandSpec;
use griller_core::evidence::{CommandOutputContent, Evidence, EvidenceContent, EvidenceKind};
use griller_core::provenance::{CaptureMethod, EvidenceSource, Provenance};
use griller_core::redaction::redact;

use crate::error::RuntimeError;
use crate::execution::{EnvSource, ExecutionBackend};

/// Collects `command_output` evidence from an executed command.
pub struct CommandExecutionCollector {
    env: EnvSource,
}

impl CommandExecutionCollector {
    /// Create the collector using the given environment source.
    pub fn new(env: EnvSource) -> Self {
        Self { env }
    }

    /// Execute `spec` in `working_directory` and return evidence.
    ///
    /// A non-zero exit, timeout, or spawn failure is captured as evidence, not
    /// treated as a claim being false.
    pub fn collect(
        &self,
        spec: &CommandSpec,
        working_directory: &Path,
    ) -> Result<Evidence, RuntimeError> {
        let mut executed = spec.clone();
        executed.working_directory = Some(working_directory.to_path_buf());
        let result = ExecutionBackend::execute(&executed, &self.env)?;

        // Redaction boundary: process output -> redaction -> evidence content.
        // Best-effort deterministic secret redaction; the report records what
        // changed and never contains the redacted material.
        let (stdout, stdout_report) = redact(&result.stdout_lossy());
        let (stderr, stderr_report) = redact(&result.stderr_lossy());
        let mut redaction = stdout_report;
        redaction.merge(&stderr_report);

        let content = CommandOutputContent {
            spec: executed.clone(),
            stdout,
            stderr,
            exit_code: result.exit_code,
            status: result.status.clone(),
            enforcement: result.enforcement,
            redaction,
            duration_ms: result.duration_ms,
        };

        let provenance = Provenance {
            source: EvidenceSource::Command,
            capture_method: CaptureMethod::ProcessExecution,
            command: Some(executed),
            working_directory: Some(working_directory.to_path_buf()),
            environment_fingerprint: Some(result.env_fingerprint),
            tool: None,
            reproducibility: griller_core::provenance::Reproducibility::Deterministic,
            directly_observed: true,
            depends_on_agent_claim: false,
        };

        Ok(Evidence::new(
            EvidenceKind::CommandOutput,
            provenance,
            EvidenceContent::CommandOutput(content),
            griller_core::evidence::now_secs(),
        ))
    }
}
