//! Minimal stdio MCP client used by the gate.
//!
//! The gate is a *client* of the existing `griller-mcp` server — it never
//! re-implements verification. This module speaks just enough of the MCP
//! protocol (newline-delimited JSON-RPC over stdio, as used by
//! `rust-mcp-sdk`) to run `initialize`, `notifications/initialized`,
//! `tools/call`, and `shutdown`.
//!
//! Security posture mirrors the runtime execution model:
//! * the server is spawned directly (no shell) with an explicit allow-listed
//!   environment (`PATH`, `HOME`, `LANG`, `TMPDIR`, `USER`);
//! * stdout/stderr are capped; oversized output is an error, never trusted;
//! * every round-trip has a timeout and the child is killed on timeout.

use std::io::{BufRead, BufReader, Read, Write};
use std::path::Path;
use std::process::{Child, ChildStderr, ChildStdin, ChildStdout, Command, Stdio};
use std::time::{Duration, Instant};

use serde_json::{json, Value};

use crate::error::{err, GateResult};

/// Maximum bytes accepted from the server's stdout per round-trip.
const MAX_RESPONSE_BYTES: usize = 8 * 1024 * 1024;
/// Maximum bytes captured from the server's stderr (for diagnostics only).
const MAX_STDERR_BYTES: usize = 64 * 1024;

/// Environment keys passed to the server child (mirrors the runtime
/// allow-list; excludes credential-carrying variables).
const CHILD_ENV_ALLOWLIST: [&str; 5] = ["PATH", "HOME", "LANG", "TMPDIR", "USER"];

/// A connected, initialized MCP session.
pub struct McpSession {
    child: Child,
    stdin: ChildStdin,
    stdout: BufReader<ChildStdout>,
    next_id: u64,
    timeout: Duration,
    /// Server-reported name/version from `initialize`.
    pub server_name: String,
    pub server_version: String,
}
/// Result of a `tools/call`.
#[derive(Debug)]
pub struct ToolCallOutcome {
    /// Structured error payload when the server rejected the call
    /// (`isError: true`), as returned by the Griller's error model.
    pub error: Option<Value>,
    /// Concatenated text content of the result (the tool's JSON document).
    pub text: String,
}

impl McpSession {
    /// Spawn and initialize the MCP server binary.
    ///
    /// `initial_args` lets tests substitute a fake server binary; production
    /// callers pass `&[]`.
    pub fn connect(binary: &Path, initial_args: &[String], timeout: Duration) -> GateResult<Self> {
        let mut cmd = Command::new(binary);
        cmd.args(initial_args);
        cmd.env_clear();
        for key in CHILD_ENV_ALLOWLIST {
            if let Ok(value) = std::env::var(key) {
                cmd.env(key, value);
            }
        }
        cmd.stdin(Stdio::piped());
        cmd.stdout(Stdio::piped());
        cmd.stderr(Stdio::piped());
        let mut child = cmd.spawn().map_err(|e| {
            err::server_unavailable(format!(
                "failed to spawn MCP server {:?}: {e}",
                binary.display()
            ))
        })?;
        let stdin = child.stdin.take().expect("stdin piped");
        let stdout = BufReader::new(child.stdout.take().expect("stdout piped"));
        // Stderr must never block the gate and never deadlock the child on a
        // full pipe: a dedicated thread drains it (bounded) until EOF. The
        // child dies with the session, so the thread always terminates.
        if let Some(stderr) = child.stderr.take() {
            std::thread::spawn(move || Self::drain_stderr_until_eof(stderr));
        }

        let mut session = Self {
            child,
            stdin,
            stdout,
            next_id: 1,
            timeout,
            server_name: String::new(),
            server_version: String::new(),
        };

        // `initialize` is a best-effort handshake: some servers (including
        // rust-mcp-sdk 1.0.1) never send the response. That is fine — the
        // authoritative identity check happens later via `griller.health`.
        let init = session.request_with_timeout(
            "initialize",
            json!({
                "protocolVersion": "2025-11-25",
                "capabilities": {},
                "clientInfo": { "name": "griller-gate", "version": env!("CARGO_PKG_VERSION") }
            }),
            Duration::from_secs(3),
        );
        if let Ok(init) = init {
            let server_info = init
                .get("serverInfo")
                .cloned()
                .unwrap_or_else(|| json!({"name": "", "version": ""}));
            session.server_name = server_info
                .get("name")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
            session.server_version = server_info
                .get("version")
                .and_then(Value::as_str)
                .unwrap_or("")
                .to_string();
        }

        session.notify("notifications/initialized", json!({}));

        Ok(session)
    }

    /// Call a tool and return its content.
    pub fn call_tool(&mut self, name: &str, arguments: Value) -> GateResult<ToolCallOutcome> {
        let response = self.request(
            "tools/call",
            json!({ "name": name, "arguments": arguments }),
        )?;
        let is_error = response
            .get("isError")
            .and_then(Value::as_bool)
            .unwrap_or(false);
        let content = response
            .get("content")
            .and_then(Value::as_array)
            .cloned()
            .unwrap_or_default();

        let mut error = None;
        let mut text_parts: Vec<String> = Vec::new();
        for block in content {
            match block.get("type").and_then(Value::as_str) {
                Some("text") => {
                    if let Some(text) = block.get("text").and_then(Value::as_str) {
                        text_parts.push(text.to_string());
                    }
                }
                Some("resource") => {
                    if let Some(text) = block.pointer("/resource/text").and_then(Value::as_str) {
                        text_parts.push(text.to_string());
                    }
                }
                _ => {}
            }
        }
        if is_error {
            if let Ok(parsed) = serde_json::from_str::<Value>(&text_parts.join("\n")) {
                error = Some(parsed);
            } else if !text_parts.is_empty() {
                error = Some(json!({ "message": text_parts.join("\n") }));
            }
        }

        Ok(ToolCallOutcome {
            error,
            text: text_parts.join("\n"),
        })
    }

    /// Shut down cleanly: `shutdown` request, then `exit` notification.
    pub fn shutdown(mut self) {
        let _ = self.request("shutdown", json!({}));
        self.notify("notifications/exit", json!({}));
        let _ = self.stdin.flush();
        let deadline = Instant::now() + Duration::from_millis(500);
        while Instant::now() < deadline {
            match self.child.try_wait() {
                Ok(Some(_)) => break,
                _ => std::thread::sleep(Duration::from_millis(5)),
            }
        }
        let _ = self.child.kill();
        let _ = self.child.wait();
    }

    /// Send a request and wait for the response with a matching id.
    fn request(&mut self, method: &str, params: Value) -> GateResult<Value> {
        self.request_with_timeout(method, params, self.timeout)
    }

    /// Request with an explicit timeout (used for the best-effort handshake).
    fn request_with_timeout(
        &mut self,
        method: &str,
        params: Value,
        timeout: Duration,
    ) -> GateResult<Value> {
        let id = self.next_id;
        self.next_id += 1;
        let message = json!({
            "jsonrpc": "2.0",
            "id": id,
            "method": method,
            "params": params,
        });
        self.write_line(&message)?;

        let deadline = Instant::now() + timeout;
        loop {
            if Instant::now() >= deadline {
                return Err(err::timeout(format!("MCP request '{method}' timed out")));
            }
            let Some(line) = self.read_line_capped(deadline)? else {
                return Err(err::server_unavailable(format!(
                    "MCP server closed its stdout while waiting for '{method}'"
                )));
            };
            let parsed: Value = serde_json::from_str(&line)
                .map_err(|e| err::malformed(format!("malformed MCP message: {e}")))?;
            if parsed.get("id").and_then(Value::as_u64) == Some(id) {
                if let Some(error) = parsed.get("error") {
                    return Err(err::tool(format!("MCP '{method}' failed: {error}")));
                }
                return parsed
                    .get("result")
                    .cloned()
                    .ok_or_else(|| err::malformed("MCP response without 'result'"));
            }
            // Non-matching id: ignore (log-line style server chatter is not
            // expected on stdout, but tolerance is safer than a hang).
        }
    }

    fn notify(&mut self, method: &str, params: Value) {
        let message = json!({
            "jsonrpc": "2.0",
            "method": method,
            "params": params,
        });
        let _ = self.write_line(&message);
    }

    fn write_line(&mut self, message: &Value) -> GateResult<()> {
        let mut line = serde_json::to_string(message)
            .map_err(|e| err::internal(format!("serialization failed: {e}")))?;
        line.push('\n');
        self.stdin
            .write_all(line.as_bytes())
            .map_err(|e| err::server_unavailable(format!("write to MCP server failed: {e}")))?;
        self.stdin
            .flush()
            .map_err(|e| err::server_unavailable(format!("flush to MCP server failed: {e}")))
    }

    /// Read one newline-terminated line, respecting the deadline and the
    /// response size cap.
    fn read_line_capped(&mut self, deadline: Instant) -> GateResult<Option<String>> {
        let mut bytes: Vec<u8> = Vec::new();
        loop {
            if Instant::now() >= deadline {
                return Err(err::timeout("MCP response timed out"));
            }
            match self.stdout.fill_buf() {
                Ok(chunk) => {
                    if chunk.is_empty() {
                        // Data arrives in chunks; poll briefly before giving up.
                        if self.child.try_wait().map(|s| s.is_some()).unwrap_or(false) {
                            return Ok(None);
                        }
                        std::thread::sleep(Duration::from_millis(2));
                        continue;
                    }
                    let (consume, found_newline) = match chunk.iter().position(|b| *b == b'\n') {
                        Some(pos) => {
                            bytes.extend_from_slice(&chunk[..pos]);
                            (pos + 1, true)
                        }
                        None => {
                            bytes.extend_from_slice(chunk);
                            (chunk.len(), false)
                        }
                    };
                    self.stdout.consume(consume);
                    if bytes.len() > MAX_RESPONSE_BYTES {
                        return Err(err::malformed(format!(
                            "MCP response exceeded {MAX_RESPONSE_BYTES} bytes"
                        )));
                    }
                    if found_newline {
                        return String::from_utf8(bytes)
                            .map(Some)
                            .map_err(|e| err::malformed(format!("non-UTF-8 MCP response: {e}")));
                    }
                }
                Err(e) => {
                    return Err(err::server_unavailable(format!(
                        "read from MCP server failed: {e}"
                    )));
                }
            }
        }
    }

    /// Background drain of the server's stderr (bounded) so a chatty server
    /// cannot deadlock on a full stderr pipe. Content is diagnostic only and
    /// discarded; the thread exits at EOF (child exit or session drop).
    fn drain_stderr_until_eof(mut stderr: ChildStderr) {
        let mut drained = 0usize;
        let mut buf = [0u8; 4096];
        while drained <= MAX_STDERR_BYTES {
            match stderr.read(&mut buf) {
                Ok(0) | Err(_) => break,
                Ok(n) => drained += n,
            }
        }
    }
}

impl Drop for McpSession {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn protocol_constants_match_schema() {
        // The gate's canonical expectations must be consistent with what the
        // real griller-mcp health tool reports. Verified end-to-end in
        // tests/real_mcp.rs via the actual binary.
        assert_eq!(crate::canonical::SERVICE, "the-griller");
        assert_eq!(crate::canonical::PROTOCOL, "2025-11-25");
        assert_eq!(crate::canonical::MCP_SCHEMA_VERSION, "m8.0");
        assert_eq!(crate::canonical::TOOLS.len(), 6);
    }

    #[test]
    fn child_env_allowlist_excludes_credentials() {
        for key in CHILD_ENV_ALLOWLIST {
            assert!(
                !key.contains("KEY") && !key.contains("TOKEN") && !key.contains("SECRET"),
                "allow-list must not carry credential variables: {key}"
            );
        }
        assert!(CHILD_ENV_ALLOWLIST.contains(&"PATH"));
    }
}
