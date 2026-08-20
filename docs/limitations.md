# Limitations

These are current, honest limitations — not footnotes. Read them before
deciding what the Griller can guard.

## Execution and isolation

**1. No OS sandbox.**
Verification commands run on the host with the calling user's privileges.
There is no seccomp, no namespace isolation, no bubblewrap/landlock. Malicious
code in a repository could, in principle, touch anything that user can touch.
Worktree isolation mitigates *repository* mutation, not *host* compromise.

**2. No kernel network enforcement.**
`network_policy = disabled` adds `GIT_TERMINAL_PROMPT=0` so tools fail closed
instead of prompting — nothing more. A command can still reach the network.
The `Enforcement` record truthfully reports `NotEnforced`.

**3. No privilege reduction.**
Commands execute as the calling user with that user's full access.

**4. Best-effort secret redaction.**
Redaction detects obvious patterns (bearer tokens, API-key prefixes, PEM
blocks, credential assignments, common cloud formats). Novel or obfuscated
secrets pass through. Command-line arguments are **not** redacted — never
pass secrets as arguments.

**5. Replay has language/package-manager limitations.**
The replay command is derived from the claim text and currently targets the
Rust/`cargo` ecosystem (`cargo test`, `clippy`, `fmt`, `check`, `build`).
Claims about other ecosystems may not get a replay attack, which means
weaker adversarial evidence — never a fabricated pass.

**6. Worktree verification focuses on committed state.**
Replay runs against a disposable worktree of `HEAD` — the committed state.
Uncommitted agent changes are not carried into isolation, so "does the
agent's proposed change work?" is not the question answered; "does the
committed state verify?" is.

**7. Non-git repositories have reduced isolation.**
Worktree isolation requires a git repository. Non-git paths fall back to
in-place execution of read-only commands in the given directory — as safe as
the machine it runs on.

## Adversarial coverage

**8. M5 has limited/foundation attack types where applicable.**
Of the eight attack kinds, four are implemented (Replay, PresenceAbsence,
StubSweep, Safety). Completeness, Invert, Robustness, and
RequirementCoverage are recognized but not planned or executed by the
default pipeline. Within the implemented kinds, the checks are deliberately
narrow: stub sweep scans changed files for a fixed marker list; safety scans
for a fixed pattern set; it is not a security scanner.

**9. ADVERSARIALLY_VERIFIED depends on sufficient attack coverage.**
The status requires executed attacks that found no objection — and the
default deterministic pipeline cannot produce it at all (it reaches at most
`VERIFIED`). This is by design: the coverage conditions are strict and the
attack catalogue is partial. Expect the strongest status to stay unreachable
until more attack kinds are implemented.

**10. Self-grill audits structural consistency, not software correctness.**
The 16 checks verify that the decision pipeline stayed consistent with its
own rules. They cannot detect wrong domain knowledge in evidence, missing
evidence that would change the outcome, or LLM-injected assumptions that
passed every structural check.

## Matching and context

**11. Deterministic matching can miss semantically equivalent names.**
Matching is explicit and rule-based: `factstorage` does not match "Fact
Store", a renamed module may not match, and a symbol spelled differently gets
at most weak evidence. The normalizer only collapses obvious casing and
separator variants so unrelated projects never collide.

**12. Context detection can have false positives/negatives.**
- Fully-uppercase foreign project names (`ATLAS`) are not treated as project
  candidates (an all-caps exclusion that favours fewer false positives).
- A capitalized third-party name missing from the guard list could be
  mistaken for a foreign project (residual false positive).
- `INSUFFICIENT_CONTEXT` (generic task, empty repo) is a WARN, never a
  BLOCK — and absence of evidence is never mismatch.

## Honest corollaries

- `VERIFIED` / `PASS` are not correctness proofs — they mean the environment
  demonstrated what the claim asserted, within the implemented coverage.
- A failed verification is a verdict, never a system error — and a system
  error is never a verdict.
- The Griller judges; it does not fix. Nothing is ever modified automatically.

See [security.md](security.md) for the security-model version of this list.