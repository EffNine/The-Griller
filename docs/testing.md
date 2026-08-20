# Testing

The Griller is a verification layer; its own test suite is correspondingly
obsessive. Everything is testable without an LLM. Tests use real `git`
repositories created in temp directories and static fixtures under
`tests/fixtures/`; the pure decision crates are tested hermetically by
constructing `Evidence` and `ClaimAssessment` values directly.

At the final M8 field-validation run: **424 tests passing**, `cargo clippy
--workspace --all-targets --all-features -- -D warnings` clean, and
`cargo audit` reported **0 vulnerabilities across 230 dependencies**. These
numbers describe that run, not a permanent promise.

## Strategy

| Strategy | Where | What it covers |
| --- | --- | --- |
| Unit tests | inside every module | hashing, redaction, matching, severity, allow-list, tool validation |
| Golden fixtures | `tests/golden.rs` (adjudicate, context), `policy_matrix.rs` | the documented decision matrices as permanent regression tests (e.g. the Atlas/CodeBro mismatch case and its positive counterpart) |
| Adversarial fixtures | `tests/fixtures/` + `security.rs` / `adversarial.rs` | lying agents, narrow test suites, prompt injection, fake credentials, shell metacharacters, malicious attack proposals |
| Deterministic tests | `invariants.rs`, evidence reproducibility | byte-identical serialization, order-independence, stable hashes, unreachability of `ADVERSARIALLY_VERIFIED` in M2 |
| Security tests | `hardening.rs`, `redaction.rs`, `security.rs` | no shell, no environment inheritance, enforcement truthfulness, secrets never persisted, injection inertness |
| MCP protocol tests | `crates/griller-mcp/tests/stdio_integration.rs` | launches the **real binary** over stdio: handshake, exact six-tool surface, structured errors, clean shutdown |
| Pipeline field tests | `crates/griller-mcp/tests/completion_flow.rs` | the full M4→M2→M5→M2→M3→M7 pipeline against fixture repositories copied to temporary git repos |
| OpenCode field validation | outside the test suite | real OpenCode discovery + `griller.check_completion` workflow (documented in [opencode.md](opencode.md)) |

## Fixtures

| fixture | purpose |
| --- | --- |
| `basic_rust/` | minimal Rust project (non-git): context, manifest, vocab, auto-collect |
| `multi_manifest/` | Cargo.toml + package.json + go.mod + README |
| `codebro/` | the wrong-project fixture (the original incident) |
| `atlas/` | the positive counterpart: an Atlas-shaped repository |
| `greet_rust/` | valid Rust project for PASS scenarios (file exists, tests pass) |
| `lying_rust/` | agent claims tests pass; the suite fails |
| `narrow_rust/` | narrow test invocation hides a failing test; full replay catches it |
| `injection_rust/` | prompt-injection content; must be inert and never leak |
| `generic_app/`, `meridian/` | additional context-detection fixtures |
| `prompt_injection.txt` | injection text treated strictly as data |
| `shell_meta/` | filename with spaces and shell metacharacters |
| `fake_secrets.txt` | fake credentials for redaction tests |

Repository-shaped fixtures are copied to a temp dir and `git init`-ed at test
time. Test setup uses `std::process::Command` directly; the code under test
always goes through the execution backend.

## Field-validation scenarios (documented in [verification-pipeline.md](verification-pipeline.md))

| Scenario | Result |
| --- | --- |
| Wrong project | BLOCK before any attack work |
| Generic task | WARN — never a false BLOCK |
| Lying completion | BLOCK via replay refutation |
| Prompt injection | inert; marker never leaks |
| Unsafe command payload | never executed |
| Valid completion | PASS with direct evidence |
| Self-grill | ran, not refuted |
| Real OpenCode | discovery + workflow working |
| Worktree pollution | bug found during field validation, fixed |

## Determinism guarantees under test

Given: same repository state + same command + same relevant environment +
same collector, evidence must be equivalent. Concretely:

- `content_hash` and `id` are stable across separate collections.
- `captured_at` and `duration_ms` do not change the hash.
- Working directory and repository root location do not change the hash.
- Environment fingerprint **does** change the hash (intended: environment is
  a precondition).
- Serialization is canonical and round-trips byte-identically.
- Same claim + same evidence set (any order) ⇒ byte-identical assessment.
- `adversarially_verified` is unreachable from the deterministic M2 path.

## Key invariants (property tests)

1. No evidence → never `verified`.
2. Agent narration alone → never `verified`.
3. Irrelevant evidence cannot support a claim.
4. Command failure alone → never `false`.
5. LLM output cannot directly assign epistemic status.
6. Adding irrelevant evidence cannot flip `verified` → `contradicted`.
7. Adding direct contradictory evidence must break `verified`.
8. Same inputs → same assessment (evidence-order independent).
9. `adversarially_verified` is unreachable in M2.
10. Assessments never carry a verdict or policy field.

## Running

```sh
cargo fmt --all -- --check
cargo check --workspace
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo audit
```