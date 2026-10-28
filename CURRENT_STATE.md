# CURRENT_STATE.md

**The current, authoritative state of `active/verification-gate`.**

Every statement here describes the repository as it stands now. For the reasoning that
produced this state, see [PHASE_HISTORY.md](PHASE_HISTORY.md) and the per-phase documents
it lists. Where an older document disagrees with this one, **this file is correct** and the
older document is a point-in-time record that has been marked as superseded.

---

## 1. Repository identity

| Field | Value |
|---|---|
| Path | `/home/afnan/projects/active/verification-gate` |
| Recovered from | `/home/afnan/projects/archive/legacy/the-griller` (untouched) |
| `HEAD` | `e575ab757ff1ed964599ac9f3d658fd9a89a2008` |
| `HEAD` subject | `feat(griller): record no-completion-assertion outcomes` |
| Parent | `df87efa6c00eb97bfcfeb574bed4b5e8304b3a31` |
| Remote | `origin` → `https://github.com/EffNine/The-Griller.git` |
| Commits ahead of `origin/main` | **6** (`a360b7c`, `16c6852`, `bfa1be0`, `0dcf9cd`, `df87efa`, `e575ab7`) — **not pushed** |
| Working tree | **clean** — 0 modified, 0 staged |
| Untracked | **0** — the 19 recovery/review records and the 6 Phase 1.5 documents were added to version control in `8800a0b` |

The two unpushed commits are deliberate. `github.com/EffNine/The-Griller` has not been
confirmed as the intended long-term home for this work, so nothing has been pushed.

---

## 2. Verification status

| Gate | Command | Result |
|---|---|---|
| Format | `cargo fmt --all -- --check` | **PASS** (exit 0) |
| Compile | `cargo check --workspace` | **PASS** (exit 0, 0 warnings) |
| Lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** (0 warnings) |
| Rust tests | `cargo test --workspace` | **PASS** — **541 passed, 0 failed**, 6 ignored, 36 test binaries |
| Plugin tests | `npx vitest run` | **PASS** — **31 / 31** |
| Cold build | from empty `target/` | **PASS** (verified in Phase 1; not re-run since) |

**Rust test count history:** 515 at recovery → 515 after `a360b7c` (the recovered work
added tests and the Phase 1.2 fix pinned more) → 523 after `16c6852` (+8 dirty-state tests)
→ 532 after `0dcf9cd` (+9 execution tests: 5 process-group, 1 stdin isolation, 1
replay-timeout, 2 gate timeout invariant) → **541** after `e575ab7` (+9 no-assertion tests,
`n1`–`n9`).

**Plugin test count history:** 23 at recovery → 24 after `a360b7c` → 24 after `0dcf9cd`
(unchanged) → **31** after `e575ab7` (+7 decision-handling tests, the first tests of that
boundary).

> Two tracked documents still state **424** tests. Both describe the M8 field-validation
> run and are historically accurate for that point in time, so they were left alone:
> `docs/testing.md:9` and `docs/history/index.md:26`. They are not current-state claims.

---

## 3. Dirty-tree policy — CLOSED

**Policy: a dirty repository is `UNVERIFIABLE`.** Option A of
[DIRTY_STATE_POLICY_OPTIONS.md](DIRTY_STATE_POLICY_OPTIONS.md), decided in
[DIRTY_STATE_POLICY_DECISION.md](DIRTY_STATE_POLICY_DECISION.md), implemented in
`16c6852`.

`run_completion_check` reads the `RepositorySnapshot` that `SnapshotCollector` already
collects from the user's working tree, and when `dirty == true` returns **before any attack
is planned or executed**. The synthesized assessment is forced to
`EpistemicStatus::Unverifiable`; the existing policy table maps
`(Completion, Unverifiable)` at `Severity::High` to `BLOCK` with reason code
`UNVERIFIABLE_COMPLETION_BLOCK`.

No new status, verdict, or reason vocabulary was introduced. The verdict core
(`crates/griller-adjudicate/src/status.rs`) was not modified.

**The invariant now holds:** *no `VERIFIED` result for code the verification engine did not
actually inspect.* This was proven, not assumed — with the dirty predicate disabled the new
security tests fail with `a dirty repository must never be VERIFIED`.

---

## 4. Historical evidence — PRESERVED, single-disk

| Property | Value |
|---|---|
| Location | `~/.local/share/griller/gate/` (outside version control, by design) |
| Decision records | **131** |
| `gate.log` | **133 lines**, 13,689 bytes |
| Record schema | `g1.0` (131/131) |
| `gate.log` mtime | `2026-08-25 20:49:49.107476018 +0800` — **unchanged since production** |
| Modified during recovery | **No** |
| Same-disk backup | `~/.local/share/griller/evidence-backup-20260927T131225Z/` |
| Backup fidelity | per-file SHA256 identical, 132/132 |
| Aggregate hash (path-normalised hash-of-hashes) | `6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817` |
| Same physical device? | **Yes** — `stat -c %d` returns `66306` for both, on `/dev/nvme0n1p2` |

**There is no independent or off-host backup.** The backup protects against accidental
deletion only. It provides no protection against disk failure, corruption, or host loss.
See [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md).

---

## 5. Defect register

### CLOSED

| Defect | Closed in | How |
|---|---|---|
| Trigger fail-open: terse completion assertions (`"Done."`, `"It's ready."`) returned no claims and silently skipped | `a360b7c` | Two rules added before `return false` in `looks_like_completion_assertion`; 25 positives and 2 injections pinned by a regression table |
| **Dirty-tree trust boundary:** replay inspected `HEAD`, so `VERIFIED` could describe uninspected code | `16c6852` | Dirty tree ⇒ `UNVERIFIABLE` before any attack runs |
| 16 files of uncommitted recovered work at risk of loss | `a360b7c` | Committed |
| **Gate default timeout (120 s) smaller than the replay budget it wrapped (600 s)** | `0dcf9cd` | Gate default **derived** from `griller_core::command::REPLAY_TIMEOUT_MS`, plus a `const _` compile-time assertion. See [TIMEOUT_SEMANTICS.md](TIMEOUT_SEMANTICS.md) |
| **Timeout killed only the direct child; descendants survived** | `0dcf9cd` | `setpgid(0,0)` on spawn; `killpg(SIGKILL)` on timeout, `child.kill()` retained as fallback. See [PROCESS_GROUP_SEMANTICS.md](PROCESS_GROUP_SEMANTICS.md) |
| **Liveness: a surviving descendant held the piped stdout/stderr open, so `wait_with_output()` blocked indefinitely past the timeout** | `0dcf9cd` | Same fix. Proven by negative control: with `killpg` removed the regression test hangs (exit 124 at 45 s on host, and in an isolated container) |
| No test for descendant cleanup | `0dcf9cd` | `crates/griller-runtime/tests/process_group.rs` — 5 tests, three independent liveness signals |
| No test for stdin isolation | `0dcf9cd` | `stdin_is_closed_so_a_reading_command_gets_eof` |
| No test pinning the derived replay timeout | `0dcf9cd` | `derived_replay_command_carries_the_canonical_replay_timeout` |
| Tracked docs stated a "120 s cap" that never matched `MAX_TIMEOUT_MS` (600 s) | `0dcf9cd` | `docs/mcp.md`, `docs/security.md`, `docs/verification-pipeline.md` corrected |
| **Trigger silent-skip left no auditable record** | `e575ab7` | The plugin now reports its extraction state and the gate persists a dedicated non-passing `GateDecision` (`status: no_completion_assertion`, `reason_code: NO_COMPLETION_ASSERTION`) through the **existing single persistence path**. The Griller is not invoked, and **no agent-facing injection occurs for this status**. Detail: [COMPLETION_TRIGGER_OBSERVABILITY_RECON.md](COMPLETION_TRIGGER_OBSERVABILITY_RECON.md) |

### OPEN

| Defect | Location | Effect |
|---|---|---|
| 4 of 8 attack kinds unimplemented | `crates/griller-mcp/src/plan.rs` (`IMPLEMENTED_ATTACKS`) | `Completeness`, `Invert`, `Robustness`, `RequirementCoverage` are rejected as `INVALID_REQUEST`, never silently dropped. Fail-closed, but `AdversariallyVerified` is unreachable. **Capability** gap, not a safety gap. |
| `(Fact, Unverifiable)` and `(Design, Unverifiable)` map to `AlwaysWarn` | `crates/griller-policy/src/table.rs:103,112` | A dirty tree with an explicit non-completion claim kind yields `WARN`, not `BLOCK`. Pre-existing policy, unreachable via the plugin (which always uses `Completion`), reachable by a direct MCP caller. |
| No kernel-level isolation | `Enforcement::m1_none()` | Truthfully reported as `NotEnforced` for network isolation and OS sandboxing. Honest by design, not a defect. |
| `e2e_mock_opencode_griller.py` cannot pass as written | `timeout=30` vs 600 s budget | Never run; requires a live MCP server. |
| `~/.config/griller/gate.json` points at a non-existent path | `~/.config/griller/` | Fails closed. Not modified. |

### PLATFORM-LIMITED

**Verified on Linux only.** Full detail in
[PLATFORM_EXECUTION_LIMITATIONS.md](PLATFORM_EXECUTION_LIMITATIONS.md).

| Item | Linux | Non-Unix |
|---|---|---|
| Process-group creation and `killpg` teardown | **PROVEN** (host + isolated container) | **NOT IMPLEMENTED** — direct-child fallback (`execution.rs:203`) |
| Descendant termination | **PROVEN** | **NOT PROVEN** |
| Timeout liveness (`execute` returns) | **PROVEN** | **NOT PROVEN** — the stdout/stderr pipe-inheritance mechanism is not platform-specific, so the pre-Phase-2 hang is expected to persist there |
| Second-run isolation from orphans | **PROVEN** | **NOT PROVEN** |

CI covers `ubuntu-latest` only. macOS/BSD take the same `cfg(unix)` branch but are untested.
**This is not a claim that non-Unix is broken — it is unverified, and its code shape is the
pre-Phase-2 shape.** No cross-platform supervision was implemented or is proposed.

### ARCHITECTURAL CHANGES ACCEPTED (not open defects)

Recorded so they are not rediscovered as surprises.

| Change | Consequence | Detail |
|---|---|---|
| `griller-runtime` moved `#![forbid(unsafe_code)]` → `#![deny(unsafe_code)]` | That crate no longer has an *absolute* no-unsafe guarantee. One annotated function; negative control confirms any other `unsafe` is a compile error. The other 7 crates keep `forbid`. | [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md) |
| `libc` added as a direct dependency of `griller-runtime` | The recorded property *"zero third-party runtime dependencies"* is now **inaccurate**. `libc` was already a transitive dependency; the `Cargo.lock` delta is +2 lines and no new package was added. | [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md) §6 |
| Gate outer envelope now **equals** the inner replay budget | A replay consuming the full 600 s could reach both deadlines together and truncate the *reporting* edge. A margin would fix it; that is a product decision, deliberately not taken. | [TIMEOUT_SEMANTICS.md](TIMEOUT_SEMANTICS.md) §4-5 |

### DEFERRED

| Item | Why deferred |
|---|---|
| Option B — carry the working-tree delta into the replay worktree | Rejected in favour of Option A. It would trade away content-addressed reproducibility and require a new provenance representation. Reconsider only on demonstrated operational cost of Option A. |
| Decision-record transparency (naming the replayed commit and tree state in the record) | The report already carries a `repository` section; surfacing it in the human-readable record summary beyond the `repository_dirty` finding was judged out of scope for this phase. |
| Off-host evidence backup | Blocked on a policy decision: 92 of 131 records embed absolute repository paths, and whether session-level paths may leave the machine is the owner's call. |
| Repository history squashed into one initial commit | Recorded in `.git/logs/HEAD`; unrecoverable from this repository. Not a task. |

### NOT STARTED

| Item |
|---|
| MCP server registration (`~/.config/opencode/opencode.jsonc` — `griller` appears **0** times) |
| OpenCode plugin installation (`~/.config/opencode/plugins/` contains **no** `griller-gate`) |
| Integration with CodeBro, Conductor, Infra, or computer-use |
| MonkeyLab revival |

---

## 6. Registration and exposure status

| Surface | State |
|---|---|
| MCP server registered | **No** — `griller` appears 0 times in `~/.config/opencode/opencode.jsonc` |
| Plugin installed | **No** — `~/.config/opencode/plugins/` contains only `codebro-context` |
| Gate running | **No** |
| Project connected to an orchestrator | **No** |

**The component is not reachable by any agent.** This is the single most important
operational fact in this file: the trust boundary in §3 is correct, but nothing is
currently exposed to the failure it would protect against.

---

## 7. Documentation status

| Item | State |
|---|---|
| Tracked project documentation | 27 files (`README.md`, 15 `docs/*.md`, 11 `docs/history/*.md`) — current as of `16c6852` |
| Recovery/review records | **25 files at the repository root, tracked as of `8800a0b`** — previously untracked and unprotected. Each carries a status banner naming what it records and what supersedes it |
| Proposed structure | [DOCS_STRUCTURE_PROPOSAL.md](DOCS_STRUCTURE_PROPOSAL.md) — **proposed, not applied.** No file was moved, renamed or deleted. The destination was not unambiguous, and three genuine ambiguities are documented there |
| Documents corrected in Phase 1.5 | All 19 prior records carry a status banner; false *current-state* claims in `RECOVERY.md`, `RECOVERY_MANIFEST.md`, `NEXT_ACTIONS.md`, `BASELINE_VERIFICATION.md`, `PHASE_1_1_RECOMMENDATION.md` and `PHASE_1_3_RECOMMENDATION.md` were corrected in place with supersession markers |
| Historical measurements preserved | Yes — the 515/0/6, 23/23 and 424-test figures are left intact where they describe a point in time, including in the two tracked files that state 424 |
| `README.md` documentation table | **Not updated.** It links `docs/` paths that do not yet exist for these documents; updating it before the structure is applied would create broken links |

---

## 8. Maintenance rule for this file

This file describes **now**. When a commit changes the state it describes, update it in the
same commit. Per-phase reasoning belongs in the phase documents; do not duplicate it here.
