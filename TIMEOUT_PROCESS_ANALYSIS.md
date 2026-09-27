# TIMEOUT_PROCESS_ANALYSIS.md

> **Status: Phase 1 analysis (RECOVERY). Both findings remain OPEN.**
>
> Neither the timeout mismatch nor the missing process-group termination has
> been changed. `DEFAULT_TIMEOUT_MS` is still 120 s at
> `crates/griller-gate/src/config.rs:10` and `child.kill()` is still
> direct-child-only at `crates/griller-runtime/src/execution.rs:189`. Scheduled
> as [NEXT_PHASE.md](NEXT_PHASE.md) steps 1–2.

INSPECT → EVIDENCE → PROPOSE. **No fix applied.** Both findings are pre-existing and were
recorded as roadmap items before this recovery.

---

## 1. Timeout layering — three constants, one mismatch

| Constant | Value | Location | Role |
|---|---:|---|---|
| `DEFAULT_TIMEOUT_MS` | **30,000 ms** | `crates/griller-core/src/command.rs:13` | default for a single command |
| `REPLAY_TIMEOUT_MS` | **600,000 ms** (10 min) | `crates/griller-core/src/command.rs:21` | budget for a full replay |
| `MAX_TIMEOUT_MS` | **600,000 ms** | `crates/griller-mcp/src/validate.rs:23` | MCP caller ceiling — **correctly aligned** |
| `DEFAULT_TIMEOUT_MS` (gate) | **120,000 ms** (2 min) | `crates/griller-gate/src/config.rs:10` | **default per-request MCP round-trip** |

### The defect

```
replay budget        600,000 ms
gate round-trip      120,000 ms   <-- 5x smaller
```

`crates/griller-gate/src/config.rs:10`:
```rust
/// Default per-request timeout (120s) applied to the MCP round-trips.
pub const DEFAULT_TIMEOUT_MS: u64 = 120_000;
```

A legitimate cold replay needing >120 s is **cut off by the gate before it can finish.**

### Consequence — and it fails closed

The gate then reports an integration failure, not a pass. The production record contains
**8 × `CHECK_COMPLETION_FAILED`** and **10 × `unavailable`**, which is the correct
behaviour. This is a **false-negative / availability** defect, not a soundness defect.

> The MCP layer is correct: `MAX_TIMEOUT_MS` is deliberately aligned to `REPLAY_TIMEOUT_MS`
> with the comment *"a full-suite replay in a cold worktree is the slowest legitimate attack,
> and callers must be able to request exactly that budget."* **The gate layer is what
> diverges.**

### Evidence it was already known and worked around

The workspace's `SYSTEM_ARCHITECTURE.md:47` records *"sync pipeline serializes traffic; ≤120s
blocking calls"* — i.e. the 120 s figure was documented as if it were a property of the
system. A deployed override existed at `~/.config/griller/gate.json` (previously observed
holding `600000`), meaning the production deployment had already worked around the default
rather than fixing it. **A fresh install today would hit the default and fail closed on cold
replays.**

### E2E harness is broken by the same mismatch

`e2e_mock_opencode_griller.py:45` hardcodes `timeout=30` against a 600 s replay budget. The
E2E harness would fail regardless of the gate default. Recorded as a separate defect; this is
why the E2E was not run in Phase 1 (`BASELINE_VERIFICATION.md` §5).

### Proposal (NOT applied)

One-line-class change: raise `DEFAULT_TIMEOUT_MS` in `griller-gate/src/config.rs` to
`600_000`, or derive it from `griller_core::command::REPLAY_TIMEOUT_MS` so the two cannot
diverge again. `griller-gate` already depends on `griller-core`
(`Cargo.toml` `[workspace.dependencies]`), so the derivation is available without a new
dependency.

**Not applied** because changing a default timeout alters fail-closed timing behaviour and
must be reviewed against the gate's latency budget, which is outside Phase 1.

---

## 2. Process-group handling — confirmed gap

### The code

`crates/griller-runtime/src/execution.rs:181-199`:

```rust
let deadline = (spec.timeout_ms > 0)
    .then(|| Instant::now() + Duration::from_millis(spec.timeout_ms));

let mut timed_out = false;
loop {
    if let Some(deadline) = deadline {
        if Instant::now() >= deadline {
            let _ = child.kill();      // <-- line 189: DIRECT CHILD ONLY
            timed_out = true;
            break;
        }
    }
    match child.try_wait() {
        Ok(Some(_)) => break,
        Ok(None) => {}
        Err(err) => return Err(RuntimeError::Io(err)),
    }
    std::thread::sleep(Duration::from_millis(5));   // 5 ms poll
}

let output = child.wait_with_output().map_err(RuntimeError::Io)?;
```

### Findings, in order of severity

1. **`child.kill()` sends SIGKILL to the direct child only.**
   Workspace-wide search for `killpg`, `setpgid`, `setsid`, `process_group` in
   `crates/griller-runtime/` returns **no matches**. There is no process-group handling
   anywhere in the project.

   For the primary use case — replaying `cargo test` — the direct child is `cargo`, and
   `cargo` spawns `rustc` and test binaries. **Those grandchildren are not signalled.**
   `wait_with_output()` then returns when the direct child dies, while grandchildren may
   still be running.

2. **Orphaned grandchildren can outlive the gate run.** They are reparented to init and
   continue. Consequences:
   - CPU/memory pressure persists after a "timed out" verdict
   - a `rustc` holding a file lock can make a *later* verification behave differently
   - `cargo test` children can write into the worktree — which is a `tempfile::tempdir()`
     that is dropped on return, so the temp dir may be removed under a live writer

3. **Polling granularity is 5 ms.** Busy-wait. Not a correctness issue, but it is a
   200 Hz spin per running command.

4. **No cleanup sweep.** There is no record of spawned PIDs, and no post-run reconciliation.
   If the process group were introduced, a stale-PID sweep would be needed; today there is
   nothing to sweep.

### What is done correctly

- `cmd.env_clear()` + explicit allowlist (`PATH, HOME, LANG, TMPDIR, USER`) — **no
  inherit-everything mode** (`execution.rs:155-156`)
- `cmd.stdin(Stdio::null())` — no interactive prompts
- `GIT_TERMINAL_PROMPT=0` when network is disabled — fails closed rather than blocking
- stdout/stderr piped, so no terminal leakage
- `ExecutionStatus` distinguishes `Completed` / `TimedOut` / `FailedToSpawn(String)`
- `exit_code` is set to `None` when `timed_out` — a timeout is **never** reported as exit 0

That last point matters: it is why a timed-out replay cannot be mistaken for a passing one.

### Does this affect the verdict?

**Not directly — it affects resource hygiene and reproducibility, not soundness.** A timed-out
execution yields `ExecutionStatus::TimedOut` → `CommandOutcome::TimedOut` → cannot contribute
direct support → the claim cannot reach `Verified`. **Fail-closed is preserved.**

The risk is *lateral*: orphans perturb subsequent verifications (CPU contention, file locks),
which can turn a deterministic replay non-deterministic. That is a reproducibility defect,
not a soundness defect.

### Production impact: UNKNOWN

The 131 records contain `timing.total_ms`. Recorded values would show whether the 120 s gate
default was ever hit. **Not analysed in Phase 1** — it requires reading 131 records and
correlating with reason codes, and the 8 `CHECK_COMPLETION_FAILED` records are the natural
starting set. Recorded as unknown rather than guessed.

---

## 3. Summary

| # | Finding | Severity | Soundness? | Fails closed? |
|---|---|---|---|---|
| T1 | Gate default timeout (120 s) < replay budget (600 s) | **MEDIUM** | no — availability only | **yes** (`unavailable` / `CHECK_COMPLETION_FAILED`) |
| T2 | Timeout kills direct child only; no process group | **MEDIUM** | no — reproducibility | **yes** (`TimedOut`, `exit_code=None`) |
| T3 | E2E harness `timeout=30` vs 600 s budget | **LOW** | no | yes (harness errors) |
| T4 | 5 ms busy-wait poll | **TRIVIAL** | no | n/a |

**None of these can turn a failure into a pass.** Both MEDIUM findings are availability and
hygiene issues on top of a sound verdict core.

**Not applied in Phase 1.** Both were already tracked as roadmap items in
`~/projects/ENGINEERING_ROADMAP.md` before this recovery:
*"The-griller: `spawn_blocking` wrap of sync pipeline; process-group kill so timeout doesn't
orphan cargo/rustc; replace eprintln! with tracing."* The recovery confirms items 1 and 2 are
still open and adds the timeout-default finding, which the roadmap did **not** list.
