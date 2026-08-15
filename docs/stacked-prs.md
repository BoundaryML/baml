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

## Create and submit a stack

Start from an up-to-date `canary` and initialize the bottom layer:

```bash
git switch canary
git pull --ff-only origin canary
gh stack init --base canary sam/parser-cleanup-01
# Edit, test, and commit the first change normally.
```

Add one branch per reviewable dependent change:

```bash
gh stack add sam/parser-cleanup-02
# Edit, test, and commit the second change normally.

gh stack add sam/parser-cleanup-03
# Edit, test, and commit the third change normally.
```

Inspect the local topology before publishing anything:

```bash
gh stack view
gh stack view --json
```

Open the submission editor, verify the bottom-to-top order and pull request contents, set new layers to draft as appropriate, and submit:

```bash
gh stack submit
```

`gh stack submit` pushes every branch, creates or updates the pull requests with the correct direct bases, and links them as a native stack. Reviewers see only each layer's focused diff and can navigate the stack on GitHub.

For non-interactive use, `gh stack submit --auto` derives titles from commits and creates new pull requests as drafts. Add `--open` only when every affected layer is ready for review.

## Update a stack

Normal commits on the top branch need no special handling. If review changes a lower branch, update that branch and cascade-rebase the layers above it:

```bash
gh stack checkout sam/parser-cleanup-01
# Edit, test, and commit or amend the lower change.
gh stack rebase --upstack
gh stack push
gh stack top
```

`gh stack push` updates rebased remote branches with `--force-with-lease --atomic`. If a rebase conflicts, resolve and stage the files, then run `gh stack rebase --continue`; use `gh stack rebase --abort` to restore the pre-rebase state.

When `canary` advances, refresh the full stack:

```bash
gh stack rebase
gh stack push
```

Use `gh stack modify` to insert, drop, fold, reorder, or rename layers. The worktree must be clean and no pull request in the stack may be queued while restructuring.

## Review a stack

Reviewers use GitHub normally. Start at the bottom pull request and use the native stack map to move upward. GitHub applies the `canary` branch protection rules, required checks, and applicable `pull_request` workflows to every layer even though each pull request directly targets the branch below it.

Authors can inspect stack and unresolved-thread status from the terminal:

```bash
gh stack view
mise run pr-unresolved
```

GitHub Actions exposes stack metadata as `github.event.pull_request.stack`. If duplicated heavyweight CI becomes expensive, use that metadata for a deliberate optimization rather than excluding child bases from validation.

## Land a stack safely

Before merging, confirm the stack is linear and every pull request through the intended stopping point is approved, open rather than draft, and passing its required checks.

Use the interactive merge command and select the highest layer you want to land:

```bash
gh stack merge --squash
```

GitHub merges the selected pull request and every unmerged layer below it as one all-or-nothing operation ordered from bottom to top. Because `canary` uses a merge queue, the complete contiguous group is enqueued in order. A failure prevents the group from partially merging, and an ejected pull request also removes every layer above it from the queue.

After the merge completes, synchronize and prune merged local branches:

```bash
gh stack sync --prune
```

`gh stack sync --prune` fetches and fast-forwards `canary`, rebases any remaining layers, pushes their updated branches, synchronizes pull request state, and removes local branches for merged layers.

If a layer is closed without merging, stop and use `gh stack modify` or the GitHub Unstack action to make the intended dependency change explicit. Do not leave a silently retargeted chain in review.

## Using `jj` or existing branches

This checkout is a regular Git worktree. In a colocated `jj` repository, keep branch and commit management in `jj` and use the stateless link command to create or adopt the GitHub stack:

```bash
gh stack link --base canary sam/parser-cleanup-01 sam/parser-cleanup-02 sam/parser-cleanup-03
```

Arguments are ordered bottom to top. Branch arguments are pushed automatically and pull requests are created when missing, so inspect the bookmarks and commits before running the command. Existing pull requests can be linked without recreating them by passing their numbers or URLs.

## Options considered

| Option | Fit for BAML | Decision |
| --- | --- | --- |
| GitHub `gh stack` | First-party stack objects and review UI; no enablement; existing protections and Actions apply to every layer; native merge-queue and atomic stack merge support; external-tool link mode supports `jj`. Public preview and same-repository linear stacks only. | Use for the pilot. |
| `git-spice` | Mature local branch manipulation and multi-forge support, but needs separate stack metadata and a manual or custom integration with BAML's squash-only GitHub merge queue. | Remove from the repo-local experiment now that GitHub provides the required native path. |
| Graphite | Mature author and reviewer experience, but requires accounts, a GitHub App, and external configuration. | Unnecessary for the smallest first-party pilot. |
| Aviator Stacked PRs | Stack-aware CLI and hosted merge queue, but introduces another service and queue. | Unnecessary while GitHub's native queue supports stacks. |
| Plain Git plus `gh pr` | No added extension, but lacks native local topology operations and makes safe cascade rebases manual. | Keep only as a fallback. |

## Pilot success criteria and next steps

Try one non-urgent two- or three-layer maintainer stack. Record whether the public-preview CLI remains predictable, reviews stay focused after cascade rebases, every expected CI workflow runs per layer, and the existing merge queue lands the selected contiguous group atomically.

If CI duplication is material, inspect the new stack event metadata and add a narrowly scoped optimization with contract tests. If the CLI or API changes during public preview, update this guide and the pilot before broad rollout.
