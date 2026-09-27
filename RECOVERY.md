# RECOVERY.md — The Griller → verification-gate

> **Status: Phase 1 record (RECOVERY, 2026-09-27T13:12:25Z).**
>
> Findings and measurements below are as observed at recovery. Statements that
> were made *current* at the time and have since become false are marked
> **SUPERSEDED** inline. For the current state, see
> [CURRENT_STATE.md](CURRENT_STATE.md).

**Recovery timestamp:** 2026-09-27T13:12:25Z
**Phase:** 1 (RECOVERY) — no refactoring, no integration, no production claims.

---

## 0. Path correction (read first)

The task brief specified the source as `/home/afnan/projects/archive/the-griller`.
**That path does not exist.** The project lives at:

```
/home/afnan/projects/archive/legacy/the-griller
```

`archive/` contains exactly two subdirectories (`legacy/`, `old/`). The real path is
`archive/legacy/the-griller`. All work in this phase used the real path.

The destination `/home/afnan/projects/active/verification-gate` **did not exist** and no
project of that name or function existed anywhere in the workspace. The 15 projects in
`active/` were checked; none is a verification gate. **The destination was created.**

---

## 1. What this project is

**The Griller** is a deterministic, LLM-free verification layer for *agent completion
claims*. An agent says "I implemented the fix and all tests pass." The Griller does not
believe it — it replays the claimed verification command against real repository state,
adjudicates the result against a fixed evidence model, applies policy, and self-audits its
own decision. It emits a verdict with a hash-addressed evidence trail.

It is the **only** component in the workspace that answers "did the agent's claim survive
contact with reality?" Nothing in `active/` duplicates it. `codebro` stores facts and
memory; it does not adjudicate claims.

### Pipeline (`griller-mcp/src/completion.rs:75`)

```
M4 context/identity  →  evidence collection  →  M2 adjudicate  →  M5 attacks
                      →  M2 re-adjudicate    →  M3 policy      →  M7 self-grill
```

---

## 2. Why it was dead (root cause, evidenced)

Not a failure of the software. It was **decommissioned administratively** and left
orphaned in `archive/`:

| Evidence | Finding |
|---|---|
| `~/.config/opencode/plugins/` | contains **only** `codebro-context` — the gate plugin is absent |
| `~/.config/opencode/opencode.jsonc` | `griller` appears **0 times** — MCP registration removed |
| `~/.config/griller/gate.json` | points at `/home/afnan/projects/active/the-griller/target/release/griller-mcp` — a path that **does not exist** |
| `~/.local/bin/` | orphaned `griller-gate`, `griller-mcp` binaries remain on PATH |
| `archive/legacy/the-griller/` | moved into archive while still carrying **16 uncommitted files** of active work |

The 131 production decision records reference `repository: /home/afnan/projects/active/the-griller`,
confirming it previously lived at `active/the-griller` and was moved without a cleanup pass.

> **Still true of the archive.** The row above describes `archive/legacy/the-griller` as it
> was found and still stands — that copy is untouched and still carries those 16
> uncommitted files. The *recovered* copy at `active/verification-gate` no longer does; its
> contents were committed in `a360b7c`. See [CURRENT_STATE.md](CURRENT_STATE.md).

---

## 3. What was preserved

| Preserved | Count / value |
|---|---|
| Git history | **6 commits**, `df002d32f85c83ecf0def25378f9ad04378a5bca`, branch `main` |
| Git remote | `https://github.com/EffNine/The-Griller.git` (verified reachable; `origin/main` == local HEAD) |
| Tag | `v0.2.0` |
| Tracked files | **208** |
| Uncommitted modifications | **16 files, +939 / −99 lines** — diff SHA256 `bd0ba55a…9067` |
| Rust source | 131 `.rs` files, 32,885 lines |
| Crates | **9** |
| Test attributes | 516 (static) |
| MCP tools | 6 |
| Docs | 25 markdown files incl. `docs/history/` (M2–M8 design record) |
| Test fixtures | `tests/fixtures/` (codebro, meridian, atlas, unknown_eco, fake_secrets.txt, …) |
| Plugin | `.opencode/plugins/griller-gate.ts` + `.test.ts` (23 tests) |
| Production evidence | **131 decision records + 133-line gate.log**, left in place at `~/.local/share/griller/gate/` (not copied, not moved, not mutated) |

### Deliberately excluded (provably generated, 0 tracked files)

| Path | Size | Justification |
|---|---|---|
| `target/` | 17 GB | `.gitignore:2-3` (`/target`, `target/`); `git ls-files target/` → **0** |
| `.opencode/node_modules/` | 101 MB | `.opencode/.gitignore:1`; `git ls-files` → **0** |

Both are regenerable by `cargo build` / `npm install`. Nothing else was excluded.

### Copy integrity verification (all matched exactly)

```
HEAD              df002d32f85c83ecf0def25378f9ad04378a5bca   ✓
HEAD tree         a83a61b9a266e2fa02f60eab97c17743e7ff5fe7   ✓
working-tree diff bd0ba55aa34f0feff0f0ff335f6a43723add...   ✓
tracked files     208                                       ✓
dirty files       16                                        ✓
commits           6                                         ✓
```

**The dirty state was preserved bit-for-bit.** This is the single most important
preservation outcome: 939 lines of uncommitted work would have been lost by a plain
`git clone`.

---

## 4. The uncommitted work (what it is)

The 16 dirty files are one coherent feature: **completion-assertion detection**
(`docs/context-detection.md` +10, `docs/gate.md` +20).

It replaces two ad-hoc keyword regexes in the *trigger layer* with a single
`looks_like_completion_assertion()` predicate, plus a large normalizer expansion
(`griller-context/src/normalizer.rs` +215, `griller-adjudicate/src/normalizer.rs` +90) and
211 lines of new golden tests.

**Why it existed** (from the in-code rationale): the old keyword list
(`pass|complete|done|…|test|verify|build|run|check|claim|ready`) matched bare nouns, so
ordinary technical discussion ("424 tests", "the gate replays") was extracted as a
completion claim and then **blocked** with `UNSUPPORTED`. The production record confirms
this was a real failure mode: `UNSUPPORTED_COMPLETION_BLOCK` is the single most frequent
reason code (39 of 131).

**It is tested and green** — see `BASELINE_VERIFICATION.md` (23/23 plugin tests).

**It was uncommitted and therefore at risk** (the state observed at recovery, 2026-09-27).
> **SUPERSEDED by `a360b7c` (2026-09-27).** The 16 files are now **committed**. They are no
> longer at risk. The 23/23 plugin figure above is the count at recovery time; the current
> count is 24/24. See [CURRENT_STATE.md](CURRENT_STATE.md) §2.

---

## 5. Baseline state: the project builds and passes

| Gate (defined by the project's own CI) | Result |
|---|---|
| `cargo fmt --all -- --check` | **PASS** (exit 0) |
| `cargo check --workspace` | **PASS** (exit 0, 0 warnings) |
| `cargo test --workspace` | **PASS** — 515 passed, 0 failed, 6 ignored, 43 binaries |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** (exit 0) |
| `npx vitest run` (plugin) | **PASS** — 23/23 |

The 6 ignored tests are macro-generated `*Tool::request_params` doctests — schema
illustrations emitted by `#[macros::mcp_tool]`, not behavioural tests. **No security-relevant
test is skipped.**

The documented figure `424 tests` (`docs/testing.md:9`) is a historical M8 field-validation
number. The current tree statically declares 516 and executed 515 + 6 ignored. **The docs
understate; the code is ahead of its own documentation.**

---

## 6. Recovery boundary

Deliberately **not** done in this phase, per the brief:

- No MCP re-registration (`opencode.jsonc` untouched)
- No plugin installation into `~/.config/opencode/plugins/`
- No CodeBro / Conductor / Infra / Atlas / computer-use integration
- No microservice, no architecture redesign
- No history rewrite, no commit of the pre-existing dirty work
- No dependency upgrades
- No `~/.local/share/griller/gate/` mutation

**Status: preserved, buildable, testable, independently trustworthy — but NOT registered and
NOT integrated. It is not running.**

---

## 7. Known limitations carried forward

1. **Does not verify uncommitted edits.** Replay uses `git worktree add --detach <path>`
   with no commit-ish (`crates/griller-runtime/src/worktree.rs:107-110`), so it verifies
   committed `HEAD`, not the working tree. The project's own `docs/limitations.md:34-38`
   states this correctly. → `DIRTY_STATE_ANALYSIS.md`
2. **Gate timeout < replay budget.** `DEFAULT_TIMEOUT_MS = 120_000`
   (`crates/griller-gate/src/config.rs:10`) vs `REPLAY_TIMEOUT_MS = 600_000`
   (`crates/griller-core/src/command.rs:21`). A cold replay >120 s yields
   `GATE_UNAVAILABLE`. → `TIMEOUT_PROCESS_ANALYSIS.md`
3. **Timeout kills the direct child only.** `child.kill()` at
   `crates/griller-runtime/src/execution.rs:189`; no `killpg`/`setpgid` anywhere in the
   workspace. Grandchildren can survive. → `TIMEOUT_PROCESS_ANALYSIS.md`
4. **4 of 8 attack kinds unimplemented.** `IMPLEMENTED_ATTACKS: [AttackKind; 4]`
   (`crates/griller-mcp/src/plan.rs:52-57`). Unsupported kinds are **rejected as
   INVALID_REQUEST, never silently dropped**. Consequence: `AdversariallyVerified` is
   documented as unreachable from M2.
5. **No kernel-level isolation.** `Enforcement::m1_none()` truthfully reports
   `NotEnforced` for both network isolation and OS sandboxing. No over-claim.
6. **16 uncommitted files** (§4) — preserved, not committed.
   > **SUPERSEDED by `a360b7c` (2026-09-27).** Now committed. Resolved.
7. **Dead config**: `~/.config/griller/gate.json` points at a non-existent path.

---

## 8. Deliverables

| File | Contents |
|---|---|
| `RECOVERY.md` | this file |
| `RECOVERY_MANIFEST.md` | machine-readable recovery record |
| `BASELINE_VERIFICATION.md` | build/test/lint results with per-category verdicts |
| `MCP_SURFACE.md` | the 6 tools, schemas, trust boundaries, exposure assessment |
| `HISTORICAL_EVIDENCE.md` | the 131 production decisions, analysis, fixture strategy |
| `DIRTY_STATE_ANALYSIS.md` | the worktree gap and the 16 uncommitted files |
| `TIMEOUT_PROCESS_ANALYSIS.md` | timeout layering and process-group handling |
| `NEXT_ACTIONS.md` | 5 actions |
