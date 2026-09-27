# RECOVERY_DOCS_CLASSIFICATION.md

> **Status: Phase 1.3 analysis. Superseded in part.**
>
> Its classification of these documents remains valid, and its §3 list of
> `a360b7c`-induced stale claims has now been **actioned** — see the status
> banners in each file. Its proposed `docs/` structure has **not** been
> applied; it remains a proposal. See
> [DOCS_STRUCTURE_PROPOSAL.md](DOCS_STRUCTURE_PROPOSAL.md).

Phase 1.3 — inventory and classification of the 14 untracked markdown files.
**Nothing was deleted, moved, or committed.**

---

## 1. Inventory

All 14 sit at the repository root of `active/verification-gate` and are untracked.

| # | File | Bytes | Lines | First heading | Phase |
|---|---|---:|---:|---|---|
| 1 | `RECOVERY.md` | 9,315 | 205 | RECOVERY.md — The Griller → verification-gate | P1 |
| 2 | `RECOVERY_MANIFEST.md` | 7,364 | 148 | RECOVERY_MANIFEST.md | P1 |
| 3 | `BASELINE_VERIFICATION.md` | 6,326 | 164 | BASELINE_VERIFICATION.md | P1 |
| 4 | `MCP_SURFACE.md` | 9,829 | 180 | MCP_SURFACE.md | P1 |
| 5 | `HISTORICAL_EVIDENCE.md` | 7,579 | 196 | HISTORICAL_EVIDENCE.md | P1.1 |
| 6 | `DIRTY_STATE_ANALYSIS.md` | 11,387 | 253 | DIRTY_STATE_ANALYSIS.md | P1 |
| 7 | `TIMEOUT_PROCESS_ANALYSIS.md` | 7,834 | 174 | TIMEOUT_PROCESS_ANALYSIS.md | P1 |
| 8 | `NEXT_ACTIONS.md` | 10,705 | 148 | NEXT_ACTIONS.md | P1 |
| 9 | `DIRTY_WORK_REVIEW.md` | 5,784 | 109 | DIRTY_WORK_REVIEW.md | P1.1 |
| 10 | `TRIGGER_SECURITY_REVIEW.md` | 14,496 | 300 | TRIGGER_SECURITY_REVIEW.md | P1.1 |
| 11 | `COMPLETION_ASSERTION_REVIEW.md` | 15,427 | 253 | COMPLETION_ASSERTION_REVIEW.md | P1.1 |
| 12 | `HISTORICAL_EVIDENCE_CROSSCHECK.md` | 9,050 | 185 | HISTORICAL_EVIDENCE_CROSSCHECK.md | P1.1 |
| 13 | `DIRTY_STATE_POLICY_OPTIONS.md` | 12,743 | 219 | DIRTY_STATE_POLICY_OPTIONS.md | P1.1 |
| 14 | `PHASE_1_1_RECOMMENDATION.md` | 9,582 | 164 | PHASE_1_1_RECOMMENDATION.md | P1.1 |

Total: 137,421 bytes. The repository's own committed documentation is 35 markdown files
(`README.md`, `docs/*.md`, `docs/history/*.md`, fixture READMEs) — so these 14 would be a
~40% increase in prose if committed at the root.

---

## 2. Classification

### A. Authoritative recovery record — 4 files

These state facts that are true and would be false or unobtainable if lost.

| File | Why authoritative | Status now |
|---|---|---|
| `RECOVERY.md` | The only record of *where the project came from*, the corrected source path, the decommissioning root cause, the copy-integrity verification, and the deliberate exclusions | **PARTLY STALE** — §2 |
| `RECOVERY_MANIFEST.md` | Machine-readable recovery record: source/target paths, commit `df002d3`, tree hash, dirty-diff SHA, 9 crates, 131 `.rs`/32,885 lines, 208 tracked, 131 decisions | **PARTLY STALE** — §2 |
| `BASELINE_VERIFICATION.md` | The reproducible baseline (5 gates, 515/0/6, cold build) — the reference point every later change is measured against | **CURRENT** except the plugin count |
| `HISTORICAL_EVIDENCE.md` | First analysis of the 131 production decisions; the fixture strategy | **CURRENT**, superseded in detail by #12 |

### B. Engineering analysis — 9 files

Substantive, evidence-cited analysis. Valuable, but conclusions rather than facts.

| File | Subject | Status now |
|---|---|---|
| `MCP_SURFACE.md` | The 6 tools, schemas, trust boundaries, exposure assessment | **CURRENT** — MCP surface unchanged by P1.2 |
| `DIRTY_STATE_ANALYSIS.md` | The worktree gap + the 16-file repo dirty state | **PARTLY STALE** — §2 |
| `TIMEOUT_PROCESS_ANALYSIS.md` | Timeout layering, process-group absence | **CURRENT** — untouched, all 6 facts re-confirmed |
| `NEXT_ACTIONS.md` | The 5 post-recovery actions | **ACTION 1 NOW DONE** — §2 |
| `DIRTY_WORK_REVIEW.md` | Per-file inventory of the 16 uncommitted files | **HISTORICAL** — that state no longer exists |
| `TRIGGER_SECURITY_REVIEW.md` | The FAIL-OPEN classification and corrective design | **ACTIONED** — its recommendation was implemented as `a360b7c` |
| `COMPLETION_ASSERTION_REVIEW.md` | The OLD-vs-NEW battery, 8 regressions, CASE table | **ACTIONED** — regressions fixed and pinned |
| `HISTORICAL_EVIDENCE_CROSSCHECK.md` | 8-number verification + the 25/12/2 classification | **CURRENT** — and its correction is now in the committed code docstring |
| `DIRTY_STATE_POLICY_OPTIONS.md` | A vs B, no choice made | **CURRENT** — superseded in role by `DIRTY_STATE_POLICY_DECISION.md` |

### C. Temporary / process artifact — 0 files

None qualify. Nothing was a scratch file, log, or throwaway; all 14 are written deliverables.

### D. Redundant / obsolete — 0 files as *whole files*

No file is entirely obsolete. But **3 contain material that is now factually wrong**, because
commit `a360b7c` changed the state they describe. They are not deletable — they are the
audit trail of *why* that commit exists — but they must not be committed uncorrected.

---

## 3. Superseded claims introduced by commit `a360b7c`

This is the material finding of this inventory. Verified by grep, not inferred.

| File | Stale claim | Line |
|---|---|---|
| `RECOVERY.md` | *"moved into archive while still carrying **16 uncommitted files** of active work"* | :58 |
| `RECOVERY.md` | *"**It is uncommitted and therefore at risk.** It is preserved here but still not committed"* | :128 |
| `RECOVERY.md` | *"**16 uncommitted files** (§4) — preserved, not committed."* | :189 |
| `DIRTY_STATE_ANALYSIS.md` | Part B header: *"Repository dirty state — the 16 uncommitted files"*; *"At risk: **Yes**"* | :8, :135 ff. |
| `NEXT_ACTIONS.md` | *"| 16 uncommitted files | Commit | Action 1 |"* | :37 |
| `NEXT_ACTIONS.md` | Action 1: *"Commit the 16 uncommitted files…"* | :64 |
| `BASELINE_VERIFICATION.md` | Plugin suite *"23 passed"* — now 24 | :127, :164 |

`TRIGGER_SECURITY_REVIEW.md` and `COMPLETION_ASSERTION_REVIEW.md` are **not** stale: they
describe the pre-fix state, which is exactly what an audit record should do. They correctly
predicted the fix that was then implemented.

---

## 4. Should A and B join the repository?

**Recommendation: yes — but in two tiers, with the audit trail kept.**

### Tier 1 — propose for the repository (6 files)

`RECOVERY.md`, `RECOVERY_MANIFEST.md`, `BASELINE_VERIFICATION.md`, `MCP_SURFACE.md`,
`TIMEOUT_PROCESS_ANALYSIS.md`, `HISTORICAL_EVIDENCE_CROSSCHECK.md`.

These are the durable reference set: provenance, reproducible baseline, interface contract,
operational-defect analysis, and the verified historical record. They would sit naturally in
the existing `docs/` tree (which already has `docs/history/` as a precedent for narrative
records).

**Prerequisite:** correct the stale lines in §3 first. Committing a recovery record that
claims 16 uncommitted files while the tree is clean would introduce exactly the
documentation-rot problem this workspace already suffers from.

### Tier 2 — propose, but as a dated audit trail (5 files)

`DIRTY_WORK_REVIEW.md`, `TRIGGER_SECURITY_REVIEW.md`, `COMPLETION_ASSERTION_REVIEW.md`,
`HISTORICAL_EVIDENCE.md`, `DIRTY_STATE_ANALYSIS.md`.

These are Phase 1.1 review outputs describing a state that no longer exists. Their value is
*why the fix looks like it does* — the 8 measured regressions, the 25/12/2 reclassification,
the FAIL-OPEN reasoning. Deleting them would destroy the justification for `a360b7c`.

**Proposal:** retain under a clearly dated location (e.g. `docs/audit/2026-09-recovery/`) with
a one-line header each marking them as a point-in-time record, not current state. The existing
`docs/history/` convention (M2–M8 records, several self-flagged as superseded) is the
precedent.

### Tier 3 — replace rather than commit (2 files)

`NEXT_ACTIONS.md` — Action 1 is complete; the remaining 4 actions are still valid. Fold the
4 survivors into a current action list.
`DIRTY_STATE_POLICY_OPTIONS.md` — superseded in role by `DIRTY_STATE_POLICY_DECISION.md`
(Phase 1.3), which makes the decision. Keep the options document only as the record of what
was weighed.

`PHASE_1_1_RECOMMENDATION.md` is the decision record for `a360b7c`; retain in Tier 2.

---

## 5. Decision required from the owner

Three things this assessment deliberately does **not** decide:

1. **Whether recovery documentation belongs in the product repository at all.** It is
   process history, not product documentation. A defensible alternative is a separate
   location outside the repo. The workspace already has this problem: `~/projects/README.md`,
   `SYSTEM_ARCHITECTURE.md`, and `ENGINEERING_ROADMAP.md` are unversioned and stale, and
   `ARCHITECTURE_LOCK.md` exists in 7 copies. Adding 14 more root-level documents to a repo
   that already has 35 is a real cost.
2. **Whether the audit trail (Tier 2) is worth its 47 KB**, given `docs/history/` already
   exists as a comparable record.
3. **Who performs the §3 corrections**, and whether corrections precede or accompany any
   commit.

**No file was modified, deleted, or staged in producing this assessment.**
