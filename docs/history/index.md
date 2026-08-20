# Milestone history

The Griller was built in milestones M0–M8. This directory preserves the
original implementation reports — the historical record of how decisions
were made, not the current truth. **The canonical documentation lives in
[the README](../../README.md)** — start with the README documentation
table; use these reports for the journey.

## Timeline

| Milestone | What it delivered | Report(s) |
| --- | --- | --- |
| M0 | Foundation: pure core (`griller-core`), deterministic evidence collection, shell-free execution backend | [command-and-execution.md](command-and-execution.md), [worktree-isolation.md](worktree-isolation.md), canonical: [docs/evidence.md](../evidence.md) |
| M1 | Hardening: secret redaction, enforcement truthfulness, controlled environments, worktree isolation guarantees | [command-and-execution.md](command-and-execution.md), [worktree-isolation.md](worktree-isolation.md), canonical: [docs/security.md](../security.md) |
| M2 | Claim adjudication: claim model, normalization, evidence matching, contradiction checking, epistemic status | [m2-claims.md](m2-claims.md), [m2-evidence-matching.md](m2-evidence-matching.md), [m2-status.md](m2-status.md) |
| M3 | Operational policy: severity, the auditable policy matrix, PASS/WARN/BLOCK verdicts, Diva renderer | [m3-policy.md](m3-policy.md) |
| M4 | Context adjudication: project/task identity, wrong-project detection | [m4-context.md](m4-context.md) |
| M5 | Red team: safe adversarial attacks, replay, allow-list validation | [m5-red-team.md](m5-red-team.md) (includes the M5.1 hardening of attack validation) |
| M6 | MCP server: stdio adapter exposing the Griller tools | covered by [m8-integration.md](m8-integration.md); canonical: [docs/mcp.md](../mcp.md) |
| M7 | Self-grill: the 16-check decision audit | [m7-self-grill.md](m7-self-grill.md) |
| M8 | Final integration: `griller.check_completion`, the six-tool surface, real MCP + OpenCode field validation | [m8-integration.md](m8-integration.md) |

## Status

M0–M8 are complete and were validated in the field (real MCP stdio, real
OpenCode discovery and workflow, 424 tests passing at the final
field-validation run). No M9 exists; nothing beyond the integration was
added to reach M8.

## Reading guide

- **New engineer:** start at [docs/getting-started.md](../getting-started.md)
  and [docs/concepts.md](../concepts.md). Do not start here.
- **Contributor:** [docs/development.md](../development.md), then the
  milestone report for the area you are changing.
- **Reviewer:** [docs/security.md](../security.md),
  [docs/limitations.md](../limitations.md), then the M8 report for what was
  field-validated.