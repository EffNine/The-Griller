# UNSAFE_BOUNDARY.md

**The `unsafe` surface introduced by Phase 2, in full.**

The project policy is that safety-relevant code should be auditable without an FFI
review. This document exists so that one narrow exception does not quietly become a
general relaxation.

Changed in commit `0dcf9cd89c7de77ff2b22472e5a3fc94a267fb63`, following an explicit
decision by the repository owner. Rationale and alternatives:
[PHASE_2_BASELINE.md](PHASE_2_BASELINE.md) §6.

---

## 1. The policy before Phase 2

`#![forbid(unsafe_code)]` was present in **all 8 workspace crates**, and the workspace
contained **zero** `unsafe` blocks:

```
griller-adjudicate   #![forbid(unsafe_code)]
griller-context      #![forbid(unsafe_code)]
griller-core         #![forbid(unsafe_code)]
griller-gate         #![forbid(unsafe_code)]
griller-mcp          #![forbid(unsafe_code)]
griller-policy       #![forbid(unsafe_code)]
griller-runtime      #![forbid(unsafe_code)]
griller-self-grill   #![forbid(unsafe_code)]
```

`forbid` cannot be overridden by a local `#[allow]`. Relaxing it is therefore a
crate-level decision, which is why it was escalated rather than taken silently.

---

## 2. The change

**One crate changed: `griller-runtime`.**

```rust
// crates/griller-runtime/src/lib.rs:21
#![deny(unsafe_code)]
```

`forbid` → `deny`. The crate-level comment (`:9-20`) records why, and points here.

**The other 7 crates are unchanged and retain absolute `forbid(unsafe_code)`.**

---

## 3. The exact unsafe operation

**One.** There is a single `unsafe` block in the entire workspace.

```rust
// crates/griller-runtime/src/execution.rs:182
#[allow(
    unsafe_code,
    reason = "killpg(2) has no safe std equivalent; confined to this function. \
              See crate-level note in lib.rs and PHASE_2_BASELINE.md §6"
)]
fn terminate_process_group(child: &mut Child) {
    let pid = child.id();
    if pid != 0 {
        // SAFETY: ...
        unsafe {
            libc::killpg(pid as libc::pid_t, libc::SIGKILL);   // :195
        }
    }
    terminate_direct_child(child);
}
```

| Property | Value |
|---|---|
| Function | `terminate_process_group` |
| File / line | `crates/griller-runtime/src/execution.rs:182` (call at `:195`) |
| Operation | `libc::killpg(pid as libc::pid_t, libc::SIGKILL)` |
| Platform | `#[cfg(unix)]` — the non-Unix variant at `:203` contains no `unsafe` |
| Scope | one function; `#[allow]` is not at module or crate scope |

**Why it is required.** Signalling a process group has no safe standard-library
equivalent:

- `std::process::Child::kill` — direct child only.
- `std::os::unix::process::CommandExt::process_group` — *setup* only; there is no matching
  safe group-*teardown* API.
- `libc::killpg` is an `unsafe extern "C"` declaration.

`CommandExt::process_group(0)` is used for the **setup** side precisely because it is safe
and cannot be used to smuggle in non-signal-safe code. Only teardown needs FFI.

**The SAFETY comment** (`:185-193`) records: `killpg` is an async-signal-safe syscall
wrapper taking a pgid and a signal with no memory access; the child is its own group
leader because `isolate_process_group` ran before `exec`, so its PGID equals its PID and
the id is a live group leader at the point of the call; and an already-exited group
returning `ESRCH` is ignored deliberately, because the fallback below reaps what remains
and neither outcome can alter the `TimedOut` status the caller reports.

**The block was not refactored.** It is 3 lines and stays that way.

---

## 4. Negative control — unrelated unsafe is still rejected

`deny` is only a meaningful relaxation if it still blocks everything else. Verified by
injection, not assumed:

```
# injected into crates/griller-runtime/src/git.rs:
pub fn probe() { unsafe { std::ptr::null::<u8>(); } }

$ cargo check -p griller-runtime
error: usage of an `unsafe` block
```

The probe was then reverted and the crate compiles clean. **Any new `unsafe` code anywhere
in `griller-runtime` is a compile error.** The relaxation permits exactly the one annotated
function and nothing else.

Coverage of the actual block:

```
$ grep -rn 'unsafe' crates/griller-runtime/src/*.rs
crates/griller-runtime/src/execution.rs:179:    unsafe_code,        # the #[allow] attribute
crates/griller-runtime/src/execution.rs:194:        unsafe {           # the single block
crates/griller-runtime/src/lib.rs:21:#![deny(unsafe_code)]
```

(The only other `unsafe` matches in the crate are the strings `"unsafe command rejected"`
and the test name `classify_rm_unsafe`.)

---

## 5. Policy going forward

- **Do not weaken further.** Do not move the `#[allow]` to module or crate scope.
- **Do not add a second `unsafe` operation** without repeating this exercise: state the
  operation, the reason, and a negative control.
- **The other 7 crates stay `forbid`.** If a future change needs FFI elsewhere, the same
  escalation applies.
- **`libc` stays.** It is not to be replaced with hand-written `extern "C"` declarations;
  that would keep the `unsafe` while removing the audited wrapper.

---

## 6. Dependency change accompanying this

`libc = "0.2"` was added to `crates/griller-runtime/Cargo.toml:15` and is used in exactly
one place: the `killpg` call above.

**It was already present in the dependency graph before Phase 2**, transitively, via
`ring`, `rustix`, `mio`, `tokio`, `quinn-udp` and others — `Cargo.lock` at `bfa1be0`
already contains a `libc` package entry. Phase 2 added no new package: the entire
`Cargo.lock` delta is **+2 lines**, both of them additions to existing `dependencies`
lists (`"libc"` under `griller-runtime`, `"griller-core"` under `griller-gate`).

So the change makes an existing transitive dependency **direct**. No new download, no new
build step, no new version. What it does change is the recorded project property "zero
third-party runtime dependencies", which is now **inaccurate** — see
[CURRENT_STATE.md](CURRENT_STATE.md).
