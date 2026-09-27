# PHASE_1_1_RECOMMENDATION.md

> **Status: Phase 1.1 decision record. Recommendation ACTIONED.**
>
> The verdict below — **COMMIT AFTER SMALL FIX** — was carried out in `a360b7c`
> on 2026-09-27. The 515/515 Rust and 23/23 plugin figures are as measured at
> review time. Preserved as the record of *why* that commit looks the way it
> does; see [COMMIT_HISTORY.md](COMMIT_HISTORY.md) for the commit's actual
> composition.

**Verdict: `COMMIT AFTER SMALL FIX`**

Evidence-based. Full reasoning below; supporting detail in
`TRIGGER_SECURITY_REVIEW.md`, `COMPLETION_ASSERTION_REVIEW.md`,
`HISTORICAL_EVIDENCE_CROSSCHECK.md`, `DIRTY_WORK_REVIEW.md`.

---

## 1. The three options considered

| Option | Rejected because |
|---|---|
| **COMMIT NOW** | The change introduces **8 measured regressions** where genuine terse completions (`"Done."`, `"All done."`, `"Ready."`, `"All 511 pass."`) now produce **no gate run, no block, no notice, and no audit record**. That is a fail-open at the trigger layer, and the trigger is part of the security boundary. Committing it would make the regression permanent and reviewed-as-accepted. |
| **DO NOT COMMIT** | The work is **genuine, tested, and fixes a real production defect** — 12 real false blocks, where an agent was blocked for writing a forensic report. It also *improves* claim quality (`"Done. All tests pass."`: OLD extracted the useless `"Done."`, NEW extracts `"All tests pass."`), adds 3 coverage classes OLD missed, and retains prompt-injection defence (verified: test `H` passes). It is 515/515 Rust + 23/23 plugin green, clippy-clean. Leaving it uncommitted in an **archived** project risks losing 939 lines. That is the worse outcome. |
| **COMMIT AFTER SMALL FIX** | ✅ **Selected.** The regression is caused by exactly **two narrow gaps in one regex predicate**, both mechanically fixable, with a test table that can pin them. The valuable 95% of the change is preserved and protected. |

---

## 2. Why the fix is genuinely *small*

Both root causes are missing alternatives in a single function,
`looks_like_completion_assertion` (`.opencode/plugins/griller-gate.ts:155-208`):

| Root cause | Affected cases | Nature |
|---|---|---|
| **Rule 5 requires an auxiliary** (`is\|are\|was\|were\|been\|being`). A standalone `"Done."` / `"Ready."` has none, and `"It's ready"` contracts to `"s ready"`. | `"Done."`, `"All done."`, `"Done!"`, `"Ready."`, `"That's done."`, `"Complete."`, `"It's ready."` | one missing alternative set |
| **Rule 2 requires the literal noun `tests`.** `"All 511 pass."` has a count and an outcome but no noun. | `"All 511 pass."` | one missing alternative |

**Nothing else changes.** Verified unchanged between revisions: path/loop structure,
`tail.reverse()`, reverse sentence iteration, the `candidate.length < 5 || > 300` guard, the
markdown-structure guard, and the prompt-injection rejection regex.

---

## 3. The smallest exact change required

**One function, two alternatives, one test table. Not implemented — specified only.**

### 3a. In `.opencode/plugins/griller-gate.ts`, inside `looks_like_completion_assertion`, immediately before `return false`:

```ts
// Terse standalone assertion: the whole sentence is the outcome.
// Restores coverage for "Done." / "All done." / "Ready." / "Complete." /
// "That's done." / "It's ready." — previously matched by the old bare-noun
// net, silently unmatched by the structured predicate.
if (/^\W*(all\s+)?(done|complete[d]?|finished|ready|shipped|merged|closed)\b[\W]*$/i.test(candidate))
  return true
// Counted outcome with no noun: "All 511 pass."
if (/^\W*all\s+\d+\s+(pass(ed|es)?|fail(ed|s)?|green|clean)\b/i.test(candidate)) return true
```

**Scope:** 2 regexes inside 1 existing function. **No** control-flow change, **no** new state,
**no** new dependency, **no** schema change, **no** change to the Rust engine, **no** change
to the skip branch.

### 3b. In `.opencode/plugins/griller-gate.test.ts`, add one table-driven test pinning all 20 terse forms:

Extractable (8 currently regressed, plus the 3 gains that must not be lost):
`"Done."`, `"All done."`, `"Done!"`, `"Ready."`, `"That's done."`, `"Complete."`,
`"It's ready."`, `"All 511 pass."`, `"Shipped."`, `"Finished."`, `"Merged it."`

Must remain null (guard against over-correction):
`"All set."`, `"Wrapped up."`, `"Handled."`, `"Taken care of."`, `"All good."`, `"Sorted."`,
`"There are 424 tests."`, `"The test suite is slow."`, `"Let me check the logs."`,
`"Implement Atlas P10.5.1 Fact Store."`, the real planning-prose regression message.

**Why the test table is part of the minimum:** the existing suite pins **none** of these 20
cases. That coverage gap is precisely why 8 regressions shipped undetected into a green
suite. A fix without a test would be unverifiable and would regress again.

### 3c. Verification required after the fix (not performed — out of scope)

```
npx vitest run                                    # expect 24+ passed (23 + 1 new)
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace                            # expect 515 passed / 0 failed
cargo clippy --workspace --all-targets --all-features -- -D warnings
```

Plus a manual re-run of the OLD-vs-NEW battery: the 8 regressions must become `same`, the 3
gains must be preserved, and the 11 must-stay-null cases must remain null.

---

## 4. Explicitly NOT part of the required fix

| Item | Why excluded |
|---|---|
| Restoring the unconditional "unavailable" injection | It was the **actual defect** — it fired on every technical discussion message. The 12 false blocks are real, but the right fix is a precise trigger, not a noisy one. |
| Making the skip write a decision record | **Valuable but not small.** Decision records are written by the `griller-gate` **binary**; the plugin only spawns it. The plugin cannot author a record. This needs a new subcommand or a separate marker file — a larger change, tracked as follow-up below. |
| Any change to the Rust engine | It is fail-closed and correct. The single `EpistemicStatus::Verified` assignment site requires direct support and zero contradiction. **Untouched.** |
| The prompt-injection rejection | Retained verbatim and working. **Untouched.** |
| The dirty-state policy | A human architectural decision. See `DIRTY_STATE_POLICY_OPTIONS.md`. |
| Timeout / process-group fixes | Separate concerns, separate actions. See §6. |

---

## 5. What the fix does and does not resolve

**Resolves:** the 8 measured fail-open cases. After the fix, every completion form in the
empirical battery is classified correctly, and the "no assertion → skip silently" policy
becomes defensible **because the classifier is accurate on everything measured**.

**Does not resolve:** the *class* of risk. `looks_like_completion_assertion` remains a fixed
verb/noun enumeration, so some future phrasing will be misclassified — and when that happens
it will again be **silent**, with no record in the evidence store. That residual is inherent
to a trigger-layer heuristic and is why the follow-up below is recommended rather than
dismissed.

---

## 6. Recommended follow-ups (tracked, not blockers for this commit)

1. **Auditable skip** — give the gate a way to record `NO_COMPLETION_ASSERTION` so a skip is
   distinguishable in `decisions/` from "gate never ran". Converts an unobservable skip into
   an auditable one. Medium complexity; the plugin cannot currently author records.
2. **Correct `~/projects/SYSTEM_ARCHITECTURE.md:29-33,47`** — it claims the Griller is live
   (it is not registered) and that it "verifies agent edits" (false for dirty trees). The
   project's own `docs/limitations.md` is correct. This misinforms regardless of the dirty-state
   decision.
3. **Gate timeout default** — `griller-gate` `DEFAULT_TIMEOUT_MS = 120_000` is 5× smaller than
   `REPLAY_TIMEOUT_MS = 600_000`. Availability defect, fails closed. Derive the constant from
   `griller-core` to prevent recurrence.
4. **Process-group termination** — `child.kill()` at `execution.rs:189`; `killpg`/`setpgid`/
   `setsid` occur **0 times** in the workspace. Orphaned `rustc` can perturb later
   verifications. Reproducibility defect, not soundness.
5. **Dirty-state policy** — human decision. `DIRTY_STATE_POLICY_OPTIONS.md`.

---

## 7. Decision rationale in one paragraph

The change is worth keeping and worth committing, because it fixes 12 real production false
blocks, improves the quality of the claims reaching the engine, and is fully tested. It is not
worth committing **as-is**, because two narrow gaps in its predicate cause the most common
completion phrases — "Done.", "All done.", "Ready." — to bypass the gate entirely and silently,
and the trigger layer is part of the security boundary regardless of how fail-closed the Rust
engine is. The corrective change is two regexes and one test table, both local and mechanical,
and the test table closes the exact coverage gap that let the regression ship inside a fully
green suite. Correcting the justification should also be part of the commit: the cited "39
false-positive blocks" is **12** genuine false positives plus 2 injection tests; the other 25
were the gate correctly refusing genuine claims. That overstatement should not be carried
forward into the repository as fact.

---

## 8. Integrity statement

No project file was modified during Phase 1.1. Analysis harnesses were written to `/tmp`. One
temporary vitest file was created under `.opencode/` solely to verify predicate fidelity
against the real source, and was removed; the suite was re-run afterwards to confirm no
residue.

```
HEAD              df002d32f85c83ecf0def25378f9ad04378a5bca   unchanged
HEAD tree         a83a61b9a266e2fa02f60eab97c17743e7ff5fe7   unchanged
working-tree diff bd0ba55aa34f0feff0f0ff335f6a43723ad...   unchanged
staged            0
```

**Nothing was committed. Nothing was pushed. The 16 uncommitted files remain uncommitted.**

> **SUPERSEDED by `a360b7c` (2026-09-27).** The 16 files are now committed, and the
> terse-assertion fix recommended above was implemented in that same commit. Still not
> pushed. The statement above is the closing status of the Phase 1.1 review, preserved as
> written — see [COMMIT_HISTORY.md](COMMIT_HISTORY.md).
