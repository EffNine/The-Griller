# PROCESS_GROUP_SEMANTICS.md

**Authoritative description of timeout teardown after Phase 2.**

Implemented in commit `0dcf9cd89c7de77ff2b22472e5a3fc94a267fb63`. Measurements and the
negative control: [PHASE_2_BASELINE.md](PHASE_2_BASELINE.md) §3 and §3.1. Platform
boundary: [PLATFORM_EXECUTION_LIMITATIONS.md](PLATFORM_EXECUTION_LIMITATIONS.md).

---

## 1. What was implemented, precisely

Three small functions in `crates/griller-runtime/src/execution.rs`:

| Location | Function | Behaviour |
|---|---|---|
| `:153` | `isolate_process_group` | `cmd.process_group(0)` on Unix — issues `setpgid(0, 0)` in the child between `fork` and `exec`, making it a group leader whose PGID equals its PID. No-op on non-Unix. |
| `:182` | `terminate_process_group` | `killpg(pid, SIGKILL)` on Unix, then falls through to `terminate_direct_child`. Direct child only on non-Unix. |
| `:209` | `terminate_direct_child` | `child.kill()` — the original behaviour, retained as a fallback. |

Call sites, both unchanged in shape:

- spawn: `isolate_process_group(&mut cmd)` immediately before `cmd.spawn()`.
- timeout: `terminate_process_group(&mut child)` at the point where `child.kill()` used to
  be, inside the polling loop.

The polling loop, the 5 ms tick, `wait_with_output()`, and the result construction are all
**unchanged**. No supervisor, no daemon, no background reaper, no new abstraction layer.
The child is still spawned directly from the `CommandSpec` with no shell.

### Why `process_group(0)` and not `pre_exec`

`Command::pre_exec` would permit arbitrary code between `fork` and `exec`, and that code
must be async-signal-safe — a class of restriction that is easy to violate accidentally.
`CommandExt::process_group` is **safe**, is implemented internally as `setpgid`, and
cannot be used to smuggle in unsafe work. The safe API is used precisely so the setup side
contributes nothing to the `unsafe` surface.

---

## 2. Why it is necessary

`std::process::Child::kill` sends `SIGKILL` to the direct child only. A `cargo test` replay
spawns `rustc` and linker grandchildren. Killing the direct child reparents those
grandchildren to init, where they keep running.

---

## 3. The liveness failure discovered when `killpg` was disabled

**This was not the originally-stated defect, and the severity was upgraded after
measurement.**

The original framing was "orphans linger and may perturb a later verification" — a
reproducibility and tidiness concern. Measuring the neutered implementation showed
something worse:

> **`ExecutionBackend::execute` does not return at all.**

The mechanism:

1. The direct child is spawned with `stdout` and `stderr` set to `Stdio::piped()`.
2. Those pipes are **inherited by every descendant** the child spawns.
3. On timeout, only the direct child is killed. Its pipe write-end closes, but the surviving
   grandchildren still hold their own copies of the write ends.
4. `wait_with_output()` (`execution.rs:202`) reads both pipes to EOF **before** returning.
5. EOF never arrives, because a live process still holds the write end open.
6. `execute()` blocks **indefinitely past its own timeout**.

The timeout is honoured — the child *is* killed — but the function never comes back. The
gate would wait forever for a decision that will never arrive.

### Negative control

With `killpg` removed and everything else unchanged:

| Environment | Result |
|---|---|
| Host, 45 s hard bound | **hung, exit 124**, no `test result:` line printed |
| Isolated container, 60 s hard bound | **hung**, no `test result:` line printed |
| Restored, host | passes in < 2 s |
| Restored, container | passes in 8.3 s |

The test used is `timeout_terminates_descendants_not_just_the_direct_child`. It never
completes without the group signal, which is what makes it a real regression test rather
than a decorative one.

---

## 4. What was deliberately NOT changed

The fail-closed contract is structural and was left exactly as it was
(`execution.rs:205-214` derives the result from the `timed_out` flag, not from the exit
code):

```rust
let exit_code = if timed_out { None } else { output.status.code() };
let status    = if timed_out { ExecutionStatus::TimedOut } else { ExecutionStatus::Completed };
```

So a timeout yields `ExecutionStatus::TimedOut`, `exit_code: None`, and
`succeeded() == false`, regardless of whether the group signal succeeded. **Termination is
best-effort and cannot alter the reported status.**

| Untouched | Evidence |
|---|---|
| Epistemic semantics | `status.rs`, `matcher.rs` not in the diff |
| `CommandOutcome::TimedOut` → `Assessed::Unverifiable` | unchanged; still non-passing |
| Environment isolation (`env_clear` + allowlist) | unchanged |
| Replay locking (`ReplayLock`) | unchanged |
| stdout/stderr capture | unchanged — now simply also closed reliably on timeout |
| Exit-code capture on normal completion | unchanged |
| Evidence generation | unchanged |
| Dirty-tree refusal | unchanged; all 8 dirty-state tests green |

`descendant_timeout_is_fail_closed` asserts the fail-closed contract **before** any
liveness assertion, so a regression there would be reported even if the teardown worked.

---

## 5. What this is not

This is **not** a process supervisor, and it should not be described as one.

- There is no reaper process, no daemon, no monitoring thread, and no restart logic.
- There is no attempt to clean up descendants on *normal* exit — only on timeout.
- Descendants that outlive a **successfully completing** command are not signalled. If a
  command exits 0 while leaving a background grandchild, that grandchild survives. This is
  unchanged from before Phase 2 and is **not** addressed by this work.
- The mechanism is a POSIX process-group signal, not containment. It does not prevent a
  descendant from doing damage in the window before the timeout fires.

Scope is exactly: *on timeout, terminate the whole process group instead of only the direct
child.*

---

## 6. Evidence signals used by the tests

Three independent liveness signals, so no single one is load-bearing
(`crates/griller-runtime/tests/process_group.rs`):

| Signal | Mechanism | Needs sleeping? | Always runs |
|---|---|---|---|
| PID liveness | descendant PID written to a file, then checked against `/proc` | bounded poll | yes |
| Tick-file stability | descendant appends forever; file must stop growing | yes (400 ms window) | yes |
| `flock` release | descendant holds an exclusive lock; the test acquires it | no | **no — skips if `flock(1)` is absent** |

The readiness marker (`ready` file) is asserted in all three: it proves the descendant was
really alive when the timeout fired, so a pass cannot be an accident of the script never
having started.

**The `flock` skip was verified to be safe**, not assumed — see
[PLATFORM_EXECUTION_LIMITATIONS.md](PLATFORM_EXECUTION_LIMITATIONS.md) §4.
