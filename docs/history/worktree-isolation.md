> **Historical milestone report (M0/M1).** See [docs/security.md](../security.md) and [docs/architecture.md](../architecture.md) for the current canonical treatment of isolation boundaries.

# Worktree isolation

## Goal

The Griller must never mutate the user's working tree during verification. All
verification commands run in a disposable copy of the repository's committed
state.

## Flow

1. `Worktree::discover(path)` resolves a path to a repository:
   - canonical root via `git rev-parse --show-toplevel`
   - `HEAD` via `git rev-parse HEAD`
   - non-git directories are reported as `is_git = false` (collectors degrade
     gracefully rather than failing)
2. `Worktree::create(path)`:
   - if the repository has a `HEAD`: `git worktree add --detach <tmp>/worktree`
     (a detached checkout; the main repo's branches are never switched)
   - if the repository has no commits yet: falls back to `git clone` into a
     temp directory (isolation without a commit to check out)
3. Commands run inside the worktree path.
4. Cleanup:
   - `git worktree remove --force <path>` unregisters and deletes the worktree
   - the temporary directory is removed
   - cleanup runs on `Drop` and explicitly via `Worktree::remove` (idempotent)

## What is isolated

- The main working tree is never touched: mutations inside the worktree do not
  appear in the user's checkout (tested).
- Worktree registrations are removed, so `git worktree list` on the main repo
  shows only the main worktree after cleanup (tested).
- Cleanup is verified even when a command inside the worktree fails (tested).

## What is NOT isolated (M1)

- The worktree contains only committed state; uncommitted changes in the main
  tree are not carried over. Patching/apply of dirty state is M2+.
- Network isolation inside the worktree is limited (see
  `command-and-execution.md`). A worktree adds filesystem isolation, not
  network isolation.
- Command execution is allowed in the non-git fallback path (a plain directory
  has no isolation boundary); the CLI refuses nothing there, so this is only as
  safe as the machine it runs on.

## Base state vs working state

M1 verifies from **committed state**. It is critical to be explicit about the
distinction a future completion verifier must reason about:

| state | meaning |
| --- | --- |
| **BASE STATE** | `HEAD` / committed repository |
| **WORKING STATE** | staged changes + unstaged changes + untracked files |

- "Does HEAD work?" is a question about **base state** — this is what an
  isolated worktree executes today.
- "Does the agent's current proposed change work?" is a question about
  **working state** — this requires carrying the working changes into
  isolation, which M1 does not do.

To make this concrete without building a patch system, M1 ships a **read-only
repository snapshot** (`griller-core::snapshot::RepositorySnapshot`, collected
by `SnapshotCollector`, emitted as `repository_snapshot` evidence). It records:

- `head` (base state)
- `staged_changes`
- `unstaged_changes`
- `untracked_files`
- `dirty`

The snapshot collector runs `git status --porcelain` only; it never stages,
writes, or patches anything (tested). The full M2 completion snapshot (applying
a proposed change into an isolated worktree) is not implemented.

## Test coverage

`crates/griller-runtime/tests/worktree.rs` covers: discovery, isolation from
the main tree, cleanup on drop, cleanup after a failed command, commit-less
fallback, `create` on non-git error, and idempotent explicit removal.
