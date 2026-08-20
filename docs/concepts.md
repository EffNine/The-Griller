# Core concepts

Everything in The Griller depends on keeping six concepts strictly apart.
Confusing any two of them is the bug this project exists to prevent.

## The six concepts

```text
CLAIM            what someone said
EVIDENCE         what the environment demonstrated
EPISTEMIC STATUS what the evidence supports saying about the claim
VERDICT          what the policy says to do about it
ATTACK           an active attempt to find a legitimate objection
SELF-GRILL       an audit of the Griller's own decision structure
```

### CLAIM

An assertion about a codebase: *"All tests pass."*, *"The file
src/greet.rs exists."*, *"Implemented Atlas P10.5.1 Fact Store."*.

- A claim is **not** evidence. Repeating a claim does not strengthen it.
- Claims carry provenance: `agent` / `user` / `derived` / `system_observed`,
  plus a kind (`completion`, `fact`, `causal`, `requirement`, `design`) and a
  target scope. The verbatim text is never thrown away.

### EVIDENCE

A structured, immutable, content-addressed record of what the environment
actually demonstrated: a replayed `cargo test` run (with exit code and
redacted output), a git diff, a file read, a repository snapshot.

- Evidence is identified by `BLAKE3(content)` — same observation, same id,
  anywhere, any time.
- Provenance records *how* it was captured (`source`, `capture_method`,
  `directly_observed`, `reproducibility`, …). Agent narration is always
  `source = agent`, `directly_observed = false` — structurally incapable of
  being upgraded into environment evidence.
- Philosophy: **evidence is a captured receipt, not a sentence saying
  something happened.**

### EPISTEMIC STATUS

What the evidence legitimately supports saying about the claim (M2):

| Status | Meaning |
| --- | --- |
| `VERIFIED` | strong direct evidence supports the claim; no known contradiction |
| `ADVERSARIALLY_VERIFIED` | verified *and* survived implemented adversarial attacks (reserved for the attack engine) |
| `UNSUPPORTED` | claim is meaningful, but no sufficient supporting evidence exists |
| `UNVERIFIABLE` | the environment could not determine the claim (missing tool, timeout) |
| `UNCERTAIN` | evidence conflicts, is incomplete, or is ambiguous |
| `CONTRADICTED` | relevant direct evidence conflicts with the claim |
| `FALSE` | direct evidence establishes the claim is false (a deliberately narrow status) |

A status is a statement about evidence, never a command.

### VERDICT

An operational decision (M3): **PASS / WARN / BLOCK**, derived from the
epistemic status × claim kind × severity through an explicit, auditable
policy matrix. A verdict says *"what should the system do next?"* — never
*"is the claim true?"*.

- `BLOCK` does **not** mean "the claim is false." It means "this claim cannot
  be accepted as-is."
- Verdicts carry stable reason codes (`CONTRADICTED_COMPLETION_BLOCK`, …) and
  required actions (`resolve_conflict`, `confirm_project`, …). Downstream
  systems parse codes, never prose.

### ATTACK

A safe, deterministic adversarial check (M5) that actively tries to find a
legitimate reason the claim should not be trusted: replay the verification
command, check presence/absence, sweep for stubs, scan for safety hazards.
An attack that finds nothing is meaningful evidence — but `NO_OBJECTION`
means "this attack found no legitimate objection within its scope," not
"the claim is proven true."

### SELF-GRILL

A deterministic, single-pass audit (M7) of the Griller's own decision
structure: do the evidence references resolve? Does the policy recompute to
the same verdict? Was the evidence independent? It audits the *pipeline*,
never the *software*. The original decision is immutable; self-grill
produces a separate result that the caller may act on.

## The four non-equivalences that matter

### UNSUPPORTED ≠ BLOCK

`UNSUPPORTED` is an epistemic status: *"no sufficient evidence exists."*
Whether that leads to a block is a policy question that depends on claim
kind and severity. An unsupported fact claim about a design question is a
WARN; an unsupported high-severity completion claim is a BLOCK. The status
never carries a verdict; the policy table decides.

### VERIFIED ≠ formal proof

`VERIFIED` means: the environment directly demonstrated what the claim
asserted, and no known evidence contradicts it. It is a judgment based on
the evidence available — not mathematical proof. A `cargo test` run that
exits 0 verifies *"tests pass"* within the scope of that run, on that
machine, against that committed state. It does not prove the software is
correct, nor that the claim holds everywhere.

### PASS ≠ software correctness proof

`PASS` means: the claim survived context adjudication, evidence
adjudication, the implemented attacks, and the self-grill audit. The
software may still be wrong in ways the pipeline cannot see — missing
evidence, wrong domain assumptions, attacks that were not implemented.
A PASS is a statement about the *claim's verification*, bounded by the
implemented attack coverage.

### ADVERSARIALLY_VERIFIED means stronger evidence under the implemented attack coverage — not mathematical proof

`ADVERSARIALLY_VERIFIED` is the strongest status: direct evidence plus at
least one applicable attack that executed successfully and found no
objection, with no contradictory evidence. It is *at least as strong as*
`VERIFIED`, but it is bounded by which attacks exist and were applicable.

Two honest consequences in the current implementation:

- The deterministic adjudicator can produce at most `VERIFIED`. Only the
  optional LLM-backed adjudicator can assign `ADVERSARIALLY_VERIFIED`, and it
  is not enabled in the default server. The status is unreachable in the
  default pipeline by design — the M5 coverage conditions are strict.
- `EXECUTION_FAILED` ≠ `REFUTED`: an attack that could not run is not
  evidence against the claim. It just prevents adversarial verification.

## The separation is the product

| Concept | Question | Produced by | Never |
| --- | --- | --- | --- |
| CLAIM | what was said? | agent/user | becomes evidence |
| EVIDENCE | what was demonstrated? | collectors (runtime) | decides anything |
| EPISTEMIC STATUS | what does evidence support? | M2 adjudication | a verdict |
| VERDICT | what should we do? | M3 policy | a truth claim |
| ATTACK | can we find an objection? | M5 red team | a verdict by itself |
| SELF-GRILL | did the pipeline stay consistent? | M7 audit | mutates the decision |

An epistemic status is never a verdict. A verdict is never a truth claim. An
attack result is never a status. A self-grill outcome is never an override.

See also: [verification-pipeline.md](verification-pipeline.md) for how the
concepts flow through one `check_completion` call, and
[evidence.md](evidence.md) for the evidence model itself.