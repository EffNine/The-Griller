# Getting started

This guide gets you from clone to a real completion check in under five
minutes. The full pipeline is also exposed over MCP — see
[`opencode.md`](opencode.md) for the OpenCode walkthrough.

## Prerequisites

- Rust toolchain (stable; the workspace builds with edition 2021)
- `git` (worktree isolation and most collectors depend on it)
- `cargo` (for replaying Rust test claims; see [limitations](limitations.md)
  for other ecosystems)

## Build

```sh
git clone <your-fork> the-griller
cd the-griller
cargo build --release -p griller-cli
cargo build --release -p griller-mcp
```

Binaries land in `target/release/griller` and `target/release/griller-mcp`.
You can also run from source with `cargo run -p griller-cli -- …` — the
examples below use that form so they work without installing anything.

## 1. Health check

```sh
cargo run -p griller-cli -- health
```

Prints JSON describing the toolchain and whether git was found. No LLM is
involved anywhere in the pipeline.

## 2. Does this task belong to this repository?

This is the original wrong-project use case, and it is read-only:

```sh
cargo run -p griller-cli -- context check \
  --task "Implement Atlas P10.5.1 Fact Store." \
  --repo /path/to/your/repo
```

Exit codes: `0` = PASS, `1` = WARN, `2` = BLOCK, `3` = system error. A strong
context mismatch (`MISMATCH_CONTEXT_BLOCK`) means "the task explicitly names a
project this repository has no evidence of." Absence of a word never blocks by
itself — see [context-detection.md](context-detection.md).

## 3. Collect evidence

Evidence is captured, content-addressed, and immutable:

```sh
# repository state, git diff, snapshot, manifest, file content
cargo run -p griller-cli -- evidence git-diff --repo .
cargo run -p griller-cli -- evidence snapshot --repo .
cargo run -p griller-cli -- evidence manifest --repo .
cargo run -p griller-cli -- evidence file --repo . --path Cargo.toml

# run a command in an isolated worktree and capture its output as evidence
cargo run -p griller-cli -- run --repo . -- cargo test
```

`--store <dir>` persists evidence to a JSON evidence store for later
adjudication. See [evidence.md](evidence.md) for the model.

## 4. Adjudicate a claim

```sh
# normalize a claim into structure (M2)
cargo run -p griller-cli -- claim normalize --claim "Tests pass"

# assess against evidence (M2) — status only, never a verdict
cargo run -p griller-cli -- claim assess --claim "Tests pass"
cargo run -p griller-cli -- claim assess --claim "Tests pass" --repo . --auto-collect

# decide an operational verdict (M3) — exit 0/1/2/3
cargo run -p griller-cli -- claim decide --claim "Tests pass"
cargo run -p griller-cli -- claim decide --claim "Tests pass" --repo . --auto-collect
```

`--auto-collect` runs matching verification commands in an isolated worktree
(never your working tree), detects the right command from manifests, and
feeds the captured output into adjudication.

## 5. Run adversarial checks (M5)

```sh
cargo run -p griller-cli -- attack --repo . --claim "Tests pass"
```

Replays the claimed verification (`cargo test`) in isolation and reports the
outcome. See [red-team.md](red-team.md) for the full attack catalogue.

## 6. Self-grill (M7)

```sh
cargo run -p griller-cli -- self-grill --claim "Tests pass" --repo .
```

Runs the 16 structural checks over the decision pipeline and reports
`HOLDS` / `WEAKENED` / `INCONCLUSIVE` / `REFUTED` / `NOT_APPLICABLE`. It never
modifies the original decision. See [self-grill.md](self-grill.md).

## 7. The flagship: check_completion over MCP

```sh
cargo run -p griller-mcp
```

Starts the MCP server over stdio. Register it in your MCP client — the
OpenCode configuration that was verified in the field is documented in
[opencode.md](opencode.md). The single call that matters:

```text
griller.check_completion(repo, claim)
```

runs M4 → M2 → M5 → M2 → M3 → M7 and returns one bounded, auditable report.
See [verification-pipeline.md](verification-pipeline.md) and
[mcp.md](mcp.md).

## Verify the project itself

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
```

At the final M8 field-validation run: 424 tests passing, clippy clean with
`-D warnings`, and `cargo audit` reported 0 vulnerabilities across 230
dependencies.

## Where to go next

| You want to… | Read |
| --- | --- |
| Understand the concepts (claim vs verdict vs proof) | [concepts.md](concepts.md) |
| See the crate structure and dependency rules | [architecture.md](architecture.md) |
| Know what the Griller does NOT guarantee | [limitations.md](limitations.md) |
| Audit the security model | [security.md](security.md) |
| Contribute | [development.md](development.md) |