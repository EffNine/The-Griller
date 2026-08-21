//! Test-only fake MCP server for gate integration tests.
//!
//! Speaks newline-delimited JSON-RPC (the MCP stdio framing used by
//! `rust-mcp-sdk`) and answers `initialize` and `tools/call` with scripted
//! responses. Used to exercise the gate's failure paths (wrong schema,
//! stale version, unavailable tool) without touching a real repository.
//!
//! Scripting: the health response is the default Griller-compatible one; the
//! `--schema-version`, `--version`, `--service`, and `--protocol` flags
//! override individual fields. `check_completion` returns a scripted report
//! selected by `--verdict` (pass|warn|block), or an error with `--tool-error`.

#![forbid(unsafe_code)]

use serde_json::{json, Value};
use std::io::{BufRead, BufReader, Write};

const TOOLS: [&str; 6] = [
    "griller.verify_claim",
    "griller.check_completion",
    "griller.check_context",
    "griller.run_attacks",
    "griller.self_grill",
    "griller.health",
];

fn main() {
    // Configuration is encoded in the executable name, e.g.
    // `fake-mcp-schema-version-m99.0` — the gate spawns argv[0] with no
    // arguments under env_clear, so this is the only channel that survives.
    let exe = std::env::args().next().unwrap_or_default();
    let exe = std::path::Path::new(&exe)
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("fake-mcp");
    let parts: Vec<&str> = exe.split('-').collect();

    let mut service = "the-griller".to_string();
    let mut version = env!("CARGO_PKG_VERSION").to_string();
    let mut schema_version = "m8.0".to_string();
    let mut protocol = "2025-11-25".to_string();
    let mut verdict = "pass".to_string();
    let mut tool_error = false;

    let mut i = 0;
    while i < parts.len() {
        match parts[i] {
            "service" => service = parts.get(i + 1).copied().unwrap_or_default().to_string(),
            "version" => version = parts.get(i + 1).copied().unwrap_or_default().to_string(),
            "schema" => {
                if parts.get(i + 1).copied() == Some("version") {
                    schema_version = parts.get(i + 2).copied().unwrap_or_default().to_string();
                }
            }
            "protocol" => protocol = parts.get(i + 1).copied().unwrap_or_default().to_string(),
            "verdict" => verdict = parts.get(i + 1).copied().unwrap_or_default().to_string(),
            "tool" if parts.get(i + 1).copied() == Some("error") => {
                tool_error = true;
            }
            _ => {}
        }
        i += 1;
    }

    run(
        service,
        version,
        schema_version,
        protocol,
        verdict,
        tool_error,
    );
}

fn run(
    service: String,
    version: String,
    schema_version: String,
    protocol: String,
    verdict: String,
    tool_error: bool,
) {
    let stdin = std::io::stdin();
    let reader = BufReader::new(stdin.lock());
    let mut stdout = std::io::stdout().lock();

    for line in reader.lines() {
        let Ok(line) = line else { break };
        if line.trim().is_empty() {
            continue;
        }
        let Ok(request) = serde_json::from_str::<Value>(&line) else {
            continue;
        };
        let method = request.get("method").and_then(Value::as_str).unwrap_or("");
        let id = request.get("id").cloned();

        let response: Value = match method {
            "initialize" => json!({
                "jsonrpc": "2.0",
                "id": id,
                "result": {
                    "protocolVersion": "2025-11-25",
                    "capabilities": { "tools": { "listChanged": false } },
                    "serverInfo": { "name": "the-griller-mcp", "version": version }
                }
            }),
            "notifications/initialized" | "notifications/exit" => continue,
            "shutdown" => json!({ "jsonrpc": "2.0", "id": id, "result": {} }),
            "tools/call" => {
                let name = request
                    .pointer("/params/name")
                    .and_then(Value::as_str)
                    .unwrap_or("");
                match name {
                    "griller.health" => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": {
                            "content": [{
                                "type": "text",
                                "text": serde_json::to_string(&json!({
                                    "service": service,
                                    "version": version,
                                    "protocol": protocol,
                                    "schema_version": schema_version,
                                    "tools": TOOLS,
                                    "capabilities": { "claim_verification": true, "context_check": true, "red_team": true, "self_grill": true },
                                    "execution": { "shell_free": true, "worktree_isolation": true, "network_isolation": false, "os_sandbox": false },
                                    "security": { "shell_free": true, "command_allow_list": true, "secret_redaction": "best_effort", "network_isolation": "not_enforced", "os_sandbox": "not_enforced", "privilege_reduction": "not_enforced" }
                                })).unwrap()
                            }]
                        }
                    }),
                    "griller.check_completion" => {
                        if tool_error {
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "isError": true,
                                    "content": [{
                                        "type": "text",
                                        "text": serde_json::to_string(&json!({
                                            "kind": "execution_failed",
                                            "message": "scripted tool error"
                                        })).unwrap()
                                    }]
                                }
                            })
                        } else {
                            let report = match verdict.as_str() {
                                "warn" => json!({
                                    "verdict": "warn",
                                    "reason_code": "EVIDENCE_WEAK",
                                    "required_action": "strengthen_evidence",
                                    "required_action_message": "Provide stronger evidence."
                                }),
                                "block" => json!({
                                    "verdict": "block",
                                    "reason_code": "CONTRADICTED",
                                    "required_action": "fix_and_retest",
                                    "required_action_message": "Fix the failing tests and re-attempt."
                                }),
                                _ => json!({
                                    "verdict": "pass",
                                    "reason_code": "EVIDENCE_MATCHED",
                                    "required_action": "none",
                                    "required_action_message": ""
                                }),
                            };
                            json!({
                                "jsonrpc": "2.0",
                                "id": id,
                                "result": {
                                    "content": [{
                                        "type": "text",
                                        "text": serde_json::to_string(&report).unwrap()
                                    }]
                                }
                            })
                        }
                    }
                    _ => json!({
                        "jsonrpc": "2.0",
                        "id": id,
                        "result": { "isError": true, "content": [{ "type": "text", "text": "unknown tool" }] }
                    }),
                }
            }
            _ => continue,
        };

        let mut encoded = serde_json::to_string(&response).unwrap();
        encoded.push('\n');
        if stdout.write_all(encoded.as_bytes()).is_err() {
            break;
        }
        let _ = stdout.flush();
    }
}
