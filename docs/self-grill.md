# Self-grill

## The philosophy

**The Griller grills itself.**

Every decision the Griller produces is itself a claim — *"this completion
claim is verified"* — and claims deserve the same scrutiny as the agent's.
Self-grill (M7) asks:

> Could the Griller itself have made a bad decision?

It answers by running 16 deterministic structural checks over the decision
artefacts (claim, assessment, evidence, policy decision, context assessment,
attack coverage) and producing a separate audit result.

```text
OriginalDecision
       ↓
Self-Grill Checks (16, deterministic, single pass)
       ↓
SelfGrillResult
```

## The rules of the audit

- **Deterministic.** Same inputs ⇒ same result. No randomness, no clock.
- **Single pass.** Exactly one pass of checks. No recursion:
  `self_grill(self_grill(…))` does not exist.
- **No LLM verdict.** The checks are pure functions over structured inputs;
  no model is consulted.
- **Original decision immutable.** Self-grill reads the decision data and
  never writes to it. The caller receives two things: the `OriginalDecision`
  (an immutable record of what was decided) and the `SelfGrillResult` (the
  audit). A REFUTED audit is reported; the original verdict stands. The
  caller decides how to act.

## The 16 checks

| # | Check | What it verifies | Typical outcome on failure |
| --- | --- | --- | --- |
| 1 | EvidenceCompleteness | Every evidence reference resolves; no duplicates masquerading as independent | REFUTED / WEAKENED |
| 2 | EvidenceIndependence | No same-source evidence counted twice | WEAKENED |
| 3 | TrustTier | Agent claims cannot independently create VERIFIED / ADVERSARIALLY_VERIFIED | REFUTED |
| 4 | ClaimScope | Evidence actually covers the claim scope (no scope inflation) | WEAKENED |
| 5 | ContextConsistency | M4 MISMATCH + PASS ⇒ structural invariant violation | REFUTED / WEAKENED |
| 6 | AttackCoverage | ADVERSARIALLY_VERIFIED requires strict M5 conditions | REFUTED |
| 7 | ContradictoryEvidence | Search for structural contradictions in evidence | WEAKENED / REFUTED |
| 8 | ExecutionValidity | Provenance, exit codes, enforcement truth | WEAKENED |
| 9 | PolicyConsistency | Recompute M3 policy; compare with original decision | REFUTED |
| 10 | ReasonActionConsistency | reason_code matches status + kind + verdict | REFUTED |
| 11 | RequiredActionConsistency | BLOCK ⇒ meaningful remediation; PASS ⇒ no contradictory action | WEAKENED / REFUTED |
| 12 | EvidenceHashIntegrity | Re-verify content hashes and id consistency | REFUTED |
| 13 | EnvironmentReproducibility | Reproducibility annotations are correct | WEAKENED |
| 14 | RedactionIntegrity | Pipeline invariant: redaction ran where required | REFUTED |
| 15 | ContextClaimConsistency | Context identity and claim target agree | WEAKENED |
| 16 | AttackTargetConsistency | Attacks actually targeted the claim scope | REFUTED / NOT_APPLICABLE |

The strongest check is #9: it re-runs the M3 policy table — an explicit,
fully auditable matrix — from the structured inputs. If the recomputed
decision differs from the original, the audit is REFUTED.

## Outcome semantics

| Outcome | Meaning |
| --- | --- |
| `HOLDS` | The original decision survives the self-audit. |
| `WEAKENED` | The decision remains possible but confidence/evidence strength is reduced. |
| `INCONCLUSIVE` | Self-audit could not determine whether the original decision is sound. |
| `REFUTED` | The self-audit finds a legitimate contradiction in the original assessment/decision. |
| `NOT_APPLICABLE` | No meaningful self-audit could be performed. |

Aggregation: **REFUTED > WEAKENED > INCONCLUSIVE > HOLDS > NOT_APPLICABLE**.
The overall outcome is the worst hit across all checks; individual check
results are preserved in `findings`. No numerical confidence is computed.

## What self-grill can and cannot do

Self-grill audits **the Griller's decision process**, not the software. It
can detect:

- structural contradictions,
- policy mismatches (recomputed ≠ original),
- evidence integrity failures (missing references, hash mismatches),
- trust-tier violations (agent narration upgraded to verified).

It cannot detect:

- incorrect domain knowledge in evidence,
- missing evidence that would change the outcome,
- LLM-injected assumptions that passed all structural checks,
- environmental factors outside the Griller's control.

A `HOLDS` is therefore not "the Griller is correct." It is "the decision
pipeline stayed structurally consistent with its own rules."

## Where it runs

- **In the pipeline:** `check_completion` runs self-grill on the real
  pipeline data — final assessment, decision, context assessment, attack
  coverage, evidence, severity. When the pipeline is blocked before
  adjudication (context mismatch), self-grill is **skipped with an explicit
  reason** rather than run on empty input (which would produce a spurious
  REFUTED).
- **Standalone:** the MCP `griller.self_grill` tool and the CLI
  `griller self-grill` command audit a decision produced elsewhere.

## CLI

```sh
griller self-grill --claim "Tests pass" --repo . [--evidence-dir <dir>] [--auto-collect]
```

Output is canonical JSON with `original` and `self_grill` top-level keys.
The M7 milestone report is preserved in
[history/m7-self-grill.md](history/m7-self-grill.md).