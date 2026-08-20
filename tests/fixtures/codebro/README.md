# CodeBro

CodeBro is the deterministic engineering context and memory layer. It records
verified engineering facts about repositories: symbols, modules, packages,
tests, build targets and dependencies. Every fact is stored with provenance
and a confidence score.

CodeBro also maintains persistent engineering memory recorded by agents. MCP
tools register handlers that expose the fact store to agent sessions. Claims
about the codebase are grilled against deterministic evidence before any
verdict is emitted.

## Modules

- `griller-core` — pure value types and deterministic hashing.
- `griller-mcp` — the MCP tool registration boundary.
