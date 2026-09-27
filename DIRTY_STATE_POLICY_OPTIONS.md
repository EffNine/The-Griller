# DIRTY_STATE_POLICY_OPTIONS.md

> **Status: Phase 1.1 options paper. SUPERSEDED by [DIRTY_STATE_POLICY_DECISION.md](DIRTY_STATE_POLICY_DECISION.md).**
>
> This file deliberately chose **neither** option. The decision was made in
> Phase 1.3 and **implemented in `16c6852` (Option A — refuse dirty)**.
> Retained as the record of what was weighed, not as the policy in force.

**This is a human architectural decision. No option is chosen here, and neither is
implemented.**

The gap: `Worktree::create` (`crates/griller-runtime/src/worktree.rs:94-141`) isolates
verification with `git worktree add --detach <path>` and **no commit-ish argument**, so the
worktree is a checkout of `HEAD`. **Uncommitted changes in the user's working tree are not
present during verification.**

The project's own docs already state this correctly:

- `docs/limitations.md:34-38` — *"'Does the agent's proposed change work?' is not the question
  answered; 'does the committed state verify?' is."*
- `docs/history/worktree-isolation.md:39-40` — *"The worktree contains only committed state;
  uncommitted changes in the main tree are not carried over. Patching/apply of dirty state is
  M2+."*

The external `SYSTEM_ARCHITECTURE.md:30-32` claims the Griller "verifies agent **edits** on
real cargo builds," which is false for a dirty tree. **The project's docs are accurate; the
workspace-level doc overstates.**

The relevant existing vocabulary, all already in the codebase and requiring no new concepts:

- `WorkingTreeState::{Clean, Dirty, Unknown}` — `crates/griller-core/src/context.rs:24`
- `ProjectContext.working_tree_state` — already populated
- `EpistemicStatus::Unverifiable` + `Reason::RelevantExecutionUnavailable(EvidenceId)` — the
  existing "the environment could not determine this" outcome
- `Enforcement::{NotEnforced, Enforced}` + `Enforcement::m1_none()` — the established
  precedent for **truthfully reporting what was not enforced** rather than over-claiming

---

## Option A — Refuse / explicitly reject dirty verification

**Statement.** When the target repository's working tree is `Dirty`, do not verify. Return an
explicit, recorded outcome saying the claim could not be verified *because the tree was dirty*.

### Trust-model impact

**Strengthens** the existing trust model. The engine already refuses to promote agent claims
into environment evidence; this refuses to produce environment evidence at all when the
environment is in a state the gate cannot faithfully snapshot. It is the same discipline as
`Enforcement::m1_none()`: report the truth about what was actually established.

**Does not** weaken any invariant. The `Verified` reachability condition (direct support +
zero contradiction) is untouched. `Unverifiable` is already a first-class, non-passing status.

**Cost:** the gate becomes unavailable in exactly the situation where an agent has just done
work — which is when gating matters most. Mitigable by requiring the operator/agent to commit
before claiming.

### Implementation complexity

**LOW.** `WorkingTreeState` already exists and is already computed in `ProjectContext`. The
change is a branch in `Worktree::create` (or immediately after it): if
`info.working_tree_state == Dirty`, return a typed error or construct a result carrying
`EpistemicStatus::Unverifiable` with `Reason::RelevantExecutionUnavailable`. **No new state
vocabulary, no new schema field, no new persistence.**

Roughly 20–40 lines plus tests.

### Reproducibility

**MAXIMAL.** A `git worktree add --detach <path>` of a fixed `HEAD` is fully deterministic
and content-addressed. `Provenance.working_directory` is already excluded from the
hash-relevant view, so two runs in different temp dirs produce identical evidence hashes.
This is the status quo, preserved.

Note: `WorkingTreeState::Unknown` (not a git repo) already routes to `git clone`, and
`Worktree::create` already errors for non-git paths. Dirty is the only uncovered case.

### Failure modes

| Failure mode | Severity | Mitigation |
|---|---|---|
| Agent is blocked for completing work it has not committed | **HIGH usability** — the most likely real complaint | Document clearly; the remedy (commit) is cheap and is already the release discipline |
| Operator habitually works dirty → gate permanently unavailable | **MEDIUM** | Same remedy; measurable via decision records |
| Someone "solves" it by committing junk to satisfy the gate | **MEDIUM** | Out of scope of this option; no new pressure to create commits is introduced beyond "commit before claiming" |
| A clean-but-stale tree passes (agent committed early, then edited) | **PRE-EXISTING, unchanged** | Not made worse; see TOCTOU note below |
| TOCTOU: tree becomes dirty between the check and the replay | **LOW** | Window is milliseconds; the check would be advisory only |

**Does not address the adjacent TOCTOU:** `event.commit` is recorded but nothing verifies it
equals the `HEAD` actually replayed. That is a separate, narrower issue and is orthogonal to
this choice.

### Compatibility with the existing evidence model

**EXCELLENT.** `Unverifiable` is already a documented status with an explicit docstring
("The available environment/evidence cannot meaningfully determine the claim"). The gate's
self-reported `execution`/`security` block already uses the *"`not_enforced`"* idiom, so
"verified nothing because the tree was dirty" fits the established voice. The 131 historical
records already contain 10 `unavailable` outcomes, so a consumer already handles this class.

**No schema change. No new reason code strictly required** — though a dedicated code such as
`DIRTY_TREE_UNVERIFIABLE` would be clearer than reusing `UNVERIFIABLE_COMPLETION_BLOCK`.

---

## Option B — Carry the uncommitted delta into verification

**Statement.** Materialise the working-tree state into the isolated worktree, so the replay
observes what the agent actually produced: apply the unstaged diff and copy untracked files
(or commit to a throwaway ref in the worktree).

### Trust-model impact

**This is a material change, and the risk is asymmetric.**

The engine's provenance model assumes evidence describes *committed repository state*. Carrying
a delta in means evidence can now describe state that exists nowhere in history — not in
`HEAD`, not in any ref, not recoverable if the working tree is lost before the gate runs. The
`Provenance` model has no representation for "this content was never committed."

Specific hazards:

| Hazard | Why it matters |
|---|---|
| Untracked-file semantics | `.gitignore`d files must **not** be copied. Deciding this correctly is subtle and security-relevant — a `.env` or a build artifact could be copied into the replay |
| Binary patches | `git diff` is not a safe carrier for binary content; a naive apply can silently no-op |
| Partial application | A patch that fails to apply must not be treated as "clean tree" — that would be an accidental fail-open |
| The delta is mutable | Between the `git diff` read and the `git apply`, the working tree can change. The evidence would describe a state that never existed |
| Evidence hashes | `Provenance` hashes command/env fingerprints, not tree content. Two different dirty states could produce **indistinguishable evidence hashes**, which weakens the "hash-addressed evidence" property |

**Mitigations exist** — commit to a throwaway ref in the worktree (`git stash create` / a
temporary index) rather than textually applying a diff. That is strictly better than
`git diff | git apply`, but it is a design decision with real consequences, not a mechanical fix.

### Implementation complexity

**HIGH.** Requires: dirty detection; a delta-capture strategy (ref-based preferred over
textual); untracked-file handling with correct ignore semantics; a guaranteed-fail path when
capture is partial; new provenance representation for uncommitted content; and a test matrix
across (clean / dirty-tracked / dirty-untracked / dirty-both / binary / ignored-files /
submodules / LFS). Plus new decision-record fields.

This is not a 40-line change. It is a genuine subsystem.

### Reproducibility

**REDUCED — and this is the central cost.**

Today: same `HEAD` → byte-identical evidence. That property is what makes the 131 historical
records auditable and what makes a replay meaningful.

With Option B: the same `HEAD` and the same working-tree content *should* reproduce, but
"the same working-tree content" is not a stable identifier — it has no hash, no ref, no
timestamp, and it can change between two runs. A verdict could depend on a state that no
longer exists. **Re-verification of an old decision would become impossible**, and the
`review_check.py`-style independent audit property weakens.

### Failure modes

| Failure mode | Severity | Mitigation |
|---|---|---|
| Partial delta application silently ignored | **HIGH** — accidental fail-open | Must be impossible by construction: any capture failure → `Unverifiable`, never a clean-tree fallback |
| `.gitignore`d secrets copied into the replay | **HIGH** | Requires correct ignore evaluation; a bug here is a real disclosure risk |
| Non-deterministic verdicts for identical `HEAD` | **MEDIUM-HIGH** | Undermines the evidence model |
| Binary / submodule / LFS content mishandled | **MEDIUM** | Detect and refuse (`Unverifiable`) rather than approximate |
| Significant complexity increase in a security-critical path | **MEDIUM** | More code in the trust boundary |
| TOCTOU between capture and replay | **MEDIUM** | Narrower window than today, but real |

### Compatibility with the existing evidence model

**POOR without extension.** The model has no way to say "this evidence describes uncommitted
content." Introducing that concept means either (a) a new `EvidenceSource`/`CaptureMethod`
member, which touches the canonical vocabulary that the Phase 1 review found to be clean and
single-sourced — or (b) silently reusing an existing member, which would **violate the
"agent claim is never silently upgraded" invariant's spirit** by making uncommitted content
look like ordinary environment evidence.

Option B also requires a decision-record schema change or the records stop being
self-describing. The 131 existing records would remain valid, but new records would not be
comparable to them.

---

## Side-by-side

| Dimension | A. Refuse dirty | B. Carry the delta |
|---|---|---|
| Trust-model impact | strengthens existing discipline | **materially changes** it; new unrepresentable state |
| Implementation complexity | **LOW** (~20–40 LOC + tests) | **HIGH** (subsystem, new semantics, test matrix) |
| Reproducibility | **maximal** (preserved) | **reduced** (uncommitted state has no stable identity) |
| Failure modes | 1 high-usability, rest low | 2 high, several medium |
| Evidence-model compatibility | **excellent** (reuses `Unverifiable`) | **poor** without vocabulary extension |
| Vocabulary change needed | none (optionally one reason code) | new source/method member or a silent lie |
| Reversibility | high — a branch to remove | low — provenance semantics would be depended upon |
| Addresses the stated need? | no — still cannot verify dirty work | **yes** |
| Fits the project's stated roadmap | **yes** — "dirty state is M2+" implies deferral | ahead of the stated roadmap |
| Fits the project's honesty idiom | **yes** — `not_enforced` precedent | requires new honesty vocabulary |

---

## Observations that bear on the decision (not a recommendation)

1. **The project's own roadmap defers this.** `docs/history/worktree-isolation.md:39-40` says
   dirty-state carry-in is "M2+". Option A is consistent with that deferral; Option B is not.
2. **The project has an established idiom for exactly this problem:** `Enforcement::m1_none()`
   truthfully reports that a requested constraint was not enforced. Option A is that idiom
   applied to a new constraint. Option B is not.
3. **The historical record cannot tell us how often this bit.** `event.commit` is recorded;
   working-tree cleanliness at decision time is **not**. 92 of 131 records target a path that
   no longer exists, so the question is not answerable retrospectively. The practical impact
   is **UNKNOWN** — which is itself an argument for preferring the reversible option.
4. **The gate is currently unregistered**, so neither option has a live consumer today. There
   is no operational pressure to resolve this now.
5. **The external doc is wrong either way.** `SYSTEM_ARCHITECTURE.md:30-32` should be corrected
   regardless of which option is chosen — and arguably should be corrected *before* either is
   implemented, since it currently misinforms.

---

## What this document deliberately does not do

It does not choose. It does not rank. It does not estimate business value. Two options are
presented with their real costs, because **the trade-off is between an honest unavailability
and a weakened evidence model** — that is a values judgement about what the system is *for*,
and it belongs to the owner, not to this review.
