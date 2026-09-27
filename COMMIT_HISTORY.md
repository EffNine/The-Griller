# COMMIT_HISTORY.md

**The three commits that define the current state of the recovered repository.**

Git history has not been rewritten. Nothing was squashed, amended, or force-pushed. The
`docs(griller): reconcile recovery state` commit created in Phase 1.5 sits on top of these
and is recorded at the end.

For current state see [CURRENT_STATE.md](CURRENT_STATE.md). For per-phase reasoning see
[PHASE_HISTORY.md](PHASE_HISTORY.md).

---

## `df002d3` — multi-language test replay and numeric assertion support

| Field | Value |
|---|---|
| Full SHA | `df002d32f85c83ecf0def25378f9ad04378a5bca` |
| Parent | `40016d3c2d9c5190b7e1d8f6c24b1096eb23fad9` — `release: The Griller v0.2.0` |
| Author date | Sat Aug 22 14:09:50 2026 +0800 |
| Files | 23 |
| Diff | +2129 / −46 |
| Pushed | **Yes** — it is on `origin/main` |

### Purpose

The last commit authored upstream before the project was archived. It added multi-language
test replay (Go, Node alongside Rust) and numeric assertion support.

### Important files

The 23 files span `crates/griller-mcp/src/{plan,completion,validate}.rs`,
`crates/griller-context/src/`, `crates/griller-adjudicate/src/`, and the corresponding
golden tests.

### Verification result

Not re-verified in isolation by this recovery work. It was included in every Phase 1+
baseline run, which passed — so it is **known-good as of the recovery baseline**, not
independently re-audited.

### Note

`40016d3` and `1e08396` below it are the pre-archive history. `df002d3` was the entry state
that Phase 1 copied. The repository's history before the v0.2.0 release commit was squashed
into a single initial commit upstream; that is recorded in `.git/logs/HEAD` and is not
recoverable from this repository.

---

## `a360b7c` — restore terse completion assertion coverage

| Field | Value |
|---|---|
| Full SHA | `a360b7c6487fd41f163b9d3fad13e05bac9696b0` |
| Parent | `df002d32f85c83ecf0def25378f9ad04378a5bca` |
| Author date | Sun Sep 27 21:50:58 2026 +0800 |
| Files | **16** |
| Diff | +1027 / −99 |
| Pushed | **No** |

### Purpose

To commit the 16 files of uncommitted work recovered from the archive, **and** to fix the
one security regression found in Phase 1.1 review.

### ⚠ This commit contains two logically distinct changes

**This is what actually happened, and it is stated here rather than tidied away.**

| Origin | Files | Lines |
|---|---:|---|
| **Inherited** — pre-existing uncommitted work found in the archived working tree, authored before this recovery began. Committed here unchanged. | 13 | +939 / −99 |
| **Phase 1.2** — the terse-assertion fix authored during this recovery | 3 | +88 / 0 |

The commit's **subject line names only the terse fix**. A reviewer reading the subject alone
will not know the commit also lands 939 lines of inherited work across 13 Rust files, and
the commit body does not mention it either.

**This is not a reason to rewrite history.** The instruction for Phase 1.2 was to create one
focused commit with a specified message, and that is what was done. If this work is ever
pushed or submitted for review, the inherited-work portion must be called out explicitly in
the push or PR description.

### Important files

Phase 1.2 authored exactly 3:

| File | Change |
|---|---|
| `.opencode/plugins/griller-gate.ts` | +21 — two rules added before `return false` in `looks_like_completion_assertion` |
| `.opencode/plugins/griller-gate.test.ts` | +67 — mirrored rules plus the `REG: terse completion assertions` table (25 positives, 2 injections) |
| `docs/gate.md` | rewritten in place to correct the `UNSUPPORTED_COMPLETION_BLOCK` decision documentation |

The other 13 are the inherited work: `Cargo.lock`, `crates/griller-adjudicate/src/normalizer.rs`,
`crates/griller-adjudicate/tests/golden.rs`, `crates/griller-cli/src/commands.rs`,
`crates/griller-context/src/normalizer.rs`, `crates/griller-context/src/task.rs`,
`crates/griller-context/tests/golden.rs`, `crates/griller-core/Cargo.toml`,
`crates/griller-core/src/command.rs`, `crates/griller-mcp/src/completion.rs`,
`crates/griller-mcp/src/plan.rs`, `crates/griller-mcp/src/validate.rs`,
`docs/context-detection.md`.

**None of these are verdict logic.** `crates/griller-adjudicate/src/status.rs` and
`crates/griller-core/src/provenance.rs` are **absent from this commit** — verified with Git.
The single `EpistemicStatus::Verified` assignment site was untouched.

### Verification result

All 5 gates green **after committing**: fmt, check, `cargo test` **515 passed / 0 failed**,
clippy, `npx vitest run` **24/24** (up from 23).

Audited in [COMMIT_INTEGRITY.md](COMMIT_INTEGRITY.md): parent correct, 16 files, no
historical evidence, no secret-shaped strings, no paths outside
`.opencode/ | Cargo.lock | crates/ | docs/`.

---

## `16c6852` — refuse verification of dirty repositories

| Field | Value |
|---|---|
| Full SHA | `16c685225c2fad07a365587a3e8a5725c31d5def` |
| Parent | `a360b7c6487fd41f163b9d3fad13e05bac9696b0` |
| Author date | Sun Sep 27 22:51:55 2026 +0800 |
| Files | **3** |
| Diff | +480 / −2 |
| Pushed | **No** |

### Purpose

Close the trust-boundary defect identified in Phase 1.3: replay inspects `HEAD`, so a
`VERIFIED` result could describe code the verification engine never looked at.

### Important files

| File | + | − | Change |
|---|---:|---:|---|
| `crates/griller-mcp/src/completion.rs` | 182 | 2 | Dirty-tree refusal before the attack loop; `repository` report section; two helpers |
| `crates/griller-mcp/tests/completion_flow.rs` | 285 | 0 | 8 new tests |
| `crates/griller-gate/src/report.rs` | 13 | 0 | `repository_dirty` finding in the record summary |

Deliberately **not** touched: `status.rs`, `provenance.rs`, `worktree.rs`, `execution.rs`,
`command.rs`, `config.rs`, `validate.rs`, `griller-mcp/src/lib.rs`, all `docs/`,
`.opencode/`. Verified with Git before staging.

### Verification result

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace` | PASS |
| `cargo clippy -- -D warnings` | 0 warnings |
| `cargo test --workspace` | **523 passed, 0 failed**, 6 ignored, 35 binaries |
| `npx vitest run` | **24/24** |

Independently re-run in an isolated OpenSandbox container: pristine `HEAD` 514 passed /
1 failed → with this commit **522 passed / 1 failed** (the same pre-existing failure). Zero
new failures.

The trust boundary was **proven** closed, not assumed: disabling only the dirty predicate
makes 6 tests fail with `a dirty repository must never be VERIFIED`.

`cargo fmt` and `cargo clippy` were verified on the host only — the container image lacks
both components and has no network to install them.

---

## `docs(griller): reconcile recovery state` — Phase 1.5

Documentation only. No production source file changed, no historical evidence changed, no
commit was rewritten, and nothing was pushed.

Verified with Git by comparing the blob manifest of all 173 tracked non-`.md` files before
and after the documentation work. See [CURRENT_STATE.md](CURRENT_STATE.md) §1.

---

## Rules for this repository

1. **Do not push** without confirming the user still wants
   `github.com/EffNine/The-Griller` to be the home of this work.
2. **Do not amend or squash** `a360b7c` or `16c6852`. The two-change composition of
   `a360b7c` is real history and is documented above instead of being hidden.
3. If this work is ever published, **state the inherited-work composition of `a360b7c` in
   the push or PR description** — the commit message does not.
