# NEXT_PHASE.md

**The remaining engineering sequence.** Nothing here is implemented, except where an item
is explicitly marked COMPLETE.

Reconciled after `e575ab7`. Phase 2 closed the timeout divergence and the process-group /
descendant-liveness defects; `e575ab7` closed the trigger observability gap. Those are no
longer open items. Current state: [CURRENT_STATE.md](CURRENT_STATE.md). Phase 2 outcome:
[PHASE_2_STATE.md](PHASE_2_STATE.md).

Items are grouped by **what each depends on**, not by preference. Where an item is blocked,
the blocker is named. No priority scores are used.

---

## 0. Two decisions that are not engineering

These are the owner's, and they gate the work below.

1. **Should this work be pushed to `github.com/EffNine/The-Griller`?** Four commits are
   unpushed. The remote's suitability as a long-term home has never been confirmed. If the
   answer is no, the repository needs a new remote — and the composition of `a360b7c`
   (inherited work + the Phase 1.2 fix in one commit) must be stated in whatever transfer
   happens. See [COMMIT_HISTORY.md](COMMIT_HISTORY.md).
2. **May session-level absolute repository paths leave this machine?** 92 of the 131
   decision records embed them. This determines whether an off-host evidence backup is
   possible at all. See [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md) §5.

Neither blocks items 1–4. Both block item 5.

---

## 1. Independent / off-host evidence backup

**Blocked by** decision 0.2 above. Not engineering work — once the policy question is
answered this is ~2.9 MB and can be done immediately.

Dependency: nothing else. If the answer is no, this item closes as "declined", and that
should be recorded rather than left ambiguous.

---

## 2. Residual trigger observability

> **COMPLETE — implemented in `e575ab7`.** Preserved below as the original problem
> statement. The plugin now reports its extraction state and the gate persists a
> `no_completion_assertion` decision (`reason_code: NO_COMPLETION_ASSERTION`) through the
> existing single persistence path, with no agent-facing injection.
> See [COMPLETION_TRIGGER_OBSERVABILITY_RECON.md](COMPLETION_TRIGGER_OBSERVABILITY_RECON.md)
> and [CURRENT_STATE.md](CURRENT_STATE.md) §5.

**Defect.** `.opencode/plugins/griller-gate.ts:393-397` — when a message contains no
completion assertion, the gate logs to the application log and returns. **No decision
record is written.** A skip is indistinguishable, in the record store, from a verification
that was never requested.

This is the residual half of the Phase 1.1 FAIL-OPEN classification. The predicate is now
fail-closed for all 27 measured forms; what remains is that an unforeseen phrasing is
silent.

**The obstacle, which is why this is not small:** the proposed `NO_COMPLETION_ASSERTION`
record **cannot be authored by the plugin**. Decision records are written by the
`griller-gate` binary, and no subcommand records a non-event.

**How it was resolved.** The plugin reports the observation
(`CompletionEvent.extraction`); the gate owns the meaning and writes the record through the
existing `persist_decision` path. No new subcommand was needed, and no second record writer
was created.

**Residual after the fix.** A skip is now recorded, but the trigger still uses a fixed
verb/noun enumeration, so an unforeseen *phrasing* is still extracted as no-assertion —
the difference is that this is now visible in the record store rather than silent.

**Depends on:** nothing. **Blocked:** item 5 (MCP exposure review) — now unblocked on the
record-structure question, but item 5 still depends on items 4 and 6.

---

## 3. `NO_COMPLETION_ASSERTION` record design

> **COMPLETE — implemented in `e575ab7`.** The shape chosen was the first option
> considered below: one new `GateStatus` variant plus a free-form `reason_code`, with no
> schema-version bump and no new `EvidenceKind`.

Separated from item 2 because it is a **design decision with schema consequences**, not an
implementation. It is the concrete form of item 2's obstacle.

**Depends on:** item 2's scope decision. **Blocks:** item 5.

**Resolved:** a `no_completion_assertion` record now exists and is produced by the gate.
It required no schema-version bump (`g1.0` unchanged) and no migration of the 131 existing
records, all of which still parse.

---

## 4. `(Fact, Unverifiable)` / `(Design, Unverifiable)` → `AlwaysWarn`

**Defect.** `crates/griller-policy/src/table.rs:103,112` map both to `AlwaysWarn`. A dirty
tree with an explicit non-completion claim kind therefore yields `WARN`, not `BLOCK`.

Pre-existing policy, not introduced by Phase 1.4. **Unreachable via the plugin**, which
always uses `Completion`; reachable by a direct MCP caller passing `claim_kind: "fact"`.

**This is a policy question, not a bug fix.** The question is whether a *verification that
could not be performed* should ever be non-blocking for a low-stakes claim kind. If the
answer is no, the rule changes; if yes, it stands and only the dirty-tree interaction needs
a separate rule.

**Depends on:** nothing. **Blocks:** item 5 — deciding it is part of assessing what `WARN`
means to an agent.

---

## 5. MCP exposure review

**A review to perform, not a task to start.** Registration is a separate, explicit decision
and is **not** authorised by this item.

The component is currently unregistered: `griller` appears 0 times in
`~/.config/opencode/opencode.jsonc`, and no `griller-gate` plugin is installed. Nothing can
reach the trust boundary Phase 1.4 closed.

> **MCP exposure remains BLOCKED.** Items 2 and 3 are now COMPLETE (`e575ab7`), so the
> unobservable-skip half of this blocker is cleared. What remains is item 4 — the
> `(Fact|Design, Unverifiable) → AlwaysWarn` policy question — plus the never-run e2e path
> and the dead `gate.json` noted below. Re-registering the gate is what would make the
> remaining `WARN` semantics reachable by an agent.

The review must cover:

- **Is the trust boundary sufficient?** Phase 1.4 closed the dirty-tree case. Confirm no
  other path lets `VERIFIED` describe uninspected code.
- **The `WARN` trust tier** — item 4 above.
- **Silent skips** — item 2 above, now COMPLETE (`e575ab7`): a skip is recorded rather than
  silent. What remains here is only the fixed-enumeration residual, not observability.
- **The e2e path has never run.** `e2e_mock_opencode_griller.py` is broken as written
  (`timeout=30` against a 600 s budget). A live MCP round-trip is unverified. See
  [BASELINE_VERIFICATION.md](BASELINE_VERIFICATION.md).
- **Stale dead config.** `~/.config/griller/gate.json` points at
  `/home/afnan/projects/active/the-griller`, which does not exist. It fails closed, but it
  should not be left to fail closed forever.

---

## 6. Items with no dependency — available whenever

These are not blocked and not urgent. They are listed so the register is complete.

| Item | State | Note |
|---|---|---|
| **4 of 8 attack kinds unimplemented** | OPEN | `Completeness`, `Invert`, `Robustness`, `RequirementCoverage` are rejected as `INVALID_REQUEST`, never silently dropped. Fail-closed; `AdversariallyVerified` unreachable. A **capability** gap, not a safety gap — implementing them is a feature, not a fix. |
| **No kernel-level isolation** | OPEN | `Enforcement::m1_none()` truthfully reports `NotEnforced` for network isolation and OS sandboxing. Honest by design. Would require a container or namespace abstraction. |
| **`e2e_mock_opencode_griller.py` broken as written** | OPEN | `timeout=30` vs a 600 s budget. Cannot pass. Only worth fixing alongside item 5, since it needs a live server. |
| **`~/.config/griller/gate.json` points at a non-existent path** | OPEN | Fails closed. Cosmetic until registration is contemplated. |
| **Outer/inner timeout margin** | NOT STARTED | A margin between the 600 s envelope and the 600 s replay budget would remove a residual edge. Deliberately deferred: it is a **product decision** about how long an agent should wait. See [TIMEOUT_SEMANTICS.md](TIMEOUT_SEMANTICS.md) §5. |
| **Non-Unix descendant cleanup** | PLATFORM-LIMITED | Unverified; no CI outside Ubuntu. Not a defect fix — implementing cross-platform supervision would be an architectural addition. See [PLATFORM_EXECUTION_LIMITATIONS.md](PLATFORM_EXECUTION_LIMITATIONS.md). |
| **Descendants surviving a *successful* command** | OPEN | Teardown signals the process group on **timeout** only. A command that exits 0 leaving a background grandchild is not cleaned up. Unchanged by Phase 2 and out of its scope. |

---

## Explicitly not in the sequence

- **Option B — carry the working-tree delta into replay.** Rejected in favour of Option A.
  Reconsider only on demonstrated operational cost of Option A, and then ref-based, with an
  explicit provenance member and a decision-record schema migration.
- **Pushing.** See decision 0.1.
- **Cross-platform process supervision.** Would be an architectural addition, not
  execution hardening.
- **Further relaxing the `unsafe` policy.** `griller-runtime` is at
  `#![deny(unsafe_code)]` with one annotated function; the other 7 crates keep `forbid`.
  See [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md) §5.
- **The wider `/projects` workspace cleanup.** Out of scope.
