# DOCS_STRUCTURE_PROPOSAL.md

**Status: proposal. Nothing has been moved, renamed, or deleted.**

All 19 recovery/review documents remain at the repository root, unchanged in location, and
all now carry a status banner. This file proposes where they should live and flags the
decisions that are genuinely ambiguous.

---

## 1. What the repository already does

The existing organisation was inspected before proposing anything, because a new structure
that ignores the existing one is worse than no new structure.

| Observation | Detail |
|---|---|
| Canonical topic docs | 15 files in `docs/` — `architecture`, `concepts`, `context-detection`, `development`, `evidence`, `gate`, `getting-started`, `limitations`, `mcp`, `opencode`, `red-team`, `security`, `self-grill`, `testing`, `verification-pipeline` |
| Point-in-time records | `docs/history/` — 10 milestone reports (M0–M8) plus `index.md` |
| Established convention | `docs/history/index.md` states plainly: *"the historical record of how decisions were made, not the current truth. **The canonical documentation lives in the README**"* |
| Navigation | a documentation table in `README.md`, linking `docs/history/` as one row |
| Root | `README.md` only — the root is otherwise kept clear |

**The convention to follow therefore already exists:** canonical current docs in `docs/`,
dated records in a subdirectory, root kept clear, `README.md` table as the index.

---

## 2. Proposed structure

Following the existing convention rather than inventing a new one:

```
README.md                          (documentation table gains new rows)
docs/
  CURRENT_STATE.md                 current state of the repository
  NEXT_PHASE.md                    remaining engineering sequence
  EVIDENCE_STATUS.md               the 131 decision records + backup status
  COMMIT_HISTORY.md                df002d3 / a360b7c / 16c6852 / docs commit
  PHASE_HISTORY.md                 what each phase did, with verification results
  recovery/                        the dated Phase 1 – 1.5 record
    index.md                       timeline + reading guide, mirroring docs/history/
    RECOVERY.md
    RECOVERY_MANIFEST.md
    BASELINE_VERIFICATION.md
    NEXT_ACTIONS.md
    MCP_SURFACE.md
    DIRTY_STATE_ANALYSIS.md
    TIMEOUT_PROCESS_ANALYSIS.md
    HISTORICAL_EVIDENCE.md
    HISTORICAL_EVIDENCE_CROSSCHECK.md
    DIRTY_WORK_REVIEW.md
    TRIGGER_SECURITY_REVIEW.md
    COMPLETION_ASSERTION_REVIEW.md
    DIRTY_STATE_POLICY_OPTIONS.md
    DIRTY_STATE_POLICY_DECISION.md
    PHASE_1_1_RECOMMENDATION.md
    PHASE_1_3_RECOMMENDATION.md
    RECOVERY_DOCS_CLASSIFICATION.md
    EVIDENCE_BACKUP_STATUS.md
    COMMIT_INTEGRITY.md
```

Note this keeps `COMMIT_HISTORY.md` and `PHASE_HISTORY.md` in `docs/` rather than
`docs/recovery/`, because they are **maintained** documents — they change as the repository
changes — whereas the 19 recovery records are **closed** and will not be edited again.

---

## 3. Classification of the 19 existing recovery documents

| Document | Class | Rationale |
|---|---|---|
| `RECOVERY.md` | KEEP — authoritative | The only record of provenance, the corrected source path, the decommissioning root cause, and the deliberate exclusions |
| `RECOVERY_MANIFEST.md` | KEEP — authoritative | Machine-readable recovery record; identity, hashes, counts |
| `BASELINE_VERIFICATION.md` | KEEP — authoritative | The reproducible baseline every later change is measured against |
| `HISTORICAL_EVIDENCE.md` | KEEP — authoritative | First analysis of the 131 records |
| `MCP_SURFACE.md` | KEEP — interface contract | The six-tool surface; the natural companion to the tracked `docs/mcp.md` |
| `DIRTY_STATE_ANALYSIS.md` | KEEP — analysis | The analysis that motivated the dirty-state decision |
| `TIMEOUT_PROCESS_ANALYSIS.md` | KEEP — analysis | Both findings still open; referenced by `NEXT_PHASE.md` |
| `NEXT_ACTIONS.md` | KEEP — **superseded in part** | Actions 1–3 complete. Retained because they explain why later work happened |
| `DIRTY_WORK_REVIEW.md` | AUDIT TRAIL | Point-in-time inventory of the 16 uncommitted files |
| `TRIGGER_SECURITY_REVIEW.md` | AUDIT TRAIL | The FAIL-OPEN analysis that justified `a360b7c` |
| `COMPLETION_ASSERTION_REVIEW.md` | AUDIT TRAIL | The 8 measured regressions that justified `a360b7c` |
| `HISTORICAL_EVIDENCE_CROSSCHECK.md` | KEEP — authoritative | 8-number verification + the 25/12/2 classification |
| `DIRTY_STATE_POLICY_OPTIONS.md` | **SUPERSEDED** | Chose neither option; superseded by the DECISION. Retain as the record of what was weighed |
| `DIRTY_STATE_POLICY_DECISION.md` | KEEP — decision record | Option A, with the reasoning and the Option B rejection |
| `PHASE_1_1_RECOMMENDATION.md` | KEEP — decision record | The COMMIT AFTER SMALL FIX verdict |
| `PHASE_1_3_RECOMMENDATION.md` | KEEP — decision record | The dirty-state policy decision and its ranking |
| `RECOVERY_DOCS_CLASSIFICATION.md` | KEEP — analysis | This proposal supersedes its §4 structure suggestion |
| `EVIDENCE_BACKUP_STATUS.md` | **SUPERSEDED** | Replaced by the tracked-in-spirit `EVIDENCE_STATUS.md`, which also resolves the 133→131 log discrepancy |
| `COMMIT_INTEGRITY.md` | KEEP — audit | The `a360b7c` audit; all checks re-run and still valid |

**Nothing is proposed for deletion.** Three documents are *superseded*, which means a
current document exists and they are retained as history — not that they should be removed.
Under the `docs/history/` convention they stay, marked.

---

## 4. Ambiguities — reported, not resolved

Three things are genuinely unclear. Each is left untouched.

**4.1 Should recovery documentation be in the product repository at all?** It is process
history, not product documentation. A defensible alternative is a location *outside* the
repo. The cost of getting this wrong in the "inside" direction is real: the repo already
carries 27 tracked markdown files, and this workspace already suffers from stale
documentation at the workspace level. This is a judgement about what the repository is *for*.

**4.2 Is the 19-document audit trail worth its ~140 KB?** `docs/history/` already exists as
a comparable record of comparable value. Some of the 19 could reasonably be dropped —
`DIRTY_WORK_REVIEW.md` in particular documents a state that no longer exists and is fully
described by `COMMIT_HISTORY.md`. I have not dropped it, because deleting an audit record
is not a decision to make while the tree is still being reconciled.

**4.3 `MCP_SURFACE.md` may duplicate the tracked `docs/mcp.md`.** Both describe the MCP
interface. The Phase 1.3 assessment concluded `MCP_SURFACE.md` was "still accurate" and
not yet folded in — both are now tracked at the repository root, neither has moved. Whether
to merge `MCP_SURFACE.md` into the tracked `docs/mcp.md` or keep it separate is a judgement
about which document is canonical. **Not merged — merging would edit a tracked, current
document on the basis of a judgement I cannot verify.**

---

## 5. What was done instead

Because the destination is not unambiguous, **nothing was moved**.

Done: every one of the 19 now carries a status banner naming what it records, what
supersedes it, and where current facts live. False *current-state* claims were corrected in
place with inline `SUPERSEDED` markers. Historical measurements were preserved unchanged.

Not done: no file was moved, renamed, deleted, or staged. `README.md` was not modified —
adding rows for documents that do not yet exist in `docs/` would break every link in the
documentation table.

**Recommended next step:** decide 4.1 first. Everything else follows from it.
