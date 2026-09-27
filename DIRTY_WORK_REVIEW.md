# DIRTY_WORK_REVIEW.md

> **Status: Phase 1.1 point-in-time review (REVIEW).**
>
> This file inventories the 16 uncommitted files **as found at recovery**. That
> dirty state no longer exists — they were committed in `a360b7c`. Preserved
> unchanged as the record of what was reviewed and why it was judged worth
> committing. See [COMMIT_HISTORY.md](COMMIT_HISTORY.md).

Phase 1.1 — inventory and classification of the 16 uncommitted files.
**Nothing was modified.** Source integrity re-verified after review (see §5).

---

## 1. Exact inventory

`git diff --numstat` against `HEAD = df002d32f85c83ecf0def25378f9ad04378a5bca`.
**16 files changed, 939 insertions(+), 99 deletions(-). 0 staged, 16 unstaged. 0 untracked
code files** (the 8 untracked entries are the Phase-1 `.md` deliverables).

| # | Path | + | − | Class | Purpose |
|---|---|---:|---:|---|---|
| 1 | `.opencode/plugins/griller-gate.ts` | 91 | 36 | **production** (trigger layer) | replaces 2 keyword regexes with `looks_like_completion_assertion()`; extraction-failure → silent skip |
| 2 | `.opencode/plugins/griller-gate.test.ts` | 119 | 32 | test | 7 new/renamed test cases for the predicate |
| 3 | `crates/griller-context/src/normalizer.rs` | 212 | 3 | **production** | ~150 past-tense report verbs added so sentence-initial capitalised verbs are not read as **project-name candidates** |
| 4 | `crates/griller-context/tests/golden.rs` | 89 | 0 | test | golden cases for the above |
| 5 | `crates/griller-adjudicate/src/normalizer.rs` | 82 | 8 | **production** | `SCOPE_QUANTIFIERS` + `scan_verb`, separating **narration verbs** ("fixed the bugs") from **verification verb lists** ("cargo fmt, cargo check, cargo test — all exit 0") |
| 6 | `crates/griller-adjudicate/tests/golden.rs` | 122 | 0 | test | golden cases for claim parsing |
| 7 | `crates/griller-core/src/command.rs` | 86 | 0 | **production** | execution/replay additions |
| 8 | `crates/griller-mcp/src/completion.rs` | 66 | 8 | **production** | pipeline wiring for the above |
| 9 | `crates/griller-mcp/src/validate.rs` | 5 | 1 | **production** | input bound |
| 10 | `crates/griller-mcp/src/plan.rs` | 5 | 0 | **production** | — |
| 11 | `crates/griller-context/src/task.rs` | 2 | 2 | **production** | — |
| 12 | `crates/griller-cli/src/commands.rs` | 4 | 1 | **production** | — |
| 13 | `crates/griller-core/Cargo.toml` | 1 | 0 | **production** | dependency line |
| 14 | `Cargo.lock` | 33 | 0 | **generated** | lockfile, follows #13 |
| 15 | `docs/gate.md` | 12 | 8 | documentation | documents the new trigger + skip behaviour |
| 16 | `docs/context-detection.md` | 10 | 0 | documentation | documents the context-normaliser fix |

**Production/runtime: 10 files. Test-only: 3. Documentation: 2. Generated: 1.**
No binary, no large artifact, no vendored dependency, no secret.

---

## 2. Correction to the framing in the task brief

The brief says *"focus specifically on `griller-context/src/normalizer.rs` and its associated
tests."* That file is **not** the completion-assertion predicate.

`griller-context/src/normalizer.rs` (+212) adds a list of ~150 **past-tense report verbs**
("replaced", "implemented", "added", … "prefteched") for a different purpose. Its own comment:

> *"Past-tense / participle forms of report verbs. Completion claims begin with them
> ("Replaced the gate plugin's …", "Implemented Atlas …"), and sentence-initial capitalization
> must never turn a report verb into a **project-name candidate**."*

So it fixes **project-name detection** in M4 context — preventing "Implemented" at the start
of a sentence from being extracted as a project name. That is a **parsing-accuracy** fix, not
a security-boundary change.

**The completion-assertion change is in the TypeScript plugin** (`.opencode/plugins/griller-gate.ts`),
which is the *trigger layer*. That is where the security question lives, and it is reviewed in
`TRIGGER_SECURITY_REVIEW.md` and `COMPLETION_ASSERTION_REVIEW.md`.

The Rust changes in aggregate are **claim/task normalisation quality improvements** that make
the engine parse claims better. They do not change any verdict rule, and they are not
fail-open-relevant.

---

## 3. What the change set is, in one paragraph

Three related improvements, all aimed at the same observed production defect — the trigger
extracting *discussion* of verification as *claims of* verification:

1. **Trigger precision** (TS): a structured completion-assertion predicate replaces bare-noun
   keyword matching.
2. **Context normalisation** (Rust): report verbs are no longer mistaken for project names.
3. **Claim normalisation** (Rust): scope quantifiers ("all", "whole", "full") and
   narration-vs-verification verb discrimination, so `scan_verb` picks the right target out of
   claims like "cargo fmt, cargo check, cargo test — all exit 0".

Items 2 and 3 are engine-quality work. **Item 1 is the security-relevant change**, and it is
the only one that alters the fail-closed posture.

---

## 4. Verification status of the change set

| Check | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace` | PASS, 0 warnings |
| `cargo test --workspace` | PASS — 515/0/6 |
| `cargo clippy -- -D warnings` | PASS |
| `npx vitest run` | PASS — 23/23 |

**The work is complete, tested and green.** Its problem is not correctness; it is that one
component of it (the silent-skip) narrows a security control, and the justification for that
narrowing is quantitatively overstated (see `HISTORICAL_EVIDENCE_CROSSCHECK.md`).

---

## 5. Post-review integrity re-verification

```
HEAD              df002d32f85c83ecf0def25378f9ad04378a5bca   unchanged
HEAD tree         a83a61b9a266e2fa02f60eab97c17743e7ff5fe7   unchanged
working-tree diff bd0ba55aa34f0feff0f0ff335f6a43723ad...   unchanged
staged            0
```

Analysis harnesses were written to `/tmp` only. **No project file was modified during this
review.** One temporary vitest file was created under `.opencode/` to verify predicate
fidelity against the real source and **was removed**; the suite was re-run afterwards to
confirm no residue.
