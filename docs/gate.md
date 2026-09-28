# The Griller automatic completion gate

The gate is the integration boundary between OpenCode (or any host agent) and the Griller's verification pipeline. It runs one-shot per completion event — there is no daemon, no polling, and the gate never emits a completion event (fail-closed).

## One-shot per boundary

When the host detects a completion boundary (the agent reached a done/finish state), it constructs a [`CompletionEvent`] and invokes `griller-gate check --config <path>` via the OpenCode plugin's `session.idle` hook. The gate connects to the `griller-mcp` server, performs a stale-server identity check, and invokes `griller.check_completion {repo, claim, timeout}`. The verdict of that pipeline becomes the gate's decision. The gate **never** re-implements verification.

## CLI usage

```sh
griller-gate check --config /path/to/gate.json
```

Stdin: a JSON [`CompletionEvent`]. Stdout: a [`GateDecision`] JSON. Exit codes: `0` (decision produced, including `Unavailable`), `2` (configuration error), `3` (gate could not produce a decision).

The gate binary reads a JSON config file (`GateConfig`) that specifies:

- **`mcp_binary`**: absolute path to the `griller-mcp` binary to spawn
- **`expected_server`**: server identity (`service`, `protocol`, `schema_version`, `version`, `tools`) for stale-server protection
- **`state_dir`**: directory for lifecycle logs and decision records
- **`timeout_ms`**: per-request timeout applied to MCP round-trips (default 120s)
- **`fail_closed`**: when true, integration failures produce `Unavailable` instead of silently accepting
- **`warn_not_pass`**: when true, a `WARN` verdict from Griller is not treated as PASS by the gate

## Decision schema

The gate produces a [`GateDecision`] with these fields (schema version `g1.0`):

| Field | Meaning |
|---|---|
| `schema_version` | `"g1.0"` (gate's own schema; distinct from MCP `m8.0`) |
| `status` | `Accepted` / `Warned` / `Blocked` / `Unavailable` |
| `verdict` | `None` / `Some(Griller verdict)` |
| `reason_code` | `None` / `Some(String)` (e.g. `EVIDENCE_MATCHED`, `STALE_SERVER`, `UNVERIFIABLE`) |
| `findings` | array of `{ key, detail }` from Griller |
| `required_action` | `None` / `Some(String)` (e.g. `confirm_project`) |
| `required_action_message` | `None` / `Some(String)` (human-readable next step) |
| `event` | echoed [`EventSummary`] from the event |
| `integration` | `IntegrationInfo` describing the gate's own integration status |
| `griller_report` | optional full `Report` from the `check_completion` call |
| `timing` | `GateTiming { connect_ms, check_ms, total_ms }` |
| `lifecycle` | `LifecycleLog` stages traversed |

## Stale-server protection

Before invoking any tool, the gate checks the MCP server's identity against canonical constants (`service: "the-griller"`, `protocol: "2025-11-25"`, `schema_version: "m8.0"`, tools: six names). A mismatch yields `STALE_SERVER` → `Unavailable` fail-closed. The authoritative identity check is `griller.health`.

## Fail-closed defaults

- **NoCompletionAssertion**: the host observed a completion boundary but extracted no completion assertion, so there was nothing to verify. Neither a refusal nor a failure. Never converted to PASS, and **never surfaced to the agent** — a non-completion message must not produce a notice. The decision is still persisted for audit.
- **Unavailable**: gate could not run (uninitialized config, MCP server unavailable, stale server, timeout). Never silently PASS. Default when no config can be loaded or the MCP binary cannot be started. Unlike `NoCompletionAssertion`, this **does** carry a `required_action_message` and does notify the agent.
- **Warned**: Griller returned WARN verdict. The gate maps to `GateStatus::Warned`; the completion is **not** accepted as verified unless `warn_not_pass` is false in config. The gate injects a `noReply` user message so the agent learns the completion was not accepted.
- **Blocked**: Griller returned BLOCK verdict. Completion is explicitly rejected.
- **Accepted**: Griller returned PASS verdict. Completion is accepted.

The fail-closed default is deliberate: when in doubt, the gate refuses to verify (Unavailable). This prevents a stale or misconfigured MCP server from silently converting an unverified completion into PASS.

## Integration with OpenCode

### Plugin: `.opencode/plugins/griller-gate.ts`

The OpenCode adapter (`opencode/plugins/griller-gate.ts`) listens for `session.idle` events. When the agent produced a new assistant completion since the last gate check, it builds a `CompletionEvent` from the message text and runs `griller-gate check --config <path>`. Non-accepted verdicts are injected back into the session as a `noReply` user message so the agent learns its completion was not accepted as verified.

Key design decisions:

- **Per-session dedupe**: each assistant message is verified at most once per session. A `Map<string, string>` tracks the last checked assistant message ID; on idle the plugin skips messages already checked.
- **In-flight guard**: a per-session boolean prevents concurrent gate runs for the same session.
- **Loop prevention**: the injected message is a user message (never a new assistant completion), so the next `session.idle` event sees the same last assistant ID and the dedupe skips it. The gate also never emits a completion event — it only produces a decision JSON on stdout, which the plugin reads and may inject a prompt.
- **Completion claim extraction**: the gate extracts a concise completion claim from the assistant message before sending it to Griller. This prevents long evidence reports, markdown tables, line numbers, and cross-project references from polluting the Griller input. See [Claim extraction](#claim-extraction) below.
- **Fail-closed surface**: if `griller-gate` is missing, misconfigured, or exits non-zero, the plugin injects a one-time notice that verification is unavailable (fail-closed).

#### Claim extraction

The gate uses a deterministic three-path extraction algorithm (no LLM) to produce the claim sent to Griller:

1. **Explicit marker**: if the message contains `### Completion Claim`, the next non-empty line(s) after the marker become the claim.
2. **Heuristic tail extraction**: the last 3 paragraphs are scanned in reverse for a sentence that *asserts* a completion (see below). The first matching sentence from the tail is used.
3. **Fallback**: the first sentence matching a completion assertion is used.

**Completion assertion predicate**: a sentence counts only if it declares an outcome — a past-tense accomplishment (`implemented`, `fixed`, `added`, `resolved`, `verified`, …), an explicit test/build outcome (`all tests pass`, `490 tests passed`, `tests (511 passed)`, `the build succeeds`), a declarative state (`is done`, `is complete`, `everything works`), a conjunctive exit-0 phrasing, a terse standalone assertion (`Done.`, `All done.`, `Ready.`, `Complete.`, `That's done.`, `It's ready.`, `Shipped.`, `Closed.`), or a counted outcome with no noun (`All 511 pass.`). Bare keywords appearing in prose (`test`, `build`, `run`, `check`, `claim`) never match: planning prose, task directives ("Implement X."), and verification *discussion* are not completion events and return no claim. A bare `ready` likewise does not match inside prose, but a sentence that is *only* an outcome word is a completion assertion.

If no claim can be extracted, the message is not a completion event. The plugin still reports
it to the gate with `extraction: "no_assertion"`, and the gate records a
`no_completion_assertion` decision (`reason_code: NO_COMPLETION_ASSERTION`) through the
normal persistence path. **The agent is not notified.** Fail-closed behaviour applies only
when a gate run itself fails (binary missing, non-zero exit, unusable output).

The distinction is deliberate: `no_completion_assertion` means the gate ran and there was
nothing to verify, while `unavailable` means something went wrong. Reusing `unavailable`
would restore the old behaviour of injecting a notice onto every technical message, which
is exactly what this separation avoids. The record carries `verdict: null`,
`griller_report: null`, and no `required_action_message`, because no claim reached the
Griller, no evidence was collected, and no epistemic status was assigned — this is a
pre-adjudication fact, not a verdict.

Hard bounds:
- Maximum extracted claim length: 300 characters.
- Empty extraction → skip silently (not a completion event).
- Markdown formatting (bold, italics, inline code) is stripped.
- Markdown tables and fenced code blocks are stripped before heuristic scanning.
- Prompt injection patterns (`ignore.*griller`, `disregard.*gate`, `bypass.*check`, `report pass/approved`) in heuristic matches are rejected.
- The original full message is preserved in `_diagnostic` for debugging but is never sent to Griller as evidence.

This ensures that a 4,000-character evidence reconciliation report containing words like "CodeBro", "Contradiction", line numbers, and markdown tables produces a concise claim such as "Implemented the feature across all modules. All tests pass." rather than the entire transcript.

### Config in OpenCode

The plugin discovers config from `GRILLER_GATE_CONFIG` env or `$HOME/.config/griller/gate.json`. The config must have `mcp_binary` set to the `griller-mcp` binary path. If absent, the plugin injects a once-per-session notice that the gate is unavailable.

### Lifecycle lines on stderr

The gate emits these lines to the host's stderr during a run:

```
griller-gate: COMPLETION_EVENT repo=<repo> session=<session>
griller-gate: <verdict> (<total>ms)
griller-gate: lifecycle: <stage1> -> <stage2> -> ...
```

These are cosmetic; the plugin does not need to parse them (it reads stdout JSON).

## Mismatch vs. other block reasons

| Condition | Gate action |
|---|---|
| Repo name mismatch (event's `repository` ≠ assistant's `path.root`) | `MISMATCH_CONTEXT_BLOCK` → gate maps to `Blocked`, injects `noReply` with `confirm_project` |
| MCP server identity stale (`service/protocol/schema_version/tools` differ from canonical) | `STALE_SERVER` → `Unavailable`, fail-closed |
| MCP server health check fails | `HealthCheckFailed` → `Unavailable` |
| `griller.check_completion` verdict PASS | `Accepted`, no injection |
| `griller.check_completion` verdict WARN | `Warned`, inject `noReply` |
| `griller.check_completion` verdict BLOCK | `Blocked`, inject `noReply` |
| Gate binary missing / config missing / spawn failure | `Unavailable`, inject one-time notice that verification is unavailable |
| Config `fail_closed=false` with unavailable server | Still `Unavailable` (gate-enforced, not configurable away) |
| Config `warn_not_pass=false` with WARN verdict | Completion IS accepted as verified; no injection |

## Troubleshooting

### Gate binary not found

- Ensure `GRILLER_GATE_BIN` env points to an executable `griller-gate`, OR
- Install `griller-gate` to `$HOME/.local/bin/`, OR
- Build: `cargo build --release -p griller-gate` and place the binary on `PATH`.

### Config missing / invalid

- Ensure `GRILLER_GATE_CONFIG` points to a valid JSON config, OR
- Place config at `$HOME/.config/griller/gate.json` with `mcp_binary` set.

### No tools appear in OpenCode after plugin reload

- Restart OpenCode (or reload config via the command palette). The plugin is auto-discovered from `.opencode/plugins/griller-gate.ts`; verify the file is present and `node --version` ≥ 20.

### Agent says completion was accepted but gate says Warned/Blocked

- The gate's `warn_not_pass` defaults to true. Set `warn_not_pass: false` in config to accept WARN as PASS. Or investigate why Griller returned WARN (e.g. incomplete evidence).

### Agent never sees the injection message

- Ensure `noReply: true` is set (it is, in the plugin). The injected message appears in the conversation as a user message; the agent may ignore it if not attending to all messages. The gate also logs the injection via `client.app.log` at `warn` level.

### Integration with the six MCP tools

The gate uses the same `griller-mcp` server as the six tools (`griller.verify_claim`, `griller.check_completion`, `griller.check_context`, `griller.run_attacks`, `griller.self_grill`, `griller.health`). The gate does NOT add or remove tools — it delegates to the existing pipeline.

## Version

The gate workspace version is `0.2.0`. The MCP schema stays `m8.0`; the gate decision schema is `g1.0`.

## Resume flow behavior

The gate reacts to `session.idle` events fired by OpenCode when the agent
reaches a natural pause (e.g. after producing an assistant message). The
current behavior is:

1. `session.idle` fires automatically when the agent finishes its turn.
2. The gate extracts the completion claim from the latest assistant message.
3. If no claim can be extracted, the message was ordinary discussion (not a
   completion event) — the gate skips it silently.
4. If extraction succeeds, the gate runs `griller-gate check` and receives a
   verdict.
5. If the verdict is `Accepted`, nothing further happens — the session
   remains idle and the agent may continue.
6. If the verdict is `Blocked`, `Warned`, or `Unavailable`, the gate injects
   a `noReply` user message into the session with the verdict details.
7. The injected `noReply` message **does not** cause OpenCode to resume the
   agent automatically. It appears as a user message in the conversation.
   The agent must explicitly acknowledge or respond to it on its next turn.

**Limitation**: the gate currently only *reports* blocks and warnings; it
does not trigger automatic agent resumption. A BLOCK results in a user
message in the chat, and the agent decides whether to act on it. Lifecycle
redesign (automatic continuation after block resolution) is a separate task.