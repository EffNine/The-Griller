> **Historical milestone report (M3).** See [docs/verification-pipeline.md](../verification-pipeline.md) and [docs/concepts.md](../concepts.md) for the current canonical treatment of policy and verdicts.

# M3 — Policy, severity, operational verdict

M2 answered: *"what can we legitimately say about a claim?"* (epistemic
status). M3 answers: *"given that assessment, what should the system do?"*
(operational verdict). Everything in `crates/griller-policy` is deterministic
and pure: no filesystem, no network, no LLM, no subprocesses.

## The pipeline

```text
Claim
  -> M2 Adjudication (EpistemicStatus)
  -> M3 Policy
  -> Severity / Impact
  -> Operational Verdict
  -> PASS / WARN / BLOCK
```

## Epistemic status != verdict

| Concept | Question | Values |
| --- | --- | --- |
| Epistemic status (M2) | What can we say about the claim? | `verified`, `unsupported`, `uncertain`, `contradicted`, `false`, `unverifiable`, `adversarially_verified` |
| Operational verdict (M3) | What should the system do? | `pass`, `warn`, `block` |

A verdict is a **decision**, never a truth claim. `BLOCK` does not mean "the
claim is false"; it means "this claim cannot be accepted as-is." M3 never
re-derives truth — it only decides what to do with an already-adjudicated
claim.

## Severity model

`griller_policy::severity`

`info < low < medium < high < critical`.

Severity describes the **consequence of accepting the claim/action**, never how
important the claimer sounds. It is resolved deterministically by
`SeverityResolver`; an LLM can never propose a severity because the resolver
reads only structured signals.

`DECLARED` vs `DERIVED` severity are distinguished (`SeveritySource`):

1. A `declared` severity (supplied by the operator/pipeline) is authoritative.
2. Otherwise a simple refactor ⇒ `low`.
3. Otherwise production-touching ⇒ `high`.
4. Otherwise derive by claim kind: `fact`/`design` ⇒ `low`;
   `completion`/`requirement`/`causal` ⇒ `medium`.

`critical` and `info` are **never** auto-derived. When context is insufficient
the resolver uses the conservative default; it never invents facts.

## The policy matrix

The full matrix is encoded in `griller_policy::table::rule_for` (one explicit
arm per kind/status pair) and reproduced here.

### Completion / Requirement / Causal

| status | info | low | medium | high | critical |
| --- | --- | --- | --- | --- | --- |
| VERIFIED | P | P | P | P | P |
| ADVERSARIALLY_VERIFIED | P | P | P | P | P |
| UNSUPPORTED | W | W | W | B | B |
| UNVERIFIABLE | W | W | W | B | B |
| UNCERTAIN | W | W | W | B | B |
| CONTRADICTED | W | W | B | B | B |
| FALSE | W | W | B | B | B |

### Fact

| status | info | low | medium | high | critical |
| --- | --- | --- | --- | --- | --- |
| VERIFIED | P | P | P | P | P |
| ADVERSARIALLY_VERIFIED | P | P | P | P | P |
| UNSUPPORTED | W | W | W | W | W |
| UNVERIFIABLE | W | W | W | W | W |
| UNCERTAIN | W | W | W | W | W |
| CONTRADICTED | W | W | B | B | B |
| FALSE | W | W | B | B | B |

### Design

| status | info | low | medium | high | critical |
| --- | --- | --- | --- | --- | --- |
| VERIFIED | P | P | P | P | P |
| ADVERSARIALLY_VERIFIED | P | P | P | P | P |
| UNSUPPORTED | W | W | W | B | B |
| UNVERIFIABLE | W | W | W | W | W |
| UNCERTAIN | W | W | W | W | W |
| CONTRADICTED | W | W | W | B | B |
| FALSE | W | W | W | B | B |

### Rationale

- **VERIFIED ⇒ PASS.** M2 only assigns `VERIFIED` with direct evidence and no
  known contradiction, so no additional verification gate is required in M3.
- **FALSE / CONTRADICTED block from MEDIUM** for impact-bearing kinds (they are
  the strongest evidence positions) and from HIGH for design.
- **UNSUPPORTED / UNVERIFIABLE / UNCERTAIN block only at HIGH+** for
  impact-bearing kinds: they are failures of evidence, not falsity. A
  lower-impact claim may proceed with a warning.
- **Design / Fact are lower stakes**, so evidence-failure statuses only warn.

## Conservative default (no rule)

If a combination has no explicit rule it **never passes silently**; it defaults
to `WARN` with reason code `NO_POLICY_RULE_WARN` and message *"Policy has no
specific rule for this combination."* This prevents new claim kinds/statuses
from accidentally becoming trusted. Because `EpistemicStatus` and `ClaimKind`
are `#[non_exhaustive]`, any future status/kind forces a policy author to add
an explicit arm; if they do not, the conservative `WARN` applies.

## FALSE vs CONTRADICTED (not merged)

| Status | Meaning |
| --- | --- |
| `FALSE` | Evidence directly establishes the claim is false. |
| `CONTRADICTED` | Available evidence conflicts with the claim. |

Both are treated as stronger than `UNSUPPORTED` (they block from MEDIUM for
impact-bearing kinds), but they are never merged and carry distinct reason
codes (`FALSE_COMPLETION_BLOCK` vs `CONTRADICTED_COMPLETION_BLOCK`) and
required actions (`CORRECT_CLAIM` vs `RESOLVE_CONFLICT`).

## UNVERIFIABLE != FALSE

`UNVERIFIABLE` means the environment could not determine the claim (missing
tool/dependency, execution unavailable, no suitable evidence mechanism). The
policy may still `BLOCK` an important completion claim, but the reason makes
the distinction explicit:

> "The completion claim cannot be accepted because verification could not be
> performed." — `UNVERIFIABLE_COMPLETION_BLOCK`

never

> "BLOCK: claim is false."

## Reason codes

Stable and machine-readable: `{STATUS}_{KIND}_{VERDICT}`, e.g.
`UNSUPPORTED_COMPLETION_BLOCK`, `CONTRADICTED_COMPLETION_WARN`,
`VERIFIED_DESIGN_PASS`. The default is `NO_POLICY_RULE_WARN`. Downstream
systems parse reason codes, never prose.

## Required actions

| Status | Action |
| --- | --- |
| VERIFIED / ADVERSARIALLY_VERIFIED | none |
| UNSUPPORTED | provide_evidence |
| UNVERIFIABLE | make_verification_available |
| UNCERTAIN | acknowledge_uncertainty |
| CONTRADICTED | resolve_conflict |
| FALSE | correct_claim |
| (future/unhandled) | define_policy_rule |

The Griller **judges; it does not fix.** Nothing is modified automatically.

## Finding model & aggregation

`Finding` wraps a `ClaimAssessment` (which references evidence by id — no
payload duplication) plus severity, verdict, reason code and required action.
`OverallDecision` holds all findings (ordered deterministically by id).

Aggregation rule:

```text
ANY BLOCK  => BLOCK
ELSE ANY WARN => WARN
ELSE        => PASS
```

A low-severity PASS can never cancel a high-severity BLOCK. Adding unrelated
findings cannot change an existing finding's decision (tested).

## Why M3 does not implement M4+

M3 only decides what to do with already-adjudicated claims. It contains no
wrong-project/context BLOCK (M4), Red Team (M5), MCP (M6), self-grill (M7), or
OpenCode integration (M8) logic.

## Diva renderer

`griller_policy::diva::DivaRenderer` renders verdicts in a fixed persona
voice. It is **decorative**: it consumes a decision/finding and never produces
one. The canonical JSON decision remains authoritative. Example lines:

- PASS: "Fine. I checked the receipts. It survived."
- UNSUPPORTED/WARN: "You made the claim. I found no receipts."
- BLOCK: "Absolutely not. You're asking me to trust this without evidence."
- UNVERIFIABLE/BLOCK: "I can't prove you're wrong. I also can't prove you're
  right. That's not a PASS."

## CLI

```sh
griller claim decide --claim "Tests pass"                       # derived severity
griller claim decide --claim "Tests pass" --severity high       # declared severity
griller claim decide --claim "Tests pass" --repo . --auto-collect
griller claim decide --claim "A" --claim "B" --severity low     # multi-finding
griller claim decide --claim "Tests pass" --severity high --diva --pretty
```

Output: canonical JSON `{ "decision": { verdict, findings }, "exit_code", note }`.
With `--diva`, persona text goes to **stderr**; stdout stays pure JSON.

Exit codes (never conflate BLOCK with system failure):

| Code | Meaning |
| --- | --- |
| 0 | PASS |
| 1 | WARN |
| 2 | BLOCK |
| 3 | system/execution error |
