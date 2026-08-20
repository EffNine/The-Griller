> **Historical milestone report (M0/M1).** See [docs/security.md](../security.md) for the current canonical treatment of the execution and safety model.

# CommandSpec, execution, and the safety model

## CommandSpec

A command is described as structured data, never as an opaque shell string:

```jsonc
{
  "executable": "git",
  "args": ["status", "--porcelain"],
  "working_directory": "/path/to/repo",
  "timeout_ms": 30000,
  "environment_policy": { "inherit_allowlist": ["PATH", "HOME", "LANG", "TMPDIR", "USER"] },
  "network_policy": "disabled"
}
```

There is deliberately **no shell string field**. The executable and arguments
are passed directly to `std::process::Command`:

- No shell is ever invoked, so `;`, `|`, `$()`, backticks, `&&`, globs, and
  `$VARS` in arguments are data, never commands.
- This makes commands hashable and allow-listable in later phases.

## Environment handling

- `EnvironmentPolicy::Clear` (default): the child runs with an **empty**
  environment. Secrets on the host are not inherited.
- `EnvironmentPolicy::InheritAllowlist(keys)`: only the listed keys pass
  through. The runtime default allow-list is `PATH, HOME, LANG, TMPDIR, USER`.

There is **no inherit-everything mode** (INVARIANT 5): the Griller never
silently inherits the full host environment. The only two ways to give a child
environment variables are `Clear` and an explicit allow-list.

The environment actually passed to the child is hashed into an
`environment_fingerprint` (BLAKE3 over sorted `key=value` pairs) recorded in
provenance. The fingerprint never contains raw values, and it is **not** part
of the evidence content hash (identity describes what was observed, not where).

## Network policy and enforcement

`CommandSpec.network_policy` is the **requested** policy. The backend does not
claim it enforces anything:

```text
DECLARED POLICY != ENFORCED POLICY
```

- `NetworkPolicy::Disabled` (default) adds `GIT_TERMINAL_PROMPT=0` so tools
  fail closed instead of blocking on interactive prompts.
- Every execution produces an `Enforcement` record reporting what was actually
  enforced: `network_isolation` and `os_sandbox`. In M1 both are always
  `NotEnforced`.
- The `Enforcement` record is part of `CommandOutputContent` and the content
  hash, so persisted evidence never pretends isolation happened.

### What M1 has

- shell-free execution
- controlled environment
- worktree isolation
- timeout enforcement
- best-effort secret redaction of stdout/stderr

### What M1 does NOT have

- kernel-level network isolation
- OS sandboxing (seccomp, namespaces, privilege reduction)
- real network namespacing — assume a `Disabled` command could still reach the
  network

These gaps must be closed before adversarial execution in later phases.

## Timeouts and outcomes

- `timeout_ms` (0 = no timeout); enforced by polling and killing the child.
- `ExecutionStatus`: `completed`, `timed_out`, `failed_to_spawn`.
- `CommandOutcome`: `success`, `failed`, `timed_out`, `could_not_spawn`.

## COMMAND_FAILED vs CLAIM_FALSE

A failed collection — non-zero exit, timeout, spawn failure — is evidence that
**the command did not behave as expected**. It says nothing about whether any
claim is true or false. There is no claim-truth field anywhere in the M1
evidence model, and none is computed. The distinction is preserved by
construction:

- `CommandOutputContent.outcome()` returns a collection outcome only.
- Verdict logic (which maps evidence to claim truth) is explicitly out of scope
  for M1.

## Backend contract

- `ExecutionBackend::execute(spec, env)` never errors on a failed process;
  failure is a structured result. The only error paths are process-wait I/O
  failures.
- stdout and stderr are fully captured.
- Results include the exit code, duration, timed-out flag, passed environment,
  and environment fingerprint.
