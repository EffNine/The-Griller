# PLATFORM_EXECUTION_LIMITATIONS.md

**What is proven about timeout cleanup, and on which platforms.**

The honest boundary. Phase 2 implemented and proved process-group teardown on Linux. It
did **not** make the behaviour universal, and this document exists so that is not
over-read.

Implementation detail: [PROCESS_GROUP_SEMANTICS.md](PROCESS_GROUP_SEMANTICS.md).
Measurements: [PHASE_2_BASELINE.md](PHASE_2_BASELINE.md).

---

## 1. Platform evidence, taken from the repository

| Signal | Finding |
|---|---|
| CI platform — `.github/workflows/ci.yml:12` | **`ubuntu-latest` only** |
| `cfg(unix)` / `cfg(windows)` / `cfg(target_os)` in `crates/*/src/*.rs`, before Phase 2 | **0 occurrences** — the codebase had no platform branching at all |
| `[target.'cfg(...)'.dependencies]` in any `Cargo.toml` | none |
| Platform declared in package metadata | none |
| `unsafe_code` policy per crate | uniform, 8/8 crates |

The repository is **Linux-tested and platform-agnostic in style**. It never claimed
Windows or macOS support, and it made no cross-platform promise that Phase 2 has broken.
Phase 2 added the *first* platform branch in the codebase.

---

## 2. Supported and PROVEN — Linux

Verified in two environments: the host, and an isolated `rust:1.97-bookworm` container.

| Property | Status | How it was proven |
|---|---|---|
| Process-group creation (`setpgid(0,0)` via `CommandExt::process_group(0)`) | **PROVEN** | exercised by all 5 tests in `tests/process_group.rs` |
| Group termination (`killpg(SIGKILL)`) | **PROVEN** | negative control: removing it makes the core test hang (exit 124 at 45 s) |
| Descendant termination | **PROVEN** | `timeout_terminates_descendants_not_just_the_direct_child` — `/proc` PID gone within 5 s |
| Descendant stops writing | **PROVEN** | `descendant_stops_writing_after_timeout` — tick file stable across 400 ms |
| Descendant releases its lock | **PROVEN** | `descendant_releases_its_lock_on_timeout` — `flock` acquirable within 5 s |
| Timeout liveness (function returns) | **PROVEN** | same core test; hangs without `killpg`, returns in < 2 s with it |
| Second-run isolation | **PROVEN** | `orphan_from_a_timed_out_run_cannot_perturb_the_next_run` — RUN B completes clean; orphan holds no lock, writes nothing, alters no result |
| Timeout stays fail-closed | **PROVEN** | `descendant_timeout_is_fail_closed` — `TimedOut`, `exit_code: None`, `succeeded() == false` |

Container prerequisites, confirmed present: `sh`, `flock`, `sleep`, `/proc`, `pid_max`
4194304. `setpgid`/`killpg` require no privilege; verified by behaviour, not by assertion.

---

## 3. NOT PROVEN — non-Unix

**PLATFORM-LIMITED.** On any platform that is not Unix, `isolate_process_group` is a no-op
(`execution.rs:158`) and `terminate_process_group` calls only `terminate_direct_child`
(`execution.rs:203`). The consequence:

| Property | Non-Unix status |
|---|---|
| Process-group termination | **NOT IMPLEMENTED** |
| Descendant termination | **NOT PROVEN** — descendants are not signalled at all |
| Absence of the liveness issue | **NOT PROVEN** — the §3 mechanism in `PROCESS_GROUP_SEMANTICS.md` applies identically: descendants inherit the piped stdout/stderr write ends, so `wait_with_output()` can still block past the timeout |
| Second-run isolation from orphans | **NOT PROVEN** |

> **Explicit statement:** on non-Unix platforms, descendant termination is **not proven**,
> and the direct-child fallback **retains the descendant-liveness limitation in full**. The
> liveness hang is not Linux-specific — it follows from pipe inheritance, which is not a
> platform-specific behaviour. It is untested there because nothing is tested there.

**No cross-platform process supervision was implemented in Phase 2, and none is proposed
as a defect fix.** Doing so would mean either a supervisor process, a platform-specific
container/job-object abstraction, or accepting Windows' different process model — all of
which are architectural additions, not execution hardening.

**This is not a claim that the non-Unix path is broken.** It is a statement that it is
**unverified**, and that its known code shape is the pre-Phase-2 shape.

---

## 4. The `flock` test skip — assessed and accepted

`descendant_releases_its_lock_on_timeout` returns early (reporting "skipping" on stderr,
without being marked `#[ignore]`) when `flock(1)` is not on `PATH`. The same conditional
guards the lock assertion in the second-run test.

**Assessment: the skip is acceptable, and that was verified rather than assumed.**

| Question | Finding |
|---|---|
| Do the decisive signals still run without `flock`? | **Yes.** PID liveness and tick-file stability are unconditional; only the lock assertion is conditional. |
| Does the descendant script itself need `flock`? | **No.** It runs `flock -x 9 2>/dev/null \|\| true`, so the lock is simply not taken and the descendant still ticks. |
| Verified empirically? | **Yes.** The test binary was re-run with a `PATH` containing `sh`, `dash`, `sleep`, `cat`, `touch`, `mkdir`, `rm`, `sed`, `grep`, `head`, `tail` and **no `flock`**. All 5 tests passed (6.82 s). |
| Does the core regression still bite without `flock`? | **Yes** — `timeout_terminates_descendants_not_just_the_direct_child` is unconditional and is the test that catches the removed-`killpg` hang. |
| Is the skip documented? | **Yes** — here, and in `PROCESS_GROUP_SEMANTICS.md` §6. |

**Accepted because the strongest signal is additive, not load-bearing.** The cost of the
skip is that a `flock`-less host reports green without the strongest of the three liveness
signals. That is a real, bounded, documented reduction in coverage on such a host.

**Not changed, deliberately:** no dependency was added to make the test mandatory, and the
test was not made `#[ignore]`-based (which would hide it on hosts that *do* have `flock`).
On both this host and the container, `flock` is present and the full signal set runs.

---

## 5. Summary

| Platform | Process group | Descendant cleanup | Timeout liveness | Status |
|---|---|---|---|---|
| **Linux** (Ubuntu CI) | implemented | proven | proven | **SUPPORTED** |
| **macOS / BSD (Unix)** | implemented (`setpgid`/`killpg` are POSIX) | not tested | not tested | **UNVERIFIED** — same code path as Linux, no CI |
| **Windows** | not implemented | not proven | not proven | **PLATFORM-LIMITED** — direct-child fallback only |

The `cfg(unix)` guard means macOS and BSD take the *same* branch as Linux. The mechanism is
standard POSIX, so it is very likely to work there — but "likely" is not "proven", and
nothing in this repository tests it.
