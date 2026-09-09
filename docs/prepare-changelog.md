Prepare a new changelog blog post in `typescript2/app-website/blog-releases/`.

Use explicit lower and upper tags or commit IDs when the human supplies them. Otherwise, discover the latest released version from [pkg.boundaryml.com](https://pkg.boundaryml.com), `release.json` / `baml-language.cfg`, and the `baml-language-a.b.c` tags; use the current `origin/canary` as the upper bound only when none was supplied. Record the assumed release date and timezone.

In a fresh checkout of BoundaryML/baml, fetch tags and resolve both bounds to immutable commit IDs before scanning. Set `LOWER_REF` and `UPPER_REF` to those selected references. The lower bound is excluded; the upper bound is included.

```sh
git fetch origin --tags
LOWER_SHA=$(git rev-parse --verify "${LOWER_REF}^{commit}")
UPPER_SHA=$(git rev-parse --verify "${UPPER_REF}^{commit}")
git log --reverse --format='%H %s' "${LOWER_SHA}..${UPPER_SHA}"
# This is a supplemental view, not the complete release inventory:
git log --reverse --format='%H %s' "${LOWER_SHA}..${UPPER_SHA}" -- baml_language/
```

Keep the full-range inventory and record why each excluded PR was omitted. Do not keep moving the upper bound while preparing the draft. For a backtest, preserve the historical post and give the draft a unique slug. All unpublished website drafts must set `isPublished: false`.

Then prepare the changelog draft to target the human's goals:

1. Exclude PRs with no user-visible effect.
   - Users do not need to know about PRs that only refactor BAML internals or change internal workflows.
   - Inspect the shipped effect before excluding a workflow or packaging change. A build-only diff can fix an installed SDK, installer, or other shipped artifact even when it does not touch `baml_language/`.
   - Exclude BAML v0-only changes under `engine/` or `typescript/`. Do not exclude a mixed PR without checking its v1 effect. Review v1 SDKs, installers, editor integrations, and user-facing documentation outside the language directory too.
   - Write the list of PRs with user-visible effect to [`step1b-prs-only-user-visible.md`](step1b-prs-only-user-visible.md).
2. Prepare a list of user-facing changes present at the upper release boundary.
   - For each retained PR, explain what changed from the user's point of view. Write short sentences that each make one point. If a sentence needs several commas or a parenthetical, split it up.
   - Inspect merged diffs when the title or description is insufficient, especially for broad PRs with several public surfaces.
   - Check every proposed entry against the net result at `UPPER_SHA`. Remove changes that were reverted, superseded, or removed before that boundary. Verify final API names, return types, configuration keys, and generated imports rather than copying an intermediate PR description.
   - Write the per-PR effects to [`step2a-pr-user-effects.md`](step2a-pr-user-effects.md).
   - Aggregate related effects in [`step2b-pr-user-effects-reaggregated.md`](step2b-pr-user-effects-reaggregated.md). Avoid grouping more than three PRs into one entry.
3. Classify each effect exactly once as HEADLINE_CHANGE, FEATURE, BREAKING_CHANGE, or BUGFIX.
   - Headline changes are rare. Use judgment when more than one category seems applicable.
   - Performance improvements are FEATURE, but a refactor alone is not evidence of improved performance.
4. Review the generated changelog.
   - Every HEADLINE_CHANGE, FEATURE, or BREAKING_CHANGE involving syntax or a library API must include a code block demonstrating the final API.
   - Every BREAKING_CHANGE must include migration instructions, including changes to behavior, paths, CLI commands, configuration, and generated artifacts. For code migrations, show before and after. State required user action even when an effect is categorized as BUGFIX.
   - For a performance-only FEATURE, include the PR's data analysis instead of a code example. Record the measured revision, workload, build profile, baseline, and measurement scope (for example parser-only, static instruction counts, or end-to-end latency). Verify that the measured implementation remains in the release. Do not use superseded intermediate measurements or neutral/noisy benchmarks as proof of a release-level improvement; qualify results when only a narrower workload was measured.
   - Check examples with the target release compiler, and compare migration examples with the lower release when possible. Distinguish compile-only validation from execution. A passing check does not establish that a runtime crash or provider failure is fixed.

The final format of the changelog blog post should look like this:

`[blog post format]`

Thanks to everyone who contributed!

# Headline change 1

This is a big deal for our users, so we want to call it out specially. Maybe this is the introduction of `baml query`, maybe something else.

# Headline change 2

This is also a big deal for our users, so we want to call it out specially.

# Features

Users previously had no ability to do this.

## Feature 1

BAML now allows $abc, here are $details.

```
...baml code snippet demonstrating the feature...

```

## Feature 2

BAML now allows $abc, here are $details.

```
...baml code snippet demonstrating the feature...

```

# Breaking changes

Users will need to update their code in response to this change.

## Breaking change 1

BAML now requires $new-pattern and bans $old-pattern.

All code that looks like this:

```
...

```

Must be updated to look like this:

```
...

```

## Breaking change 2

BAML no longer supports $abc, users must now $def.

Update every usage of

```
example usage of $abc

```

To look like this

```
example usage of $def

```

# Bug fixes

- Fixed: $abc now $correct-behavior
- Fixed: $def no longer $wrong-behavior
- etc

`[/blog post format]`

In parallel, we should also collect every BAML v1 PR (i.e. all PRs in the specified revision range, excluding only BAML v0 PRs) and then identify every external user that we should notify when we publish this changelog:

- Secrets are stored in infisical under `projectId=bdd280e2-259c-4750-9b16-a8597a67214c` and `environment=dev-changelog-ops`
- We need to check every PR, every referenced Linear issue, GitHub issue, and Discord thread.
  - Use local `gh auth login`, `LINEAR_API_KEY`, and `DISCORD_OPSBOT_BOT_TOKEN` to fetch all issues.
  - We want to notify every user who contributed to a PR, every user who filed an issue, every user who reported an issue on Discord.
- The followup actions we want to do, when the changelog goes out, are to post on:
  - every GitHub PR opened by an external user which was included in this release
  - every GitHub issue opened by an external user which was fixed (partial or complete) in this release
  - every Discord thread started by an external user which was addressed by this release
- Prepare a [`step3-followup-actions.md`](step3-followup-actions.md) that includes everything we need to do.

`[step3-followup-actions]`

# GitHub PR: link

> Quote from the external user

```
(The response we're going to post, specifically referencing what is going out in the release which addresses this and also links to https://boundaryml.com/changelog.)

```

# GitHub issue: link

> Quote from the external user

```
(The response we're going to post, specifically referencing what is going out in the release which addresses this and also links to https://boundaryml.com/changelog.)

```

# Discord thread: link

> Quote from the external user

```
(The response we're going to post, specifically referencing what is going out in the release which addresses this and also links to https://boundaryml.com/changelog.)

```

`[/step3-followup-actions]`