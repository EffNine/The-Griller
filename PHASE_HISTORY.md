# PHASE_HISTORY.md

**What actually happened, in order.** This is a record of completed work and its
verification, not a plan and not a ranking.

For the current state of the repository, see [CURRENT_STATE.md](CURRENT_STATE.md).
For commit-level detail, see [COMMIT_HISTORY.md](COMMIT_HISTORY.md).

Timestamps are recorded only where they were captured during the work. Where a phase did
not record one, that is stated rather than estimated.

---

## Phase 1 — Recovery

**Objective.** Recover the archived `the-griller` project into a governed active project
without copying blindly, without redesigning, and without losing work.

**Outcome: complete.**

### What was done

- Established that the brief's source path `archive/the-griller` **does not exist**; the
  real source is `archive/legacy/the-griller`.
- Confirmed no `verification-gate` project pre-existed in `active/` (15 projects checked).
- Copied with `cp -a` **including `.git` and dirty state**, not `git clone` — a clone would
  have destroyed 939 lines of uncommitted work. Excluded only `target/` (17 GB) and
  `.opencode/node_modules/` (101 MB), both gitignored with 0 tracked files.
- Verified the copy was bit-identical by three independent methods.
- Reproduced the build and test baseline from cold.

### Deliverables (8)

`RECOVERY.md`, `RECOVERY_MANIFEST.md`, `BASELINE_VERIFICATION.md`, `MCP_SURFACE.md`,
`HISTORICAL_EVIDENCE.md`, `DIRTY_STATE_ANALYSIS.md`, `TIMEOUT_PROCESS_ANALYSIS.md`,
`NEXT_ACTIONS.md`.

### Source changes

**None.** This phase changed no production source.

### Tests

Baseline only: `cargo fmt` PASS, `cargo check` PASS, `cargo test` **515 passed / 0 failed /
6 ignored**, `cargo clippy -D warnings` PASS, `npx vitest run` **23/23**.

### Security / trust result

No change to the trust model. One finding recorded: replay verifies committed `HEAD`, not
uncommitted edits (`worktree.rs:107-110`), which made the workspace-level claim "verifies
agent edits" false for dirty trees.

### Unresolved at the end of this phase

16 uncommitted files of genuine work at risk; the dirty-state trust boundary; a gate
timeout mismatch; no process-group kill.

---

## Phase 1.1 — Review and preservation

**Objective.** Review the recovered uncommitted work, verify the historical evidence, and
classify risk before committing anything.

**Outcome: complete.** 6 review artifacts produced; 1 evidence backup created.

### What was done

- Traced every claim in the 8 inherited documents back to code or production evidence.
  **All 8 historical numbers confirmed.**
- Classified the 39 `UNSUPPORTED_COMPLETION_BLOCK` records: **25 genuine refusals,
  12 false positives, 2 prompt-injection attempts.**
- Classified the trigger change as **C. FAIL-OPEN** — scoped to the trigger layer only; the
  Rust verdict engine remained fail-closed.
- Measured **8 fail-open regressions** against the new predicate, including 2 blocked by
  the `^\W*` anchor (`"That's done."`, `"It's ready."`).
- Created and verified the evidence backup
  `~/.local/share/griller/evidence-backup-20260927T131225Z/` by 3 independent methods.

### Source changes

**None.** This phase was inspection-only.

### Security / trust result

Decision was **COMMIT AFTER SMALL FIX** — neither COMMIT NOW (8 measured fail-open
regressions) nor DO NOT COMMIT (the work fixes 12 real false blocks, is fully tested, and
was at risk of loss).

### Unresolved at the end of this phase

The terse-assertion regression; the dirty-state policy decision.

---

## Phase 1.2 — Completion assertion regression fix

**Commit:** `a360b7c6487fd41f163b9d3fad13e05bac9696b0`
**Parent:** `df002d32f85c83ecf0def25378f9ad04378a5bca`

**Outcome: complete, not pushed.**

### What was done

- Implemented the Phase 1.1 fix in the trigger predicate, plus a correction the exact
  proposal missed: an optional leading contraction group `([a-z]+['’]s\s+)?` beyond the
  literal regex, because the proposal blocked `"That's done."` and `"It's ready."`
  (2 of the 8 regressions). Verified 0 mismatches across all 20 measured forms.
- Applied the same rules to the predicate copy inside the test file, which differs from the
  plugin only in the `original` truncation bound.
- Added the `REG: terse completion assertions` regression table: 25 positives, 2 injections.
- Corrected the `UNSUPPORTED_COMPLETION_BLOCK` decision documentation in `docs/gate.md`.

### Source changes

**3 files touched by this phase** (+88 lines): `.opencode/plugins/griller-gate.ts`,
`.opencode/plugins/griller-gate.test.ts`, `docs/gate.md`.

The commit as a whole contains **16 files** — the 13 remaining files are the inherited
pre-existing work, committed unchanged. See [COMMIT_HISTORY.md](COMMIT_HISTORY.md).

### Tests

All 5 gates green **after committing**: fmt, check, `cargo test` **515 passed / 0 failed**,
clippy, `npx vitest run` **24/24** (up from 23 — the new regression test).

### Security / trust result

Trigger fail-open **closed** for all 27 measured forms. Fail-closed behaviour preserved
across the entire measured set. Residual risk is structural: a fixed verb/noun enumeration
plus silent skips that leave no audit record — **not** closed by this phase, and still open.

### Not fixed, deliberately

`"All 36 test suites pass (pnpm test)."` — verified pre-existing (both revisions skip it),
and broadening the predicate was explicitly forbidden.

### Unresolved at the end of this phase

The dirty-tree trust boundary; silent-skip observability.

---

## Phase 1.3 — Policy and preservation

**Outcome: complete.** 5 artifacts produced. Nothing implemented, committed, or pushed.

### What was done

- Inventoried and classified the 14 then-untracked markdown files; found that 3 contained
  claims `a360b7c` had falsified.
- Re-verified the evidence backup: 131/131 records, `gate.log` 13,689 bytes, per-file
  SHA256 identical 132/132, aggregate
  `6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817`, originals untouched.
  Classified it as **B — two logical copies on the same physical disk**, explicitly *not* an
  independent backup (same `stat -c %d` device `66306`).
- **Traced the trust boundary and found it violated.** This was the substantive result of
  the phase:
  - `completion.rs:197` — all three evidence collectors read `req.repo`, the **user's**
    tree, so `SnapshotCollector` *does* capture `dirty`/`staged`/`unstaged`/`untracked`.
  - `worktree.rs:94-141` — replay isolation is `git worktree add --detach` with no
    commit-ish, i.e. a checkout of `HEAD`.
  - `completion.rs:485-501` — replay is pointed at that worktree.
  - `status.rs:153-249` — the verdict rules contain **no** reference to `dirty`, `is_clean`,
    or any staged/unstaged field.
  - `matcher.rs` — `GitDiff` evidence is consumed **only** for file-change and
    file-removal claims.
  - `context.rs:82` — the only `WorkingTreeState::Clean` literal is inside `#[cfg(test)]`.

  So the gate **saw** the dirty state and **ignored** it for the claims that matter most.
  "Agent changed files → verification runs → verification result" was only partially true:
  step 2 verified `HEAD`, not the change.
- Decided **Option A — refuse dirty** on the project's own evidence model, and recorded
  why Option B was rejected.

### Source changes

**None.** This phase was a policy and documentation phase by instruction.

### Security / trust result

Trust boundary **identified and scoped precisely** — narrower than "the gate cannot see
dirty state": file-change and file-removal claims were already sound. The defect affected
claims whose support comes from replay.

### Unresolved at the end of this phase

The defect itself. 14 documents untracked and 3 stale.

---

## Phase 1.4 — Dirty-tree refusal

**Commit:** `16c685225c2fad07a365587a3e8a5725c31d5def`
**Parent:** `a360b7c6487fd41f163b9d3fad13e05bac9696b0`

**Outcome: complete, not pushed.**

### What was done

- Traced `RepositorySnapshot`, `WorkingTreeState`, `SnapshotCollector`, `completion.rs`,
  `status.rs`, the report model, the existing `Unverifiable` variants, and the existing
  clean/dirty tests before changing anything.
- Implemented Option A at the earliest safe insertion point: after initial adjudication,
  **before** the attack loop, so no attack is planned and nothing can become support.
- **Added no new vocabulary.** Reused `EpistemicStatus::Unverifiable`,
  `Reason::RelevantExecutionUnavailable(snapshot_evidence_id)`,
  `InsufficiencyNote { relationship: Unverifiable }`, and the existing policy rule
  `(Completion, Unverifiable) => BlockAt(High)`. The synthesized assessment has exactly the
  shape `status.rs` rule 5 already emits.
- Added an optional `repository` section to the report carrying path, `HEAD`, branch, dirty
  flag, staged/unstaged/untracked porcelain, and a refusal reason that states replay would
  have inspected `HEAD`. No file contents, no secrets.
- Added a `repository_dirty` finding to the gate's report summary so the decision record
  explains itself. The full report is already embedded verbatim in `GateDecision.griller_report`,
  so no decision-record schema change was needed.
- Added 8 tests, including both attack shapes.

### Source changes

3 files, **+480 / −2**:

| File | + | − |
|---|---:|---:|
| `crates/griller-mcp/src/completion.rs` | 182 | 2 |
| `crates/griller-mcp/tests/completion_flow.rs` | 285 | 0 |
| `crates/griller-gate/src/report.rs` | 13 | 0 |

`status.rs`, `provenance.rs`, `worktree.rs`, `execution.rs`, `command.rs`, `config.rs`,
`validate.rs` and `griller-mcp/src/lib.rs` were **not** touched — verified with Git.

### Tests

`cargo fmt` PASS · `cargo check` PASS · `cargo clippy -D warnings` 0 warnings ·
`cargo test --workspace` **523 passed / 0 failed** (515 + 8) · `npx vitest run` **24/24**.

**Independently re-verified in an isolated OpenSandbox container.** Pristine `HEAD` scored
514 passed / 1 failed; the change scored **522 passed / 1 failed** — the same single
pre-existing failure (`self_grill_returns_limited_capability`, which fails identically on
unmodified `HEAD`). 514 + 8 = 522, so zero new failures in isolation.

`cargo fmt` and `cargo clippy` were verified on the host only: the container image lacks
those components and has no network to install them. Stated rather than glossed over.

### Security / trust result

**The trust boundary is now closed.** Proven, not assumed: with only the dirty predicate
disabled, 6 tests fail with `a dirty repository must never be VERIFIED`, and
`uncommitted_breakage_cannot_be_verified_from_a_passing_head` fails on exactly the
`VERIFIED` outcome the fix removes — over a `HEAD` that genuinely does produce supporting
evidence.

### Unresolved at the end of this phase

`fmt`/`clippy` sandbox coverage; `(Fact|Design, Unverifiable) => AlwaysWarn`; timeout
mismatch; process-group kill; silent-skip observability; 19 untracked documents.

---

## Phase 1.5 — Documentation and repository hygiene

**Commit:** `bfa1be0f2c810b928e176691660ae6b203039999`

**Outcome: complete.**

Documentation only. No production source change — proven by Git: the blob manifest of all
173 tracked non-`.md` files was identical before and after. 25 markdown files, +4767 / −0.

Reconciled the 19 recovery records (status banners, supersession markers, corrections to
false *current-state* claims while preserving historical measurements), and created
`CURRENT_STATE.md`, `PHASE_HISTORY.md`, `COMMIT_HISTORY.md`, `NEXT_PHASE.md`,
`EVIDENCE_STATUS.md` and `DOCS_STRUCTURE_PROPOSAL.md`.

**Unresolved at the end of this phase:** the proposed `docs/` structure was **not** applied
— the destination was not unambiguous and three genuine ambiguities are recorded in
`DOCS_STRUCTURE_PROPOSAL.md` §4. The push decision and the off-host backup policy question
were both still open.

---

## Phase 2 — Execution timeout and process cleanup

**Commit:** `0dcf9cd89c7de77ff2b22472e5a3fc94a267fb63`
**Parent:** `bfa1be0`

**Outcome: complete, not pushed.** Full record: [PHASE_2_STATE.md](PHASE_2_STATE.md).

### Source changes

| File | Change |
|---|---|
| `crates/griller-gate/src/config.rs` | `DEFAULT_TIMEOUT_MS` now **derived** from `griller_core::command::REPLAY_TIMEOUT_MS`, with a `const _` compile-time assertion |
| `crates/griller-gate/Cargo.toml` | added `griller-core` (to make the reference legal) |
| `crates/griller-runtime/src/execution.rs` | `isolate_process_group` (`setpgid(0,0)`) + `terminate_process_group` (`killpg(SIGKILL)`, `child.kill()` retained as fallback) |
| `crates/griller-runtime/src/lib.rs` | `#![forbid(unsafe_code)]` → `#![deny(unsafe_code)]` |
| `crates/griller-runtime/Cargo.toml` | added `libc` |
| `crates/griller-mcp/src/plan.rs` | test pinning the derived replay timeout |
| `docs/mcp.md`, `docs/security.md`, `docs/verification-pipeline.md` | corrected a "120 s cap" claim that never matched `MAX_TIMEOUT_MS` (600 s) |

`status.rs`, `matcher.rs`, `provenance.rs`, `worktree.rs`, `completion.rs` and the trigger
plugin were **not** touched.

### Tests

`cargo test --workspace` **532 passed / 0 failed** (523 + 9) · `npx vitest run` 24/24 ·
fmt clean · clippy 0 warnings. Isolated container: 531 + 1 = 532, the one failure being a
pre-existing environment-specific test in an unmodified file.

### Security / trust result

**No change to the trust model.** The dirty-tree boundary from Phase 1.4 is intact and all
8 of its tests still pass. Timeout remains fail-closed (`TimedOut`, `exit_code: None`,
non-passing) and no epistemic code was modified.

**A liveness bug was closed, and it was worse than first assessed.** With `killpg`
disabled, a surviving descendant inherited the piped `stdout`/`stderr`, so
`wait_with_output()` blocked **indefinitely past the timeout** — a gate invocation could
block forever. Proven by negative control: the regression test hangs (exit 124 at 45 s)
with `killpg` removed, and passes in under 2 s with it restored.

### Architectural changes accepted

- **`griller-runtime` relaxed from `forbid` to `deny(unsafe_code)`**, by explicit owner
  decision, for exactly one annotated `killpg` call. Negative control confirms any other
  `unsafe` in the crate is still a compile error. The other 7 crates keep `forbid`.
- **`libc` became a direct dependency**, which makes the property recorded in **Phase 1.1
  §1** — *"Zero third-party runtime dependencies"* — **inaccurate**. It is left in place
  and marked superseded rather than rewritten, because it was true when written. See
  [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md) §6.

### Unresolved at the end of this phase

Non-Unix descendant cleanup and liveness (**PLATFORM-LIMITED**, no CI outside Ubuntu);
descendants surviving a *successfully completing* command; outer/inner timeout margin
(deferred as a product decision); silent-skip observability; `NO_COMPLETION_ASSERTION`
design; `(Fact|Design, Unverifiable) => AlwaysWarn`; MCP exposure review; push decision;
off-host evidence backup.
