# TIMEOUT_SEMANTICS.md

**Authoritative description of the timeout model after Phase 2.** The timeout behaviour
itself is not changed by this document; this records what Phase 2 changed and why.

Implemented in commit `0dcf9cd89c7de77ff2b22472e5a3fc94a267fb63`. Evidence and
measurements: [PHASE_2_BASELINE.md](PHASE_2_BASELINE.md). Current defect register:
[CURRENT_STATE.md](CURRENT_STATE.md).

---

## 1. The two timeouts are different kinds of quantity

This is the fact that made the old numbers a structural problem rather than a typo.

| | What it bounds | Constant | Value |
|---|---|---|---|
| **Outer envelope** | The **entire** `check_completion` MCP round-trip: context assessment, evidence collection, every attack, and the self-grill | `griller_gate::DEFAULT_TIMEOUT_MS` | **600 000 ms** (was 120 000) |
| **Inner budget** | **One** attack's command. Applied to every derived replay command | `griller_core::command::REPLAY_TIMEOUT_MS` | **600 000 ms** |
| **Ceiling** | The largest timeout a caller may request | `griller_mcp::MAX_TIMEOUT_MS` | **600 000 ms** |
| (untouched) | `CommandSpec` default when no timeout is set | `griller_core::command::DEFAULT_TIMEOUT_MS` | 30 000 ms |

The gate's constant reaches the deadline in `mcp_client.rs:215` (`request_with_timeout`).
The replay budget reaches the command in `plan.rs:368`. The ceiling is enforced by
`validate_timeout` in `validate.rs:77`.

---

## 2. Why the old 120 s envelope was structurally problematic

Before Phase 2 the outer envelope was a hardcoded `120_000` while the inner budget it
wrapped was `600_000`.

**The envelope was smaller than the thing it enclosed.** A replay that legitimately needed
200 s — a cold Rust build of a large crate is the ordinary case — was killed by the gate at
120 s. The inner budget was never reached, so the 600 s figure was dead configuration on
every cold run.

The failure mode was **fail-closed but misleading**: the gate reported
`GATE_UNAVAILABLE`, which reads as "the verifier is not running" rather than "the verifier
ran out of wall-clock". The correct inference — "this replay is slower than the gate
allows" — was unavailable, because nothing in the design distinguished the two.

It was also a **duplicated literal**. `120_000` and `600_000` were two independent
constants with no relationship expressed between them, so nothing prevented drift and
nothing failed when they had already drifted.

---

## 3. Current relationship: derived equality

```rust
// crates/griller-gate/src/config.rs:20
pub const DEFAULT_TIMEOUT_MS: u64 = griller_core::command::REPLAY_TIMEOUT_MS;

// crates/griller-gate/src/config.rs:25
const _: () = assert!(DEFAULT_TIMEOUT_MS >= griller_core::command::REPLAY_TIMEOUT_MS);
```

The gate's default is now **derived** from the canonical constant, not restated. Two
consequences:

1. **The invariant is enforced by the compiler.** The `const _` assertion fails the build
   if anyone reintroduces a value smaller than the replay budget. The original defect
   cannot be reintroduced silently.
2. **Drift is structurally impossible.** There is one number. Adding a second literal is
   no longer possible without deleting the derivation.

`griller-core` was added as a direct dependency of `griller-gate` to make the reference
legal; the module boundaries allowed clean reuse, so no duplicate constant was needed.

Regression coverage: `gate_default_timeout_is_not_shorter_than_the_replay_budget` and
`gate_config_default_timeout_uses_the_derived_envelope`
(`crates/griller-gate/tests/gate_matrix.rs`), and
`derived_replay_command_carries_the_canonical_replay_timeout`
(`crates/griller-mcp/src/plan.rs`).

---

## 4. Why equality was accepted for this phase

**Outer default == inner replay budget == 600 000 ms.**

Equality removes the structural defect: the envelope can no longer cut short a replay that
the inner budget permits. The remaining question is whether equality is *sufficient*.

**It is not strictly sufficient, and that is accepted and recorded rather than hidden.**
The outer envelope must also cover the work that happens *around* the replay — context
assessment, evidence collection, and the self-grill audit. A replay consuming the full
600 s would reach the inner deadline and the outer deadline at effectively the same
instant, so the envelope could truncate the *reporting* of a replay that had just
legitimately completed.

Equality was accepted because:

1. It eliminates the class of defect that actually caused false `GATE_UNAVAILABLE` results.
2. It is derived, so it cannot silently regress.
3. A cold replay taking the full 600 s is a pathological case, not the common one — the
   motivating scenario (a build needing a few minutes) is now served correctly.
4. The alternative — envelope = inner + margin — requires choosing a margin, which is a
   product decision about how long an agent should be made to wait, not a mechanical fix.

---

## 5. What is NOT implemented

**No margin is added between the outer envelope and the inner budget.** They are equal.

**A margin remains a separate product decision** and is not scheduled as a defect fix. If
it is taken, it should be answered first:

- How long is an agent willing to wait for a verdict?
- Should the margin scale with the inner budget, or be a fixed allowance?
- Does a longer envelope change the gate's responsiveness contract for interactive use?

Until those are answered, equality stands. **Do not treat "outer == inner" as a
correctness property** — it is a removal of a known defect, not a proof of sufficiency.

---

## 6. Status summary

| Item | Status |
|---|---|
| Outer envelope smaller than inner budget | **CLOSED** |
| Duplicated, drift-prone timeout literals | **CLOSED** — single derived source |
| Compiler-enforced invariant | **CLOSED** — `const _` assertion |
| Replay budget reachable within the envelope | **CLOSED** |
| Margin between outer and inner | **NOT STARTED** — product decision, deliberately |
| Cold replay completing inside the envelope | **CLOSED** for the motivating case; pathological full-budget case still theoretically truncatable at the reporting edge |
