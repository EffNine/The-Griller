//! Stale-server protection: verify the running server before any tool call.
//!
//! The gate checks the `griller.health` report against the expected build
//! identity (service, protocol, schema version, Griller version, tool
//! surface). Any mismatch means a different/older server binary is running —
//! the gate FAILS CLOSED with an explicit integration error. It never
//! silently assumes the correct binary is running, and it never turns an
//! integration error into PASS.

use std::time::Duration;

use crate::config::GateConfig;
use crate::error::{err, GateResult};
use crate::mcp_client::McpSession;
use crate::HealthReport;

/// Fetch and verify `griller.health` against the expected server identity.
pub fn check_server_health(
    session: &mut McpSession,
    config: &GateConfig,
) -> GateResult<HealthReport> {
    let outcome = session
        .call_tool("griller.health", serde_json::json!({}))
        .map_err(|e| err::server_unavailable(format!("health call failed: {e}")))?;

    if let Some(error) = outcome.error {
        return Err(err::stale_server(format!(
            "griller.health returned an error: {}",
            error
        )));
    }

    let health: HealthReport = serde_json::from_str(&outcome.text).map_err(|e| {
        err::malformed(format!(
            "griller.health returned unparseable JSON ({e}); got: {}",
            truncate(&outcome.text, 200)
        ))
    })?;

    verify_health(&health, config)?;
    Ok(health)
}

/// Verify a parsed health report against the expectations.
pub fn verify_health(health: &HealthReport, config: &GateConfig) -> GateResult<()> {
    let expected = &config.expected_server;

    if health.service != expected.service {
        return Err(err::stale_server(format!(
            "service mismatch: running '{}', expected '{}'",
            health.service, expected.service
        )));
    }
    if health.protocol != expected.protocol {
        return Err(err::stale_server(format!(
            "protocol mismatch: running '{}', expected '{}'",
            health.protocol, expected.protocol
        )));
    }
    if health.schema_version != expected.schema_version {
        return Err(err::stale_server(format!(
            "schema_version mismatch: running '{}', expected '{}'",
            health.schema_version, expected.schema_version
        )));
    }
    if health.version != expected.version {
        return Err(err::stale_server(format!(
            "griller version mismatch: running '{}', expected '{}' (rebuild griller-mcp together with the gate)",
            health.version, expected.version
        )));
    }
    for tool in &expected.tools {
        if !health.tools.iter().any(|t| t == tool) {
            return Err(err::stale_server(format!(
                "tool '{tool}' missing from running server (tools: {})",
                health.tools.join(", ")
            )));
        }
    }
    Ok(())
}

/// Expected-server equality for the timeout probing helper (test use).
pub fn probe_timeout() -> Duration {
    Duration::from_millis(200)
}

fn truncate(text: &str, max: usize) -> String {
    if text.len() <= max {
        text.to_string()
    } else {
        format!("{}…", &text[..max])
    }
}

/// Extract the health document from a `tools/call` outcome's text.
pub fn parse_health_text(text: &str) -> GateResult<HealthReport> {
    serde_json::from_str(text).map_err(|e| err::malformed(format!("health JSON parse failed: {e}")))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::ExpectedServer;

    fn health(service: &str, version: &str, schema: &str, protocol: &str) -> HealthReport {
        HealthReport {
            service: service.into(),
            version: version.into(),
            protocol: protocol.into(),
            schema_version: schema.into(),
            tools: crate::canonical::TOOLS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        }
    }

    fn config_with(expected: ExpectedServer) -> GateConfig {
        GateConfig {
            mcp_binary: "/tmp/griller-mcp".into(),
            expected_server: expected,
            state_dir: "/tmp/state".into(),
            timeout_ms: 5000,
            fail_closed: true,
            warn_not_pass: true,
        }
    }

    #[test]
    fn identical_health_passes() {
        let expected = ExpectedServer::canonical();
        let h = health(
            &expected.service,
            &expected.version,
            &expected.schema_version,
            &expected.protocol,
        );
        assert!(verify_health(&h, &config_with(expected)).is_ok());
    }

    #[test]
    fn wrong_version_is_stale() {
        let expected = ExpectedServer::canonical();
        let h = health(
            &expected.service,
            "9.9.9",
            &expected.schema_version,
            &expected.protocol,
        );
        let result = verify_health(&h, &config_with(expected));
        assert!(result.is_err());
        let e = result.unwrap_err();
        assert_eq!(e.kind, crate::error::GateErrorKind::StaleServer);
        assert!(e.message.contains("version mismatch"));
    }

    #[test]
    fn wrong_schema_is_stale() {
        let expected = ExpectedServer::canonical();
        let h = health(
            &expected.service,
            &expected.version,
            "m99.0",
            &expected.protocol,
        );
        assert!(verify_health(&h, &config_with(expected)).is_err());
    }

    #[test]
    fn wrong_service_is_stale() {
        let expected = ExpectedServer::canonical();
        let h = health(
            "some-other-server",
            &expected.version,
            &expected.schema_version,
            &expected.protocol,
        );
        assert!(verify_health(&h, &config_with(expected)).is_err());
    }

    #[test]
    fn missing_tool_is_stale() {
        let expected = ExpectedServer::canonical();
        let mut h = health(
            &expected.service,
            &expected.version,
            &expected.schema_version,
            &expected.protocol,
        );
        h.tools.retain(|t| t != "griller.check_completion");
        assert!(verify_health(&h, &config_with(expected)).is_err());
    }

    #[test]
    fn wrong_protocol_is_stale() {
        let expected = ExpectedServer::canonical();
        let h = health(
            &expected.service,
            &expected.version,
            &expected.schema_version,
            "2024-11-05",
        );
        assert!(verify_health(&h, &config_with(expected)).is_err());
    }

    #[test]
    fn extra_tools_are_tolerated() {
        let expected = ExpectedServer::canonical();
        let mut h = health(
            &expected.service,
            &expected.version,
            &expected.schema_version,
            &expected.protocol,
        );
        h.tools.push("griller.extra".into());
        assert!(verify_health(&h, &config_with(expected)).is_ok());
    }
}
