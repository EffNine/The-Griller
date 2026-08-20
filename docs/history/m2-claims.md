> **Historical milestone report (M2).** See [docs/concepts.md](../concepts.md) for the current canonical treatment of claims, normalization, and the LLM boundary.

# M2 — Claims, requirements, normalization

This document covers the claim model, requirement provenance, claim
normalization, and the LLM output boundary (`crates/griller-adjudicate`).

## The central question

M2 answers:

> Given a claim and the evidence available, what can we legitimately say about
> the truth/status of that claim?

M2 does **not** answer "should we block the agent?" — that belongs to M3.

## Architectural principle

```
LLM proposes. Evidence disposes.
```

- An LLM (or the deterministic normalizer) may **propose**: normalize text,
  propose predicates, identify candidate targets, propose semantic structure.
- The LLM may **not**: assign truth, assign epistemic status, emit
  PASS/WARN/BLOCK, or override deterministic evidence.
- All LLM output is **untrusted structured input**.
- The deterministic adjudication layer decides epistemic status.

## Claim model

`griller_adjudicate::claim`

| Field | Meaning |
| --- | --- |
| `id` | Content-addressed id: `BLAKE3(canonical_json(source, kind, text, target, predicates, requirement_provenance))`. Equal claims have equal ids. |
| `source` | `agent` \| `user` \| `derived` \| `system_observed` |
| `kind` | `completion` \| `fact` \| `causal` \| `requirement` \| `design` |
| `text` | The verbatim natural-language claim. **Never thrown away.** |
| `target` | Structured scope: `repository` \| `file` \| `function` \| `module` \| `command` \| `test` \| `artifact` + optional name |
| `predicates` | Structured predicates (subject / canonical verb / polarity / object) |
| `requirement_provenance` | For requirement claims: `declared` \| `derived` \| `observed` |

Claim kinds are deliberately few. No new kind was invented without a need.

### CLAIM ≠ EVIDENCE

A claim is an assertion about the codebase. Evidence is a structured record of
what the environment demonstrated. The two are never interchangeable:

- A claim never becomes evidence by being repeated.
- An agent's statement is recorded as `agent_claim` evidence — provenance
  forces `source = agent`, `directly_observed = false`,
  `depends_on_agent_claim = true` — so it can never be upgraded into
  environment evidence.

## Requirement provenance

`griller_adjudicate::requirement`

Requirements MUST distinguish how they entered the system, and MUST NOT be
silently merged:

| Provenance | Meaning | Example |
| --- | --- | --- |
| `declared` | Explicitly stated by the user/task. | User: "Fix login timeout." |
| `derived` | Inferred by an LLM or deterministic analysis. | "Existing authentication behaviour should remain compatible." |
| `observed` | Existing repository/system constraint observed from evidence. | "Integration tests expect a 30 second timeout." |

Guarantees:

- `merge_requirements` deduplicates by `(text, provenance)`. The same text with
  `declared` and `derived` provenance stays **two** requirements.
- A derived requirement is never surfaced as declared (`of_provenance` filters
  strictly).
- The pipeline preserves `requirement_provenance` on the `Claim` and the
  `NormalizedClaim` unchanged.

## Claim normalization

`griller_adjudicate::normalizer`

`ClaimNormalizer` is a trait. Two implementations exist in M2:

| Implementation | When | Failure mode |
| --- | --- | --- |
| `DeterministicNormalizer` | Always available, no LLM. | Never fails; at worst returns `quality = unrecognized`, which the matcher treats as opaque. |
| `LlmNormalizer` (via `llm::parse_llm_proposal`) | Injectable/mocked LLM-backed path. | Malformed/malicious output is rejected; `FallbackNormalizer` degrades to deterministic. |

Normalization maps natural language to structure. Example:

```
"I fixed the authentication timeout."
  -> subject: authentication
     predicate: change (canonical)
     object: authentication_timeout
     target: test/... (or explicit)
```

The canonical proposition (`subject + predicate + polarity + object`) is the
deterministic key used for evidence matching. The verbatim original text is
preserved on `NormalizedClaim.original` in every path.

`NormalizationQuality` is a structural signal (`complete` / `partial` /
`unrecognized`). It is **not** a probability and it never influences epistemic
status directly.

## The LLM output boundary

`griller_adjudicate::llm`

LLM normalization output is untrusted. `LlmClaimProposal` is the only shape
accepted and it contains **only claim-structure fields**
(`claim`, `kind`, `subject`, `predicate`, `object`, `polarity`,
`target_scope`, `target_name`).

- Parsed with `deny_unknown_fields`: fields such as `epistemic_status`,
  `status`, `verdict`, `decision`, `confidence` are **hard parse errors**.
- `NormalizedClaim` has no field that can represent a status or verdict, so
  even a compromised normalizer cannot inject one.
- A well-formed proposal is applied as an **overlay** on top of the
  deterministic normalization; missing fields fall back to deterministic
  values, and the original text always comes from the caller.
- `FallbackNormalizer` converts any parse error into deterministic
  normalization. Malicious output degrades the *structure* at worst, never the
  *status*.

Known limitation (documented, tested): an LLM-proposed `target_name` can widen
or narrow which deterministic matching rules apply (e.g. a garbage target name
turns evidence `insufficient`). This only moves an assessment toward
UNSUPPORTED / UNVERIFIABLE — it cannot produce VERIFIED, FALSE, or any verdict.

See `tests/security.rs` and golden case **I**.
