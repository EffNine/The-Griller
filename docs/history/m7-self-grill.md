> **Historical milestone report (M7).** See [docs/self-grill.md](../self-grill.md) for the current canonical treatment of self-grill.

# M7: Self-Grill

## Purpose

The Griller must be able to audit its own reasoning and evidence pipeline.
Self-grill asks:

> Could the Griller itself have made a bad decision?

It does this by running 16 deterministic structural checks over the
decision artefacts (claim, assessment, evidence, policy decision, context
assessment, attack coverage) and producing a separate audit result.

Self-grill **never** mutates the original decision. It produces:

```text
OriginalDecision  +  SelfGrillResult
```

## Architecture

```
SelfGrillInput (claim + assessment + decision + context + attacks + evidence)
    -> SelfGrillRunner::run()
    -> SelfGrillResult { outcome, coverage, findings }
```

The runner executes exactly one pass of checks. It never calls itself.

## Checks

| # | Check | What it verifies | Typical outcome on failure |
|---|-------|-----------------|---------------------------|
| 1 | EvidenceCompleteness | Every evidence reference resolves; no duplicates masquerading as independent | REFUTED / WEAKENED |
| 2 | EvidenceIndependence | No same-source evidence counted twice | WEAKENED |
| 3 | TrustTier | Agent claims cannot independently create VERIFIED / ADVERSARIALLY_VERIFIED | REFUTED |
| 4 | ClaimScope | Evidence actually covers the claim scope (no scope inflation) | WEAKENED |
| 5 | ContextConsistency | M4 MISMATCH + PASS => structural invariant violation | REFUTED / WEAKENED |
| 6 | AttackCoverage | ADVERSARIALLY_VERIFIED requires strict M5 conditions | REFUTED |
| 7 | ContradictoryEvidence | Search for structural contradictions in evidence | WEAKENED / REFUTED |
| 8 | ExecutionValidity | Provenance, exit codes, enforcement truth | WEAKENED |
| 9 | PolicyConsistency | Recompute M3 policy; compare with original decision | REFUTED |
| 10 | ReasonActionConsistency | reason_code matches status + kind + verdict | REFUTED |
| 11 | RequiredActionConsistency | BLOCK => meaningful remediation; PASS => no contradictory action | WEAKENED / REFUTED |
| 12 | EvidenceHashIntegrity | Re-verify content hashes and id consistency | REFUTED |
| 13 | EnvironmentReproducibility | Reproducibility annotations are correct | WEAKENED |
| 14 | RedactionIntegrity | Pipeline invariant: redaction ran where required | REFUTED |
| 15 | ContextClaimConsistency | Context identity and claim target agree | WEAKENED |
| 16 | AttackTargetConsistency | Attacks actually targeted the claim scope | REFUTED / NOT_APPLICABLE |

## Outcome Semantics

- **HOLDS**: The original decision survives the self-audit.
- **WEAKENED**: The decision remains possible but confidence/evidence
  strength is reduced.
- **REFUTED**: The self-audit finds a legitimate contradiction in the
  original assessment/decision.
- **INCONCLUSIVE**: Self-audit could not determine whether the original
  decision is sound.
- **NOT_APPLICABLE**: No meaningful self-audit could be performed.

## Aggregation

Priority: REFUTED > WEAKENED > INCONCLUSIVE > HOLDS > NOT_APPLICABLE.

Individual check results are preserved in `findings`. The overall
`outcome` is the worst hit across all checks. No numerical confidence is
computed.

## Immutability

The original decision is treated as an immutable snapshot. Self-grill
reads the decision data but never writes to it. The caller receives:

1. `OriginalDecision` — an immutable record of what was decided
2. `SelfGrillResult` — the audit of that decision

The caller may decide how to act on a REFUTED or WEAKENED result.
Self-grill does not automatically overwrite the original verdict.

## Non-Recursion

Self-grill runs exactly one pass. There is no:

```text
self_grill(self_grill(...))
```

The checks inspect the decision artefacts. They do not inspect their
own outputs.

## Deterministic Policy Recomputation

Check #9 re-runs the M3 policy table deterministically:

```text
PolicyInput (assessment + severity context)
    -> PolicyTable.decide()
    -> ExpectedDecision
```

If `ExpectedDecision != OriginalDecision`, the check returns REFUTED.
This is one of the strongest self-grill invariants because the policy
table is fully auditable and deterministic.

## Evidence Integrity

Check #12 re-derives evidence ids from content hashes and verifies
every assessment reference resolves. This catches tampered or stale
evidence records.

## Context Consistency

Check #5 re-verifies the M4 context invariant:

- M4 = MISMATCH and verdict = PASS => REFUTED (critical)
- M4 = MISMATCH and verdict = WARN => WEAKENED
- M4 = AMBIGUOUS and verdict = PASS => WEAKENED

This does not reimplement M4. It reads the existing
`ContextAssessment` and checks for invariant violations.

## Attack Consistency

Check #6 enforces the ADVERSARIALLY_VERIFIED strict conditions:
all applicable attacks must be implemented, executed, and produce
determinate (non-inconclusive, non-failed) outcomes.

Check #16 ensures attacks actually targeted the claim rather than
irrelevant material.

## Limitations

Self-grill audits **the Griller's decision process**. It does **not**
prove that the underlying software is correct. It can detect:

- Structural contradictions
- Policy mismatches
- Evidence integrity failures
- Trust-tier violations

It cannot detect:

- Incorrect domain knowledge in evidence
- Missing evidence that would change the outcome
- LLM-injected assumptions that passed all structural checks
- Environmental factors outside the Griller's control

## Relationship to Other Modules

- **M2**: Self-grill reads the `ClaimAssessment` produced by M2. It
  does not re-adjudicate.
- **M3**: Check #9 recomputes the M3 policy table to verify consistency.
- **M4**: Check #5 re-verifies the M4 context invariant using the
  existing `ContextAssessment`.
- **M5**: Checks #6 and #16 verify the M5 attack coverage conditions.
- **M6**: The MCP `griller.self_grill` tool exposes this capability.

## CLI

```bash
griller self-grill \
  --claim "Tests pass" \
  --repo /path/to/repo \
  [--evidence-dir <dir>] \
  [--auto-collect] \
  [--diva] \
  [--pretty]
```

Output is canonical JSON with `original` and `self_grill` top-level
keys. Diva output goes to stderr.

## MCP

The `griller.self_grill` tool now performs full M7 self-grilling.
`griller.health` reports `self_grill: true` with capability details.
