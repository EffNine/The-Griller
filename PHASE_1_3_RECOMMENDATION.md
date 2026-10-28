# PHASE_1_3_RECOMMENDATION.md

> **Status: Phase 1.3 recommendation. Its §2 defect is now CLOSED.**
>
> The trust-boundary defect described in §2 was implemented in `16c6852`
> (2026-09-27). Its ranking of next actions is superseded by
> [NEXT_PHASE.md](NEXT_PHASE.md); its current state summary by
> [CURRENT_STATE.md](CURRENT_STATE.md). Preserved as the record of the decision
> and of the evidence that motivated it.

Phase 1.3 — policy and preservation. **Nothing implemented, nothing committed, nothing
pushed.**

---

## 1. Is verification-gate safe enough to preserve as an active standalone project?

**Yes — to preserve. Not yet to trust with an agent's claims.**

The distinction is deliberate and is the substance of this document.

**Preservation case — strong:**

| Property | Evidence |
|---|---|
| Source recovered intact | 9 crates, 131 `.rs`, 32,885 lines; 6-commit history with a reachable remote; `HEAD` = `a360b7c` |
| Reproducible from cold | 4 CI gates locally verified; 515 Rust + 24 plugin tests green |
| Zero third-party runtime dependencies | 9 crates, 7 workspace deps, all internal or `serde`/`blake3`/`clap`/`thiserror`/`tempfile` — **SUPERSEDED in Phase 2**: `libc` is now a direct dependency of `griller-runtime` (already present transitively; `Cargo.lock` grew by 2 lines and no package was added). See [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md) §6 |
| Verdict core is sound | single `Verified` assignment site requiring direct support **and** zero contradiction; every failure mode maps to `Unverifiable`/`Unsupported` |
| Honesty discipline verified | `Enforcement::m1_none()` reports unsatisfied constraints truthfully; `health` advertises `not_enforced` ×3 and `secret_redaction: best_effort` |
| Field-proven | 131 real adversarial verifications, 74.8% refusals, with full adjudication embedded |
| Zero uncommitted work | tree clean since `a360b7c`; the 939-line at-risk body is now committed |

**Trust case — one unresolved defect, described in §2.** It is a *known, documented,
narrow* defect in an unregistered component. It is not a reason to archive; it is a reason
not to re-register the gate yet.

**Recommendation: keep it as an active project. Do not re-register the MCP server or install
the plugin until §2 is closed.**

---

## 2. The exact remaining trust-boundary defect

> **Invariant:** *An agent must never receive a VERIFIED result for code the verification
> engine did not actually inspect.*

**Currently violated.** Traced, not assumed:

- `completion.rs:197` — all three evidence collectors read **`req.repo`**, the user's tree.
  `SnapshotCollector` produces a `RepositorySnapshot` carrying `dirty`, `staged_changes`,
  `unstaged_changes`, `untracked_files`. `GitDiffCollector` produces the diff and porcelain.
  **The dirty state is captured and recorded.**
- `worktree.rs:94-141` — replay isolation is `git worktree add --detach <path>` with no
  commit-ish, i.e. a checkout of `HEAD`.
- `completion.rs:485-501` — replay sets `cmd.working_directory` to the worktree, overriding
  the `req.repo` assignment at line 260.
- `status.rs:153-249` — the verdict rules contain **no** reference to `dirty`, `is_clean`,
  `staged_changes`, `unstaged_changes`, or `untracked`.
- `matcher.rs` — `GitDiff` evidence is consumed **only** by `match_file_change` and
  `match_file_remove`. It is never consulted for a test-outcome claim.
- `context.rs:82` — the only `WorkingTreeState::Clean` literal in the codebase is inside
  `#[cfg(test)]`; no production collector populates `ProjectContext.working_tree_state`.

**Failure path:** agent commits nothing, edits `src/lib.rs` to break a test, claims *"All
tests pass."* Replay runs against clean `HEAD` → tests pass → **Direct** support → zero
contradicting evidence → `EpistemicStatus::Verified` → `VERIFIED_COMPLETION_PASS`. The
uncommitted diff sits in the evidence set, unweighed.

**And the companion claim is only partially true.** "agent changed files → verification runs
→ verification result" holds at steps 1 and 3, but step 2 verifies **`HEAD`**, not the change.
The result describes a tree the agent did not produce.

**Precise scope:** test-outcome and any other claim whose support comes from replay. File-change
and file-removal claims *do* consult `GitDiff` and are therefore sound. This is a
narrower and more precisely located defect than "the gate cannot see dirty state" — the gate
**sees** it and **ignores** it for the claims that matter most.

---

## 3. Should dirty verification be refused or supported?

> **Refused — Option A.** Full reasoning in `DIRTY_STATE_POLICY_DECISION.md` §6.

The short form, on the project's own evidence model:

1. `Unverifiable` + `Reason::RelevantExecutionUnavailable` already exist for exactly this case.
2. `RepositorySnapshot.dirty` is already collected from the user's tree by an already-registered collector. **Option A connects two things the project already has**; Option B requires a new representation of state the provenance model cannot express without either extending the canonical vocabulary or misrepresenting uncommitted content as environment evidence — the precise failure `Provenance::environment()` exists to prevent.
3. Option A preserves content-addressed reproducibility and re-verifiability. Option B trades them away for convenience.
4. It matches the project's own stated deferral ("dirty state is M2+") and its own honesty idiom (`Enforcement::NotEnforced`).
5. It is reversible; Option B creates a semantic dependency that is not.
6. Empirical pressure is **UNKNOWN** — the 131 records log `event.commit` but not tree cleanliness, and 92 target a vanished path. The reversible option is the right response to an unquantified risk.

**Stated against it:** Option A will block agents that work dirty, and that friction will
generate pressure to route around the gate. If that appears, the fix is the workflow (commit
before claiming), not the invariant. Option B should be reconsidered only on demonstrated
operational cost, and then ref-based with an explicit provenance member and a record-schema
migration.

---

## 4. What should be fixed next

Ranked by trust impact, then implementation risk, then reversibility, then dependency on
architectural decisions. No numerical scores.

### First — implement the dirty-tree refusal (Option A)

- **Trust impact:** the highest available. It closes the only currently-known path to a
  `VERIFIED` result for uninspected code.
- **Implementation risk:** **Low.** A branch where `SnapshotCollector`'s result is already
  available, plus wiring the snapshot into the report. No new vocabulary, no schema change.
  Reversible by deleting a branch.
- **Dependency on architectural decisions:** **none.** It is the decision this phase just
  made. It is implementable immediately and blocks nothing.
- **Verification:** a test that constructs a dirty tree, runs `check_completion`, and asserts
  `Unverifiable` with the new reason code; plus the existing 515/24 suites unchanged.

### Second — make the decision record state what was verified

- **Trust impact:** high and adjacent. Even with Option A, a reader of a record should be able
  to see which tree produced the verdict. Today a `VERIFIED` record and a
  `dirty-tree-refused` record are distinguishable by reason code, but nothing states the
  replayed commit was verified *as a commit*.
- **Implementation risk:** **Low.** `event.commit` is already recorded; surfacing the snapshot
  in the report is additive.
- **Dependency:** none.
- **Verification:** a record-shape test asserting the replayed commit and the tree state are
  both present.

### Third — the gate/replay timeout mismatch (120 s vs 600 s)

> **CLOSED in Phase 2 (`0dcf9cd`).** The gate default is now derived from
> `griller_core::command::REPLAY_TIMEOUT_MS` with a compile-time assertion. The reasoning
> below is preserved as written. See [TIMEOUT_SEMANTICS.md](TIMEOUT_SEMANTICS.md).

- **Trust impact:** **none** — availability only, and it fails closed (`unavailable`).
- **Implementation risk:** **Low**, but it changes agent-visible latency, so the gate's
  latency budget is a judgement call.
- **Dependency:** none. Derive the constant from `griller_core::command::REPLAY_TIMEOUT_MS`
  rather than hard-coding, so it cannot diverge again.
- **Verification:** a test asserting `gate::DEFAULT_TIMEOUT_MS >= core::REPLAY_TIMEOUT_MS`.

### Fourth — process-group termination on timeout

- **Trust impact:** none directly. `killpg`/`setpgid`/`setsid` are absent; `cargo`'s
  `rustc` grandchildren survive a timeout and can perturb a *later* verification. This is a
  **reproducibility** defect, which matters here because reproducibility is the property the
  whole evidence model rests on.
- **Implementation risk:** **Low.** `setpgid` in `pre_exec` plus `killpg` on timeout.
- **Dependency:** none.
- **Verification:** spawn a shell that backgrounds a long grandchild, force a timeout, assert
  no descendant survives.

---

## 5. What should explicitly NOT be fixed yet

| Item | Why not now |
|---|---|
| **Option B — carry the delta** | Rejected in favour of A. Premature; revisit only on demonstrated operational cost, and then with a provenance member and a schema migration. |
| **`NO_COMPLETION_ASSERTION` decision record** | Real observability gap, but the *plugin* cannot author a record — records are written by the `griller-gate` binary. That needs a new subcommand: larger than the terse fix, and not a trust-boundary item. Do it after the trust fix. <br>**Update:** implemented in `e575ab7` (2026-09-30) — and it did **not** need a new subcommand. The plugin reports its extraction state and the gate authors the record through the existing single persistence path. See [COMPLETION_TRIGGER_OBSERVABILITY_RECON.md](COMPLETION_TRIGGER_OBSERVABILITY_RECON.md). |
| **MCP re-registration / plugin installation** | The §2 defect is open. Re-registering the gate is what would make it reachable by an agent. Fix first. <br>**Update:** §2 is now CLOSED (`16c6852`), but the gate remains deliberately unregistered. Re-registration is now gated on the review in [NEXT_PHASE.md](NEXT_PHASE.md) step 5, not on the trust fix. |
| **Integration with CodeBro or Conductor** | Explicitly out of scope for this phase, and premature while the trust boundary is open. <br>**Update:** the trust boundary is now closed, but this remains out of scope and is not scheduled. |
| **The 4 unimplemented attack kinds** (`Completeness`, `Invert`, `Robustness`, `RequirementCoverage`) | Already fail-closed — rejected as `INVALID_REQUEST`, never silently dropped. `AdversariallyVerified` is therefore unreachable, which is a *capability* gap, not a safety gap. |
| **Off-host evidence backup** | Blocked on a policy decision about whether session-level absolute paths may leave the machine. 2.9 MB; cheap once decided. |
| **Committing the 15 recovery/review documents** | Three contain claims that `a360b7c` made stale. Correct first, then decide placement. |
| **Any wider `/projects` cleanup** | Out of scope. |

---

## 6. State at the end of Phase 1.3

| Property | Value |
|---|---|
| `HEAD` | `a360b7c6487fd41f163b9d3fad13e05bac9696b0` |
| Tracked working tree | **clean** |
| Unpushed | 1 commit — **not pushed** |
| Untracked | 20 markdown files (14 recovery/review + 6 Phase 1.3 artifacts) |
| Gates | fmt ✓ · check ✓ · test 515/0 ✓ · clippy ✓ · vitest 24/24 ✓ |
| Historical evidence | untouched; same-disk backup byte-identical (131/131, aggregate `6303dc42…`) |
| Production source modified | **none** |
