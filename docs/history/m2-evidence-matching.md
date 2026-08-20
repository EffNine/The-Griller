> **Historical milestone report (M2).** See [docs/verification-pipeline.md](../verification-pipeline.md) and [docs/evidence.md](../evidence.md) for the current canonical treatment of evidence matching.

# M2 — Evidence relationships, matching, SupportMatrix

This document covers how evidence relates to claims, how matching works, and
what the SupportMatrix contains (`crates/griller-adjudicate`).

## Relationship model

`griller_adjudicate::relationship`

Each evidence record is classified against a claim into exactly one
relationship:

| Relationship | Meaning |
| --- | --- |
| `supports` | The environment directly demonstrated something consistent with the claim. |
| `contradicts` | The environment directly demonstrated something conflicting with the claim. |
| `irrelevant` | The evidence does not pertain to the claim (different domain). |
| `insufficient` | Relevant, but not enough to adjudicate (agent narration, a broad run that cannot isolate a named target). |
| `unverifiable` | Relevant, but the environment could not determine it (timeout, missing tool/dependency). |

These relationships are **not** collapsed into truth immediately; the
adjudication layer decides the status from the full matrix.

## Evidence strength and quality

`EvidenceStrength` is a documented ordering of proximity — **not** a calibrated
probability:

| Strength | Meaning |
| --- | --- |
| `none` | No informational value. |
| `weak` | Reserved for later calibrated rules; not emitted by the M2 matcher. |
| `moderate` | Relevant but indirect (symbol presence inferred by text search). |
| `strong` | Reserved for later calibrated rules; not emitted by the M2 matcher. |
| `direct` | The environment directly demonstrated the proposition (completed exit code, direct presence observation). |

`EvidenceQuality` records provenance-derived facts (booleans, not scores):

```json
{ "direct_observation": true, "reproducible": true, "independent": true, "relevant": true }
```

## Independence

Two pieces of evidence derived from the same agent narration are **not**
independent corroboration.

- Agent narration always carries `source = agent`, `directly_observed = false`,
  `depends_on_agent_claim = true`.
- `EvidenceQuality.independent = !depends_on_agent_claim`.
- Only environment evidence (`directly_observed && !depends_on_agent_claim`)
  can ever `support` or `contradict` a claim.

So "the agent says tests pass" plus "another LLM says the agent reported tests
pass" verify nothing. Direct process execution is distinguishable from
narration.

## Evidence matching

`griller_adjudicate::matcher` — fully deterministic, no guessing.

Claims are first classified:

`tests pass` → `TestsPass`, `build fails` → `BuildFails`,
`formatting passes` → `FormattingPasses`, `X was changed` → `FileChanged`,
`X was removed` → `FileRemoved`, `X exists/does not exist` → `EntityExists` /
`EntityAbsent`, else `Unknown`.

### Command claims (tests / build / format)

- Relevant evidence is command output whose command matches the domain
  (`cargo test`, `pytest`, `npm test`, `go test`, …; `cargo build`, `make`,
  …; `cargo fmt --check`, `prettier`, …).
- Completed exit 0 → supports; completed non-zero → contradicts; timeout or
  spawn failure → unverifiable.
- A **named test target** (e.g. "authentication tests pass") requires a
  matching command scope: `cargo test authentication` is specific, a bare
  `cargo test` is `insufficient` (it cannot isolate the subset).
- Domain names for build/format claims ("build", "format") are not filters.

### File / entity claims

- `X changed` — git diff / snapshot showing the path changed (or any change for
  a repository-wide claim) → supports; a clean tree → contradicts; file
  presence alone → insufficient.
- `X removed` — diff/snapshot deletion of the path → supports; path still
  present → contradicts; diff silent on the path → insufficient.
- `X exists` / `X does not exist` — path-level presence from file-content,
  diff, or snapshot → direct; symbol presence via text search → moderate.

### When matching cannot be confident

The matcher returns `insufficient`, `unverifiable`, or `irrelevant`. It never
guesses.

## Claim-specific relevance

A successful unrelated command does not support a claim:

| Claim | Evidence | Relationship |
| --- | --- | --- |
| "Authentication tests pass." | `cargo fmt --check` exit 0 | `irrelevant` |
| "Project formatting passes." | `cargo fmt --check` exit 0 | `supports` |
| "Build succeeds." | `cargo clippy` exit 0 | `irrelevant` |
| "Tests pass." | `cargo test` exit 1 | `contradicts` |

## SupportMatrix

`griller_adjudicate::support`

`SupportMatrix { proposition, matches }` holds one `EvidenceMatch` per evidence
record:

- Deterministically ordered by evidence id (assessment is order-independent).
- De-duplicated by evidence id.
- Accessors: `supports()`, `contradicts()`, `irrelevant()`,
  `insufficient()`, `unverifiable()`, `relevant()`.

Same claim + same evidence set + same rules → identical matrix, byte-identical
canonical serialization.
