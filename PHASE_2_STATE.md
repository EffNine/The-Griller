# PHASE_2_STATE.md

**Authoritative summary of Phase 2 — timeout and process-cleanup hardening.**

Concise by design. Every statement is backed by the repository or by a recorded
measurement. Detail lives in the linked documents.

---

**PHASE:** 2 — execution reliability (timeout + process cleanup only; no new behaviour
outside that scope)

**COMMIT:** `0dcf9cd89c7de77ff2b22472e5a3fc94a267fb63`
**PARENT:** `bfa1be0f2c810b928e176691660ae6b203039999`
**MESSAGE:** `fix(griller): harden execution timeout cleanup`
**PUSHED:** **No** — 4 commits ahead of `origin/main`

**STATUS:** Complete. Both objectives delivered, plus two coverage gaps found en route.
Baseline and negative control recorded in [PHASE_2_BASELINE.md](PHASE_2_BASELINE.md).

---

**TIMEOUT:** **CLOSED**

The gate's outer MCP envelope was a hardcoded `120_000` while the per-attack replay budget
it wrapped was `600_000` — the envelope was smaller than its contents, so a replay needing
200 s was aborted at 120 s and reported as `GATE_UNAVAILABLE`.

Now derived, not duplicated
(`crates/griller-gate/src/config.rs:20`):

```rust
pub const DEFAULT_TIMEOUT_MS: u64 = griller_core::command::REPLAY_TIMEOUT_MS;
const _: () = assert!(DEFAULT_TIMEOUT_MS >= griller_core::command::REPLAY_TIMEOUT_MS);
```

The invariant is compiler-enforced, so the original defect cannot be reintroduced
silently. `griller-core` was added as a direct dependency of `griller-gate`; the module
boundaries allowed clean reuse, so **no duplicate constant was created**.

**Unchanged:** the 30 s `CommandSpec` default, the 600 s MCP ceiling, and
caller-specified timeout semantics. No timeout behaviour changed beyond this derivation.

→ [TIMEOUT_SEMANTICS.md](TIMEOUT_SEMANTICS.md)

---

**PROCESS GROUP:** **CLOSED on Linux** · **PLATFORM-LIMITED elsewhere**

| | |
|---|---|
| Spawn | `CommandExt::process_group(0)` → `setpgid(0,0)` in the child (`execution.rs:153`) |
| Timeout | `killpg(pid, SIGKILL)` (`execution.rs:195`) |
| Fallback | `child.kill()` retained (`execution.rs:210`) |
| Result | still `ExecutionStatus::TimedOut`, `exit_code: None` |
| Epistemic semantics | **untouched** — `status.rs`, `matcher.rs` not in the diff |

`process_group(0)` is the **safe** API and is used for setup precisely so that side
contributes nothing to the `unsafe` surface. Only teardown needs FFI.

**No supervisor, no daemon, no reaper, no new abstraction layer.** The child is still
spawned directly from the `CommandSpec` with no shell.

→ [PROCESS_GROUP_SEMANTICS.md](PROCESS_GROUP_SEMANTICS.md)

---

**DESCENDANT CLEANUP:** **CLOSED on Linux** — and the defect was worse than first stated

The original framing was "orphans linger". Measurement showed a **liveness** bug:

> `ExecutionBackend::execute` **never returns**. A surviving descendant inherits the piped
> `stdout`/`stderr` write ends, so `wait_with_output()` blocks waiting for an EOF that never
> arrives. The timeout fires and the child dies, but the call hangs. A gate invocation
> could block forever.

**Negative control** — `killpg` removed, everything else unchanged:

| Environment | Result |
|---|---|
| Host, 45 s bound | **hung, exit 124**, no result printed |
| Isolated container, 60 s bound | **hung**, no result printed |
| Restored (host) | passes in < 2 s |
| Restored (container) | passes in 8.3 s |

Verified in `crates/griller-runtime/tests/process_group.rs` (5 tests) using three
independent liveness signals: `/proc` PID liveness, tick-file stability, and `flock`
release.

---

**SECOND-RUN ISOLATION:** **PROVEN**

`orphan_from_a_timed_out_run_cannot_perturb_the_next_run` tests the actual risk rather than
a PID disappearing. RUN A times out while holding a lock and writing to a file; RUN B then
runs a normal successful command **in the same directory** and is asserted to:

- complete with `exit_code == Some(0)` and clean stdout;
- find the orphan's lock acquirable (it holds nothing);
- observe a stable tick file (it writes nothing);
- and RUN A's own result is never upgraded into a pass.

---

**PLATFORM:** **Linux supported; non-Unix PLATFORM-LIMITED**

Evidence from the repository: CI is `ubuntu-latest` only; the codebase had **zero**
`cfg(unix)`/`cfg(windows)` branches before Phase 2; no `[target.*.dependencies]`. Phase 2
introduced the first platform branch.

**On non-Unix, descendant termination is not proven, and the direct-child fallback retains
the descendant-liveness limitation in full** — the pipe-inheritance mechanism is not
platform-specific. No cross-platform supervision was implemented, and none is proposed as
a defect fix.

This is **not** a claim that non-Unix is broken. It is unverified, and its code shape is
the pre-Phase-2 shape. macOS/BSD take the same `cfg(unix)` branch and are untested.

→ [PLATFORM_EXECUTION_LIMITATIONS.md](PLATFORM_EXECUTION_LIMITATIONS.md)

---

**UNSAFE POLICY:** **relaxed in exactly one crate, by owner decision**

```rust
// crates/griller-runtime/src/lib.rs:21
#![deny(unsafe_code)]   // was #![forbid(unsafe_code)]
```

**One** `unsafe` block exists in the workspace:
`libc::killpg(pid as libc::pid_t, libc::SIGKILL)` in `terminate_process_group`
(`execution.rs:195`), with a SAFETY comment and a function-scoped
`#[allow(unsafe_code)]`. It is required because process-group teardown has no safe
standard-library equivalent.

**Negative control:** injecting `unsafe` elsewhere in `griller-runtime` produces
`error: usage of an unsafe block`. The relaxation permits exactly the one annotated
function. The other **7 crates retain absolute `forbid(unsafe_code)`**.

→ [UNSAFE_BOUNDARY.md](UNSAFE_BOUNDARY.md)

---

**DEPENDENCY CHANGE:** `libc` — already present, now direct

| Question | Answer |
|---|---|
| Why | `killpg(2)`, the only way to signal a process group |
| Where used | One call site: `execution.rs:195` |
| Already transitively present? | **Yes** — `Cargo.lock` at `bfa1be0` already contained a `libc` package entry, pulled in by `ring`, `rustix`, `mio`, `tokio`, `quinn-udp` |
| `Cargo.lock` changed? | **Yes, +2 lines** — `"libc"` added to `griller-runtime`'s dependency list, `"griller-core"` to `griller-gate`'s. **No new `[[package]]` entry, no version change, no new download** |

**Consequence:** the recorded project property *"zero third-party runtime dependencies"*
(now stated in [PHASE_HISTORY.md](PHASE_HISTORY.md) §1 and
[PHASE_1_3_RECOMMENDATION.md](PHASE_1_3_RECOMMENDATION.md)) is **inaccurate**. It is
corrected rather than deleted, because those documents describe the state at the time they
were written.

---

**TESTS:** 532 passed / 0 failed · 24/24 plugin · fmt clean · clippy 0 warnings

| Gate | Result |
|---|---|
| `cargo fmt --all -- --check` | PASS |
| `cargo check --workspace` | PASS |
| `cargo clippy --workspace --all-targets --all-features -- -D warnings` | 0 warnings |
| `cargo test --workspace` | **532 passed, 0 failed**, 6 ignored, 36 binaries |
| `npx vitest run` | 24 / 24 |

**Isolated container** (`rust:1.97-bookworm`): 531 passed + 1 failed = 532 total, matching
the host. The single failure is the pre-existing, environment-specific
`tests::self_grill_returns_limited_capability` in an **unmodified** file. **Zero new
failures in isolation.**

**+9 tests added:** 5 process-group, 1 stdin isolation, 1 replay-timeout, 2 gate timeout
invariant. Two coverage gaps were found and closed en route: stdin isolation and the
derived replay timeout both previously had **no** test.

**Preserved unchanged:** all 8 dirty-tree refusal tests green, including
`uncommitted_breakage_cannot_be_verified_from_a_passing_head`.

---

**OPEN ITEMS:**

| Item | Status |
|---|---|
| Outer/inner timeout margin | **NOT STARTED** — product decision, deliberately not taken |
| Non-Unix descendant cleanup and liveness | **PLATFORM-LIMITED** — unverified, no CI |
| Descendants surviving a *successfully completing* command | **OPEN** — not signalled; unchanged and out of Phase 2 scope |
| Trigger silent-skip has no auditable record | **OPEN** |
| `NO_COMPLETION_ASSERTION` record | **NOT STARTED** — the plugin cannot author one; needs a gate subcommand |
| `(Fact\|Design, Unverifiable) → AlwaysWarn` | **OPEN** — policy question |
| 4 unimplemented attack kinds | **OPEN** — capability gap, fail-closed |
| No kernel-level isolation | **OPEN** — truthfully reported as `NotEnforced` |
| `e2e_mock_opencode_griller.py` broken as written | **OPEN** |
| `~/.config/griller/gate.json` points at a non-existent path | **OPEN** — fails closed |
| Independent/off-host evidence backup | **OPEN** — blocked on a policy decision |
| Push decision (`github.com/EffNine/The-Griller`) | **OPEN** — 4 commits unpushed |

→ [NEXT_PHASE.md](NEXT_PHASE.md)
