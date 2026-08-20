# Development

## Workspace structure

A Cargo workspace of eight crates. The dependency direction is the
architecture (see [architecture.md](architecture.md)):

```
crates/
├── griller-core/          # pure value types, canonical JSON, BLAKE3 hashing
├── griller-runtime/       # execution, git, worktree, collectors, evidence store, identity, attacks
├── griller-adjudicate/    # M2: claims, normalization, matching, epistemic status (pure)
├── griller-policy/        # M3: severity, policy matrix, verdicts (pure)
├── griller-context/       # M4: project/task identity, context status (pure)
├── griller-self-grill/    # M7: 16-check decision audit (pure)
├── griller-cli/           # the `griller` binary (adapter)
└── griller-mcp/           # MCP stdio server, six tools (adapter)
tests/
└── fixtures/              # static golden + adversarial fixtures
```

## Crate responsibilities

| Crate | Responsibility | May do I/O? |
| --- | --- | --- |
| `griller-core` | domain/value types, canonical serialization, BLAKE3 hashing | never |
| `griller-runtime` | everything touching the outside world: process execution, git, worktree, collectors, evidence store, project identity collection, attack execution | **the only crate that does** |
| `griller-adjudicate` | claim model, deterministic normalizer + LLM boundary, matcher, SupportMatrix, contradiction checking, epistemic status, attack coverage | never |
| `griller-policy` | severity resolution, the auditable policy matrix, verdicts, required actions, findings, the M4→M3 bridge, the Diva renderer | never |
| `griller-context` | project identity model, task identity, context matcher, context status, LLM task-proposal boundary | never |
| `griller-self-grill` | SelfGrillInput/Result, the 16 checks, the single-pass runner | never |
| `griller-cli` | clap surface → Griller APIs → canonical JSON on stdout | adapter (delegates to runtime) |
| `griller-mcp` | six MCP tools + the `check_completion` orchestration | adapter (delegates to runtime) |

## Architectural rules

1. **No business logic in CLI/MCP adapters.** If a decision needs to be made,
   it belongs in a decision crate; if I/O is needed, it belongs in
   `griller-runtime`. The adapters validate input, call the pipeline, and
   serialize results. The one orchestration exception is the
   `check_completion` pipeline in `griller-mcp/src/completion.rs`, which
   wires existing stages and does not re-implement any of them.
2. **Pure crates stay pure.** No filesystem, no subprocesses, no network, no
   clock, no randomness, no LLM client in `griller-adjudicate`,
   `griller-policy`, `griller-context`, `griller-self-grill` — or
   `griller-core` itself.
3. **The LLM is a proposer, never a decider.** LLM outputs cross a strict
   parsing boundary (`deny_unknown_fields`; status/verdict/severity/confidence
   are hard parse errors). A provider is never required.
4. **Evidence is immutable and content-addressed.** New evidence kinds must
   define what is hashed and what is excluded; identity describes *what* was
   observed, provenance describes *where/how*.
5. **No silent fallbacks that flatter.** A policy combination without a rule
   defaults to WARN, never PASS. An unimplemented attack is recognized, not
   silently executed.
6. **Boring, explicit, testable Rust.** Prefer the obvious implementation.
   `#![forbid(unsafe_code)]` where the codebase does it.

## Build and test commands

```sh
cargo build --workspace                # dev build
cargo build --release -p griller-cli   # CLI binary
cargo build --release -p griller-mcp   # MCP server binary

cargo check --workspace                # type check
cargo test --workspace                 # 424 tests at final M8 validation
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo fmt --all -- --check             # formatting must be byte-clean
cargo audit                            # dependency vulnerability scan
```

`cargo fmt --all -- --check` and clippy with `-D warnings` are part of the
definition of done. The workspace config sets `lto = true` for release
profiles.

## Development workflow

1. **Understand the milestone map.** M0–M8 are complete; the milestone
   reports live in [docs/history/](history/). Read the canonical docs first
   ([concepts](concepts.md), [verification-pipeline](verification-pipeline.md)),
   then the history report for the milestone you touch.
2. **Find where the change belongs** using the crate-responsibility table.
   A new evidence collector → `griller-runtime::collectors`. A new status
   rule → the decision crate + its golden tests. A new MCP tool → adapter
   only, plus protocol tests.
3. **Write the tests first.** Golden fixtures for decisions, adversarial
   fixtures for security properties, protocol tests for the MCP surface.
   The suite is LLM-free by construction.
4. **Add the fixture.** Repository-shaped fixtures live in `tests/fixtures/`
   and are copied + `git init`-ed at test time so fixtures stay pristine.
5. **Run the full verification** (commands above) — all of it, before
   finishing.
6. **Keep the docs honest.** If behaviour changed, update the canonical doc
   that describes it; add a note to the relevant history report.

## Where new functionality belongs

| New capability | Belongs in |
| --- | --- |
| evidence collector or execution concern | `griller-runtime` |
| claim/status/matching rule | `griller-adjudicate` (+ golden tests) |
| policy matrix row or severity rule | `griller-policy` (+ `policy_matrix.rs`) |
| context identity/matching rule | `griller-context` (+ golden tests) |
| audit/consistency check | `griller-self-grill` (checks are pure functions) |
| CLI surface | `griller-cli` (adapter only) |
| MCP surface | `griller-mcp` (adapter only, + stdio protocol tests) |

The golden rule: a feature that requires deciding something belongs in the
decision layer; a feature that requires doing something belongs in
`griller-runtime`; a feature that only translates input/output belongs in an
adapter.