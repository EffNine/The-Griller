# COMMIT_INTEGRITY.md

> **Status: Phase 1.3 audit of `a360b7c`. Its commit checks all still hold.**
>
> Every check in §1–§5 was re-run and still holds. Its §6 observation —
> that `a360b7c` bundles the inherited work with the Phase 1.2 fix — is now
> documented prominently in [COMMIT_HISTORY.md](COMMIT_HISTORY.md). `a360b7c`
> remains unpushed. **§6 item 2 is now out of date** (`a360b7c` is no longer the
> only unpushed commit — there are now three) and **item 3 is resolved** (the
> documents are tracked as of `8800a0b`); both are marked inline.

Phase 1.3 — audit of commit `a360b7c`.
**Not pushed. Not amended. No file modified in producing this document.**

---

## 1. Identity

| Property | Value | Verdict |
|---|---|---|
| Full SHA | `a360b7c6487fd41f163b9d3fad13e05bac9696b0` | — |
| Parent | `df002d32f85c83ecf0def25378f9ad04378a5bca` | ✅ **matches expected** |
| Subject | `fix(griller): restore terse completion assertion coverage` | ✅ as instructed |
| Author | `EffNine <nrwntp5f9g@privaterelay.appleid.com>` | pre-existing repo identity |
| Files | 16 | ✅ exactly the intended set |
| Diff | +1027 / −99 | ✅ |

The parent is the Phase 1.1 entry state (`df002d3`, *"multi-language test replay…"*). The
commit therefore sits on top of the recovered history rather than replacing any of it.

---

## 2. Contents — 16 files

| File | + | − | Origin |
|---|---:|---:|---|
| `.opencode/plugins/griller-gate.ts` | 112 | 36 | **inherited (91/36) + Phase 1.2 (+21)** |
| `.opencode/plugins/griller-gate.test.ts` | 186 | 32 | **inherited (119/32) + Phase 1.2 (+67)** |
| `docs/gate.md` | 12 | 8 | **inherited + Phase 1.2 (rewritten in place)** |
| `Cargo.lock` | 33 | 0 | inherited |
| `crates/griller-adjudicate/src/normalizer.rs` | 82 | 8 | inherited |
| `crates/griller-adjudicate/tests/golden.rs` | 122 | 0 | inherited |
| `crates/griller-cli/src/commands.rs` | 4 | 1 | inherited |
| `crates/griller-context/src/normalizer.rs` | 212 | 3 | inherited |
| `crates/griller-context/src/task.rs` | 2 | 2 | inherited |
| `crates/griller-context/tests/golden.rs` | 89 | 0 | inherited |
| `crates/griller-core/Cargo.toml` | 1 | 0 | inherited |
| `crates/griller-core/src/command.rs` | 86 | 0 | inherited |
| `crates/griller-mcp/src/completion.rs` | 66 | 8 | inherited |
| `crates/griller-mcp/src/plan.rs` | 5 | 0 | inherited |
| `crates/griller-mcp/src/validate.rs` | 5 | 1 | inherited |
| `docs/context-detection.md` | 10 | 0 | inherited |

**"Inherited" means:** already present as uncommitted work in the archived source repository
before Phase 1.1 began. It is committed here, but it was **not authored by this recovery
work** — it is the completion-assertion refactor plus context/claim normalisation, preserved
as instructed.

**Phase 1.2 authored exactly 3 files' worth of change (+88 lines):** the terse-assertion
rules in the plugin, the mirrored rules plus the regression table in the test, and the
`docs/gate.md` correction. The remaining 13 files were verified byte-identical to their
Phase 1.1 entry state before staging.

---

## 3. Verification against the required checks

| Check | Result | Evidence |
|---|---|---|
| Parent is `df002d3` | ✅ | `git rev-parse a360b7c^` = `df002d32f85…` |
| Contains the intended 16-file recovery state | ✅ | 16 files, all from the Phase 1.1 inventory |
| Plus the Phase 1.2 changes | ✅ | 3 files, +88 lines, all intended |
| **No Rust verdict changes** | ✅ | `status.rs`, `provenance.rs`, `worktree.rs`, `execution.rs`, `policy/lib.rs` — **0 occurrences in the commit** |
| No historical evidence | ✅ | no path under `decisions/`, `gate.log`, or any `~/.local/share` reference |
| No unrelated workspace files | ✅ | every path is `.opencode/`, `Cargo.lock`, `crates/`, or `docs/` |
| No secret material | ✅ | 0 secret-shaped lines (`sk-`, `ghp_`, `AKIA`, `mcs_`, private-key headers) |
| Not pushed | ✅ | 1 unpushed commit vs `origin/main` |
| Older commits not amended | ✅ | `40016d3` → `1e08396` → `df002d3` → `a360b7c`, all intact |

### On the Rust files that *are* present

`crates/griller-core/src/command.rs` (+86), `griller-context/src/normalizer.rs` (+212),
`griller-adjudicate/src/normalizer.rs` (+82) and `griller-mcp/src/completion.rs` (+66) appear
in the commit. These are **normalisation and pipeline wiring**, inherited from the archived
working tree. They are **not** verdict logic:

- `command.rs` — execution/spec types, not the adjudication rules
- `normalizer.rs` (context/adjudicate) — claim and project-name parsing
- `completion.rs` — pipeline wiring

The adjudication decision function (`griller-adjudicate/src/status.rs`) and the provenance
invariants (`griller-core/src/provenance.rs`) are **provably absent from the commit**. The
single `EpistemicStatus::Verified` assignment site is untouched.

---

## 4. Verification state of the committed tree

All gates re-run **after** committing, against the committed content:

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | exit 0 |
| `cargo check --workspace` | exit 0 |
| `cargo test --workspace` | **515 passed, 0 failed**, 0 failed binaries |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | exit 0 |
| `npx vitest run` | **24 passed** |

Committed-content spot checks: the regression test, both terse rules, the corrected code
docstring, and the `docs/gate.md` correction are all present in `a360b7c`.

---

## 5. State after the commit

| Property | Value |
|---|---|
| Working tree (tracked) | **clean** — 0 modified, 0 staged |
| Untracked | 15 markdown files (14 recovery/review + `COMMIT_INTEGRITY.md` and the other Phase 1.3 artifacts) |
| Unpushed commits | 1 (`a360b7c`) |
| Historical evidence | untouched; same-disk backup verified byte-identical |

---

## 6. Observations for the owner

1. **One commit carries two logically distinct changes**: the inherited completion-assertion
   refactor and the Phase 1.2 terse-assertion fix. This was the instruction ("one focused
   commit", specified message), and the message does describe the terse fix. But a reviewer
   reading only the subject line will not know the commit also lands 939 lines of inherited
   work across 13 Rust files. The body does not mention it either. **Worth stating in the
   push/PR description** rather than rewriting history.
2. **The commit is the only unpushed commit**, so pushing is a single operation — but it
   pushes to `github.com/EffNine/The-Griller`, and whether that remote is still the intended
   home for this work was flagged in Phase 1.1 and has not been answered.
   > **OUT OF DATE.** `a360b7c` is no longer the only unpushed commit. There are now
   > three: `a360b7c`, `16c6852`, `8800a0b`. The underlying question — is that remote the
   > right home — is still unanswered and still blocks any push.
3. **The 15 untracked documents remain unprotected** and three of them now contain stale
   claims. See `RECOVERY_DOCS_CLASSIFICATION.md` §3.
   > **RESOLVED.** Those documents are now tracked (`8800a0b`) and the stale claims were
   > corrected with explicit supersession markers. See
   > [CURRENT_STATE.md](CURRENT_STATE.md) §7.
