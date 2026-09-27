# EVIDENCE_BACKUP_STATUS.md

> **Status: Phase 1.3 record. Superseded by [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md).**
>
> The verification and same-disk classification below remain correct.
> [EVIDENCE_STATUS.md](EVIDENCE_STATUS.md) is the current document and
> additionally resolves the 133→131 `gate.log` discrepancy.

Phase 1.3 — re-verification of the historical evidence backup.
**Neither location was modified.** All commands were read-only.

---

## 1. Locations

| Role | Path |
|---|---|
| Original (live, read by the gate) | `~/.local/share/griller/gate/` |
| Backup (created Phase 1.1) | `~/.local/share/griller/evidence-backup-20260927T131225Z/` |

Both are outside any git repository, per the Phase 1.1 instruction to keep historical
evidence out of version control.

---

## 2. Re-verification results

| Check | Result |
|---|---|
| Decision files — source | **131** |
| Decision files — backup | **131** |
| `gate.log` — source | **13,689 bytes / 133 lines** |
| `gate.log` — backup | **13,689 bytes** |
| Per-file SHA256 comparison | **IDENTICAL — 132 / 132** |
| Aggregate (path-normalised hash-of-hashes), source | `6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817` |
| Aggregate, backup | `6303dc42886ac143a9c401ca6c2991162e737…` — see note |
| Originals unchanged | **Yes** — `gate.log` mtime still `2026-08-25 20:49:49.107476018 +0800` |

Aggregate note: both compute to
`6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817` — identical. The
`BACKUP_MANIFEST.txt` written into the backup in Phase 1.1 is excluded from the comparison
(it is backup metadata, not evidence).

**Result: the backup is byte-identical to the original, and the original has not been
touched since the evidence was produced (2026-08-25).**

---

## 3. Classification

> ### **B — two logical copies on the same physical disk.**

This is stated precisely, and the distinction is not pedantic.

| Test | Result | Consequence |
|---|---|---|
| Same device number (`stat -c %d`) | **66306 for both** | same filesystem |
| Same mount | `/dev/nvme0n1p2` (ext4) | same physical device |
| Different host? | No — both are under `/home/afnan` | single machine |
| Encrypted/independent? | No — `cp -a` copy | no additional protection |

**This is NOT an independent backup, and must not be described as one.** It protects against
*accidental* loss — a stray `rm -rf`, a bad script, a mistaken `git clean`, an over-eager
cleanup sweep. It provides **zero** protection against:

- disk or filesystem failure
- filesystem corruption
- accidental deletion of the whole `~/.local/share/griller/` tree
- ransomware or a destructive script targeting the home directory
- any host-level loss

**The evidence remains effectively single-copy from a durability standpoint.**

---

## 4. What the evidence is, and why it matters

| Property | Value |
|---|---|
| Records | 131 unique decision records (133 `gate.log` lines − 2 duplicates) |
| Date range | 2026-08-21 → 2026-08-25 |
| Schema | `g1.0` (131/131) |
| Size | 2.9 MB (`decisions/` 2,917,789 B + `gate.log` 13,689 B) |
| Parseable | 131 / 131 |
| Repositories gated | the-griller 92, conductor 15, koma 11, `/` 7, codebro 3, gumi 1, 2 fixtures |
| Verdicts | blocked 98 · accepted 23 · unavailable 10 |
| Integration health | 129 `connected, not stale` · 2 `STALE_SERVER` (fail-closed) |

**It is the only empirical proof that this component ever worked in anger** — 131 real
adversarial verifications, 74.8% of them refusals, with the full adjudication embedded in
each record. Nothing else in the workspace records the gate operating against live
in-development repositories.

**It is not in git** (correctly — it is runtime state, not source) and **not off-host**.

---

## 5. Durability options — described, not performed

**No external backup was created**, per instruction. For the owner's decision:

| Option | Protection gained | Cost / caveat |
|---|---|---|
| Leave as-is (two same-disk copies) | accidental deletion only | none |
| Add an off-host copy (sync to another machine / object storage) | disk loss, corruption, host loss | egress of session-level repository paths; requires a destination |
| Commit to a **private** repository (not this one) | versioned, host-independent | 92 of 131 records embed absolute repo paths; the store is runtime state, not source — a policy question |
| Encrypt then copy off-host | same as above without plaintext egress | key management burden for 2.9 MB |

**Assessment:** the cost here is low (2.9 MB) and the downside is losing the only field
evidence the project has. The blocker is not effort — it is a policy decision about whether
session-level absolute paths may leave the machine. That decision belongs to the owner.

---

## 6. Integrity statement

No file in either location was created, modified, deleted, or re-timed during this
verification. All operations were reads and comparisons.
