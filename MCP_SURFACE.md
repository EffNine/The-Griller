# MCP_SURFACE.md

> **Status: Phase 1 inventory (RECOVERY). Still accurate.**
>
> The six-tool surface is unchanged and **nothing has been registered,
> connected, or exposed** — `griller` still appears 0 times in
> `~/.config/opencode/opencode.jsonc`. The one change since is an *optional*
> `repository` section added to the `check_completion` **output** in `16c6852`;
> the tool list, input schemas and transport are untouched. See
> [CURRENT_STATE.md](CURRENT_STATE.md) §6 and [NEXT_PHASE.md](NEXT_PHASE.md)
> step 5.

Inventory of the existing MCP interface. **Nothing was registered, connected, or exposed.**
This document exists so the surface can be reviewed *before* any exposure decision.

Server identity (`crates/griller-mcp/src/lib.rs:75-78`):

| Field | Value |
|---|---|
| service | `the-griller` |
| version | `0.2.0` (from `CARGO_PKG_VERSION`) |
| protocol | `2025-11-25` (`ProtocolVersion::V2025_11_25`, `lib.rs:860`) |
| mcp_schema | `m8.0` |
| transport | stdio (MCP), `rust-mcp-sdk 1.0.1` / `rust-mcp-transport 1.0.0` |
| binary | `griller-mcp` (`crates/griller-mcp/src/main.rs`) |

**6 tools**, declared via `#[macros::mcp_tool(name = "…")]` at `lib.rs:66, 159, 293, 401, 553, 606`.

---

## 1. Tool-by-tool inventory

### `griller.health` — `lib.rs:66`
- **Input:** none.
- **Output:** `service`, `version`, `protocol`, `schema_version`, `tools[]`, `capabilities{}`, `execution{}`, `security{}`.
- **Side effects:** none (pure).
- **Trust boundary:** none — no repo, no claim, no execution.
- **Failure behavior:** cannot fail on input; serialisation error only.
- **Evidence generated:** none.
- **Externally safe:** **YES.**

Its self-reported security posture is the most important string in the codebase — it
advertises exactly what is *not* guaranteed:

```json
"security": {
  "shell_free": true,
  "command_allow_list": true,
  "worktree_isolation": true,
  "secret_redaction": "best_effort",
  "network_isolation": "not_enforced",
  "os_sandbox": "not_enforced",
  "privilege_reduction": "not_enforced"
}
```

`"execution": { "shell_free": true, "worktree_isolation": true, "network_isolation": false, "os_sandbox": false }`

**Assessment:** this is exemplary. Three constraints are honestly reported as *not enforced*
rather than implied. `secret_redaction` is `"best_effort"`, not `"enforced"` — a deliberate
downgrade of its own claim. **No over-claiming found anywhere in the health surface.**

---

### `griller.verify_claim` — `lib.rs:159`
- **Description:** *"Independently verify a claim against repository evidence. Agent narration is not treated as verification."*
- **Input:** `repo: String` (req), `claim: String` (req), `claim_kind: Option<String>`, `target: Option<String>`, `severity: Option<String>`, `evidence_mode: String` (req).
- **Side effects:** reads the repository; collects evidence.
- **Trust boundary:** the `repo` path is caller-supplied → validated at the boundary (`validate.rs`), and all evidence is captured into an isolated worktree.
- **Failure behavior:** invalid input → `McpErrorKind::InvalidRequest`; unavailable evidence → `EpistemicStatus::Unverifiable` (never `Verified`).
- **Evidence generated:** yes — `EvidenceRecord`s with forced provenance.
- **Externally safe:** **CONDITIONAL** — safe only if `repo` is confined. The tool will read any path the process can read. Confinement is the *caller's* responsibility (the gate confines it; a raw MCP client must not).

---

### `griller.check_context` — `lib.rs:293`
- **Description:** *"Determine whether a task appears to target the current repository, including explicit foreign-project mismatch detection."*
- **Input:** `repo: String` (req), `task: String` (req).
- **Side effects:** read-only.
- **Trust boundary:** same `repo` confinement concern.
- **Failure behavior:** `ContextError` variants; mismatch is reported, not an error.
- **Evidence generated:** context assessment only.
- **Externally safe:** **CONDITIONAL** — read-only, but still path-reachable.

This is the **M4** stage and the reason 32 of 131 production decisions blocked with
`MISMATCH_CONTEXT_BLOCK` — the gate caught agents claiming completion on the wrong repository.

---

### `griller.run_attacks` — `lib.rs:401`
- **Description:** *"Run applicable safe adversarial verification attacks against a claim. Attack kinds are validated strictly: unknown kinds are rejected, never silently dropped."*
- **Input:** `repo`, `claim`, `claim_kind?`, `target?`, `attack_kinds: Option<Vec<String>>`, `timeout: u64` (req).
- **Side effects:** **EXECUTES COMMANDS.** The only tool that runs caller-selected attack specs.
- **Trust boundary:** highest-risk surface. Mitigations observed:
  - shell-free: commands are `executable + argv`, never a shell string (`griller-core/src/command.rs`)
  - `cmd.env_clear()` + explicit allowlist (`PATH,HOME,LANG,TMPDIR,USER`) — **no inherit-everything mode**
  - fixed per-kind attack allowlist: `IMPLEMENTED_ATTACKS: [AttackKind; 4]` (`plan.rs:52-57`)
  - unknown kinds → `INVALID_REQUEST`, never silently dropped (`plan.rs:59-65`)
  - `MAX_ATTACKS = 20`, `MAX_TIMEOUT_MS = 600_000` (`validate.rs:16,23`)
  - worktree isolation
- **Failure behavior:** `AttackOutcome::ExecutionFailed` — and the doc is explicit: **`EXECUTION_FAILED != REFUTED`**. A failed attack is not proof the claim is false.
- **Evidence generated:** yes, per-attack.
- **Externally safe:** **NO — not without a caller-side confinement layer.** This is the tool that would need the strictest policy if ever exposed beyond a local trusted gate.

---

### `griller.check_completion` — `lib.rs:553`
- **Description:** *"Independently evaluate an agent's completion claim using repository context, evidence collection, deterministic adjudication, applicable safe adversarial checks, and a self-grill audit of the resulting decision."*
- **Input:** `repo`, `claim`, `claim_kind?`, `timeout: u64` (req). Internally `CompletionRequest { repo: PathBuf, claim: String, claim_kind: ClaimKind, timeout_ms: u64 }` (`completion.rs:63-72`).
- **Side effects:** **EXECUTES COMMANDS** — the full M4→M2→M5→M2→M3→M7 pipeline.
- **Trust boundary:** union of `check_context` + `run_attacks`, at full strength.
- **Failure behavior:** fail-closed by construction (see `BASELINE_VERIFICATION.md` §"fail-closed" and `MCP_SURFACE` note below).
- **Evidence generated:** yes — the complete report, hash-addressed.
- **Externally safe:** **NO** — same reasoning as `run_attacks`; it subsumes it.

---

### `griller.self_grill` — `lib.rs:606`
- **Description:** *"M7 deterministic self-audit of the Griller's own decision pipeline. Produces an immutable original decision + a self-grill result with 16 structural checks. Never overwrites the original verdict."*
- **Input:** none observed.
- **Side effects:** none on the repository (the one write reference is the immutable result record).
- **Trust boundary:** none external.
- **Failure behavior:** reports `refuted: true` **without** mutating the original decision — verified in code (`completion.rs:366-375`).
- **Evidence generated:** an immutable original + a self-grill result.
- **Externally safe:** **YES.**

---

## 2. Summary matrix

| Tool | Executes cmds | Writes state | Needs `repo` confinement | Externally safe |
|---|---|---|---|---|
| `griller.health` | no | no | no | **YES** |
| `griller.verify_claim` | evidence collection | evidence | **yes** | conditional |
| `griller.check_context` | no (read-only) | no | **yes** | conditional |
| `griller.run_attacks` | **yes** | evidence | **yes** | **NO** |
| `griller.check_completion` | **yes** | evidence | **yes** | **NO** |
| `griller.self_grill` | no | immutable result | no | **YES** |

## 3. Input bounds (the trust boundary, `crates/griller-mcp/src/validate.rs`)

```
MAX_TEXT_LEN           = 10_000     claims, tasks, reasons
MAX_ATTACKS            = 20         attack kinds per call
DEFAULT_ATTACK_TIMEOUT = 30_000 ms
MAX_TIMEOUT_MS         = 600_000 ms aligned with REPLAY_TIMEOUT_MS
MAX_EVIDENCE_SNIPPETS  = 10         echoed per response
repo path              ≤ 2_000 chars, must exist
```

> Header comment: *"Bounds are intentionally small: the Griller is a verification layer, not
> a data sink."* — correct posture.

`MAX_TIMEOUT_MS` is deliberately aligned with `REPLAY_TIMEOUT_MS` (600 s) with the comment
*"a full-suite replay in a cold worktree is the slowest legitimate attack, and callers must be
able to request exactly that budget."* The **MCP layer is correct**; the mismatch is in the
gate layer (`TIMEOUT_PROCESS_ANALYSIS.md`).

## 4. Fail-closed property of this surface

Every tool degrades to a non-passing state:

- `ExecutionStatus::{FailedToSpawn, TimedOut}` → `CommandOutcome::{CouldNotSpawn, TimedOut}`,
  and `griller-core/src/command.rs` states: **"`COMMAND_FAILED != CLAIM_FALSE`"** — a failed
  evidence collection is never proof a claim is false.
- `AttackOutcome::ExecutionFailed` — **"`EXECUTION_FAILED != REFUTED`"**.
- Unsupported attack kinds → `INVALID_REQUEST`.
- Unknown tool / bad params → `CallToolError`, never a default pass.
- Gate↔server version mismatch → `STALE_SERVER` + `unavailable` (2 real production instances).

**No code path in this surface returns a positive result because something failed.**

## 5. Exposure recommendation (NOT actioned)

For any future exposure, in order:

1. Expose **`griller.health`** and **`griller.self_grill`** only. Both are safe and both
   provide real value (capability disclosure and self-audit).
2. **`check_context`** is safe if `repo` is confined by the caller.
3. **`verify_claim` / `run_attacks` / `check_completion`** must remain behind a
   confinement layer that the Griller does **not** currently provide itself. The
   `repo` path is validated for existence and length, **not** for containment within an
   allowed root. There is no allowlist.
4. Do not expose over a network transport. stdio + a local trusted gate is the designed
   posture, and the project has no remote transport.

> **This is a finding, not a defect introduced by recovery:** the absence of a
> repo-root allowlist is inherent to the original design. It was previously mitigated by
> the gate being the only caller. That mitigation no longer exists, because the gate is
> unregistered.
