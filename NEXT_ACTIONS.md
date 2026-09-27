# NEXT_ACTIONS.md

> **Status: Phase 1 action list (RECOVERY). Actions 1, 2 and 3 are COMPLETE.**
>
> Preserved as written because the actions explain why the later work happened.
> The current engineering sequence is [NEXT_PHASE.md](NEXT_PHASE.md); the
> current state is [CURRENT_STATE.md](CURRENT_STATE.md).

Phase 1 (RECOVERY) is complete. This file records the component boundary classification
(STEP 8) and the five actions that follow.

**State: preserved, buildable, testable, independently trustworthy. NOT registered, NOT
running, NOT integrated, NOT production-ready.**

---

## STEP 8 — Recovery boundary

### KEEP AS-IS (no change; verified correct or load-bearing)

| Component | Why |
|---|---|
| `crates/griller-adjudicate/src/status.rs` | The verdict core. Total function over `(supports.is_empty(), contradicts.is_empty())`, 4 exhaustive arms, **one** `Verified` site requiring direct support + zero contradiction. No failure path reaches `Verified`. |
| `crates/griller-core/src/provenance.rs` | `Provenance::environment()` forces `directly_observed=true` / `depends_on_agent_claim=false`. The "agent claim never silently upgraded" invariant is enforced at the constructor. |
| `crates/griller-core/src/command.rs` `Enforcement` | Truthfully reports `NotEnforced` for unsatisfied policy. No over-claiming. |
| `crates/griller-mcp/src/validate.rs` | Input bounds. `"a verification layer, not a data sink"`. `MAX_TIMEOUT_MS` correctly aligned to `REPLAY_TIMEOUT_MS`. |
| `crates/griller-mcp/src/lib.rs` `health` | Self-reports `network_isolation: "not_enforced"`, `os_sandbox: "not_enforced"`, `privilege_reduction: "not_enforced"`, `secret_redaction: "best_effort"`. Exemplary. |
| `crates/griller-gate/src/config.rs` `ExpectedServer` | Version/protocol/schema/tool contract; **fails closed** on mismatch. Proven by 2 real `STALE_SERVER` records. |
| `crates/griller-runtime/src/worktree.rs` | Fresh `tempfile::tempdir()` per call, `flock` replay lock, `git worktree add --detach`. No cross-run contamination. |
| `.github/workflows/ci.yml` | Four real gates, all locally verified satisfiable. |
| `docs/limitations.md`, `docs/history/worktree-isolation.md` | **Accurate.** They state the dirty-state limit correctly. The external `SYSTEM_ARCHITECTURE.md` overstates; the project's own docs do not. |
| `~/.local/share/griller/gate/` (131 records) | Irreplaceable field evidence. Do not mutate, normalise, or delete. |

### ADAPT (needed before any exposure; not started)

| Component | Needed change | Blocked on |
|---|---|---|
| `crates/griller-runtime/src/worktree.rs` | Dirty-state carry-in, or a distinct fail-closed state when the tree is dirty | A policy decision on what "verified" means |
| `crates/griller-gate/src/config.rs:10` | `DEFAULT_TIMEOUT_MS` 120 s → derived from `REPLAY_TIMEOUT_MS` (600 s) | Review of the gate latency budget |
| `crates/griller-runtime/src/execution.rs:189` | `child.kill()` → process-group kill (`setpgid` + `killpg`) | None — mechanical, but changes runtime behaviour |
| `crates/griller-mcp/src/lib.rs:813-827` | `spawn_blocking` around the synchronous pipeline | tokio runtime change |
| `.opencode/plugins/griller-gate.ts` | Review + commit the uncommitted completion-assertion work | **DONE in `a360b7c`** — reviewed in Phase 1.1, fixed, committed |
| 16 uncommitted files | Commit | Action 1 — **DONE in `a360b7c`** |

### DEPRECATE (works, but should not be relied on)

| Component | Why |
|---|---|
| `AttackKind::{Completeness, Invert, Robustness, RequirementCoverage}` | 4 of 8 unimplemented. **Rejected as `INVALID_REQUEST`, never silently dropped** — fail-closed, but `AdversariallyVerified` is therefore unreachable. |
| `e2e_mock_opencode_griller.py` | `timeout=30` vs a 600 s budget. Cannot pass as written. |
| `~/.config/griller/gate.json` | Points at `/home/afnan/projects/active/the-griller` — a non-existent path. Fails closed. |
| `~/.local/bin/griller-{gate,mcp}` | Orphaned binaries from the unregistered deployment. |
| `docs/testing.md:9` ("424 tests") | Historical M8 figure; current tree executes 515. Docs lag code. |

### UNKNOWN (cannot be determined in Phase 1)

| Item | Why unknown |
|---|---|
| Live MCP round-trip behaviour | No server was started (registration is out of scope) |
| Whether the 120 s gate default was ever hit in production | Requires correlating `timing.total_ms` across 131 records; not done |
| Whether any of the 131 decisions were made against a **dirty** tree | `event.commit` is recorded; tree cleanliness at decision time is **not** |
| Real-world acceptance of `board` / `vm` / `kernel` / `dast` / `browser` | Self-documented `DEFERRED`; no contradicting or confirming observation |
| Granular M0–M8 history | Squashed into one initial commit; not recoverable from this repository |
| Whether the 6 ignored doctests ever asserted anything | Macro-generated schema illustrations |

---

## 5 Actions

> **Action status (added 2026-09-27).** This file is the **Phase 1** action list and its
> five actions are preserved below as written, because they explain why the later work
> happened. Three of the five are now **COMPLETE**:
>
> | Action | Status | Done in |
> |---|---|---|
> | 1. Commit the 16 uncommitted files | **COMPLETE** | `a360b7c` |
> | 2. Back up `~/.local/share/griller/gate/` | **COMPLETE** — same-disk copy only; no off-host backup | Phase 1.1 |
> | 3. Decide and document the dirty-state policy | **COMPLETE** — Option A chosen | Phase 1.3, implemented in `16c6852` |
> | 4. Fix the gate timeout mismatch | **OPEN** | — |
> | 5. Add process-group termination | **OPEN** | — |
>
> The current engineering sequence is [NEXT_PHASE.md](NEXT_PHASE.md). The current state is
> [CURRENT_STATE.md](CURRENT_STATE.md).

### 1. Commit the 16 uncommitted files to the recovered repository
> **COMPLETE — done in `a360b7c` (2026-09-27), with the Phase 1.2 fix in the same commit.**
> Preserved below as the original reasoning. See
> [COMMIT_HISTORY.md](COMMIT_HISTORY.md) for what that commit actually contains.
- **Why** — 939 lines of **complete, tested** work (23/23 plugin tests, 515/515 Rust) exist only
  in a working tree. It fixes a real measured production defect (39 `UNSUPPORTED_COMPLETION_BLOCK`
  records from the trigger matching bare nouns). It is currently uncommitted in a project that
  was archived. One `disk failure` or one careless `git checkout` destroys it.
- **Affected component** — `active/verification-gate` working tree; 16 files
  (`.opencode/plugins/griller-gate{,.test}.ts`, `griller-{context,adjudicate}/src/normalizer.rs`,
  `griller-core/src/command.rs`, `griller-mcp/src/completion.rs`, 2 golden test files,
  `griller-core/Cargo.toml`, `griller-cli/src/commands.rs`, `griller-mcp/src/plan.rs`,
  `griller-mcp/src/validate.rs`, `griller-context/src/task.rs`, `Cargo.lock`, 2 docs).
- **Expected risk** — **LOW.** The diff is already fully verified green. Residual risk is
  *semantic*, not mechanical: it changes the gate trigger from **fail-closed** (inject
  "unavailable" on extraction failure) to **silent-skip**. That is defensible (the old
  behaviour injected a notice onto every technical discussion) and is confined to the
  trigger layer — the Rust verdict engine is untouched and still fail-closed. It should
  nonetheless be **reviewed as a deliberate security decision**, not merged silently.
- **Verification required** — `cargo fmt --check`, `cargo check --workspace`,
  `cargo test --workspace` (expect 515/0/6), `cargo clippy -- -D warnings`, `npx vitest run`
  (expect 23/23) all re-run **after** committing; plus a human reading of the
  fail-closed → silent-skip diff in `docs/gate.md` and `griller-gate.ts:370-380`.
- **Note** — do **not** push to `origin` without confirming the user still wants
  `github.com/EffNine/The-Griller` to be the home of this work.

### 2. Back up `~/.local/share/griller/gate/` (131 records, 3.1 MB)
- **Why** — the only empirical proof the component ever worked in anger: 131 real
  adversarial verifications with embedded evidence, 74.8% of them **refusals**. It is not in
  git, not backed up, and the project it documents was archived. Trivially small.
- **Affected component** — `~/.local/share/griller/gate/{decisions/,gate.log}`.
- **Expected risk** — **NONE** if done as a copy. **Do not move or mutate the originals.**
- **Verification required** — byte-identical copy (compare file count 131 + 133-line log, and
  a checksum of the directory); originals unmodified.

### 3. Decide and document the dirty-state policy (do not implement yet)
- **Why** — the project's most consequential limitation. `git worktree add --detach` with no
  commit-ish means **an agent's uncommitted edits are invisible to verification**. The
  workspace's `SYSTEM_ARCHITECTURE.md:30-32` claims the opposite. The project's own docs are
  correct. This is a definition question: does "verified" mean *the commit is green* or
  *the working tree is green*? Only a human should answer.
- **Affected component** — `crates/griller-runtime/src/worktree.rs:94-141`; documentation
  in `~/projects/SYSTEM_ARCHITECTURE.md:29-33,47` and `docs/limitations.md`.
- **Expected risk** of deciding wrong — **HIGH and asymmetric.** Option (a) *refuse to verify
  a dirty tree* is fail-closed and local. Option (b) *carry the delta in* changes the trust
  model and must handle untracked files, binary patches, and `.gitignore` semantics. Do not
  pick (b) by default.
- **Verification required** — after a decision: a test that creates a dirty tree, runs a
  verification, and asserts the documented behaviour. Plus correction of the external
  architecture doc either way.

### 4. Fix the gate timeout default (120 s → derived from `REPLAY_TIMEOUT_MS`)
- **Why** — a legitimate cold replay needing >120 s is cut off and reported
  `unavailable` / `CHECK_COMPLETION_FAILED`. The production record contains 8
  `CHECK_COMPLETION_FAILED` and 10 `unavailable` outcomes; a previously-deployed config had
  already overridden the default to 600000 rather than fixing it. A fresh install hits the
  default. Availability defect — it fails closed, so it is not a soundness bug.
- **Affected component** — `crates/griller-gate/src/config.rs:10`. `griller-gate` already
  depends on `griller-core`, so `REPLAY_TIMEOUT_MS` can be referenced without a new
  dependency.
- **Expected risk** — **LOW-MEDIUM.** Mechanically trivial; the risk is that a longer default
  timeout makes the gate slow to fail, which changes agent-visible latency. Deriving the
  constant rather than hard-coding it prevents recurrence.
- **Verification required** — a test asserting `gate::DEFAULT_TIMEOUT_MS >=
  core::REPLAY_TIMEOUT_MS`; re-run the full suite; confirm `docs/gate.md` matches.

### 5. Add process-group termination on timeout
- **Why** — `child.kill()` at `execution.rs:189` signals the direct child only. For
  `cargo test` that is `cargo`; its `rustc` and test-binary grandchildren are **not**
  signalled and are reparented to init. Orphans can hold file locks and consume CPU, making a
  **later** verification behave differently — a reproducibility defect. Already a roadmap
  item before this recovery.
- **Affected component** — `crates/griller-runtime/src/execution.rs:181-199`.
- **Expected risk** — **LOW.** `setpgid` in `pre_exec` + `killpg(SIGKILL)` on timeout is
  mechanical. Second-order risk: a PID/PGID reuse race, which is negligible here because the
  group is created at spawn and the window is milliseconds.
- **Verification required** — a test that spawns a shell which backgrounds a long-running
  grandchild, forces a timeout, and asserts no descendant survives. Then re-run the full
  suite.

---

## Explicitly out of scope for the next phase

Re-registering the MCP server or installing the plugin (a separate, deliberate decision);
integrating with CodeBro or Conductor; fixing the E2E harness; implementing the 4 missing
attack kinds; any history rewrite; any dependency upgrade; any mutation of the production
evidence.
