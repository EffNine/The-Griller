# NEXT_PHASE.md

**The remaining engineering sequence.** Nothing here is implemented.

For what is already done, see [PHASE_HISTORY.md](PHASE_HISTORY.md). For the current state,
see [CURRENT_STATE.md](CURRENT_STATE.md). For the earlier, broader list this supersedes, see
[NEXT_ACTIONS.md](NEXT_ACTIONS.md) — its Actions 1, 2 and 3 are complete.

Items are ordered by dependency: each step assumes the previous ones are done. Within a
step, the order given is the recommended one. No priority scores are used.

---

## Precondition: two decisions that are not engineering

These block or reshape the work below and are the owner's to make, not the implementer's.

1. **Should this work be pushed to `github.com/EffNine/The-Griller`?** Two commits
   (`a360b7c`, `16c6852`) are unpushed. The remote's suitability as a long-term home has
   never been confirmed. If the answer is no, the repository needs a new remote, and the
   two commits' composition should be described in whatever transfer happens — see
   [COMMIT_HISTORY.md](COMMIT_HISTORY.md).
2. **May session-level absolute repository paths leave this machine?** 92 of the 131
   decision records embed them. This determines whether an off-host evidence backup is
   possible at all. See [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md) §5.

Neither blocks steps 1–4 below. Both block step 5.

---

## 1. Gate timeout default mismatch

**Defect.** `crates/griller-gate/src/config.rs:10` sets
`DEFAULT_TIMEOUT_MS = 120_000`; `crates/griller-core/src/command.rs:21` sets
`REPLAY_TIMEOUT_MS = 600_000`. A replay that legitimately needs more than two minutes is
abandoned at the gate.

**Effect:** availability only. It fails **closed** — the gate reports
`GATE_UNAVAILABLE` rather than a false pass. No trust-boundary impact.

**Change.** Derive the gate default from `griller_core::command::REPLAY_TIMEOUT_MS` rather
than hard-coding a second constant, so the two cannot diverge again.

**Verification.** A test asserting `gate::DEFAULT_TIMEOUT_MS >= core::REPLAY_TIMEOUT_MS`, so
the invariant is enforced rather than documented.

**Open question, not decided here:** this changes agent-visible latency. Whether the gate
should wait 10 minutes or fail fast at 2 minutes is a product judgement about the gate's
latency budget, not a mechanical fix. Resolve that before implementing.

---

## 2. Process-group termination

**Defect.** `crates/griller-runtime/src/execution.rs:189` calls `child.kill()`, which
signals only the direct child. `setpgid`, `killpg` and `setsid` occur **0** times in the
codebase. `cargo`'s `rustc` grandchildren survive a timeout.

**Effect:** a **reproducibility** defect, not a verdict defect. An orphaned compiler can
keep writing into a worktree and perturb a *later* verification. This matters more than it
would elsewhere because the whole evidence model rests on content-addressed reproducibility.

**Change.** `setpgid` in `pre_exec`, then `killpg` on timeout. The process must become a
group leader so the whole tree can be signalled.

**Verification.** Spawn a shell that backgrounds a long-lived grandchild, force a timeout,
assert no descendant survives.

**Note.** `pre_exec` runs after `fork` and must be async-signal-safe. Only
`setsid`/`setpgid`/`exec` belong there — no allocation, no Rust std calls.

---

## 3. Execution cleanup tests

Follows step 2, and is a separate deliverable: the current execution tests do not cover
descendant lifetime at all, so step 2 could regress silently.

Cover at minimum:

- a timeout with a surviving grandchild (the step-2 regression test)
- a normal exit that still leaves a detached descendant
- `stdin` is `null`, so a command that reads stdin gets EOF rather than hanging
- the temp worktree is removed even when execution fails or times out
  (`run_command_in_isolation` calls `wt.remove()` on both paths today — assert it, do not
  assume it)

---

## 4. Residual trigger observability

**Defect.** `.opencode/plugins/griller-gate.ts:393-397`: when a message contains no
completion assertion, the gate logs to the application log and returns. **No decision
record is written.** A skip is therefore indistinguishable, in the record store, from a
verification that was never requested.

This is a real observability gap and it is the residual half of the Phase 1.1 FAIL-OPEN
classification. The predicate itself is now fail-closed for all 27 measured forms; what
remains is that an unforeseen phrasing is *silent*.

**The obstacle, which is why this is not a small change:** the proposed
`NO_COMPLETION_ASSERTION` record **cannot be authored by the plugin**. Decision records are
written by the `griller-gate` binary, and there is no subcommand to record a non-event.
Implementing it means either a new gate subcommand or a new record kind — a change to the
gate's write path, not to the plugin.

Scope it before building: decide whether the record is a new `GateStatus` variant, a new
`EvidenceKind`, or a lifecycle event without a record. Each has different blast radius
against the 131 existing records and the `g1.0` schema.

**Verification.** An end-to-end run producing a skip that is visible in the record store,
plus a check that existing record parsing is unaffected.

---

## 5. MCP exposure review

**Not a task to start yet — a review to perform before the gate is reachable by an agent.**

The component is currently unregistered: `griller` appears 0 times in
`~/.config/opencode/opencode.jsonc`, and no `griller-gate` plugin is installed in
`~/.config/opencode/plugins/`. Nothing can reach the trust boundary that Phase 1.4 closed,
which is the correct state while defects 1–4 remain open.

Before registering, the review must cover:

- **Is the trust boundary sufficient?** Phase 1.4 closed the dirty-tree case. Confirm no
  other path lets `VERIFIED` describe uninspected code.
- **The low-stakes claim-kind gap.** `(Fact, Unverifiable)` and `(Design, Unverifiable)`
  both map to `AlwaysWarn` (`crates/griller-policy/src/table.rs:103,112`). A dirty tree
  with an explicit non-completion claim kind yields `WARN`, not `BLOCK`. Unreachable via the
  plugin, which always uses `Completion`; reachable by a direct MCP caller. Decide whether
  that is acceptable at exposure.
- **The trust-tier contract.** What the agent is told on `WARN` versus `BLOCK`, and whether
  a `WARN` is a real escape hatch in practice.
- **The e2e path has never run.** `e2e_mock_opencode_griller.py` is broken as written
  (`timeout=30` against a 600 s budget) and has never been executed. A live MCP round-trip
  is unverified. See [BASELINE_VERIFICATION.md](BASELINE_VERIFICATION.md).
- **Stale dead config.** `~/.config/griller/gate.json` points at
  `/home/afnan/projects/active/the-griller`, which does not exist. It fails closed, but it
  should not be left to fail closed forever.

Registration itself is a separate, explicit decision. Do not register as part of this step.

---

## Not in the sequence

- **Option B — carry the working-tree delta into replay.** Rejected in favour of Option A.
  Reconsider only on demonstrated operational cost of Option A, and then ref-based, with an
  explicit provenance member and a decision-record schema migration.
- **The 4 unimplemented attack kinds.** Already fail-closed — rejected as
  `INVALID_REQUEST`, never silently dropped. `AdversariallyVerified` being unreachable is a
  capability gap, not a safety gap. Implementing them is a feature, not a fix.
- **Pushing.** See the preconditions above.
- **The wider `/projects` workspace cleanup.** Out of scope.
