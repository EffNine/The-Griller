//! The gate runner: connect -> verify -> check_completion -> decision.
//!
//! This is the only orchestration in the gate. It wires the existing pieces:
//! 1. spawn + initialize the `griller-mcp` server;
//! 2. stale-server verification via `griller.health` against the expected
//!    service/protocol/schema/version/tools — any mismatch fails closed;
//! 3. `griller.check_completion` with the event's repository and claim;
//! 4. translate the Griller verdict into a [`GateDecision`] with an explicit
//!    integration status.
//!
//! No verification logic lives here; the decision core is untouched.

use std::path::Path;
use std::time::{Duration, Instant};

use serde_json::Value;

use crate::config::GateConfig;
use crate::error::err;
use crate::health_check::check_server_health;
use crate::lifecycle::{LifecycleLog, LifecycleStage};
use crate::mcp_client::McpSession;
use crate::report::summarize_report;
use crate::CompletionExtraction;
use crate::{
    CompletionEvent, EventSummary, GateDecision, GateStatus, GateTiming, IntegrationInfo,
    GATE_SCHEMA_VERSION,
};

/// The result of one gate run: either a decision, or a hard integration
/// failure that could not even be framed as a decision.
#[derive(Debug)]
#[allow(clippy::large_enum_variant)]
pub enum GateOutcome {
    /// A structured decision (including `Unavailable` for fail-closed paths).
    Decision(GateDecision),
    /// The gate itself could not run (e.g. invalid config, event rejected).
    Failed(crate::error::GateError),
}

/// Run the gate for one completion event.
pub fn run_gate(event: &CompletionEvent, config: &GateConfig) -> GateOutcome {
    let started = Instant::now();
    let mut lifecycle = LifecycleLog::new();
    lifecycle.record(LifecycleStage::CompletionEvent);

    if let Err(message) = config.validate() {
        return GateOutcome::Failed(err::invalid_config(message));
    }

    // The host observed a completion boundary but extracted no completion
    // assertion, so there is nothing to verify. This is checked after config
    // validation (a real operator error still wins) and before any MCP work,
    // because connecting and adjudicating a non-claim would be meaningless.
    //
    // It is a *pre-adjudication observation*: no claim reached the Griller, no
    // evidence was collected, and no epistemic status was assigned. The record
    // says exactly that, and deliberately does not reuse `Unavailable`, which
    // means something went wrong.
    if event.extraction == Some(CompletionExtraction::NoAssertion) {
        lifecycle.record(LifecycleStage::GrillerGateStarted);
        lifecycle.record(LifecycleStage::GateNoCompletionAssertion);
        return GateOutcome::Decision(no_completion_assertion_decision(event, &lifecycle));
    }

    if !config.fail_closed {
        // The fail-closed default is deliberate. An explicitly disabled
        // fail-closed gate still never converts an integration failure into
        // PASS — it reports Unavailable with the policy noted.
        lifecycle.record(LifecycleStage::GrillerGateStarted);
        lifecycle.record(LifecycleStage::GateUnavailable);
        return GateOutcome::Decision(unavailable_decision(
            event,
            &lifecycle,
            config,
            "fail_closed=false: verification skipped by configuration",
            "VERIFICATION_DISABLED",
            0,
        ));
    }

    lifecycle.record(LifecycleStage::GrillerGateStarted);

    let mut session = match McpSession::connect(
        &config.mcp_binary,
        &[],
        Duration::from_millis(config.timeout_ms),
    ) {
        Ok(session) => session,
        Err(e) => {
            lifecycle.record(LifecycleStage::GateUnavailable);
            return GateOutcome::Decision(unavailable_decision(
                event,
                &lifecycle,
                config,
                &e.message,
                e.kind.code(),
                0,
            ));
        }
    };
    lifecycle.record(LifecycleStage::GrillerConnected);
    let server_info = crate::ServerInfo {
        name: session.server_name.clone(),
        version: session.server_version.clone(),
    };

    // Stale-server protection: verify the running server matches the build
    // this gate was compiled against. FAIL CLOSED on any mismatch.
    let connect_start = Instant::now();
    let health = match check_server_health(&mut session, config) {
        Ok(health) => health,
        Err(e) => {
            lifecycle.record(LifecycleStage::GateUnavailable);
            let integration = IntegrationInfo {
                connected: true,
                server_info: Some(server_info),
                health: None,
                stale: true,
                stale_reason: Some(e.message.clone()),
            };
            let decision = unavailable_decision_with(
                event,
                &lifecycle,
                config,
                integration,
                &e.message,
                e.kind.code(),
                connect_start.elapsed().as_millis() as u64,
            );
            return GateOutcome::Decision(decision);
        }
    };

    lifecycle.record(LifecycleStage::ServerChecked);
    let t_connect = connect_start.elapsed().as_millis() as u64;

    // The actual verification: the existing check_completion pipeline.
    lifecycle.record(LifecycleStage::CheckCompletion);
    let check_start = Instant::now();
    let outcome = session.call_tool(
        "griller.check_completion",
        serde_json::json!({
            "repo": event.repository,
            "claim": event.claim,
            "timeout": config.timeout_ms,
        }),
    );
    let t_check = check_start.elapsed().as_millis() as u64;

    let (report, tool_error) = match outcome {
        Ok(outcome) => match outcome.error {
            Some(error_payload) => (None, Some(error_payload)),
            None => match serde_json::from_str::<Value>(&outcome.text) {
                Ok(value) => (Some(value), None),
                Err(_) => (
                    None,
                    Some(
                        serde_json::json!({ "message": "check_completion returned malformed JSON" }),
                    ),
                ),
            },
        },
        Err(e) => {
            lifecycle.record(LifecycleStage::GateUnavailable);
            let integration = IntegrationInfo {
                connected: true,
                server_info: Some(server_info),
                health: Some(health.clone()),
                stale: false,
                stale_reason: None,
            };
            let decision = unavailable_decision_with(
                event,
                &lifecycle,
                config,
                integration,
                &e.message,
                e.kind.code(),
                t_connect,
            );
            return GateOutcome::Decision(decision);
        }
    };

    let summary = report.as_ref().map(summarize_report);
    match summary {
        Some(summary) => {
            let status = match summary.verdict.as_str() {
                "pass" => GateStatus::Accepted,
                "warn" if config.warn_not_pass => GateStatus::Warned,
                "warn" => GateStatus::Accepted,
                "block" => GateStatus::Blocked,
                other => {
                    // An unknown verdict is an integration anomaly: never PASS.
                    lifecycle.record(LifecycleStage::GateUnavailable);
                    let decision = unavailable_decision_with(
                        event,
                        &lifecycle,
                        config,
                        IntegrationInfo {
                            connected: true,
                            server_info: Some(server_info),
                            health: Some(health.clone()),
                            stale: false,
                            stale_reason: None,
                        },
                        &format!("unexpected verdict '{other}' in report"),
                        "UNEXPECTED_VERDICT",
                        t_connect,
                    );
                    return GateOutcome::Decision(decision);
                }
            };

            let final_stage = match status {
                GateStatus::Accepted => LifecycleStage::GateAccepted,
                GateStatus::Warned => LifecycleStage::GateWarned,
                GateStatus::Blocked => LifecycleStage::GateBlocked,
                GateStatus::Unavailable => LifecycleStage::GateUnavailable,
                // Unreachable: `NoCompletionAssertion` short-circuits above,
                // before any Griller verdict exists. Mapped explicitly so the
                // match stays exhaustive and the compiler keeps checking it.
                GateStatus::NoCompletionAssertion => LifecycleStage::GateNoCompletionAssertion,
            };
            lifecycle.record(final_stage);

            let decision = GateDecision {
                schema_version: GATE_SCHEMA_VERSION.to_string(),
                status,
                verdict: Some(summary.verdict),
                reason_code: Some(summary.reason_code),
                findings: summary.findings,
                required_action: Some(summary.required_action),
                required_action_message: Some(summary.required_action_message),
                event: EventSummary {
                    repository: event.repository.clone(),
                    agent_id: event.agent_id.clone(),
                    session_id: event.session_id.clone(),
                    commit: event.commit.clone(),
                    timestamp_ms: event.timestamp_ms,
                },
                integration: IntegrationInfo {
                    connected: true,
                    server_info: Some(server_info),
                    health: Some(health),
                    stale: false,
                    stale_reason: None,
                },
                griller_report: report,
                timing: GateTiming {
                    connect_ms: t_connect,
                    check_ms: t_check,
                    total_ms: started.elapsed().as_millis() as u64,
                },
                lifecycle: lifecycle.into_vec(),
            };
            GateOutcome::Decision(decision)
        }
        None => {
            // Tool error or malformed report: explicit integration failure.
            let reason = tool_error
                .as_ref()
                .and_then(|e| e.get("message"))
                .and_then(Value::as_str)
                .unwrap_or("check_completion returned no usable report")
                .to_string();
            lifecycle.record(LifecycleStage::GateUnavailable);
            GateOutcome::Decision(unavailable_decision_with(
                event,
                &lifecycle,
                config,
                IntegrationInfo {
                    connected: true,
                    server_info: Some(server_info),
                    health: Some(health.clone()),
                    stale: false,
                    stale_reason: None,
                },
                &reason,
                "CHECK_COMPLETION_FAILED",
                t_connect,
            ))
        }
    }
}

/// Reason code recorded when the host extracted no completion assertion.
///
/// Free-form and stable, in the same `SCREAMING_SNAKE` style as
/// `CHECK_COMPLETION_FAILED` and `STALE_SERVER`. It is a `String` field, not an
/// enum, so adding it required no vocabulary change and no migration of the
/// existing records.
pub const NO_COMPLETION_ASSERTION_REASON: &str = "NO_COMPLETION_ASSERTION";

/// Build the decision for "the host observed a completion boundary but there
/// was no completion assertion to verify".
///
/// This is deliberately shaped like [`unavailable_decision_with`] — same
/// `GateDecision`, same single persistence path — but semantically distinct:
///
/// * `status` is [`GateStatus::NoCompletionAssertion`], never `Unavailable`,
///   because **nothing failed**.
/// * `verdict` is `None` and `griller_report` is `None`, because the Griller
///   was never invoked and no adjudication occurred. There is no claim, no
///   evidence, and no epistemic status anywhere in this record.
/// * `required_action` is `None` and `required_action_message` is `None`,
///   because there is nothing for the host to fix and **nothing for the host to
///   show the agent**. A host that treats every decision carrying a
///   `required_action_message` as agent-visible will correctly stay silent.
///
/// The host remains the authority on *whether* an assertion exists — extraction
/// is a pure function of the assistant text and only the host saw it. The gate
/// is the sole authority on what that observation *means* and on how it is
/// written down.
fn no_completion_assertion_decision(
    event: &CompletionEvent,
    lifecycle: &LifecycleLog,
) -> GateDecision {
    GateDecision {
        schema_version: GATE_SCHEMA_VERSION.to_string(),
        status: GateStatus::NoCompletionAssertion,
        verdict: None,
        reason_code: Some(NO_COMPLETION_ASSERTION_REASON.to_string()),
        findings: vec![crate::report::Finding {
            key: "completion_trigger".into(),
            detail: "no completion assertion was extracted from the assistant message; \
                     there was nothing to verify"
                .into(),
        }],
        required_action: None,
        required_action_message: None,
        event: EventSummary {
            repository: event.repository.clone(),
            agent_id: event.agent_id.clone(),
            session_id: event.session_id.clone(),
            commit: event.commit.clone(),
            timestamp_ms: event.timestamp_ms,
        },
        integration: IntegrationInfo {
            // The Griller was never contacted. Reporting it as connected would
            // misstate what happened, exactly as `m1_none()` exists to prevent
            // for enforcement.
            connected: false,
            server_info: None,
            health: None,
            stale: false,
            stale_reason: Some("griller not invoked: no completion assertion".into()),
        },
        griller_report: None,
        timing: GateTiming {
            connect_ms: 0,
            check_ms: 0,
            total_ms: 0,
        },
        lifecycle: lifecycle.clone().into_vec(),
    }
}

fn unavailable_decision(
    event: &CompletionEvent,
    lifecycle: &LifecycleLog,
    config: &GateConfig,
    reason: &str,
    reason_code: &str,
    connect_ms: u64,
) -> GateDecision {
    unavailable_decision_with(
        event,
        lifecycle,
        config,
        IntegrationInfo {
            connected: false,
            server_info: None,
            health: None,
            stale: false,
            stale_reason: None,
        },
        reason,
        reason_code,
        connect_ms,
    )
}

#[allow(clippy::too_many_arguments)]
fn unavailable_decision_with(
    event: &CompletionEvent,
    lifecycle: &LifecycleLog,
    config: &GateConfig,
    integration: IntegrationInfo,
    reason: &str,
    reason_code: &str,
    connect_ms: u64,
) -> GateDecision {
    GateDecision {
        schema_version: GATE_SCHEMA_VERSION.to_string(),
        status: GateStatus::Unavailable,
        verdict: None,
        reason_code: Some(reason_code.to_string()),
        findings: vec![crate::report::Finding {
            key: "integration".into(),
            detail: reason.to_string(),
        }],
        required_action: Some("verify_griller_installation".into()),
        required_action_message: Some(format!(
            "The automatic gate could not verify this completion: {reason}. \
             The completion is NOT accepted as verified. Check the griller-mcp \
             binary ({}) and the gate configuration, then re-attempt completion.",
            config.mcp_binary.display()
        )),
        event: EventSummary {
            repository: event.repository.clone(),
            agent_id: event.agent_id.clone(),
            session_id: event.session_id.clone(),
            commit: event.commit.clone(),
            timestamp_ms: event.timestamp_ms,
        },
        integration,
        griller_report: None,
        timing: GateTiming {
            connect_ms,
            check_ms: 0,
            total_ms: 0,
        },
        lifecycle: lifecycle.clone().into_vec(),
    }
}

/// Convenience for hosts: resolve the HEAD commit for the event repository
/// (shell-free) and attach it to a copy of the event.
pub fn with_head_commit(mut event: CompletionEvent) -> CompletionEvent {
    if event.commit.is_none() {
        event.commit = crate::head::resolve_head_commit(Path::new(&event.repository));
    }
    event
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{canonical, ExpectedServer};
    use std::path::PathBuf;

    pub(crate) fn test_config(mcp_binary: PathBuf) -> GateConfig {
        GateConfig {
            mcp_binary,
            expected_server: ExpectedServer::canonical(),
            state_dir: PathBuf::from("/tmp/griller-gate-test-state"),
            timeout_ms: 60_000,
            fail_closed: true,
            warn_not_pass: true,
        }
    }

    fn test_event(repo: &str, claim: &str) -> CompletionEvent {
        CompletionEvent {
            schema_version: crate::GATE_SCHEMA_VERSION.into(),
            repository: repo.into(),
            claim: claim.into(),
            agent_id: Some("test-agent".into()),
            session_id: Some("test-session".into()),
            commit: None,
            timestamp_ms: 1,
            // Absent means "assertion extracted" — the pre-existing behaviour.
            extraction: None,
        }
    }

    #[test]
    fn canonical_expected_server_matches_health_contract() {
        let expected = ExpectedServer::canonical();
        assert_eq!(expected.service, canonical::SERVICE);
        assert_eq!(expected.protocol, canonical::PROTOCOL);
        assert_eq!(expected.schema_version, canonical::MCP_SCHEMA_VERSION);
        assert_eq!(expected.version, env!("CARGO_PKG_VERSION"));
        assert_eq!(expected.tools.len(), 6);
    }

    #[test]
    fn invalid_config_fails_without_running() {
        let config = GateConfig {
            mcp_binary: PathBuf::from("relative/path"),
            ..test_config(PathBuf::new())
        };
        match run_gate(&test_event("/tmp", "tests pass"), &config) {
            GateOutcome::Failed(e) => {
                assert_eq!(e.kind, crate::error::GateErrorKind::InvalidConfig)
            }
            GateOutcome::Decision(_) => panic!("invalid config must not produce a decision"),
        }
    }

    #[test]
    fn fail_closed_false_is_still_never_pass() {
        let config = GateConfig {
            fail_closed: false,
            ..test_config(PathBuf::from("/nonexistent/griller-mcp"))
        };
        match run_gate(&test_event("/tmp", "tests pass"), &config) {
            GateOutcome::Decision(ref d) => {
                assert!(d.status.is_unavailable());
                assert!(!d.status.is_accepted());
            }
            GateOutcome::Failed(_) => panic!("expected a decision"),
        }
    }
}
