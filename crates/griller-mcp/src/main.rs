//! `griller-mcp` binary: the MCP stdio server entry point.
//!
//! Stdout is reserved for the MCP protocol. Diagnostics go to stderr.

use griller_mcp::run_server;

fn main() {
    let rt = tokio::runtime::Runtime::new().expect("create tokio runtime");
    if let Err(e) = rt.block_on(run_server()) {
        eprintln!("griller-mcp: {e}");
        std::process::exit(3);
    }
}
