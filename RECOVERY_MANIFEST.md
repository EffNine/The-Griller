# RECOVERY_MANIFEST.md

> **Status: Phase 1 record (RECOVERY, 2026-09-27T13:12:25Z).**
>
> Machine-readable recovery observations. Entries **L1** and **L6** have since
> been resolved; see the resolution table below. Current state:
> [CURRENT_STATE.md](CURRENT_STATE.md).

Machine-readable record of the Phase 1 recovery. Every value is observed, not assumed.

## Identity

| Field | Value |
|---|---|
| `recovery_timestamp_utc` | `2026-09-27T13:12:25Z` |
| `phase` | `1 — RECOVERY` |
| `source_path_actual` | `/home/afnan/projects/archive/legacy/the-griller` |
| `source_path_in_brief` | `/home/afnan/projects/archive/the-griller` — **does not exist** |
| `destination_path` | `/home/afnan/projects/active/verification-gate` |
| `destination_pre_existed` | `false` (verified: 15 projects in `active/`, none a verification gate) |

## Source repository state

| Field | Value |
|---|---|
| `source_commit` | `df002d32f85c83ecf0def25378f9ad04378a5bca` |
| `source_tree_hash` | `a83a61b9a266e2fa02f60eab97c17743e7ff5fe7` |
| `source_branch` | `main` |
| `source_commits_total` | `6` |
| `source_tag` | `v0.2.0` |
| `source_remote` | `https://github.com/EffNine/The-Griller.git` |
| `remote_reachable` | `true` — `git ls-remote origin` → `df002d32…` == local HEAD (history is pushed) |
| `source_stashes` | `0` |
| `source_dirty_files` | `16` (0 staged, 16 unstaged) |
| `source_dirty_diff_sha256` | `bd0ba55aa34f0feff0f0ff335f6a43723add740dffe014b7dde58f7c57ea9067` |
| `source_dirty_stat` | `16 files changed, 939 insertions(+), 99 deletions(-)` |
| `version` | `0.2.0` (`Cargo.toml`) |
| `license` | `MIT OR Apache-2.0` |
| `edition` | `2021` |
| `rust_toolchain_pinned` | none (no `rust-toolchain.toml`); built with `rustc 1.97.1` / `cargo 1.97.1` |

## Commit history (complete — 6 commits, all authored `EffNine`)

```
df002d3  2026-08-22  feat(griller): multi-language test replay and numeric assertion support
40016d3  2026-08-21  release: The Griller v0.2.0 — automatic completion gate and security hardening
1e08396  2026-08-20  chore: update Cargo.lock for griller-gate workspace member
87ed1e3  2026-08-20  Fix tests broken by initializing the workspace as a git repository
1baa223  2026-08-20  Clean up versioned fixture artifacts
aacd99a  2026-08-20  The Griller: deterministic verification layer for agent claims (M0-M8)
```

> **History is squashed.** `aacd99a` is a single `commit (initial)` containing the entire
> M0–M8 build. The reflog (8 entries) confirms no finer-grained history exists in this copy.
> Granular per-milestone history is **not recoverable from this repository**.

## Size and inventory

| Field | Value |
|---|---|
| `crate_count` | `9` |
| `rust_files` | `131` |
| `rust_lines` | `32,885` |
| `tracked_files` | `208` |
| `test_attributes_static` | `516` (`#[test]` + `#[tokio::test]`) |
| `test_files` | `30` |
| `cfg_test_modules` | `42 files` |
| `mcp_tools` | `6` |
| `doc_files` | `25` (incl. `docs/history/` M2–M8) |
| `size_excl_generated` | `7.5 MB` (source) + `6.4 MB` (`.git`) |
| `size_after_recovery` | `3.4 GB` (includes the cold `target/` produced by the baseline build) |
| `source_archive_size_before` | `18 GB` (17 GB of which was `target/`) |

### Per-crate source

| Crate | Files | Lines |
|---|---:|---:|
| `griller-context` | 15 | 6,932 |
| `griller-adjudicate` | 19 | 5,846 |
| `griller-runtime` | 26 | 4,767 |
| `griller-mcp` | 9 | 3,340 |
| `griller-gate` | 13 | 2,832 |
| `griller-core` | 12 | 2,600 |
| `griller-self-grill` | 5 | 2,332 |
| `griller-policy` | 13 | 2,263 |
| `griller-cli` | 5 | 1,814 |

## Evidence record counts

| Field | Value |
|---|---|
| `evidence_store_path` | `~/.local/share/griller/gate/` (**outside the repo; not copied, not moved, not mutated**) |
| `decision_records` | `131` |
| `gate_log_lines` | `133` |
| `gate_log_duplicate_lines` | `2` (timestamps `1787323028989`, `1787320284381` each appear twice → 133 − 2 = 131 unique) |
| `decision_record_schema` | `g1.0` (131/131) |
| `decision_records_parseable` | `131 / 131` (0 unparseable) |
| `decision_store_size` | `3.1 MB` |
| `decision_date_range` | `2026-08-21` → `2026-08-25` |
| `decisions_with_live_server` | `129 / 131` (`connected=true, stale=false`) |
| `decisions_fail_closed_stale` | `2 / 131` (`STALE_SERVER`, version 0.1.0 vs expected 0.2.0) |
| `evidence_inside_repo` | none (`griller-data/` does not exist; `.gitignore` reserves it) |

## Verification baseline (observed, Phase 1)

| Check | Command | Result |
|---|---|---|
| format | `cargo fmt --all -- --check` | **PASS** exit 0 |
| compile | `cargo check --workspace` | **PASS** exit 0, 0 warnings |
| test | `cargo test --workspace` | **PASS** 515 passed / 0 failed / 6 ignored / 43 binaries |
| lint | `cargo clippy --workspace --all-targets --all-features -- -D warnings` | **PASS** exit 0 |
| plugin tests | `npx vitest run` | **PASS** 23/23 |
| cold build | from empty `target/` | **PASS** |

`e2e_mock_opencode_griller.py` (repo root): **NOT RUN** — requires a live MCP server + a
mock agent session; it is out of scope for a read-only recovery baseline. See
`BASELINE_VERIFICATION.md`.

## Known limitations (carried forward, not fixed)

| # | Limitation | Location | Severity |
|---|---|---|---|
| L1 | Replay verifies committed `HEAD`, not uncommitted edits | `crates/griller-runtime/src/worktree.rs:107-110` | **HIGH** — the headline claim ("verifies agent edits") is false for dirty trees |
| L2 | Gate default timeout 120 s < replay budget 600 s | `crates/griller-gate/src/config.rs:10` vs `crates/griller-core/src/command.rs:21` | **MEDIUM** — cold replays fail closed as `GATE_UNAVAILABLE` |
| L3 | Timeout kills direct child only; no process-group kill | `crates/griller-runtime/src/execution.rs:189` | **MEDIUM** — orphaned grandchildren |
| L4 | 4 of 8 attack kinds unimplemented → `AdversariallyVerified` unreachable | `crates/griller-mcp/src/plan.rs:52-57` | **MEDIUM** — fail-closed (rejected, not faked) |
| L5 | No kernel-level isolation; truthfully reported as `NotEnforced` | `griller-core/src/command.rs` `Enforcement::m1_none()` | LOW (honest, by design) |
| L6 | 16 uncommitted files (completion-assertion feature) | see `DIRTY_STATE_ANALYSIS.md` | **MEDIUM** — at risk until committed |
| L7 | `~/.config/griller/gate.json` points at a non-existent path | `~/.config/griller/` | LOW — fails closed |
| L8 | History squashed into one initial commit | `.git/logs/HEAD` | LOW — unrecoverable from this repo |
| L9 | Docs understate test count (424 documented vs 515 executed) | `docs/testing.md:9` | LOW — docs stale, not code |

### Resolution status of the above

The table above is the **Phase 1 observation** and is preserved as written. Two entries have
since been resolved:

| # | Status | Resolved in |
|---|---|---|
| **L1** | **RESOLVED** — a dirty repository is now `UNVERIFIABLE`. The trust boundary this entry described no longer exists. | `16c6852` (2026-09-27) |
| **L6** | **RESOLVED** — the 16 files are committed. | `a360b7c` (2026-09-27) |

L2, L3, L4, L5, L7, L8 and L9 remain open, unchanged, and are carried in
[CURRENT_STATE.md](CURRENT_STATE.md) §5. The 515/23 figures in the verification table above
are the Phase 1 baseline; the current counts are 523 and 24.

## Known dirty-state gaps

See `DIRTY_STATE_ANALYSIS.md` for the full analysis. Summary:

1. **Verification gap (L1)** — the isolated worktree is a checkout of `HEAD`; uncommitted
   work is invisible to verification. This is documented correctly in the project's own
   `docs/limitations.md:34-38` and `docs/history/worktree-isolation.md:39-40`.
2. **The 16 uncommitted files** are a *repository-hygiene* dirty state, not a verification
   dirty state. They are preserved and tested but not committed.
   > **SUPERSEDED by `a360b7c` (2026-09-27).** Now committed.

   Item 1 above — the verification gap itself — is separately **SUPERSEDED by `16c6852`**
   (2026-09-27): a dirty tree is now `UNVERIFIABLE`. See
   [DIRTY_STATE_POLICY_DECISION.md](DIRTY_STATE_POLICY_DECISION.md) and
   [CURRENT_STATE.md](CURRENT_STATE.md) §3.

## Explicitly not done in Phase 1

- MCP re-registration (`~/.config/opencode/opencode.jsonc` untouched)
- Plugin installation (`~/.config/opencode/plugins/` untouched)
- Integration with CodeBro / Conductor / Infra / Atlas / computer-use
- Any microservice or architecture change
- Any history rewrite
- Any dependency upgrade
- Any mutation of `~/.local/share/griller/gate/`
- Any commit of the pre-existing dirty work

**Not registered. Not running. Not integrated. Not production-ready.**
