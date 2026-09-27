# EVIDENCE_STATUS.md

**Status of the 131 production gate decision records.**

The evidence itself is **not** in this repository and **must not be copied into it**. It is
runtime state under `~/.local/share/griller/`, which is the correct place for it: it is
machine-generated operational output, not source, and 92 of the 131 records embed absolute
repository paths from the machine that produced them.

Scope note: this file records the state of the evidence and its backup.
[EVIDENCE_BACKUP_STATUS.md](EVIDENCE_BACKUP_STATUS.md) is the Phase 1.3 document that first
established the backup and is preserved as a point-in-time record. Where they differ, this
file is current.

---

## 1. Inventory

| Property | Value |
|---|---|
| Location | `~/.local/share/griller/gate/` |
| Decision records | **131** (`decisions/*.json`) |
| `gate.log` | **133 lines**, 13,689 bytes |
| Record schema | `g1.0` — 131/131 |
| Embedded report schema | `m8.0` |
| Date range | 2026-08-21 → 2026-08-25 |
| Total size | ≈ 2.9 MB |
| In version control | **No** — deliberately |
| Modified during recovery | **No** |

The evidence is the **only empirical proof that this component ever worked in anger** — 131
real adversarial verifications with the full adjudication embedded in each record. Nothing
else in the workspace records the gate operating against live in-development repositories.

---

## 2. The 133 → 131 discrepancy, resolved

`gate.log` has **133** lines but there are **131** decision files. This is not data loss.

**Cause: 2 duplicated log lines.** Exactly 2 decision ids appear twice in `gate.log`:

| Duplicated id | Log lines |
|---|---|
| `1787320284381` | 2 |
| `1787323028989` | 2 |

133 lines − 2 duplicate entries = **131 unique decision records**, which matches the file
count exactly. Every log line has a corresponding record; no record is missing and no log
line is orphaned.

The duplication is consistent with the gate re-issuing a lifecycle event for the same
decision — it is a property of the log format, not evidence corruption.

---

## 3. Backup status

| Property | Value |
|---|---|
| Backup path | `~/.local/share/griller/evidence-backup-20260927T131225Z/` |
| Created | Phase 1.1, timestamp `20260927T131225Z` |
| Decision records | **131** — matches source |
| `gate.log` | **13,689 bytes** — matches source |
| Per-file content comparison | **IDENTICAL — 132 / 132** (131 records + `gate.log`) |
| Aggregate hash, source | `6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817` |
| Aggregate hash, backup | `6303dc42886ac143a9c401ca6c2991162e8170d56453899f780b692194afd817` |
| Originals modified | **No** — `gate.log` mtime still `2026-08-25 20:49:49.107476018 +0800` |

The aggregate is a **path-normalised hash-of-hashes**: each file is hashed, the
`path hash` lines are sorted, and the result is hashed. Normalisation is required because
`sha256sum` embeds the file path in its output, so the same content at two different paths
produces different lines. One decision filename contains spaces
(`1787319906750-live-LIVE-FORENSIC (user-described msg).json`), so any comparison must use
`find -print0 | xargs -0` — a plain `xargs` splits on whitespace and silently miscounts.

`BACKUP_MANIFEST.txt` inside the backup is excluded from the comparison: it is backup
metadata, not evidence.

---

## 4. There is no independent backup

| Test | Result |
|---|---|
| Same device number (`stat -c %d`) | **66306 for both** |
| Same mount | `/dev/nvme0n1p2` (ext4) |
| Different host | **No** — both under `/home/afnan` |
| Encrypted / independent | **No** — plain `cp -a` copy |

**This is two logical copies on one physical disk. It is not an off-host backup and must
not be described as one.**

It protects against **accidental loss** — a stray `rm -rf`, a bad script, a mistaken
`git clean`, an over-eager cleanup sweep. It provides **zero** protection against:

- disk or filesystem failure
- filesystem corruption
- deletion of the whole `~/.local/share/griller/` tree
- any host-level loss

**From a durability standpoint the evidence remains effectively single-copy.**

---

## 5. Durability options — recorded, not performed

No external or off-host backup has been created. For the owner's decision:

| Option | Protection gained | Cost / caveat |
|---|---|---|
| Leave as-is | accidental deletion only | none |
| Off-host copy (another machine, object storage) | disk loss, corruption, host loss | egress of session-level absolute paths; needs a destination |
| Encrypted off-host copy | same, without plaintext egress | key management for 2.9 MB |
| Private repository (not this one) | versioned, host-independent | 92 records embed absolute paths; the store is runtime state, not source — a policy question |

**Assessment:** the cost is low (2.9 MB) and the downside is losing the only field evidence
the project has. The blocker is not effort — it is a policy decision about whether
session-level absolute paths may leave the machine. That belongs to the owner.

---

## 6. Integrity statement

During Phase 1 and every phase since, no file in
`~/.local/share/griller/gate/` has been created, modified, deleted, or re-timed. All
operations against it have been reads and comparisons. The evidence remains exactly as
produced on 2026-08-25.
