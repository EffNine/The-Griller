# OpenCode integration

This guide is the exact workflow that was verified against real OpenCode
during M8 field validation. The configuration syntax below is the real thing
— not invented.

## 1. Build the MCP server

```sh
cd the-griller
cargo build --release -p griller-mcp
```

## 2. Locate the binary

The build produces:

```sh
target/release/griller-mcp
```

From the repository root that is an absolute path like
`/home/you/the-griller/target/release/griller-mcp`. You will need the
absolute path for the next step. **Replace the example path in this guide
with your actual release binary path** — nothing here assumes a specific
filesystem layout.

## 3. Register the server in OpenCode

OpenCode reads `opencode.jsonc` (in your project or `~/.config/opencode/`).
Add an MCP entry under the `mcp` key:

```jsonc
{
  // ... your existing config ...

  "mcp": {
    "griller": {
      "type": "local",
      "command": [
        "/home/you/the-griller/target/release/griller-mcp"
      ],
      "enabled": true
    }
  }
}
```

This is the configuration shape that was verified in the field — `type:
"local"`, a command array whose first element is the absolute path to the
`griller-mcp` binary, and `enabled: true`.

## 4. Verify discovery

Restart OpenCode (or reload the config). In an OpenCode session, the
Griller's six tools should appear:

```text
griller.health
griller.verify_claim
griller.check_context
griller.run_attacks
griller.check_completion
griller.self_grill
```

Ask OpenCode to list MCP tools, or check the MCP status panel. Exactly six
tools will be present — the server is deliberately small.

## 5. Call health

Ask OpenCode (or any MCP client) to call:

```text
griller.health
```

You should get JSON like:

```jsonc
{
  "service": "the-griller",
  "version": "0.1.0",
  "protocol": "2025-11-25",
  "schema_version": "m8.0",
  "tools": [ /* the six names */ ],
  "execution": {
    "shell_free": true,
    "worktree_isolation": true,
    "network_isolation": false,
    "os_sandbox": false
  },
  "security": {
    "secret_redaction": "best_effort",
    "network_isolation": "not_enforced",
    "os_sandbox": "not_enforced",
    "privilege_reduction": "not_enforced"
  }
}
```

The report is the honesty contract: the server states exactly what it
enforces and what it does not.

## 6. Call check_completion

The flagship call takes a repository path and the agent's completion claim:

```text
griller.check_completion(
  repo: "/absolute/path/to/repo",
  claim: "Implemented the fact store. All tests pass."
)
```

The response leads with the verdict. For a wrong-project claim against a
CodeBro repository, the real field-validated response was:

```jsonc
{
  "schema_version": "m8.0",
  "verdict": "block",
  "reason_code": "MISMATCH_CONTEXT_BLOCK",
  "required_action": "confirm_project",
  "required_action_message": "Stop before modifying the repository and confirm the intended project/repository.",
  "context": {
    "status": "mismatch",
    "reason": "The task explicitly names another project with no supporting presence here."
  },
  "assessment": { "initial": null, "final": null },
  "attacks": { "planned": [], "results": [] },
  "self_grill": { "ran": false, "skipped_reason": "context mismatch blocked before adjudication; …" }
}
```

In a working OpenCode workflow this looks like: the agent claims completion →
OpenCode calls `griller.check_completion` → the Griller replays the
verification in an isolated worktree and returns PASS/WARN/BLOCK → the agent
reports the verdict instead of (or alongside) its own claim.

## Troubleshooting

### MCP not discovered

- Confirm the config file was saved and OpenCode restarted.
- Confirm the `mcp` key exists and the entry is inside it, not outside.
- Confirm `enabled: true` is set.
- Try a minimal config with only the griller entry to isolate a syntax error
  elsewhere in the file.

### Server exits immediately

The server is a long-running stdio process — if it exits, OpenCode reports
the server failed to start. Common causes:

- The binary path is wrong. Verify: `ls -la <your-path>/griller-mcp` and run
  `<your-path>/griller-mcp` with nothing on stdin — it should sit and wait,
  not exit.
- The binary was not built: `cargo build --release -p griller-mcp` first.

### Tool unavailable

- Call `griller.health` first; if it works, the server is alive.
- The tool surface is exactly six names — if a tool name was mistyped
  (`griller.checkCompletion` instead of `griller.check_completion`), the
  call is rejected. Use the names from `griller.health`.

### Replay cannot find cargo

Replay runs the derived verification command in an isolated worktree with a
**controlled environment**: only an explicit allow-list of keys (`PATH`,
`HOME`, `LANG`, `TMPDIR`, `USER`) is passed to the child. If `cargo` is not
on `PATH` for the child, replay fails to spawn — which the report records
honestly as `EXECUTION_FAILED` (never as a refutation). Ensure `cargo` is
installed and on `PATH`.

### Permission issue

The server runs with your user's privileges (there is no privilege
reduction — see [security.md](security.md)). If the repository path is not
readable by your user, evidence collection fails with a structured error.
The server itself never needs write access to your working tree: verification
runs in disposable worktrees, and context checks are read-only.

### Something smells wrong

`griller.health` reports the security boundaries. If a client claims the
Griller sandboxes execution or blocks network access, the client is lying —
the health report is the source of truth.

## Notes from the field

- The M8 field validation ran this exact sequence: build → register → real
  OpenCode discovery → `griller.health` → `griller.check_completion` against
  real fixtures. It also caught a worktree-pollution bug during field
  validation that was fixed before the final run — which is why validating
  outside unit tests matters.
- The binary path in the config example is *your* path. The docs deliberately
  do not hardcode a project-specific filesystem layout.