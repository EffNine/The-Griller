# Evidence model

> **Evidence is a captured receipt, not a sentence saying something
> happened.**

Evidence is immutable, structured, and content-addressed. It records what the
environment demonstrated — a replayed command, a git diff, a file read, a
snapshot — with provenance that says *how* it was captured. An agent's
narration is a claim, never evidence.

## Record schema

```jsonc
{
  "id": "b3<...64 hex...>",              // == content_hash
  "kind": "command_output",              // evidence kind
  "provenance": { /* see below */ },
  "content": {
    "type": "command_output",            // internally tagged enum
    "data": { /* typed content */ }
  },
  "captured_at": 1700000000,             // unix seconds; NOT hashed
  "content_hash": "b3<...64 hex...>"     // BLAKE3 over canonical JSON
}
```

## Evidence kinds

| kind | content |
| --- | --- |
| `command_output` | spec, stdout, stderr (redacted), exit_code, status, enforcement, redaction report, duration_ms |
| `git_diff` | unstaged diff, staged diff, porcelain status |
| `file_content` | relative path, size, bytes, text (if UTF-8) |
| `manifest` | relative path, format, raw text |
| `repository_state` | the `ProjectContext` |
| `repository_snapshot` | read-only base + working state (HEAD, staged, unstaged, untracked) |
| `agent_claim` | claim text + optional note |

## Content-addressed identity

```
content_hash = BLAKE3(canonical_json(hash_payload))
```

**EVIDENCE IDENTITY describes WHAT was observed.
PROVENANCE describes WHERE/HOW/WHEN it was observed.**

`hash_payload` is an explicitly built view:

**Included**
- `kind`
- content, minus unstable fields (below)
- `command_output.enforcement` — the **enforced** execution semantics
- provenance: `source`, `capture_method`, `command` (minus working directory),
  `tool`, `reproducibility`, `directly_observed`, `depends_on_agent_claim`

**Excluded** (unstable, location-only, or environment context)
- `captured_at` (timestamp)
- `command_output.duration_ms` (varies per run)
- `command_output.redaction` (deterministically derivable from content)
- `provenance.environment_fingerprint` (WHERE/HOW, not WHAT)
- absolute locations: `provenance.working_directory`,
  `command_output.spec.working_directory`, `git_diff.path`,
  `repository_state.context.repository_root`

Consequences (all tested):

| same identity | different |
| --- | --- |
| same observed content + different timestamp | same hash |
| same observed content + different working directory | same hash |
| same observed content + different environment fingerprint | same hash |
| different observed output / exit code | different hash |

The environment fingerprint remains available as provenance; it is simply not
part of the identity.

## Provenance

Provenance records structured facts, not a numeric trust score:

- `source` — `git`, `filesystem`, `manifest`, `command`, `repository`, `agent`
- `capture_method` — `git_invocation`, `file_read`, `directory_scan`,
  `process_execution`, `declared`
- `command` — the `CommandSpec`, if applicable
- `working_directory` — where it was captured
- `environment_fingerprint` — BLAKE3 over the environment actually passed to
  the command (a fingerprint, never raw values)
- `tool` — e.g. `git 2.53.0`
- `reproducibility` — `deterministic` | `non_deterministic` | `unknown`
- `directly_observed` — bool
- `depends_on_agent_claim` — bool

### Trust tiers

Trust is a provenance property, not a score:

| Tier | Provenance shape | What it can do |
| --- | --- | --- |
| Environment evidence | `source ≠ agent`, `directly_observed = true`, `depends_on_agent_claim = false` | the only class that can `support` or `contradict` a claim |
| Agent narration | `source = agent`, `directly_observed = false`, `depends_on_agent_claim = true` | recorded as `agent_claim` evidence; can never be upgraded |
| Derived requirement | `source = derived` | requirement input; never surfaced as `declared` |

- Agent-derived evidence always has `source = agent`, `directly_observed =
  false`, `depends_on_agent_claim = true`.
- Environment collectors never emit `agent_claim` evidence and always set
  `directly_observed = true`.
- `Evidence::from_agent_claim` is the only constructor that produces
  agent-claim provenance, so the two classes cannot be conflated by accident.

Two pieces of evidence derived from the same agent narration are **not**
independent corroboration. "The agent says tests pass" plus "another LLM says
the agent reported tests pass" verify nothing.

## Reproducibility

Every record's provenance declares whether the observation is expected to
reproduce: `deterministic`, `non_deterministic`, or `unknown`. It is an
annotation, not a guarantee — the self-grill audit (check #13) verifies that
the annotations are honest, and M2's `EvidenceQuality` records
`reproducible: true/false` as a boolean fact, never a probability.

## Secret redaction

Process output passes through a redaction boundary before entering evidence:

```text
process output → redaction → Evidence content
```

- **Best-effort deterministic secret redaction.** Detects bearer tokens,
  common API-key prefixes, PEM private-key blocks, credential-assignment
  patterns, and common cloud credential formats.
- Replaced with a `[REDACTED-<category>]` token.
- `CommandOutputContent.redaction` records whether redaction occurred, the
  total count, and per-category counts. The report never contains the
  redacted material.
- Redaction is deterministic; non-secret output passes through unchanged.
- **Scope:** the boundary covers stdout/stderr. Command arguments are
  caller-controlled spec data and are not redacted; do not pass secrets as
  command-line arguments.
- This is **not** complete secret detection and must not be represented as
  such.

## Execution enforcement

Every `command_output` record carries an `Enforcement` field reporting what
the backend *actually* enforced — `network_isolation` and `os_sandbox`. In
the current implementation both are always `NotEnforced`, and the record says
so. A requested policy is never represented as enforced:

```text
DECLARED POLICY != ENFORCED POLICY
```

The enforcement record is part of the content hash, so persisted evidence can
never pretend isolation happened.

## Evidence references

The decision crates never embed evidence payloads. A `ClaimAssessment`,
`ContextAssessment`, or `AttackResult` carries **evidence references** — the
content-addressed ids of the records it relied on. Anything downstream can
resolve a reference and verify the record's hash; nothing downstream can
sneak in a payload that was never captured. This is what makes the whole
decision trace auditable end to end.

## Canonical serialization

- JSON with object keys sorted recursively.
- Implemented in `griller_core::hash::canonical_bytes` /
  `canonical_string`.
- Serialization must be byte-stable: serialize → deserialize → serialize
  yields identical bytes (tested).

## Storage

`JsonStore` writes one canonical JSON file per record at
`<dir>/<id>.json`, atomically (temp file + rename). Lookup by id reads a file;
lookup by hash scans. No database, no vector store. The `EvidenceStore` trait
keeps the backend replaceable.

## What evidence is NOT

- Not a sentence — a captured receipt.
- Not a verdict — evidence never decides anything; M2 adjudicates, M3 decides.
- Not a probability — no confidence scores, no "87% verified."
- Not a guarantee — a captured run proves what that run demonstrated, in that
  environment, against that state.