Prepare a new changelog blog post, in `typescript2/app-website/blog-releases/`

Figure out the latest canary release that went out:

- [pkg.boundaryml.com](http://pkg.boundaryml.com)
- release.json / baml-language.cfg
- baml-language-a.b.c git tags

In a fresh checkout of BoundaryML/baml, run the following commands:

```
git fetch origin --tags
git log --reverse --format='%h %s' ${LATEST_CANARY_RELEASE_TAG}..origin/canary -- baml_language/

```

Then prepare the changelog draft to target the human's goals:

1. Exclude PRs with no user-visible effect.
  - Users do not need to know about PRs that only refactor BAML internals.
  - Users do not need to know about PRs that only change GitHub workflows.
  - Users do not need to know about changes to BAML v0: changes to `engine/` or `typescript/` are irrelevant to BAML users.
  - Write the list of PRs with user-visible effect to [`step1b-prs-only-user-visible.md`](http://step1b-prs-only-user-visible.md)
2. Prepare a list of user-facing changes present in this release.
  - For each PR with user-visible effect, prepare a concise explanation, from the user's POV, of what changed.
    - Write short sentences that each make one point. If a sentence needs several commas or a parenthetical, split it up.
    - If a PR's commit message does not have enough detail about what changed, inspect the diff for the PR itself to understand.
    - Write the list of PRs &amp; each PR's what-changed-for-the-user list to [`step2a-pr-user-effects.md`](http://step2a-pr-user-effects.md).
  - Then, aggregate this into a list of what-changed-for-the-user entries, where each entry is a concise what-changed-for-the-user.
    - If individual what-changed-for-the-user entries span multiple PRs, group them together.
    - Avoid grouping more than three PRs at a time into a single entry.
    - Write this list to [`step2b-pr-user-effects-reaggregated.md`](http://step2b-pr-user-effects-reaggregated.md).
3. For each what-changed-for-the-user entry, identify whether it is best described as: HEADLINE_CHANGE or FEATURE or BREAKING_CHANGE or BUGFIX.
  - Most changes are **not** headline changes, this is very rare.
  - If a change can be described as two or more of FEATURE, BREAKING_CHANGE, BUGFIX, use your best judgment to choose exactly one of them to classify it as.
  - Performance improvements are FEATURE.
4. Review the generated changelog.
  - Every HEADLINE_CHANGE, FEATURE, or BREAKING_CHANGE involving a syntax or library change must have at least one code block demonstrating how the user can use it.
  - If a FEATURE is performance-only, instead just include the data analysis from the PR that proves the performance improvement.

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
- Prepare a [`step3-followup-actions.md`](http://step3-followup-actions.md) that includes everything we need to do.

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