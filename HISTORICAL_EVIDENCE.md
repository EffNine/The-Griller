# HISTORICAL_EVIDENCE.md

> **Status: Phase 1 record (RECOVERY) + Phase 1.1 analysis.**
>
> The evidence at `~/.local/share/griller/gate/` was never modified. For its
> current status, including the resolved 133→131 log discrepancy and the
> same-disk backup caveat, see [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md).

The 131 production gate decisions are the most valuable asset in this recovery. They are
**irreplaceable field evidence** and were **not touched**: no copy, no move, no mutation, no
normalisation.

**Location (unchanged):** `~/.local/share/griller/gate/`

---

## 1. Inventory

| Artifact | Count | Size | Format |
|---|---:|---|---|
| `decisions/*.json` | **131** | 3.1 MB | JSON, schema `g1.0` |
| `gate.log` | **133 lines** | 13.7 KB | `<epoch_ms> status=<Status> session=<id> repo=<path>` |
| Date range | 2026-08-21 → 2026-08-25 | | |
| Parseable | **131 / 131** | | 0 unparseable |

### The 133 vs 131 discrepancy — resolved

`gate.log` has 133 lines but there are 131 decision files. Cause: **two log lines are exact
duplicates.** Two timestamps each appear twice:

```
2 × 1787323028989
2 × 1787320284381
```

133 log lines − 2 duplicate lines = **131 unique decisions**. The two counts are consistent;
**no decision record is missing and none is orphaned.**

---

## 2. Verdict distribution (top-level `status`)

This is the authoritative outcome record. Read from all 131 records.

| Verdict | Count | Share |
|---|---:|---:|
| `blocked` | **98** | 74.8% |
| `accepted` | **23** | 17.6% |
| `unavailable` | **10** | 7.6% |

**The gate refused three quarters of all completion claims it evaluated.** This is the single
most important fact about the production record: the component was load-bearing, and it
declined to rubber-stamp.

### Reason codes (deterministic, from the engine)

| Reason code | Count | Meaning |
|---|---:|---|
| `UNSUPPORTED_COMPLETION_BLOCK` | 39 | meaningful claim, no sufficient supporting evidence |
| `MISMATCH_CONTEXT_BLOCK` | 32 | task did not belong to this repository |
| `VERIFIED_COMPLETION_PASS` | 23 | direct evidence supported the claim |
| `CONTRADICTED_COMPLETION_BLOCK` | 21 | direct evidence conflicted with the claim |
| `CHECK_COMPLETION_FAILED` | 8 | the check itself errored |
| `UNVERIFIABLE_COMPLETION_BLOCK` | 6 | environment could not determine the claim |
| `STALE_SERVER` | 2 | server version mismatch → fail closed |

Only **23 of 131 (17.6%)** reached `VERIFIED_COMPLETION_PASS`.

> **Note on the earlier `UNSUPPORTED` dominance.** The uncommitted in-repo work
> (`RECOVERY.md` §4) exists precisely because of the 39 `UNSUPPORTED_COMPLETION_BLOCK`
> records: the trigger was extracting *discussion* of testing as *claims of* testing. The
> production record documents the bug the uncommitted change fixes. **This is the clearest
> example in the project of the evidence trail earning its keep.**

---

## 3. What was actually gated

| Repository | Runs |
|---|---:|
| `/home/afnan/projects/active/the-griller` | 92 |
| `/home/afnan/projects/active/conductor` | 15 |
| `/home/afnan/projects/active/koma` | 11 |
| `/` (root — fail-closed) | 7 |
| `/home/afnan/projects/active/codebro` | 3 |
| `/home/afnan/projects/active/gumi` | 1 |
| `/tmp/tmp.PRhy5jxbqh/codebro` (fixture) | 1 |
| `/tmp/fake_go_repo` (fixture) | 1 |

Real sessions: `ses_fdbe…`, `ses_fc7c0f78…`, `ses_fcbf02b…` and others.
**The gate was operating against live, in-development repositories — not a test harness.**

The 7 runs against `/` are the fail-closed path: a repository that could not be resolved
produced `unavailable`, not a pass.

---

## 4. Integration health recorded in the evidence

| Recorded state | Count |
|---|---:|
| `connected=true, stale=false` (live, correctly-versioned server) | **129** |
| `connected=true, stale=true` (version mismatch → `STALE_SERVER`) | 2 |

The 2 stale records are a real, valuable incident: a `0.1.0` server was running while the
gate expected `0.2.0`. Sample message:

> `griller version mismatch: running '0.1.0', expected '0.2.0' (rebuild required)`

The gate **refused to operate** rather than trusting a mismatched server. This is the
`ExpectedServer` contract (`crates/griller-gate/src/config.rs`) working exactly as designed.

---

## 5. Record structure (schema `g1.0`)

```
status              : blocked | accepted | unavailable
reason_code         : <one of 7>
findings[]          : bounded list
required_action     : machine-readable action
required_action_message : human-readable
event               : { repository, agent_id, session_id, commit, timestamp_ms }
integration         : { connected, server_info{name,version}, stale, stale_reason }
  embedded          : full griller_report (the whole adjudication)
timing              : { connect_ms, check_ms, total_ms }
lifecycle[]         : ordered stage events
```

Two properties make these records unusually valuable:

1. **`event.commit` is recorded.** Every decision states the commit it was made against.
2. **The full `griller_report` is embedded**, not just the verdict — so the adjudication can
   be re-examined without re-running anything.

---

## 6. Can they be replayed?

**No — and that is a limitation, not a defect.**

Each record embeds the complete report including the evidence that produced it, so a decision
can be **read, re-adjudicated, and audited offline**. What cannot be done is **re-execution**:
replaying requires re-running the claimed verification command against the repository at
`event.commit`, and:

- 92 of 131 target `/home/afnan/projects/active/the-griller` — **that path no longer exists**
  (the project is now `active/verification-gate` and `archive/legacy/the-griller`)
- the 15 `conductor` decisions target a repo that has since changed
- wall-clock and environment facts (`timing`, `tool` versions) do not reproduce

**Verdict: the records are auditable, not re-executable.** That is sufficient for their
primary purpose (proving the gate worked and was not a rubber stamp).

---

## 7. Fixture strategy (proposed, NOT applied)

The 131 records are suitable as **regression fixtures**, with care.

### Proposed layout (not created)

```
tests/fixtures/production-decisions/
  manifest.json          # index: id, date, repo, status, reason_code
  accepted/*.json        # the 23 passes — must keep passing
  blocked/*.json         # a representative subset of the 98 blocks
  unavailable/*.json     # all 10, including both STALE_SERVER cases
```

### Rules

1. **Copy, never move.** Originals stay at `~/.local/share/griller/gate/`. The fixtures are
   a read-only mirror.
2. **Preserve bytes exactly.** No reformatting, no key reordering, no normalisation. The
   records are evidence.
3. **Assert only the verdict and reason_code.** Do **not** assert on `timing`, `tool`
   versions, or `commit` — these are environment facts that legitimately differ.
4. **The 23 `accepted` cases are the regression anchor.** If a future change causes any of
   them to block, that is a behaviour change requiring justification.
5. **The 2 `STALE_SERVER` cases are the most valuable fixtures in the set** — they prove
   fail-closed-on-mismatch. They should be tested explicitly.
6. **The 7 `repo=/` cases** are the fail-closed-on-unresolvable-repo path. Also worth pinning.

### Value

Nothing else in the workspace has a record of 131 real adversarial verifications with
embedded evidence. It is the only empirical proof that this component ever worked in anger.

---

## 8. Integrity note

`decisions/` is **not** in the repository and is **not** under version control. It lives only
at `~/.local/share/griller/gate/`. It is:

- not covered by any git history
- not backed up anywhere observed
- ~3.1 MB, so trivially backed up

**This is the highest-value un-backed-up artifact in the workspace.** Recommended as
`NEXT_ACTIONS.md` action 2.
