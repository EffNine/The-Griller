# MCP server

The Griller exposes itself to coding agents over the Model Context Protocol
(MCP) through the `griller-mcp` binary — **an adapter, not the core**. Every
tool handler translates a request into existing Griller APIs and returns
structured JSON. No business logic lives in the MCP crate beyond the
orchestration of the `check_completion` pipeline.

## Transport: STDIO

The server speaks MCP over standard input/output: JSON-RPC messages,
newline-delimited, one per line. Stdout is reserved for the protocol —
diagnostics go to stderr. This is the configuration OpenCode and other MCP
clients use to launch the server as a local subprocess.

## Exactly six tools

The tool surface is deliberate and tested: a `tools/list` request returns
exactly these six names, no hidden seventh tool.

| Tool | Purpose |
| --- | --- |
| `griller.health` | Server identity, schema version, capabilities, security boundaries |
| `griller.verify_claim` | Adjudicate a single claim against repository evidence |
| `griller.check_context` | M4: does this task belong to this repository? |
| `griller.run_attacks` | Run applicable safe adversarial checks against a claim |
| `griller.check_completion` | The full M4→M2→M5→M2→M3→M7 pipeline |
| `griller.self_grill` | M7 audit of a decision produced elsewhere |

`griller.check_completion` is the flagship. The others exist for callers that
need a single stage.

## Tool reference

### griller.health

- **Purpose:** capability discovery. Tells the caller what the server can do
  and — just as important — what it cannot.
- **Inputs:** none.
- **Outputs:** `service`, `version`, `protocol`, `schema_version` ("m8.0"),
  the six tool names, `capabilities`, `execution` (`shell_free`,
  `worktree_isolation`, `network_isolation: false`, `os_sandbox: false`), and
  `security` (`secret_redaction: "best_effort"`, `network_isolation:
  "not_enforced"`, `os_sandbox: "not_enforced"`, `privilege_reduction:
  "not_enforced"`).
- **Security consideration:** the health report is the honesty contract —
  it states exactly what is enforced and what is not. Read it before trusting
  the server.
- **Typical use:** first call after registration.

### griller.verify_claim

- **Purpose:** independently adjudicate one claim against repository evidence.
  Agent narration is never treated as verification.
- **Inputs:** `repo` (path), `claim` (text), optional `claim_kind`
  (`completion` | `fact` | `causal` | `requirement` | `design`), `target`,
  `severity`, `evidence_mode` (accepted; current collection is repository
  state + snapshot).
- **Outputs:** the M2 `ClaimAssessment` (status, evidence references),
  derived severity, verdict, reason code, required action.
- **Security consideration:** read-only. No commands are executed.
- **Typical use:** quick fact checks, e.g. *"does this file exist?"* or
  *"is there evidence for this claim?"*

### griller.check_context

- **Purpose:** the wrong-project detector — decide whether a task belongs to
  the given repository.
- **Inputs:** `repo` (path), `task` (text).
- **Outputs:** `task_identity`, `project_identity`, `context_status`,
  bounded positive/negative evidence, severity, verdict, reason code,
  required action (`confirm_project` on mismatch).
- **Security consideration:** read-only; runs deterministic collectors only.
  A task containing injection text is data — it cannot alter the assessment
  beyond the entities it names.
- **Typical use:** gate before high-confidence repository modification.

### griller.run_attacks

- **Purpose:** run the applicable safe adversarial checks against a claim.
- **Inputs:** `repo`, `claim`, optional `claim_kind`, `target`,
  optional `attack_kinds` (validated strictly: unknown kinds are an
  `INVALID_REQUEST`, never silently dropped), optional `timeout` (ms,
  capped).
- **Outputs:** planned attack specs, per-attack results with evidence
  references, coverage summary, execution errors.
- **Security consideration:** replay commands are derived by the Griller from
  the claim — never from the caller — and execute in a disposable worktree
  with an allow-listed environment and network disabled. The four
  limited/foundation kinds are recognized but never planned (see
  [red-team.md](red-team.md)).
- **Typical use:** attack a specific claim when you only want the M5 stage.

### griller.check_completion

- **Purpose:** the full completion check — one call, one auditable report.
- **Inputs:** `repo`, `claim`, optional `claim_kind`, optional `timeout`.
- **Outputs:** `schema_version`, `verdict` (PASS/WARN/BLOCK) first, then
  `reason_code`, `required_action`, `request`, `context`, `assessment`
  (initial + final), `attacks` (planned/results/coverage/rejected),
  `self_grill`, `timing`. Bounded: ≤ 10 evidence snippets, ≤ 10 000 char
  claims, < 256 KiB reports.
- **Security consideration:** a context mismatch BLOCKS before any attack
  work; an unsafe claim payload is never turned into a command; replay runs
  in an isolated worktree. A failed verification is always a **verdict**,
  never an error.
- **Typical use:** the final gate after an agent claims completion.

### griller.self_grill

- **Purpose:** M7 audit of the Griller's own decision structure.
- **Inputs:** `claim`, `repo`, optional `claim_kind`, optional
  `evidence_dir`.
- **Outputs:** `original` (immutable decision) + `self_grill`
  (`ran`, `refuted`, per-check `findings`, overall outcome). Never overrides
  the original verdict.
- **Security consideration:** read-only; deterministic single pass.
- **Typical use:** verify that a decision produced by another path is
  structurally consistent.

## Errors

Errors are structured results, never fabricated verdicts:

| kind | code | meaning |
| --- | --- | --- |
| `invalid_request` | INVALID_REQUEST | malformed or missing fields, oversized inputs |
| `repository_not_found` | REPOSITORY_NOT_FOUND | repo path does not exist |
| `not_a_repository` | NOT_A_REPOSITORY | path exists but is not a repository |
| `unsafe_command` | UNSAFE_COMMAND | command violates the allow-list |
| `execution_failed` | EXECUTION_FAILED | backend failure |
| `timeout` | TIMEOUT | a bound was exceeded |
| `unsupported_operation` | UNSUPPORTED_OPERATION | operation not supported by this tool |
| `internal_error` | INTERNAL_ERROR | unexpected internal failure |

A system error is **never** reported as a verdict: "the Griller crashed" must
not be readable as "the claim is false."

## Bounds

| bound | value |
| --- | --- |
| claim/task text | ≤ 10 000 chars |
| repo path | ≤ 2 000 chars |
| attack kinds per request | ≤ 20 |
| per-attack timeout | default 30 s, replay budget 600 s, capped 600 s (`MAX_TIMEOUT_MS`) |
| evidence snippets in responses | ≤ 10 |
| report size | < 256 KiB in practice |

## Typed schemas

Each tool's input is a typed struct (`serde` + `JsonSchema`): unknown fields
are rejected, missing required fields fail validation, and each field is
independently bounded at the boundary. The schemas are derived, so `tools/list`
always matches the actual server behaviour — the six-tool surface and the
schemas are asserted by protocol-level tests that launch the real binary.

## MCP is an adapter

The MCP crate adds the orchestration of `check_completion` and nothing else:
handlers validate input, call the pure crates and the runtime, and serialize
results. The decision logic — status, context, policy, attacks, self-grill —
lives in the crates it depends on. If MCP vanished tomorrow, the CLI and the
crate APIs would still be the same Griller.

See [opencode.md](opencode.md) for a working OpenCode integration, and
[history/m8-integration.md](history/m8-integration.md) for the milestone
report.