# DIRTY_STATE_ANALYSIS.md

> **Status: Phase 1 analysis (RECOVERY).**
>
> **Both the repository-hygiene gap and the verification gap it describes have
> since been closed.** The 16 uncommitted files were committed in `a360b7c`; a
> dirty tree is now `UNVERIFIABLE` (`16c6852`, Option A of
> [DIRTY_STATE_POLICY_DECISION.md](DIRTY_STATE_POLICY_DECISION.md)). The 515/23
> figures are the recovery baseline. Preserved as the analysis that motivated
> the decision — see [CURRENT_STATE.md](CURRENT_STATE.md) §3.

Two entirely different things are called "dirty state" here. They are separated because
they have opposite risk profiles.

| | A. Verification dirty-state gap | B. Repository dirty state |
|---|---|---|
| What | replay cannot see uncommitted edits | 16 uncommitted files in the recovered tree |
| Risk | correctness — a gate that verifies the wrong thing | data loss — 939 lines uncommitted |
| Status | **pre-existing design gap, documented by the project** | **recovered, tested, uncommitted** |

**Nothing was fixed in Phase 1.** Both are documented with evidence and a proposed direction.

---

# PART A — The verification dirty-state gap

## A.1 The gap, stated precisely

`Worktree::create` (`crates/griller-runtime/src/worktree.rs:94-141`) isolates verification
with:

```rust
let spec = git_command(
    &info.root,
    &["worktree", "add", "--detach", &path_arg(&wt_path)],
);
```

**No commit-ish is passed.** `git worktree add --detach <path>` checks out the current
`HEAD`. Therefore:

> **The isolated worktree contains only committed state. Uncommitted changes in the user's
> working tree are not present during verification.**

If the repository has no commits, it falls back to `git clone` (same outcome).

## A.2 What survives between runs

| State | Location | Reset? |
|---|---|---|
| Isolated worktree | `tempfile::tempdir()` — fresh per `Worktree::create` | **Yes** — new temp dir each call |
| Command environment | `cmd.env_clear()` + fixed allowlist | **Yes** — per process |
| CWD | `cmd.current_dir(spec.working_directory)` | **Yes** — per process |
| Concurrent replay | `flock`-based `ReplayLock` keyed by repo path, in `$TMPDIR` | Serialised, not shared |
| **The user's working tree** | the real repo | **Never touched** — read-only by design |
| **The user's `HEAD`** | the real repo | **Never advanced** |

**Conclusion: no cross-run contamination exists in the execution layer.** Each verification
gets a fresh worktree, a cleared environment, and a serialising lock. The design is clean.

## A.3 Can evidence originate from a previous run?

**No.** Evidence is captured into a per-run worktree and stamped with provenance
(`griller-core/src/provenance.rs`):

- `Provenance::environment(source, capture_method, working_directory)` **forces**
  `directly_observed = true` and `depends_on_agent_claim = false`
- agent-sourced evidence is a distinct constructor path using `CaptureMethod::Declared` with
  `directly_observed = false`, `depends_on_agent_claim = true`

There is exactly one way to create "environment evidence" and it cannot express agent claims.
The invariant *"an agent claim is never silently upgraded into environment evidence"* is
enforced **at the constructor level**, not by convention.

Additionally, `Provenance` carries a `working_directory` field — but the
*hash-relevant view deliberately excludes it*, with the comment:

> *"working_directory is intentionally excluded: it is a location, not a property of the
> captured state."*

So two runs in different temp dirs produce the **same** evidence hash. That is correct and
it means the hash is content-addressed, not location-addressed.

## A.4 Is a result reproducible from a clean state?

**Yes, for committed state.** Given the same `HEAD`, the same worktree content, and the same
tool versions, replay is deterministic — `Reproducibility::Deterministic` is the declared
expectation for git/filesystem/command evidence.

**No, for dirty state — because dirty state is not in scope at all.** The result is
reproducible; it simply describes a different tree than the one the agent edited.

## A.5 The real consequence

This is the project's **most consequential known limitation**, and the project states it
correctly and without hedging:

`docs/limitations.md:34-38`:
> *"**'Does the agent's proposed change work?' is not the question answered; 'does the
> committed state verify?' is.**"*

`docs/history/worktree-isolation.md:39-40`:
> *"The worktree contains only committed state; uncommitted changes in the main tree are not
> carried over. Patching/apply of dirty state is M2+."*

**The project's documentation is accurate. `SYSTEM_ARCHITECTURE.md:30-32` at the workspace
root is NOT** — it claims the Griller "verifies agent edits on real cargo builds." For a dirty
tree that is false. **The external doc overstates; the project's own docs do not.**

### Empirical check against the production record

The 131 decisions each record `event.commit`. The 15 `conductor` decisions and 11 `koma`
decisions were made against repositories that were being actively developed at the time.
Whether those trees were dirty at decision time is **not recorded** — the gate recorded the
commit, not the cleanliness. So the production record **cannot** tell us how often the gap
actually bit. **UNKNOWN.**

## A.6 A second, narrower gap: TOCTOU on the recorded commit

The decision records `event.commit`. **Nothing verifies that the commit recorded equals the
`HEAD` actually replayed.** A claim bound to commit X could be adjudicated against commit Y
if `HEAD` moved between claim capture and replay.

This is a **narrower and rarer** version of the same problem, and it is not documented in
`docs/limitations.md`. Reported here as an observation, not a fix proposal.

## A.7 Proposed direction (NOT applied)

The project already scoped this: *"Patching/apply of dirty state is M2+"*. A minimal
deterministic approach would be, inside `Worktree::create` only:

1. Detect `WorkingTreeState::Dirty` (the enum already exists — `griller-core/src/context.rs:24`
   with `Clean` / `Dirty` / `Unknown`, and `ProjectContext.working_tree_state` already
   populates it).
2. Either (a) refuse and return a distinct fail-closed state, or (b) carry the delta in via
   `git diff HEAD` → apply, plus copy untracked files.

**Not implemented. Not designed further. Flagged for the next phase**, because option (a) vs
(b) is a policy decision about what "verified" means, not a bug fix. Option (a) is
fail-closed and local; option (b) changes the trust model.

---

# PART B — Repository dirty state (the 16 uncommitted files)

## B.1 What they are

One coherent feature: **completion-assertion detection**.

| File | Δ | Role |
|---|---:|---|
| `.opencode/plugins/griller-gate.ts` | +127/−? | replaces 2 ad-hoc keyword regexes with `looks_like_completion_assertion()` |
| `.opencode/plugins/griller-gate.test.ts` | +151/−? | new tests for the predicate |
| `crates/griller-context/src/normalizer.rs` | +215 | assertion classification |
| `crates/griller-adjudicate/src/normalizer.rs` | +90 | assertion classification |
| `crates/griller-core/src/command.rs` | +86 | — |
| `crates/griller-mcp/src/completion.rs` | +74 | — |
| `crates/griller-adjudicate/tests/golden.rs` | +122 | new golden cases |
| `crates/griller-context/tests/golden.rs` | +89 | new golden cases |
| `docs/gate.md` | +20/−? | documents the new behaviour |
| `docs/context-detection.md` | +10 | documents the feature |
| `crates/griller-core/Cargo.toml` | +1 | — |
| `crates/griller-cli/src/commands.rs` | +5/−? | — |
| `crates/griller-mcp/src/plan.rs` | +5 | — |
| `crates/griller-mcp/src/validate.rs` | +6/−? | — |
| `crates/griller-context/src/task.rs` | +4/−? | — |
| `Cargo.lock` | +33 | — |

**Total: 16 files, +939 / −99.** 0 staged, 16 unstaged. 0 untracked files.

## B.2 Why it exists (from the code's own rationale)

The in-code comment documents a real production failure:

> *"The historical keyword list (pass|complete|done|…|test|verify|build|run|check|claim|ready)
> matched bare nouns, so any technical discussion ("424 tests", "the gate replays", "run the
> tests") turned into a completion claim and got blocked with unsupported."*

The production record corroborates it: `UNSUPPORTED_COMPLETION_BLOCK` is the most frequent
reason code (39/131). **The work is a fix for an observed, measured production defect.**

## B.3 It is tested and green

`npx vitest run` → **23/23 passed**. Full Rust suite with the changes applied → **515 passed,
0 failed**. `cargo clippy -D warnings` → exit 0. `cargo fmt --check` → exit 0.

**The uncommitted work is not broken work.** It is complete, tested work that was never
committed.

## B.4 ⚠️ The security-relevant behaviour change

This must be surfaced, not buried.

**Before:** extraction failure → **fail closed** — inject an "unavailable" notice onto the
session.
**After:** extraction failure → **skip silently** — no gate run, no injection.

From `docs/gate.md` (the uncommitted version):
> *"If no claim can be extracted, the message is not a completion event — the plugin skips it
> silently (no gate run, no injection). Fail-closed behaviour applies only when a gate run
> itself fails (binary missing, non-zero exit, unusable output)."*

And in code (`.opencode/plugins/griller-gate.ts:370-380`):
```ts
if (!extractedClaim) {
  // No recognizable completion assertion in this message. An ordinary
  // assistant message is not a completion event — skip silently rather
  // than failing closed (the old behaviour injected an "unavailable"
  // notice onto every technical discussion message).
  state.lastChecked = latest.info.id
  await log("info", "no completion assertion in message; skipping gate", {...})
  return
}
```

### Precise assessment — this is NOT a verdict fail-open

The change is in the **trigger layer** (TypeScript plugin: *should the gate run at all?*),
**not** in the **verdict layer** (Rust engine: *what is the verdict?*).

| Layer | Changed? | Fail-closed? |
|---|---|---|
| Trigger (plugin) | **YES** | was fail-closed, now silent-skip |
| Verdict (Rust engine) | **NO** | **unchanged and fail-closed** |

The Rust engine is untouched by this behavioural change and still cannot return `Verified`
without direct supporting evidence and zero contradicting evidence. A gate run that *fails*
(binary missing, non-zero exit, unusable output) still injects an unavailable notice — the
docs and code both state this, and the historical record contains 10 such `unavailable`
outcomes.

### The real trade-off

| | Before | After |
|---|---|---|
| Genuine completion claim | gated | gated |
| Technical discussion mentioning "test"/"build" | **gated, and blocked** (false positive) | skipped (correct) |
| Message with no recognisable assertion | **inject unavailable** (noisy, arguably wrong) | skipped |

The change trades a *noisy* fail-closed default for *precision*. It is defensible, and the
false-positive rate it fixes is documented by 39 blocked decisions. **But it does reduce
defence-in-depth**, and it is uncommitted and therefore unreviewed. Flagged in
`NEXT_ACTIONS.md` action 1.

**Not changed by this work:** the prompt-injection rejection in the trigger
(`/ignore.*griller|disregard.*gate|bypass.*check|report\s+(pass|approved)/i`) is retained
verbatim in the new code path.

---

## B.5 Recovery status of Part B

| Property | Value |
|---|---|
| Preserved | **Yes** — diff SHA256 `bd0ba55a…9067` verified identical pre/post copy |
| Tested | **Yes** — 23/23 plugin, 515/515 Rust |
| Committed | **No** |
| Pushed | **No** |
| At risk | **Yes** — uncommitted work in a project that was archived |

The single highest-value preservation action available is to commit it. See
`NEXT_ACTIONS.md` action 1.
