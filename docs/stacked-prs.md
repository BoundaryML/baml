# Stacked pull requests

Status: maintainer pilot using GitHub's first-party stacked pull requests, which are in public preview. The workflow uses the official [`gh stack`](https://docs.github.com/en/pull-requests/get-started/stacked-prs-quickstart) extension and requires no repository or organization setting change.

## Why this workflow

BAML's default branch is `canary`. The repository allows squash merges only, automatically deletes merged branches, uses GitHub's merge queue, and runs merge-group CI. GitHub stacks integrate with those controls directly: every pull request in a stack is evaluated against the stack base, existing branch protection and Actions workflows apply to every layer, and the merge queue can enqueue a contiguous stack in the correct order as one atomic operation.

The official extension creates and tracks local branch stacks, opens and links pull requests as native GitHub stacks, cascade-rebases dependent branches, safely pushes rebased branches with `--force-with-lease --atomic`, and synchronizes local state after merges. It also offers `gh stack link` for branches managed by `jj`, Sapling, git-town, or another external tool.

## Repository observations

Snapshot from 2026-08-14:

- The 15 open pull requests inspected all targeted `canary`; the repository had no existing linked stacks or stack-specific tooling.
- Recent branch names commonly use `<github-login>/<topic>`, with additional `agent/`, `codex/`, `fix/`, `chore/`, and Dependabot prefixes. There is no documented enforced pattern.
- GitHub permits squash merge only, automatically deletes merged branches, allows branch updates, and has repo-level auto-merge disabled. `canary` has linear-history protection and lands through GitHub's merge queue.
- CI already handles `merge_group`. GitHub evaluates every stack layer against `canary`, so workflows whose `pull_request` trigger filters to `canary` still run for every pull request in a stack.
- The existing pull request template asks for issue context, changes, testing, screenshots, and a checklist. Native stack navigation supplements that context rather than replacing it.

## Constraints

- Stacks are public preview and their CLI or API may change.
- Every pull request must be in `BoundaryML/baml`. Stacks cannot include forks.
- A stack is one linear chain. It cannot represent branching or merging dependency graphs.
- Auto-merge is not supported for stacked pull requests. Use `gh stack merge` or the GitHub stack merge UI, which integrates with the repository's merge queue.
- Name branches consistently with existing repository practice, usually `<github-login>/<topic>`. Add a numeric suffix when it makes the order clearer, for example `sam/parser-cleanup-01` and `sam/parser-cleanup-02`.
- Keep each branch independently reviewable and create layers as drafts until their shape is stable.

## One-time setup

The extension requires GitHub CLI 2.90.0 or later and Git 2.20 or later:

```bash
gh --version
git --version
gh auth status
gh extension install github/gh-stack
```

Upgrade an existing installation with:

```bash
gh extension upgrade gh-stack
```

The extension stores local topology in Git repository metadata. Authentication remains managed by GitHub CLI and no credential belongs in this repository.
