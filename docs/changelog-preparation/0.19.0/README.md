# BAML 0.19.0 changelog preparation

Prepared using the procedure in [docs/prepare-changelog.md from PR #4784](https://github.com/BoundaryML/baml/blob/15d6400d2e/docs/prepare-changelog.md), including the full-range and final-source checks established by the 0.18.0 backtest.

Assumed cut: September 8, 2026, America/Los_Angeles. Base: `baml-language-0.18.0` (`7622555396a99db466afaea09dea2cad259d4033`). Candidate: canary `5b398f2b60cfa78258ac9534e8322d056564e85f`. The candidate timestamp falls on September 9 UTC; “today” here is the user’s Pacific date. Later canary changes are not included.

The [release draft](../../../typescript2/app-website/blog-releases/2026-09-08-baml-0.19.0.md) is unpublished. This preparation does not bump versions, tag a release, or send notifications. Before publication, compare the actual release SHA with this pinned cut, reconcile any extra changes, update the publication date if needed, and enable the post only after the release is available.

- [Full inventory](step1a-all-prs.md): all 74 PRs, including inclusion/exclusion decisions.
- [User-visible PRs](step1b-prs-only-user-visible.md): 25 retained PRs.
- [Effects by PR](step2a-pr-user-effects.md) and [aggregated effects](step2b-pr-user-effects-reaggregated.md): 34 entries, each classified once.
- [Followup drafts](step3-followup-actions.md): three replies for two external users, plus exclusions and an unresolved Discord-thread reference.
- [Source coverage](source-coverage.md): 65 non-v0 PR discussions, referenced issues, and source-thread coverage.
- [Validation](validation.md): candidate compiler checks, 0.18.0 comparisons, execution probes, and remaining release-time checks.

The wrapper fix is independently delivered: verify the current installer and compatible wrapper before posting that issue response. The float PR mentions a Discord proposal without a link; the PR reply is prepared, but a separate Discord draft needs a verified destination.
