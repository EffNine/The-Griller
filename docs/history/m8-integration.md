> **Historical milestone report (M8).** See [docs/verification-pipeline.md](../verification-pipeline.md) and [docs/mcp.md](../mcp.md) for the current canonical treatment of the completion pipeline and MCP surface.

# M8: Final Integration — The Complete Completion Check

## What M8 delivers

M8 is the final integration milestone: the MCP server now exposes
`griller.check_completion`, a single call that runs the **entire
verification pipeline** end to end — M4 context, M2 adjudication, M5
adversarial attacks, M2 re-adjudication, M3 policy, M7 self-grill — and
returns one auditable, bounded JSON report.

```text
griller.check_completion(claim, repo)
  -> M4  context assessment (does the task belong to this repo?)
  -> M2  initial adjudication (epistemic status from evidence)
  -> M5  plan + execute applicable safe attacks (worktree-isolated)
  -> M2  re-adjudication with attack evidence
  -> M3  policy decision (PASS / WARN / BLOCK)
  -> M7  self-grill audit of that decision (never overrides it)
  -> CompletionReport (schema_version "m8.0")
```

## The six MCP tools (final)

| Tool | Purpose |
|------|---------|
| `griller.health` | Server identity, schema version, capabilities |
| `griller.verify_claim` | Single-claim M2 adjudication against evidence |
| `griller.check_context` | M4: does this task belong to this repository? |
| `griller.run_attacks` | Run applicable safe attacks against a claim |
| `griller.check_completion` | The full M4→M2→M5→M2→M3→M7 pipeline |
| `griller.self_grill` | M7 audit of a decision produced elsewhere |

`griller.check_completion` is the flagship tool. The others remain for
callers that need a single stage.

## Verdicts and errors

```text
PASS = 0   the claim survived adjudication + attacks + self-grill
WARN = 1   material caveats (uncertain status)
BLOCK = 2  contradiction, context mismatch, or unverifiable claim
SYSTEM ERROR = 3  MCP error result (is_error: true), never a verdict
```

System errors are reported as structured MCP `is_error` results with a
machine-readable `kind` (`invalid_request`, `repository_not_found`,
`not_a_repository`, `unsafe_command`, `execution_failed`, `timeout`,
`unsupported_operation`, `internal_error`). A failed verification is
always a **verdict**, never an error.

## Pipeline behavior (deterministic)

### M4 context short-circuit

A strong context mismatch (`mismatch`) blocks the pipeline **before** any
adjudication or attack work:

- `assessment.initial` and `assessment.final` are `null`
- `attacks.planned` / `attacks.results` are empty
- `self_grill.ran` is `false`, with `skipped_reason` explaining that no
  M2 decision was produced for the audit to check
- `verdict` is `BLOCK` with the M4 required action

A correct context (e.g. `matched`/`likely_match` on the fixture whose
project identity matches the task) is never rejected for unfamiliarity —
the pipeline proceeds to evidence and lets M2/M3 decide.

### Evidence collection

- Project identity (M4) — always.
- Repository state, snapshot, git diff (M2) — git-dependent collectors
  are skipped on non-git repositories instead of failing the request.
- File content — when the normalized claim names a path-like target
  (`src/greet.rs`), that file is read so existence claims resolve with
  direct evidence. A missing file is not an error.

### Attack planning and execution

`plan::applicable_attacks` selects from the four implemented attack kinds
(Replay, PresenceAbsence, StubSweep, Safety):

- The replay command is **derived by the Griller from the claim** and
  never taken from the caller: `test`/`pass`/`fail` → `cargo test`,
  `clippy`/`lint` → `cargo clippy --all-targets --all-features`,
  `fmt`/`format` → `cargo fmt --all -- --check`, `check` →
  `cargo check --workspace`, `build`/`compil` → `cargo build`.
- Replay commands inherit only the allow-listed environment keys
  (`PATH`, `HOME`, …) and run with network disabled; they execute in a
  **worktree** (in-place fallback for non-git repositories). The user's
  working tree is never mutated.
- Unsupported attack kinds in `attack_kinds` are an `invalid_request` —
  never silently dropped. Unknown kinds and oversized lists are rejected.

### Re-adjudication and policy

Replay evidence is fed back into M2 so "tests pass" claims are matched
against the actual replayed run. The final epistemic status drives the
M3 policy table (`Completion` × status), producing the reason codes:

```text
VERIFIED / ADVERSARIALLY_VERIFIED -> PASS
UNSUPPORTED / UNVERIFIABLE        -> BLOCK (make verification available)
CONTRADICTED                      -> BLOCK (fix the contradiction)
UNCERTAIN                         -> WARN
```

Deterministic adjudication produces at most `VERIFIED`;
`ADVERSARIALLY_VERIFIED` requires the optional LLM adjudicator, which is
not enabled in the default server.

### Self-grill

Self-grill runs on the **real pipeline data** via
`SelfGrillInput::from_pipeline(assessment, decision, context, coverage,
evidence, severity, source)`. It is an audit layer:

- it never mutates or overrides the original decision;
- a REFUTED audit adds `self_grill.refuted: true` and a message; the
  original verdict stands;
- when the pipeline is blocked before adjudication (context mismatch),
  self-grill is skipped rather than run on empty input (which would
  produce a spurious REFUTED via PolicyConsistencyCheck).

## Report structure

The decision fields lead the document so a caller can read the verdict
without parsing the whole payload:

```jsonc
{
  "schema_version": "m8.0",
  "verdict": "block",                    // decision first
  "reason_code": "CONTRADICTED_COMPLETION_BLOCK",
  "required_action": "RESOLVE_CONFLICT",
  "required_action_message": "...",
  "request": { "repo": "...", "claim": "...", "claim_kind": "completion" },
  "context":    { "status": "...", "verdict": "...", "reason": "...",
                  "positive_evidence": [...], "negative_evidence": [...] },
  "assessment": { "initial": {...}, "final": {...} },
  "attacks":    { "planned": [...], "results": [...], "coverage": {...},
                  "rejected": [...] },
  "self_grill": { "ran": true, "refuted": false, "message": null,
                  "original": {...}, "result": {...} },
  "timing":     { "context_ms": ..., "identity_ms": ..., "evidence_ms": ...,
                  "adjudication_ms": ..., "attacks_ms": ...,
                  "self_grill_ms": ..., "total_ms": ..., "evidence_count": ... }
}
```

Output is bounded: evidence lists are truncated to
`MAX_EVIDENCE_SNIPPETS` (10), claims are capped at `MAX_TEXT_LEN`
(10 000 chars), and a report stays well under 256 KiB in practice.

## Field tests (fixtures)

`crates/griller-mcp/tests/completion_flow.rs` runs the full pipeline
against fixture repositories (copied to temporary git repositories):

| Scenario | Fixture | Expectation |
|----------|---------|-------------|
| Wrong project | `codebro` + "Implement Atlas P10.5.1 Fact Store." | BLOCK via context mismatch, zero attacks |
| Correct project | `atlas` + "Implement Atlas P10.5.1 Fact Store." | context not BLOCK; evidence-driven verdict |
| Lying agent | `lying_rust` + "All tests pass." | replay refutes → BLOCK |
| Narrow test suite | `narrow_rust` + "All tests pass." | full-suite replay refutes → BLOCK |
| Prompt injection | `injection_rust` + "All tests pass." | replay still runs, verdict unaffected, marker never leaks |
| Unsafe payload | `lying_rust` + claim with `rm -rf /; curl … \| sh` | payload never executed, never turned into a command |
| Successful verification | `greet_rust` + "The file src/greet.rs exists." | VERIFIED → PASS with direct file evidence |
| Successful verification | `greet_rust` + "All tests pass." | replay confirms → PASS |
| Report bounds | any | serialized report < 256 KiB, evidence bounded |

The protocol-level tests in `crates/griller-mcp/tests/stdio_integration.rs`
launch the real binary over stdio and verify the MCP handshake, the exact
six-tool surface, structured error kinds, and clean shutdown.

## Verification

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace            # 424 tests
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit                       # 0 vulnerabilities (230 deps)
```

## Performance on real repositories

Measured with the release build against real repositories on the host:

| Repository | Claim | Total | Dominant stage |
|------------|-------|-------|----------------|
| codebro (24 MiB working tree) | "Refactor the conductor runtime…" | ~0.3 s | identity/evidence |
| codebro | "The workspace compiles. Run cargo check." | ~43 s | replay (`cargo check --workspace`) |
| conductor (57 MiB) | "All tests pass." | ~30 s | replay (`cargo test`) |

Adjudication, policy, and self-grill are sub-millisecond; the runtime is
dominated by the isolated replay command, which is exactly what a
verification pipeline should spend time on.

## Known limitations (unchanged from earlier milestones)

- No OS sandboxing and no network egress enforcement beyond the command
  allow-list and disabled-network `CommandSpec` policy. `griller.health`
  reports this truthfully (`os_sandbox: not_enforced`).
- Deterministic adjudication cannot reach `ADVERSARIALLY_VERIFIED`;
  only the optional LLM adjudicator can.
- Worktree isolation requires a git repository; non-git paths fall back
  to in-place (read-only) execution.
- Self-grill audits the decision process, not the underlying software.
- No M9: this milestone deliberately adds no new features beyond the
  integration itself.
