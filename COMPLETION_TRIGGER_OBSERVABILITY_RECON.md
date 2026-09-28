# COMPLETION_TRIGGER_OBSERVABILITY_RECON.md

**Phase 3 — recon/design only.** No production source, test, Cargo, or plugin change was
made. All statements below are backed by the source or schema cited.

Baseline recorded before inspection and re-verified after: blob manifest of all **174**
tracked non-`.md` files = `c0ad46d76101d72fa1c2ee9a21bd444bdf01a3793369f2bcc94cca0dd0ed288a`,
`Cargo.lock` blob `2bb2af08afe0d814bafe82c61f209235741ca8f3`. Both unchanged.

> **HEAD note.** The brief states `df87efa6c00eb97bfcfeb574ed4b5e8304b3a31`; the repository
> has `df87efa6c00eb97bfcfeb574bed4b5e8304b3a31`. Same commit — one character dropped in the
> brief.

---

## 1. STATUS

Reconnaissance complete. **The architecture is close to sufficient but has exactly one
missing contract**, and it is narrower than the `NO_COMPLETION_ASSERTION` proposal
assumed.

Headline answers:

| Question | Answer |
|---|---|
| Can extraction fail? | **No.** It has no error channel. See §4. |
| Does "no assertion" collapse with "extraction failed"? | **Yes — by construction**, not by accident. §5 |
| Can the existing record schema hold a "nothing to verify" record? | **Yes, already.** `unavailable_decision()` proves the shape. §6 |
| Does `NO_COMPLETION_ASSERTION` need a new `Reason` / `EpistemicStatus`? | **No.** Neither enum needs to represent it. §6 |
| Does the plugin have enough information to author a trustworthy record? | **Yes for "no assertion"; no for anything about verifiability.** §9 |
| Is the missing piece a vocabulary gap or a protocol gap? | **A protocol gap, not a vocabulary gap.** §11 |

---

## 2. EXECUTION_PATH

Traced end to end from source.

```
opencode host
  └─ session.idle event
     └─ GrillerGate event handler            .opencode/plugins/griller-gate.ts:357
        ├─ guards (:358–387)                  see §4 table
        ├─ extract_completion_claim(text)     griller-gate.ts:389
        │    → { claim: string | null, original: string }        :48–135
        ├─ if claim === null → LOG + RETURN   griller-gate.ts:391–402   ← THE GAP
        └─ build CompletionEvent {schema_version, repository, claim,
                                   agent_id, session_id, timestamp_ms,
                                   _diagnostic}                     :406–418
           └─ runGate(event)                  :322–354
              └─ spawn `griller-gate check --config <cfg>`, write event to stdin
                 └─ main::run_check           crates/griller-gate/src/main.rs:58
                    ├─ load_config            main.rs:112
                    ├─ read_event             main.rs:123  (schema + non-empty claim)
                    └─ run_gate               runner.rs:41
                       ├─ config invalid      → GateOutcome::Failed        runner.rs:47   NO RECORD
                       ├─ connect fail        → Decision(Unavailable)      runner.rs:55,75 RECORD
                       ├─ call griller.check_completion                     runner.rs:125
                       │    ├─ tool error     → Decision(Unavailable)      runner.rs:~140 RECORD
                       │    └─ report         → summarize_report           report.rs:33
                       │         verdict pass → GateStatus::Accepted
                       │         verdict warn → GateStatus::Warned
                       │         verdict block→ GateStatus::Blocked
                       │         other        → Decision(Unavailable)      runner.rs:~180 RECORD
                       └─ persist_decision     main.rs:98  ← ONLY record write site
           └─ handleDecision(decision)         :449–496
              ├─ status === "accepted" → log, no injection          :457
              └─ else → inject summary + required_action_message    :473
                        (or one-shot fallback when decision === null) :491
```

**Record write sites: exactly one.** `persist_decision` is called only from
`main.rs:98`, inside `GateOutcome::Decision`. A record exists **iff** the gate produced a
`GateDecision`. `GateOutcome::Failed` (invalid config, rejected event) produces **no
record**.

---

## 3. CURRENT_TRIGGER_STATES

The trigger has a two-valued return, not a state machine.

```ts
// griller-gate.ts:48-50
function extract_completion_claim(text: string): { claim: string | null; original: string }
```

| Returned | Meaning | Then |
|---|---|---|
| `claim: string` | an assertion was located | gate is invoked |
| `claim: null` | no assertion located (line 134) | **log and return** (`:391`) |

**There is no third value and no error channel.** Confirmed by inspection:

- `throw` statements in `griller-gate.ts`: **0**
- `try`/`catch` wrapping the call at `:389`: **none** (4 `try` blocks exist, all around
  `client.*` calls and `JSON.parse`)
- The only `null` return is `:134` — `// No recognizable completion claim found.`

So the extraction function is **total**: it maps every string to a claim or to `null`, and
cannot fail. The three-path algorithm (marker `:56`, heuristic tail `:88`, fallback `:119`)
has no error branch, no I/O, and no dependency that could throw.

---

## 4. SILENT_SKIP_PATHS

Every path where the handler returns without invoking the gate. Exact locations in
`.opencode/plugins/griller-gate.ts`.

| # | Line | Condition | Classification | Rationale |
|---|---|---|---|---|
| 1 | `:358` | `event.type !== "session.idle"` | **SAFE / AUDITABLE** | Not a completion boundary. No completion attempt exists to record. |
| 2 | `:361` | `state.inFlight` | **UNKNOWN** | Re-entrancy guard. Drops the event with **no log at all** — not even the app log used by path 9. Cannot be distinguished from "never fired". See §12. |
| 3 | `:370-372` | `client.session.messages` throws | **FAIL-OPEN (silent)** | Logs `warn` to the app log, then returns. A genuine assistant completion is **never gated and never surfaced to the agent**. The agent proceeds believing it completed. |
| 4 | `:376` | no assistant messages | **SAFE / AUDITABLE** | No completion attempt. |
| 5 | `:378` | `latest.info.id === state.lastChecked` | **SAFE / AUDITABLE** | Deliberate per-session dedupe, documented at `docs/gate.md:66`. |
| 6 | `:379` | `latest.info.error` | **SAFE / AUDITABLE** | Message errored; not a completion. |
| 7 | `:380` | `latest.info.summary` | **SAFE / AUDITABLE** | Summary message, not a completion. |
| 8 | `:387` | `!text` (no text parts) | **SAFE / AUDITABLE** | Nothing to extract. |
| 9 | **`:391-402`** | **`claim === null`** | **SAFE BUT NON-AUDITABLE** | **The documented gap.** Logs `info` to the app log, sets `state.lastChecked`, returns. No record, no gate run, no injection. |
| 10 | `:423-427` | gate produced no JSON | **FAIL-CLOSED** | `handleDecision(..., null)` → one-shot fallback injected at `:491`. |
| 11 | `:429-433` | gate exited non-zero / spawn failed | **FAIL-CLOSED** | Logged `error` + fallback injected. |
| 12 | `:437-441` | `JSON.parse` failed | **FAIL-CLOSED** | Logged `error` + fallback injected. |

**Path 9 is the one under investigation.** It is intentional and documented:

> `docs/gate.md:82` — *"If no claim can be extracted, the message is not a completion
> event — the plugin skips it silently (no gate run, no injection). Fail-closed behaviour
> applies only when a gate run itself fails."*

**Path 3 is a second, undocumented instance of the same class** and was not previously
recorded. It is a genuine completion that goes ungated with no agent-visible consequence.

**Coverage gap:** the plugin test file has exactly one `describe` block
(`extract_completion_claim`, line 207) with **24 cases**. `GrillerGate`, the `event`
handler, `runGate` and `handleDecision` are **never imported or exercised**. **Paths 2, 3
and 9 have no test at all.**

---

## 5. STATE_COLLAPSE_POINTS

The four states in scope, and whether the current architecture distinguishes them.

| State | Distinguishable today? | Where it lands |
|---|---|---|
| **A. No completion assertion exists** | Yes — but only *in the trigger*, and only as `null` | `extract_completion_claim` `:134` → trigger path 9 |
| **B. Assertion expected, extraction failed** | **Cannot exist.** Extraction is total (§3) | collapses into A **by construction** |
| **C. Assertion exists, was evaluated** | Yes | full pipeline → `GateStatus::Accepted/Warned/Blocked` |
| **D. Assertion exists, verification unavailable/unverifiable** | Yes, and well-modelled | `EpistemicStatus::Unverifiable` → `UNVERIFIABLE_*_BLOCK`; integration failure → `GateStatus::Unavailable` |
| **E. Blocked by a known repo/trust condition (dirty tree)** | Yes, added in `16c6852` | `EpistemicStatus::Unverifiable` + `repository.refusal_reason` in the report |

### The single collapse point

**States A and B are the same state.** Not because a distinction was dropped, but because
**the system never had one**: `extract_completion_claim` has no error channel, so "I looked
and found nothing" and "I could not look" are not separately expressible.

This is a **missing contract, not a missing distinction**. The question "how do I
distinguish A from B?" has the answer "A is the only one that can occur".

Consequence for the stated objective: *"a trigger extraction failure"* (item 3) is
currently an **unreachable state**, not an unobservable one. Representing it would mean
*inventing* a failure mode for a total function — which is why §11 recommends **not**
adding vocabulary for it.

### A second, different collapse: the two `unverifiable` meanings

`EpistemicStatus::Unverifiable` and `GateStatus::Unavailable` are **different types at
different layers** and are easy to conflate in a report:

- `EpistemicStatus::Unverifiable` — adjudication ran; the environment could not determine
  the claim.
- `GateStatus::Unavailable` — the gate could not run at all; no adjudication happened.

Both are non-passing, so this does not weaken fail-closed behaviour. It does mean a record
reader must check `status` *and* `griller_report` to know which happened. Historical
evidence confirms both occur: 10 records with `status: "unavailable"`
(`reason_code`: 8 × `CHECK_COMPLETION_FAILED`, 2 × `STALE_SERVER`) alongside 6 with
`"unverifiable"`.

---

## 6. EXISTING_VOCABULARY

What already exists, and whether it can carry the target states **without** new types.

### Decision record (`GateDecision`, `crates/griller-gate/src/lib.rs:186-211`)

| Field | Type | New value needed? |
|---|---|---|
| `status` | `GateStatus` — **closed 4-variant enum** (`lib.rs:100-112`) | **YES — a new variant** |
| `verdict` | `Option<String>` (`:195`) | no |
| `reason_code` | **`Option<String>` — free-form, not an enum** (`:197`) | **no** |
| `findings` | `Vec<Finding { key: String, detail: String }>` — **free-form** | **no** |
| `required_action` / `required_action_message` | `Option<String>` | no |
| `event` | `EventSummary` — repository, agent_id, session_id, commit, timestamp_ms. **No claim field** | no |
| `griller_report` | **`Option<serde_json::Value>`, `skip_serializing_if = "Option::is_none"`** (`:206`) | no |
| `timing`, `lifecycle`, `integration` | structs | no |

### The decisive precedent

`unavailable_decision()` (`runner.rs:295-334`) already constructs a **complete
`GateDecision` with `griller_report: None`, `verdict: None`, and no claim anywhere**,
persisted to the same `decisions/` directory by the same single write site.

> **The record schema can already represent "the gate ran and there was nothing to
> verify".** No new field, no new type, no schema version bump, no migration of the 131
> existing records.

### Adjudication vocabulary (`crates/griller-adjudicate/src/status.rs`)

| Type | Shape | Can it express "no assertion"? |
|---|---|---|
| `EpistemicStatus` | 7 variants, `#[non_exhaustive]`, `status.rs:24-53` | **No, and should not.** It is a verdict about a *claim*. With no claim there is no verdict. |
| `Reason` | 11 variants, closed, `status.rs:57-78` | **No.** Closest is `NoRelevantEvidence` / `IrrelevantEvidenceOnly`, both of which presuppose a claim exists. |
| `InsufficiencyNote` | `{ evidence_id, relationship, reason }` | **No.** Requires an `EvidenceId`; there is no evidence. |

**Conclusion: nothing in the adjudication layer should change.** "No completion assertion"
is a **pre-adjudication fact about the host's input**, not an epistemic status. Forcing it
into `EpistemicStatus` would be a category error and would put a non-verdict into a
fail-closed decision surface.

### Adjacent existing vocabulary

| Item | Location | Reusable? |
|---|---|---|
| `AttackOutcome::NotApplicable` | `crates/griller-core/src/attack.rs:166` | Semantic precedent only — it is a *per-attack* outcome, not a per-event one |
| `not_applicable: usize` | `attack_coverage.rs:64` | counting field for attacks; not an event status |
| `expected_skipped: Option<u64>` | `claim.rs:176` | unrelated (test-count claims) |
| `GateStatus::Unavailable` | `lib.rs:109-111` | **Wrong semantics** — means "could not run". See §8. |

**No existing record has ever represented a skip.** The 131 historical records contain
statuses `accepted`(23), `unsupported`(111), `blocked`(98), `contradicted`(33),
`verified`(28), `likely_match`(41), `matched`(20), `mismatch`(25), `unavailable`(10),
`ambiguous`(25), `unverifiable`(6), `weak_relevance`(3) — and nothing resembling
"nothing to verify".

---

## 7. NO_COMPLETION_ASSERTION_OPTIONS

Four options, assessed against the actual constraints.

### Option 1 — Plugin authors the record itself (TypeScript)

The plugin would construct a `GateDecision`-shaped JSON and write it to
`~/.local/share/griller/gate/decisions/`.

- **Rejected.** It requires the plugin to reimplement the `g1.0` record schema,
  `GateStatus` serialisation, the lifecycle log, and the filename convention
  (`main.rs:186-189`). The plugin becomes a second, unaudited writer of the audit
  store. Any drift is invisible to the Rust type system. This is the "plugin becomes a
  policy engine" failure mode explicitly ruled out.
- **Rejected on a second, harder ground:** the plugin does not know the repository's
  `state_dir` (it reads only `resolveGateConfig()` at `:290` and passes a `--config`
  path). It would have to re-derive the path convention.

### Option 2 — Reuse `GateStatus::Unavailable`

- **Rejected — semantically wrong and behaviourally regressive.** `Unavailable` is
  documented as *"The Griller could not run (unavailable, malformed, stale, timeout)"*
  (`lib.rs:109-110`). Nothing failed. More importantly, `handleDecision`
  (`griller-gate.ts:473`) injects a message for **every** status that is not `"accepted"`,
  so `Unavailable` would re-inject a "completion not accepted" notice onto every ordinary
  technical message — **exactly the regression that removing the old fail-closed
  default fixed** and that `docs/gate.md:82` documents as deliberate.
- It would also pollute the 10 existing `unavailable` records' semantics.

### Option 3 — Send an explicit "no assertion" state into `check_completion`

- **Rejected — blocked by the existing contract, at both boundaries.**
  `CompletionRequest.claim` is `String`, not `Option<String>`
  (`completion.rs:63-73`), and `validate_text` (`validate.rs:50-57`) **rejects an empty
  claim** with `InvalidRequest`. Symmetrically, `CompletionEvent.claim` is required and
  `read_event` (`main.rs:139-141`) rejects an empty claim before `run_gate`.
- It would also mean asking the *verification engine* to adjudicate a non-claim, which
  puts a host-side fact into the epistemic layer. That is the category error of §6.

### Option 4 — Plugin sends an explicit extraction outcome; **gate authors the record**

The plugin reports a *fact about its own input*; the Rust gate remains the sole owner of
record shape, status vocabulary, and persistence.

- **Recommended.** See §11.

---

## 8. SECURITY_ANALYSIS

### 8.1 Silently skipping (current, path 9)

| Property | Assessment |
|---|---|
| Can it produce a false `VERIFIED`? | **No.** No gate run, no report, no record. Nothing claims verification. |
| Can an agent be misled? | **No.** No message is injected; the agent proceeds with its own belief, exactly as if the gate did not exist. |
| Is the trust model weakened? | **No** — because the gate is **unregistered** (`griller` appears 0× in `~/.config/opencode/opencode.jsonc`; no plugin installed). Nothing reaches it. |
| Is it auditable? | **No.** Only an app-log line, which is not part of the decision store. |
| Verdict | **Acceptable *now*, unacceptable *on exposure*.** The risk is entirely an *audit* gap, and it only becomes a *trust* gap once item 5 (MCP exposure review) proceeds. |

### 8.2 Emitting an observable "no assertion"

| Risk | Analysis |
|---|---|
| Could it collapse into `PASS`? | **Only if `GateStatus::Accepted` were used.** A dedicated non-accepted status makes this structurally impossible. |
| Could a consumer treat "recorded" as "checked"? | Possible. Mitigated by the status name being explicit, and by the `griller_report: None` signal already used by `Unavailable`. |
| Volume / cost | One record per non-completion assistant message. The store is append-only JSON; 131 records ≈ 2.9 MB. Sustained use would grow it. **Retention is a real consideration, not a blocker.** |
| Could a hostile agent *suppress* its own check? | **Yes — and it already can.** Crafting text that fails extraction yields a skip today. Making the skip *visible* does not change this, but it makes it **detectable**, which is the point. This is a strong argument for the change. |

### 8.3 Treating extraction failure as "no assertion"

Currently **moot** — extraction cannot fail (§3). If a failure mode were ever introduced,
mapping it to "no assertion" would be **FAIL-OPEN**: a genuine completion could be silently
ungated. **The design must keep the two distinct if a failure mode is ever added** — but
adding one now would be inventing a state for a total function.

### 8.4 Routing extraction failure to `UNVERIFIABLE`

Correct *semantics* (non-passing, and `Unverifiable` means "the environment could not
determine this") — but it requires the failure to reach adjudication, which needs a claim
to adjudicate. **Not applicable to a total function.** If extraction later acquires a
failure mode, `Unverifiable` is the right epistemic answer *provided* a claim was
extracted first.

### 8.5 Invoking the gate with an explicit "no assertion" state

This is Option 3, rejected in §7 on contract grounds. It also has the security
disadvantage of putting a host-side assertion into the verification protocol, where it
could be forged by any MCP caller.

### 8.6 The invariant that must not break

> **Insufficient evidence must never collapse into `PASS` / `VERIFIED`.**

Any design must satisfy: *a new observable non-completion state is never `Accepted`.* The
recommended design enforces this structurally — the new `GateStatus` variant is
non-`Accepted` by construction, and `handleDecision`'s `status === "accepted"` test
(`griller-gate.ts:457`) is the single point that can let a completion proceed.

---

## 9. OWNERSHIP_BOUNDARY

### What the plugin legitimately owns

Extraction is a **total, deterministic, pure function of the assistant text**
(`griller-gate.ts:48`). The plugin is the only component that has seen that text. For the
single question *"did an assertion exist in this message?"* the plugin is the **authoritative
source**, and its answer is not a policy judgement — it is a measurement.

It is already making this decision today; the proposal only makes it **visible**.

### What the plugin must not own

| Must not | Why |
|---|---|
| Record shape / schema | `g1.0` `GateDecision` is a Rust type with `#[derive(Serialize)]` and a versioned filename convention (`main.rs:186-189`) |
| Status vocabulary | `GateStatus` is a closed Rust enum — a second definition in TS would drift silently |
| Persistence | The gate is the sole writer of `decisions/` and `gate.log` (`main.rs:98`) |
| Anything about verifiability | The plugin has no repository state, no evidence, and no access to the MCP session. It cannot know whether verification *would* have succeeded. |

### Does the plugin have enough information to author a trustworthy record?

**For state A (no assertion): yes.** It can report the fact, plus enough context
(`repository`, `session_id`, message length, whether the explicit marker was present) for
the gate to author a defensible record.

**For states B, D, E: no.** These require repository and evidence state. They are already
handled inside the Rust pipeline and must stay there.

**Verdict:** the correct split is **plugin = sensor, gate = policy and record owner.** The
plugin reports an observation; the gate decides what that observation means and writes it
down.

---

## 10. COMPATIBILITY_SURFACE

Everything a change would touch. **Nothing was modified.**

| Consumer | Location | Impact of a new `GateStatus` variant | Impact of a new event field |
|---|---|---|---|
| **Plugin** | `griller-gate.ts:456` (`decision?.status as string`) | **Must change.** Currently `status === "accepted"` else inject. A new status would be treated as "not accepted" and **inject a message** — the regression in §7 Option 2 | **Must change** if the event carries the outcome |
| **Plugin tests** | `griller-gate.test.ts` — 24 cases, all in `describe("extract_completion_claim")` | **No impact** — the handler is not exercised at all | none |
| **Gate serialisation** | `lib.rs:102` `#[serde(rename_all = "snake_case")]` | New variant gains a stable wire name; **old readers of new records** would see an unknown string | additive `Option` field is backward-compatible |
| **Gate tests** | `tests/gate_matrix.rs` — 17 cases; `src/lib.rs`, `runner.rs` unit tests | `GateStatus::` appears **19×** across `crates/griller-gate/`; `match self.status` at `lib.rs:216` and `match status` at `runner.rs:199` would need a new arm if the enum is matched exhaustively | additive |
| **Decision records** | 131 historical JSON files | `#[serde(default)]` is present on the optional fields, but `status` has **no** `#[serde(other)]` fallback — an old binary reading a new status would **fail to deserialise** | additive; historical records untouched |
| **MCP protocol** | `griller.check_completion` schema, `lib.rs:557-563` | **No impact** — the gate would short-circuit before calling MCP, exactly as `unavailable_decision` does | n/a |
| **CLI** | `griller-gate check`, `main.rs:54`; `render_lifecycle` `main.rs:145` | Renders from the decision; a new status would render via existing `{}` paths | n/a |
| **`griller_report`** | `Option<Value>` | unaffected | n/a |
| **Plugin message text** | `griller-gate.ts:466-482` | Would need a branch so the new status does **not** inject | n/a |
| **Documentation** | `docs/gate.md:51,66-82`; `docs/opencode.md`; `NEXT_PHASE.md` items 2–3 | Describes the skip as intentional; must be reconciled | n/a |
| **Downstream automation** | **None found** — grep across the workspace found no consumer of `decisions/*.json` outside the gate itself | — | — |

**Two compatibility risks worth naming:**

1. **`GateStatus` has no `#[serde(other)]`.** A new variant makes new records
   **unreadable by an older `griller-gate` binary**. Given the gate is unregistered and
   unpushed, this is currently theoretical — but it is a forward-compatibility
   consideration, not a non-issue.
2. **The plugin's `status === "accepted"` test is a fail-open default for unknown
   statuses.** Today, an unrecognised status falls into the "inject" branch — which is
   fail-*closed* and therefore safe. Any change must preserve that direction.

---

## 11. RECOMMENDED_MINIMAL_DESIGN

**One new value, in one existing enum, plus one optional field on an existing struct, plus
one new branch in the plugin. No new type, no new protocol, no new service, no schema
version bump.**

### 11.1 The missing contract

> **A completion-check attempt that finds nothing to verify must be representable as a
> first-class, non-passing, auditable outcome — and it must be authored by the component
> that owns record shape.**

That is the whole gap. Everything else already exists.

### 11.2 The four changes

**Change 1 — one new `GateStatus` variant** (`crates/griller-gate/src/lib.rs`, beside
`Unavailable` at `:109-111`):

```
NoCompletionAssertion
```

- Non-`Accepted` **by construction** — the invariant of §8.6 holds structurally.
- Semantically distinct from `Unavailable` (nothing failed) and from `Blocked` (nothing was
  refused).
- Serialises to a stable snake_case wire name.

**Change 2 — one reason code, no new enum.** `reason_code` is `Option<String>`
(`lib.rs:197`) — free-form. The existing precedent is `CHECK_COMPLETION_FAILED` /
`STALE_SERVER` on the 10 `unavailable` records. A value such as
`NO_COMPLETION_ASSERTION` needs **no enum change and no migration**.

**Change 3 — the record is authored in Rust, by the existing single write path.** A new
constructor beside `unavailable_decision()` (`runner.rs:295`) — for example
`no_assertion_decision()` — building the same `GateDecision` shape with
`griller_report: None`, `verdict: None`, and a `Finding { key: "completion_trigger",
detail }` recording *why* nothing was extracted (marker absent, heuristic exhausted, or
guard-reached). Reached through `main.rs:98`, so persistence, the filename convention, and
`gate.log` all stay in one place.

**Change 4 — the plugin reports the fact, not the policy.** The event gains one optional
field (for example `extraction_outcome`) defaulting to absent = "assertion present",
preserving every existing caller. The gate maps it to Change 1/3. The plugin's
`handleDecision` gains one branch so the new status is **recorded and not injected** —
preserving the current `docs/gate.md:82` behaviour exactly.

### 11.3 What this deliberately does NOT do

| Not done | Why |
|---|---|
| No `EpistemicStatus` variant | "No assertion" is a fact about the host's input, not a verdict. Adjudication is never reached. |
| No `Reason` variant | Same reason; and `InsufficiencyNote` requires an `EvidenceId` that does not exist. |
| No change to `griller.check_completion` | Its contract ("evaluate a claim") is correct as-is. |
| No `NO_COMPLETION_ASSERTION` *epistemic* status | Would be a category error. |
| No extraction-failure state | Extraction is total (§3). Adding one would invent a failure mode. |
| No new file, service, or record directory | `unavailable_decision` proves the existing store suffices. |
| No retroactive change to the 131 records | They are historical evidence. |
| No change to `docs/gate.md:82` policy | The *behaviour* stays identical; only its auditability changes. |

### 11.4 Why this is the smallest sufficient change

- The record shape is **already proven** by `unavailable_decision()`.
- `reason_code` and `findings` are **already free-form** — no enum, no schema bump.
- `griller_report: Option<…>` with `skip_serializing_if` already supports a report-less
  record.
- The only genuinely new *type* is one `GateStatus` variant, and the only new *logic* is
  one branch in the plugin and one constructor in Rust.

---

## 12. DO_NOT_IMPLEMENT_YET

Recorded so a later phase does not rediscover them as new.

| # | Item | Why not now |
|---|---|---|
| 1 | Any of the four changes in §11.2 | This phase is recon/design only. |
| 2 | Path 3 (`:370-372`, message-list failure) | A **second** silent-skip class, not previously recorded. Same underlying gap, but it needs its own decision: should a message-listing failure be fail-*closed* to the agent? That is a policy change with a real UX cost. |
| 3 | Path 2 (`:361`, `state.inFlight`) | Drops an event with **no log at all**. Whether it needs a record is unclear — it may be legitimate coalescing. Needs a decision, not a fix. |
| 4 | `#[serde(other)]` on `GateStatus` | Forward-compatibility for old binaries reading new records. Real but currently theoretical. |
| 5 | Retention / rotation for `decisions/` | Volume grows once every message is recorded. Not a blocker; do not design it before the volume is real. |
| 6 | Making extraction fallible | Would require `try`/`catch`, a failure state, and a decision on its semantics. **Out of scope** — a total function should stay total. |
| 7 | MCP exposure | Explicitly excluded. And this work is a **prerequisite** for it, not a consequence. |
| 8 | Any change to the adjudication layer | `status.rs`, `matcher.rs`, `provenance.rs` must stay untouched. |

---

## 13. NEXT_PHASE_DEPENDENCIES

Ordered by dependency, not preference. Full register in [NEXT_PHASE.md](NEXT_PHASE.md).

```
  ┌─ OWNER DECISIONS (not engineering) ────────────────────────┐
  │ D1 push to github.com/EffNine/The-Griller?                  │
  │ D2 may session-level absolute paths leave the machine?      │  → gates E1
  └─────────────────────────────────────────────────────────────┘

  ┌─ THIS WORK (Phase 4 candidate) ────────────────────────────┐
  │ P1 add GateStatus variant + reason code                    │
  │ P2 add no_assertion_decision() in runner.rs                │
  │ P3 add extraction_outcome to CompletionEvent (optional)     │
  │ P4 teach plugin handleDecision the new status              │
  │ P5 tests: plugin skip branch (currently UNTESTED),         │
  │          gate record shape, back-compat                    │
  │    blocked by: nothing                                       │
  │    blocks:     M1 (MCP exposure review)                     │
  └─────────────────────────────────────────────────────────────┘

  ┌─ EXPOSURE ─────────────────────────────────────────────────┐
  │ M1 MCP exposure review  ← blocked by P1–P5, and by:        │
  │      · the (Fact|Design, Unverifiable) → AlwaysWarn policy │
  │      · the never-run e2e path                              │
  │      · dead ~/.config/griller/gate.json                    │
  │    then D1 (push decision) is separate but adjacent        │
  └─────────────────────────────────────────────────────────────┘

  E1 independent evidence backup  ← blocked by D2 only
```

**The blocking relationship that matters:** MCP exposure (M1) is what turns the audit gap
into a trust gap. Everything in §11 exists to make the skip **visible** before an agent can
reach it. Exposure must not proceed first.

---

## 14. FILES THAT WOULD NEED MODIFICATION NEXT

**Listed only. None was modified in this phase.**

| File | Change | Size estimate |
|---|---|---|
| `crates/griller-gate/src/lib.rs` | one `GateStatus` variant; possibly `#[serde(other)]` | ~6 lines |
| `crates/griller-gate/src/runner.rs` | `no_assertion_decision()` beside `unavailable_decision()` (`:295`); one mapping arm | ~40 lines |
| `crates/griller-gate/src/lib.rs` (`CompletionEvent`) | one `Option<…>` field, default-absent | ~4 lines |
| `crates/griller-gate/src/main.rs` | **no change expected** — records flow through the existing single write site (`:98`) | 0 |
| `.opencode/plugins/griller-gate.ts` | set the field at the event-build site (`:406`); one non-injecting branch in `handleDecision` (`:456`) | ~20 lines |
| `crates/griller-gate/tests/gate_matrix.rs` | new cases: record shape, status serialisation, no-injection | ~40 lines |
| `.opencode/plugins/griller-gate.test.ts` | **first tests for the event handler at all** — paths 2, 3, 9 are untested | ~60 lines |
| `docs/gate.md`, `docs/opencode.md` | reconcile `:82` — behaviour unchanged, auditability changed | ~15 lines |
| `NEXT_PHASE.md`, `CURRENT_STATE.md`, `PHASE_HISTORY.md` | close items 2–3, record Phase 3 | ~40 lines |

**Explicitly NOT in that list:** `crates/griller-adjudicate/**`,
`crates/griller-mcp/src/completion.rs`, `crates/griller-runtime/**`,
`crates/griller-core/**`, `Cargo.toml` / `Cargo.lock`, the MCP tool schema, and the
historical evidence store.

---

**IMPLEMENTATION: NOT STARTED**

Production source, tests, `Cargo.toml`, `Cargo.lock`, plugin behaviour, and MCP
registration are all unchanged. Verified: blob manifest of all 174 tracked non-`.md` files
is `c0ad46d76101d72fa1c2ee9a21bd444bdf01a3793369f2bcc94cca0dd0ed288a`, identical to the
pre-inspection baseline, and `Cargo.lock` blob `2bb2af08afe0d814bafe82c61f209235741ca8f3`
is absent from the working tree diff.
