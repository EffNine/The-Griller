# DIRTY_STATE_POLICY_DECISION.md

> **Status: Phase 1.3 decision. IMPLEMENTED in `16c6852` (2026-09-27).**
>
> Option A was implemented exactly as recommended: a dirty repository is
> `UNVERIFIABLE`, refused before any attack runs, with no new status, verdict
> or reason vocabulary. The Option B rejection recorded below still stands.
> Current state: [CURRENT_STATE.md](CURRENT_STATE.md) §3.

Phase 1.3 — decision on dirty-state semantics.
**Nothing is implemented.** This supersedes `DIRTY_STATE_POLICY_OPTIONS.md`, which recorded
the two options and deliberately chose neither.

---

## 1. Current behaviour

`Worktree::create` (`crates/griller-runtime/src/worktree.rs:94-141`) isolates replay with
`git worktree add --detach <path>` and **no commit-ish argument**. The worktree is therefore a
checkout of `HEAD`. Replay (`crates/griller-mcp/src/completion.rs:485-501`) sets
`cmd.working_directory` to that worktree path, overriding an earlier assignment of
`req.repo` at line 260.

**Net: the replayed tests execute against committed `HEAD`, never the agent's uncommitted
edits.**

### But the dirty state *is* collected

This is the nuance that makes the behaviour more misleading than the Phase 1.1 framing
implied, and it is the most important finding of this phase.

`crates/griller-mcp/src/completion.rs:197`:

```rust
for collector in &collectors {
    match collector.collect(&req.repo) {   // <-- the USER's tree
```

Three collectors are registered (`:168-174`): `RepositoryStateCollector`, `SnapshotCollector`,
and `GitDiffCollector`. **All three read `req.repo` — the user's working tree — not the
isolated worktree.** Consequences:

| Evidence | Source | Sees uncommitted changes? |
|---|---|---|
| `RepositorySnapshot` (has `dirty: bool`, `staged_changes`, `unstaged_changes`, `untracked_files`) | user tree | **YES** |
| `GitDiff` (`git diff`, `git diff --cached`, `git status --porcelain`) | user tree | **YES** |
| **Replay** (the "tests pass" claim) | isolated worktree | **NO** |

So the gate **captures** the dirty state and the diff as first-class evidence, records it,
and then **never weighs it against the replay result**:

- `crates/griller-adjudicate/src/status.rs` — the status assignment — contains **no** reference to `dirty`, `is_clean`, `staged_changes`, `unstaged_changes`, or `untracked`.
- `GitDiff` evidence is consumed by `matcher.rs` **only** for `match_file_change` and `match_file_remove` — i.e. claims of the form "X changed" / "X was removed". It is **never** consulted for a test-outcome claim.
- `ProjectContext.working_tree_state` (`griller-core/src/context.rs:51`) exists as a field but its only `Clean` literal in the codebase is at line 82, **inside `#[cfg(test)]`**. No production collector populates it, and nothing in the pipeline reads it.

**The dirtiness is visible in the evidence set and invisible in the verdict.**

---

## 2. Why this is potentially misleading

`docs/limitations.md:34-38` states the position honestly:

> *"**'Does the agent's proposed change work?' is not the question answered; 'does the
> committed state verify?' is.**"*

The project's own documentation is accurate. The misleading parts are elsewhere:

1. **External documentation overstates.** `~/projects/SYSTEM_ARCHITECTURE.md:30-32` claims the Griller *"verifies agent **edits** on real cargo builds."* For a dirty tree that is false.
2. **The evidence record reads as comprehensive.** A decision record containing a `GitDiff` evidence entry and a passing replay invites a reader to conclude the change was verified. It was not — only `HEAD` was.
3. **`event.commit` implies coverage.** Every one of the 131 records logs a commit. A reader reasonably assumes the claim was evaluated against that commit's *content* as edited. It was evaluated against the committed content only.
4. **The gap is silent.** A `VERIFIED_COMPLETION_PASS` for untested uncommitted work produces no warning, no downgrade, and no distinct reason code. It is indistinguishable, in the record, from a genuine full verification.

**This is the trust-boundary defect, and §3 of this document fixes its scope precisely.**

---

## 3. The trust boundary

> **Invariant:** *An agent must never receive a VERIFIED result for code that the verification
> engine did not actually inspect.*

### Does the invariant currently hold?

**NO — for test-outcome claims against a dirty working tree.**

Concrete path:

1. Agent makes uncommitted edits to `src/lib.rs` that break a test.
2. Agent claims *"All tests pass."*
3. Claim is classified a completion assertion (predicate — correct), gate runs.
4. `GitDiffCollector` and `SnapshotCollector` read the user's tree: they **record** that
   `src/lib.rs` is modified. Neither is consulted for a test claim.
5. `AttackKind::Replay` executes in a **clean worktree at `HEAD`**: the uncommitted breakage is
   not there. Tests pass. `ExecutionOutcome::Success` with exit 0.
6. `match_*` for a test-outcome claim sees a **Direct** support.
7. Zero contradicting evidence → `EpistemicStatus::Verified` (the single assignment site,
   `status.rs:202`).
8. Reason `DirectSupport(evidence_id)`. Status `verified`. Gate returns
   `VERIFIED_COMPLETION_PASS` and the agent proceeds.

**The engine did not inspect the code the agent changed, and the agent was told `VERIFIED`.**

### Is the companion claim true — "agent changed files → verification runs → verification result"?

**Only partially, and the middle step is the problem:**

| Step | Actually true? |
|---|---|
| agent changed files | Yes |
| **verification runs** | Yes — but **against `HEAD`, not the change** |
| verification result | Yes — `VERIFIED_COMPLETION_PASS`, describing a tree the agent did not produce |

So the chain is *not* "change → verify → result about the change". It is
"change → verify **`HEAD`** → result about `HEAD`, presented as a result about the claim."

**This was not assumed.** It was traced through `completion.rs:197` (collectors read
`req.repo`), `:485-501` (replay reads the worktree), `worktree.rs:94-141` (worktree = `HEAD`),
`status.rs:153-249` (verdict rules), and `matcher.rs` (`GitDiff` scope).

### Does each option preserve the invariant?

| | Invariant preserved? | Mechanism |
|---|---|---|
| **Current behaviour** | **NO** | `Verified` can be returned for uninspected code |
| **A. Refuse dirty** | **YES** | A dirty tree yields `Unverifiable`, which is non-passing by construction |
| **B. Carry delta** | **YES** | The replay executes the agent's actual content, so the inspected code *is* the claimed code |

Both options restore the invariant. They differ in what they cost and in how much they
distrust the operator.

---

## 4. Option A — refuse dirty

**Statement.** If the target repository's working tree is dirty, do not verify. Return
`EpistemicStatus::Unverifiable` with a distinct reason code, e.g. `DIRTY_TREE_UNVERIFIABLE`.

### Trust-model consequences
Strengthens the existing discipline. The engine already refuses to promote agent claims into
environment evidence; this refuses to produce environment evidence when the environment is
in a state the gate cannot faithfully snapshot. It reuses the project's own established
idiom: `Enforcement::m1_none()` truthfully reports a requested constraint that was not
enforced.

### Reproducibility consequences
**Maximal, and unchanged.** A `git worktree add --detach` of a fixed `HEAD` is deterministic
and content-addressed. `Provenance.working_directory` is already excluded from the
hash-relevant view, so two runs in different temp dirs produce identical evidence hashes.
Historical records stay re-verifiable in principle.

### Evidence-model consequences
**None required.** `EpistemicStatus::Unverifiable` and `Reason::RelevantExecutionUnavailable`
already exist with exactly the right docstring: *"The available environment/evidence cannot
meaningfully determine the claim (tool missing, execution unavailable, timeout)."* The 131
existing records already contain 10 `unavailable` outcomes, so consumers handle the class.
Optionally add a dedicated reason code for clarity; not strictly necessary.

### Implementation scope
**LOW.** `RepositorySnapshot` already carries `dirty: bool` and `is_clean()`, and
`SnapshotCollector` is already registered in `check_completion` reading the user's tree. The
change is a branch between collection and adjudication, plus wiring the snapshot into the
report. No new vocabulary, no schema change, no new collector. Roughly 40–80 lines plus tests.

### Migration impact
**None.** No existing record changes meaning. New records gain a reason code. The 131 archived
records remain valid and comparable.

### Rollback implications
**Trivial.** A single branch to remove. No persisted state, no schema to unwind.

### Cost, stated plainly
The gate becomes unavailable in exactly the situation where an agent has just done work. The
remedy (commit) is cheap and matches existing release discipline, but it is friction, and
habitually working dirty makes the gate permanently unavailable. This is the real cost and it
is a usability cost, not a safety one.

---

## 5. Option B — carry the delta into verification

**Statement.** Materialise the working-tree state into the isolated worktree so replay
executes the agent's actual content.

### Trust-model consequences
**Materially changes it.** Evidence would describe state that exists in no ref and no
commit — unrecoverable if the working tree changes before the gate runs. The `Provenance`
model has no member for "this content was never committed", so representing it requires
either extending the canonical vocabulary or **misrepresenting uncommitted content as
ordinary environment evidence** — the latter being precisely the failure mode
`Provenance::environment()` was designed to make impossible.

Specific hazards: `.gitignore`d files must not be copied (a `.env` or artifact copied into
replay is a disclosure risk); binary patches are not safely carried by `git diff`; a partial
application must never degrade to "clean tree", which would be an accidental fail-open; and
the delta is mutable between read and apply.

A ref-based capture (`git stash create` or a temporary index in the worktree) is strictly
safer than textual `git diff | git apply`, but it remains a design decision with real
consequences, not a mechanical fix.

### Reproducibility consequences
**Reduced.** Today, same `HEAD` → byte-identical evidence. Under Option B, the same `HEAD`
with a different working tree yields different evidence, and "the same working tree" is not a
stable identifier — no hash, no ref, no timestamp. Re-verification of an old decision becomes
impossible, weakening the independent-audit property the evidence model exists to provide.

### Evidence-model consequences
**Poor without extension.** Requires either a new `EvidenceSource`/`CaptureMethod` member —
touching the canonical vocabulary the Phase 1 review found clean and single-sourced — or a
silent misrepresentation. Also requires a decision-record schema change, after which new
records are no longer directly comparable to the 131 archived ones.

### Implementation scope
**HIGH.** Dirty detection; a capture strategy; untracked-file handling with correct ignore
semantics; a guaranteed-fail path on partial capture; new provenance representation;
a test matrix across clean / dirty-tracked / dirty-untracked / both / binary / ignored /
submodules / LFS; and new record fields. A subsystem, not a patch.

### Migration impact
**Moderate.** Existing records stay valid, but new records use a schema the archived ones do
not have, so longitudinal comparison is affected.

### Rollback implications
**Hard.** Once provenance semantics depend on "uncommitted content", reverting is not a matter
of removing a branch — anything that consumed the richer provenance must be revisited.

---

## 6. Recommendation

> ## **Adopt Option A — refuse dirty verification.**
> Recommended on the basis of the project's own evidence model, not on engineering
> convenience.

Reasoning, in the order the evidence supports it:

1. **Option A restores the trust invariant with no new concepts.** `Unverifiable` already
   exists precisely for "the environment could not determine this", and `RepositorySnapshot.dirty`
   is already collected from the user's tree by a collector already registered in
   `check_completion`. Option A connects two things the project already has. Option B
   requires a new representation of state the model currently cannot express.
2. **Option A preserves reproducibility; Option B trades it away.** Content-addressed
   evidence and re-verifiability are the property that makes this system's output worth
   having. Option B weakens the core asset to serve a convenience.
3. **Option A follows the project's stated roadmap** — `docs/history/worktree-isolation.md:39-40`
   says dirty-state carry-in is "M2+". Option A is consistent with that deferral; Option B is
   not.
4. **Option A matches the project's own honesty idiom.** `Enforcement::NotEnforced` /
   `m1_none()` exists to report truthfully what was *not* enforced. Option A is that idiom
   applied to a new constraint; Option B requires a new honesty vocabulary.
5. **Option A is reversible; Option B is not.** A branch versus a semantic dependency.
6. **The gate is currently unregistered**, so there is no live consumer to disrupt — the cost
   of A is deferred, not paid now.
7. **Empirical pressure is UNKNOWN.** The 131 records log `event.commit` but not tree
   cleanliness, and 92 target a path that no longer exists. The problem is real in
   principle and unquantified in practice. **The reversible option is the correct response
   to an unquantified risk.**

**Stated against the recommendation:** Option A will block agents that work dirty, and that
is a genuine usability cost that will generate pressure to work around the gate. If that
pressure materialises, the correct response is to fix the workflow (commit before claiming),
not to weaken the invariant. Option B should be reconsidered only if Option A's cost is
demonstrably unacceptable in operation — and it should then be built ref-based, with an
explicit provenance member, and with a schema migration for the decision records.

**Not implemented.** This is a decision record, not a change.
