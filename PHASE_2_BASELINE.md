# PHASE_2_BASELINE.md

**Phase 2 — execution reliability. Inspection only; no code was changed to produce this
document.**

For the current repository state see [CURRENT_STATE.md](CURRENT_STATE.md). For the
sequence this phase implements see [NEXT_PHASE.md](NEXT_PHASE.md) steps 1–3.

---

## 1. The execution path, end to end

```
griller-gate                      crates/griller-gate/
  └─ GateConfig.timeout_ms        config.rs:66 -> default_timeout() -> DEFAULT_TIMEOUT_MS (120_000)
     └─ McpClient::connect(.., timeout)          mcp_client.rs:59, :91
        └─ request_with_timeout(..)              mcp_client.rs:199, :215  <-- OUTER ENVELOPE deadline
           └─ griller.check_completion  (stdio)
              └─ run_completion_check           griller-mcp/src/completion.rs:75
                 ├─ replay attack planned       plan.rs:368 -> .with_timeout(REPLAY_TIMEOUT_MS)
                 ├─ ReplayLock::acquire         completion.rs:602, :653
                 └─ run_command_in_isolation     completion.rs:688
                    └─ Worktree::create(repo)     <-- isolated checkout of HEAD
                    └─ CommandExecutionCollector::collect(cmd, worktree_dir)
                       └─ ExecutionBackend::execute   griller-runtime/src/execution.rs:142
                          ├─ Command::spawn        execution.rs:164
                          ├─ poll loop             execution.rs:186-200
                          │    └─ timeout -> child.kill()   execution.rs:189
                          └─ wait_with_output      execution.rs:202
```

---

## 2. The three constants, confirmed from source

| Constant | Value | Location | Role |
|---|---:|---|---|
| `griller_gate::DEFAULT_TIMEOUT_MS` | **120_000** | `crates/griller-gate/src/config.rs:10` | **Outer** MCP round-trip envelope for the whole `check_completion` call |
| `griller_core::REPLAY_TIMEOUT_MS` | **600_000** | `crates/griller-core/src/command.rs:21` | **Inner** per-attack command budget for derived replay commands |
| `griller_mcp::MAX_TIMEOUT_MS` | **600_000** | `crates/griller-mcp/src/validate.rs:21` | MCP-request **ceiling**; its own doc comment says *"Aligned with `REPLAY_TIMEOUT_MS`"* |
| `griller_core::DEFAULT_TIMEOUT_MS` | 30_000 | `crates/griller-core/src/command.rs:13` | `CommandSpec` default when no timeout is set. **Not in scope** — leave unchanged |

### Are they used as assumed? Yes — with one important correction to the framing

- **Gate 120 s is used.** `default_timeout()` (`config.rs:66`) feeds
  `GateConfig.timeout_ms`, which becomes `McpClient.timeout` and is the deadline in
  `request_with_timeout` (`mcp_client.rs:215`).
- **Replay 600 s is used.** `plan.rs:368` applies it to every derived replay command;
  `completion.rs:602` and `:653` also use it as the `ReplayLock` acquisition wait.
- **MCP ceiling 600 s is used.** `validate_timeout` (`validate.rs:77`) rejects anything
  above it.

**The correction:** the two timeouts are not the same kind of quantity. 120 s bounds the
**entire pipeline call** (context + evidence + attacks + self-grill). 600 s bounds **one
attack's command**. The defect is therefore not merely "the numbers disagree" — it is that
the **outer envelope is smaller than the inner budget it contains**, so a replay that
legitimately needs 200 s is aborted at 120 s and the gate reports a failure for work that
would have succeeded.

**Consequence for the fix:** making the gate default equal to `REPLAY_TIMEOUT_MS` removes
the divergence, but leaves the outer envelope equal to a single inner budget rather than
larger than it. See §6.

---

## 3. The defect, precisely

`execution.rs:189`:

```rust
let _ = child.kill();
```

`std::process::Child::kill` sends `SIGKILL` to the direct child **only**. The codebase
contains **zero** occurrences of `setpgid`, `killpg` or `setsid`:

```
grep -rcE 'setpgid|killpg|setsid' crates/*/src/*.rs   ->  0 files
```

A `cargo test` replay spawns `rustc` and linker grandchildren. On timeout the direct child
dies and those grandchildren are **reparented to init and keep running**. They can:

- keep writing into a temporary worktree that `run_command_in_isolation` then removes;
- hold a `cargo` target-directory lock;
- perturb a *later* verification in the same environment.

This is a **reproducibility** defect, not a verdict defect. The evidence model is
content-addressed, so a perturbation that changes what a later replay observes breaks the
property the whole system rests on.

### 3.1 Severity upgraded after measurement — it is also a liveness bug

The first framing above understated the defect. Measured during implementation, with
`killpg` removed and everything else unchanged:

**`ExecutionBackend::execute` does not return at all.** The surviving descendant inherits
the piped `stdout`/`stderr` write ends, so the call to `wait_with_output()`
(`execution.rs:202`) blocks waiting for EOF that never comes. The 1.5 s timeout is
honoured — the child *is* killed — but the function never comes back.

Verified: the regression test
`timeout_terminates_descendants_not_just_the_direct_child` was run with `killpg`
neutered and a 45 s hard bound. It **hung for the full 45 s and exited 124**, never
printing a result. With `killpg` restored the same test passes in under 2 s.

So the defect is *not* only "orphans linger". A gate invocation can **hang past its own
timeout indefinitely**, which for an interactive completion gate means the agent waits
forever for a decision that will never arrive. That makes the fix a reliability
requirement rather than a tidy-up, and it is why this was escalated rather than deferred.

---

## 4. The safety property is already structural — and must stay that way

`execution.rs:205-214` derives the result from the `timed_out` flag, not from the child's
exit code:

```rust
let exit_code = if timed_out { None } else { output.status.code() };
let status    = if timed_out { ExecutionStatus::TimedOut } else { ExecutionStatus::Completed };
```

So a timeout **cannot** produce a success, and `exit_code` is `None`. Downstream,
`CommandOutcome::TimedOut` maps to `Assessed::Unverifiable` ("the outcome is undetermined"),
which is non-passing by construction.

**This phase must not touch epistemic semantics.** `status.rs` and the matcher are out of
scope; the fix is confined to how the process tree is torn down.

---

## 5. Platform reality, from the repository itself

| Signal | Finding |
|---|---|
| `cfg(unix)` / `cfg(windows)` / `cfg(target_os)` in `crates/*/src/*.rs` | **0 occurrences** — no platform branching anywhere today |
| CI platforms (`.github/workflows/ci.yml:12`) | **`ubuntu-latest` only** |
| Declared platforms in `Cargo.toml` | none — no `[target.'cfg(...)'.dependencies]` |
| Crate that would need branching | `griller-runtime`, where the process is spawned |

**Conclusion: Linux is the only platform the repository tests or claims.** The smallest
correct mechanism is POSIX `setpgid` + `killpg`, guarded by `cfg(unix)`, with an explicit
documented fallback elsewhere rather than faked portability.

---

## 6. Dependency finding — this is a real architectural change

**`libc` is not currently a dependency of any workspace crate.** The project's "zero
third-party runtime dependencies" was recorded as a *strength* in
[PHASE_HISTORY.md](PHASE_HISTORY.md) §1.

Killing a process **group** requires `killpg(2)` or `kill(-pgid, sig)`. There is no
standard-library equivalent — `std` offers `Child::kill` (direct child only) and, since Rust
1.64, the **safe** `CommandExt::process_group(pgroup)` for the spawn side only.

| Option | Assessment |
|---|---|
| Add `libc` to `griller-runtime` | **Recommended.** It is the standard binding layer, it is what `std` itself uses, and — critically — **`libc 0.2.189` is already in `Cargo.lock` and already compiled into the tree** (pulled in by `ring`, `rustix`, `mio`, `tokio`, `quinn-udp` and others). Adding it as a direct dependency introduces **no new download and no new build cost**. |
| Hand-declared `extern "C" { fn killpg(..) -> i32; }` | Avoids the dependency but introduces hand-written FFI and `unsafe` into a codebase that currently has almost none. More likely to be wrong, harder to review. |
| Shell wrapper around the command | **Rejected outright** — it would destroy the shell-free design that is a stated safety property (`execution.rs:5-6`). |

**This is a disclosed change to the dependency profile.** It is a judgement call, and the
zero-dependency alternative exists if the owner prefers it.

---

## 7. Test baseline

| Location | Count | Notes |
|---|---:|---|
| `crates/griller-runtime/src/execution.rs` | **0** unit tests | the implementation is untested in-crate |
| `crates/griller-runtime/tests/execution.rs` | **14** integration tests | includes `command_timeout_kills_the_process` (`sleep 10`, 200 ms timeout) — direct child only, no descendant |
| `crates/griller-runtime/tests/hardening.rs` | present | unrelated area |

**Coverage gap that matters here:** no existing test asserts anything about *descendants*.
`command_timeout_kills_the_process` would pass unchanged whether or not grandchildren
survive. That is precisely the untested risk this phase closes.

---

## 8. Pre-change verification state

| Gate | Result at `bfa1be0` |
|---|---|
| `cargo test --workspace` | 523 passed / 0 failed / 6 ignored, 35 binaries |
| `npx vitest run` | 24 / 24 |
| `cargo fmt --all -- --check` | clean |
| `cargo clippy -- -D warnings` | 0 warnings |
| Production source blob manifest | `af2134dc604521f3f6f2b44e041d35cd7657d9bbb970e3e28d93e6fc62246a74` |
