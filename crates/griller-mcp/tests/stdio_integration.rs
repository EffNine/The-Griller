//! M8 end-to-end MCP stdio integration test.
//!
//! Launches the real `griller-mcp` server binary as a subprocess, performs a
//! real stdio MCP handshake (initialize), lists tools, calls tools with real
//! protocol messages, validates the structured responses, and shuts down
//! cleanly.
//!
//! This is a protocol-level test: it never calls Rust handler functions
//! directly. It uses the SDK's client runtime as the minimal test client.

mod common;

use std::time::Duration;

use async_trait::async_trait;
use rust_mcp_sdk::mcp_client::ClientHandler;
use rust_mcp_sdk::schema::{
    CallToolRequestParams, ClientCapabilities, Implementation, InitializeRequestParams,
    LATEST_PROTOCOL_VERSION,
};
use rust_mcp_sdk::{
    mcp_client::{client_runtime, McpClientOptions},
    McpClient, StdioTransport, ToMcpClientHandler, TransportOptions,
};

use common::fixture_git_repo;

struct TestClientHandler;

#[async_trait]
impl ClientHandler for TestClientHandler {}

fn client_params() -> InitializeRequestParams {
    InitializeRequestParams {
        capabilities: ClientCapabilities::default(),
        client_info: Implementation {
            name: "griller-mcp-integration-test".into(),
            version: "0.0.1".into(),
            title: Some("Griller MCP integration test client".into()),
            description: None,
            icons: vec![],
            website_url: None,
        },
        protocol_version: LATEST_PROTOCOL_VERSION.into(),
        meta: None,
    }
}

fn call(name: &str, arguments: serde_json::Value) -> CallToolRequestParams {
    CallToolRequestParams {
        name: name.to_string(),
        arguments: arguments
            .as_object()
            .cloned()
            .or_else(|| (arguments.is_null()).then(serde_json::Map::new)),
        meta: None,
        task: None,
    }
}

fn tool_text(result: &rust_mcp_sdk::schema::CallToolResult) -> String {
    result.content[0].as_text_content().unwrap().text.clone()
}

#[tokio::test]
async fn stdio_end_to_end() {
    let bin = env!("CARGO_BIN_EXE_griller-mcp");

    // 1. Launch the server binary over stdio.
    let transport = StdioTransport::create_with_server_launch(
        bin,
        vec![],
        None,
        TransportOptions {
            timeout: Duration::from_secs(120),
            ..Default::default()
        },
    )
    .expect("launch griller-mcp server");

    // 2. Establish the client and perform initialization.
    let client = client_runtime::create_client(McpClientOptions {
        client_details: client_params(),
        transport,
        handler: TestClientHandler.to_mcp_client_handler(),
        task_store: None,
        server_task_store: None,
        message_observer: None,
    });
    client
        .clone()
        .start()
        .await
        .expect("client start + initialize");

    // 3. List tools: exactly six, canonical names, no hidden seventh tool.
    let listed = client.request_tool_list(None).await.expect("tools/list");
    let names: Vec<&str> = listed.tools.iter().map(|t| t.name.as_str()).collect();
    assert_eq!(
        names.len(),
        6,
        "exactly six tools must be exposed, got: {names:?}"
    );
    for expected in [
        "griller.health",
        "griller.verify_claim",
        "griller.check_context",
        "griller.run_attacks",
        "griller.check_completion",
        "griller.self_grill",
    ] {
        assert!(names.contains(&expected), "missing tool {expected}");
    }

    // 4. Call health.
    let health = client
        .request_tool_call(call("griller.health", serde_json::json!({})))
        .await
        .expect("griller.health");
    assert!(!health.is_error.unwrap_or(false));
    let health_json: serde_json::Value =
        serde_json::from_str(&tool_text(&health)).expect("health JSON");
    assert_eq!(health_json["service"], "the-griller");
    assert_eq!(health_json["schema_version"], "m8.0");
    assert_eq!(health_json["tools"].as_array().unwrap().len(), 6);
    assert_eq!(health_json["security"]["network_isolation"], "not_enforced");
    assert_eq!(health_json["security"]["os_sandbox"], "not_enforced");

    // 5. Call check_context with the wrong-project scenario.
    let repo = fixture_git_repo("codebro");
    let repo = repo.path().to_path_buf();
    let ctx = client
        .request_tool_call(call(
            "griller.check_context",
            serde_json::json!({
                "repo": repo,
                "task": "Implement Atlas P10.5.1 Fact Store."
            }),
        ))
        .await
        .expect("griller.check_context");
    assert!(!ctx.is_error.unwrap_or(false));
    let ctx_json: serde_json::Value = serde_json::from_str(&tool_text(&ctx)).expect("ctx JSON");
    assert_eq!(ctx_json["context_status"], "mismatch");
    assert_eq!(ctx_json["verdict"], "block");

    // 6. Call check_completion on the wrong project: BLOCK, no attacks run.
    let done = client
        .request_tool_call(call(
            "griller.check_completion",
            serde_json::json!({
                "repo": repo,
                "claim": "Implement Atlas P10.5.1 Fact Store. All tests pass."
            }),
        ))
        .await
        .expect("griller.check_completion");
    assert!(!done.is_error.unwrap_or(false));
    let done_json: serde_json::Value = serde_json::from_str(&tool_text(&done)).expect("done JSON");
    assert_eq!(done_json["schema_version"], "m8.0");
    assert_eq!(done_json["verdict"], "block");
    assert_eq!(done_json["context"]["status"], "mismatch");
    assert_eq!(
        done_json["attacks"]["results"].as_array().unwrap().len(),
        0,
        "no attacks may run after a context mismatch"
    );
    assert_eq!(done_json["self_grill"]["ran"], false);
    assert!(done_json["self_grill"]["skipped_reason"]
        .as_str()
        .unwrap()
        .contains("context mismatch"));
    // BLOCK must carry actionable remediation.
    assert!(!done_json["required_action_message"]
        .as_str()
        .unwrap()
        .is_empty());

    // 7. Invalid request: unknown tool must be rejected, either as a
    //    protocol error or as a structured is_error result — never as a
    //    fabricated verdict.
    let bogus = client
        .request_tool_call(call("griller.does_not_exist", serde_json::json!({})))
        .await;
    match bogus {
        Err(_) => {}
        Ok(r) => assert!(r.is_error.unwrap_or(false), "unknown tool must be rejected"),
    }

    // 8. Terminate cleanly.
    client.shut_down().await.expect("clean shutdown");
}

#[tokio::test]
async fn stdio_rejects_invalid_inputs() {
    let bin = env!("CARGO_BIN_EXE_griller-mcp");
    let transport = StdioTransport::create_with_server_launch(
        bin,
        vec![],
        None,
        TransportOptions {
            timeout: Duration::from_secs(60),
            ..Default::default()
        },
    )
    .expect("launch griller-mcp server");

    let client = client_runtime::create_client(McpClientOptions {
        client_details: client_params(),
        transport,
        handler: TestClientHandler.to_mcp_client_handler(),
        task_store: None,
        server_task_store: None,
        message_observer: None,
    });
    client.clone().start().await.expect("client start");

    // Missing required arguments -> rejected (protocol error or structured
    // is_error result; never a verdict).
    let missing = client
        .request_tool_call(call("griller.check_context", serde_json::json!({})))
        .await;
    match missing {
        Err(_) => {}
        Ok(r) => {
            assert!(r.is_error.unwrap_or(false), "missing args must be rejected");
        }
    }

    // Nonexistent repo -> structured error result with REPOSITORY_NOT_FOUND.
    let not_found = client
        .request_tool_call(call(
            "griller.check_context",
            serde_json::json!({
                "repo": "/nonexistent/griller-path-xyz",
                "task": "anything"
            }),
        ))
        .await
        .expect("check_context on missing repo");
    assert!(not_found.is_error.unwrap_or(false));
    let err_json: serde_json::Value =
        serde_json::from_str(&tool_text(&not_found)).expect("error JSON");
    assert_eq!(err_json["kind"], "repository_not_found");

    // Empty claim -> INVALID_REQUEST.
    let empty = client
        .request_tool_call(call(
            "griller.check_context",
            serde_json::json!({ "repo": "/tmp", "task": "   " }),
        ))
        .await
        .expect("check_context empty task");
    assert!(empty.is_error.unwrap_or(false));
    let err_json: serde_json::Value = serde_json::from_str(&tool_text(&empty)).expect("error JSON");
    assert_eq!(err_json["kind"], "invalid_request");

    // Unsupported attack kind -> INVALID_REQUEST (never silently dropped).
    let bad_attack = client
        .request_tool_call(call(
            "griller.run_attacks",
            serde_json::json!({
                "repo": "/tmp",
                "claim": "tests pass",
                "attack_kinds": ["replay", "mind_read"]
            }),
        ))
        .await
        .expect("run_attacks with bad kind");
    assert!(bad_attack.is_error.unwrap_or(false));
    let err_json: serde_json::Value =
        serde_json::from_str(&tool_text(&bad_attack)).expect("error JSON");
    assert_eq!(err_json["kind"], "invalid_request");
    assert!(err_json["message"].as_str().unwrap().contains("mind_read"));

    client.shut_down().await.expect("clean shutdown");
}
