# BAML 0.18.0 changelog backtest

The procedure produces useful release notes, but needs two additional safeguards: inspect release-relevant changes outside `baml_language/`, and verify every advertised API at the final tag. This backtest found one workflow-only runtime fix that the prescribed filter misses, one advertised reflection feature that was removed before release, and substantial missing migration guidance.

The [unpublished draft](../../../typescript2/app-website/blog-releases/2026-08-27-baml-0.18.0-backtest.md) is separate from the [historical post](../../../typescript2/app-website/blog-releases/2026-08-27-baml-0.18.0.md). It has a unique slug and `isPublished: false`; the website loader excludes such posts. No release was published and no followup was sent.

## Scope and artifacts

This applies [the provided procedure](../../prepare-changelog.md) to `baml-language-0.17.0..baml-language-0.18.0`, with the lower tag excluded and upper tag included. The lower commit is `36545fde3913aa3699a27aed11365541c8123821`; the upper commit is `7622555396a99db466afaea09dea2cad259d4033`. Current canary was not substituted for the upper bound. Tags were fetched before inventorying.

| Measure | Result |
| --- | ---: |
| Commits / PRs in the full range | 114 |
| PRs in the prescribed `baml_language/` log | 89 |
| V0-only PRs excluded from followups | 4 |
| Non-v0 PRs screened for followups | 110 |
| PRs with retained user-facing effects | 64 |
| Aggregated effects | 69 |
| Classification | 1 headline, 18 features, 12 breaking changes, 38 bug fixes |
| External notification drafts | 17 destinations: 9 PRs, 8 issues |

The 64-PR draft includes #4502 as an explicit correction to the literal path/workflow filters. Following those filters exactly would leave 63 retained PRs. Non-release website copy and internal product metrics were screened for followups but excluded from the language changelog.

- [Complete inventory and exclusion reasons](step1a-all-prs.md)
- [Step 1b: user-visible PRs](step1b-prs-only-user-visible.md)
- [Step 2a: effects for each PR](step2a-pr-user-effects.md)
- [Step 2b: aggregated effects and classifications](step2b-pr-user-effects-reaggregated.md)
- [Step 3: followup drafts](step3-followup-actions.md)
- [Additional source coverage](source-coverage.md)
- [Validation evidence and limits](validation.md)

## Findings against the historical changelog

### The path and workflow filters lose a shipped fix

[#4502](https://github.com/BoundaryML/baml/pull/4502) fixes the v1 Node bridge on Alpine for x86_64 and aarch64. Its only changed file is `.github/workflows/build2-nodejs-sdk.reusable.yaml`. The prescribed path-limited log misses it, and the blanket exclusion of workflow-only PRs also removes it. The workflow builds `baml_language/sdks/typescript/bridge_typescript`; this is not a v0 change. The PR records a four-cell Alpine validation with unfixed controls and fixed builds.

Recommendation: inventory the entire range first. Classify files by the product they build or ship. Exclude workflow changes only when they have no end-user artifact or behavior effect.

### A merged feature was removed before the release

[#4519](https://github.com/BoundaryML/baml/pull/4519) added generic function descriptors with `is_generic`, `generic_params`, `specialize`, and `get`. [#4560](https://github.com/BoundaryML/baml/pull/4560) removed those methods and the descriptor surface before the upper tag. The historical post still cites #4519 under expanded reflection.

The released 0.18.0 CLI reports E0007 for `descriptor.specialize(...)`. The [final-tag function reflection source](https://github.com/BoundaryML/baml/blob/baml-language-0.18.0/baml_language/crates/baml_builtins2/baml_std/reflect/ns_function/function.baml) exposes `as_type`, `params`, and `return_type`, without specialization. The backtest excludes #4519. It also excludes the simpler add/revert pair [#4464](https://github.com/BoundaryML/baml/pull/4464) and [#4469](https://github.com/BoundaryML/baml/pull/4469).

Recommendation: compute net effects at the upper tag. Inspect later edits to each advertised surface, even when the later PR is labeled as an internal refactor. Validate examples with the released artifact when available.

### Commit subjects and PR descriptions are insufficient

The current description of [#4493](https://github.com/BoundaryML/baml/pull/4493) describes optional-chain generic lowering, but its merged diff changes AnyClass rulings, field handles, and diagnostics. Optional-call lowering belongs to [#4495](https://github.com/BoundaryML/baml/pull/4495). The draft uses the merged diff and final source.

Other broad PRs contain several effects that the historical post omits:

| PR | Missing or underspecified effect |
| --- | --- |
| [#4135](https://github.com/BoundaryML/baml/pull/4135) | `bool.random()` is newly introduced. Integer left-shift overflow now truncates. |
| [#4541](https://github.com/BoundaryML/baml/pull/4541) | Tests gain a five-minute deadline. Shutdown gains a 15-second grace period. Both have environment overrides. There are also compiler, lambda-inference, diagnostic-rendering, and cleanup fixes beyond formatting. |
| [#4570](https://github.com/BoundaryML/baml/pull/4570) | Remove the output type parameter from `ai.Agent`; restore streamed usage events. |
| [#4604](https://github.com/BoundaryML/baml/pull/4604) | `TurnStream.next()` changes from a string to an array of strings. `Runner` changes its method contract. Prompt interpolation may throw. Host media and renderer parity are fixed. |
| [#4606](https://github.com/BoundaryML/baml/pull/4606) | Existing file, socket, and process-pipe APIs change. EOF becomes null on the shared read contract. TCP per-call timeout parameters are replaced with a timeout wrapper. |
| [#4601](https://github.com/BoundaryML/baml/pull/4601) | Explicit type arguments must be removed from `to_string` and `to_json`. The historical `to_string<unknown>()` replacement advice is already stale. |

Recommendation: enumerate effects per PR before aggregation. Treat descriptions as leads and merged code as the authority, especially for broad or stacked PRs.

### Ten additional PRs have user-facing effects

The historical post cites 56 distinct PRs. The backtest retains 54 of those, excludes #4461 and #4519, and adds these ten:

| PR | User-facing effect |
| --- | --- |
| [#4460](https://github.com/BoundaryML/baml/pull/4460) | Diagnose unsupported indirect runtime checks and preserve useful mounted-package diagnostics. |
| [#4502](https://github.com/BoundaryML/baml/pull/4502) | Correct musl Node bridge artifacts. |
| [#4529](https://github.com/BoundaryML/baml/pull/4529) | Correct method dispatch on top-level bindings and preserve Session-call errors. |
| [#4531](https://github.com/BoundaryML/baml/pull/4531) | Type-check Session assignments. |
| [#4568](https://github.com/BoundaryML/baml/pull/4568) | Reject incompatible/corrupt compiler artifacts before decoding. |
| [#4571](https://github.com/BoundaryML/baml/pull/4571) | Prevent Session helper-name collisions. |
| [#4581](https://github.com/BoundaryML/baml/pull/4581) | Tie editor results to current buffers and project revisions. |
| [#4593](https://github.com/BoundaryML/baml/pull/4593) | Reject imprecise `throws unknown` declarations. |
| [#4619](https://github.com/BoundaryML/baml/pull/4619) | Reject missing required fields in class construction. |
| [#4621](https://github.com/BoundaryML/baml/pull/4621) | Reject unspecialized generic function values without a concrete contextual signature. |

These are editorial findings, not recall/precision scores against an assumed perfect historical changelog. The old post was consulted during the backtest; this was not a blinded evaluation.

### Performance claims need their own evidence

[#4461](https://github.com/BoundaryML/baml/pull/4461) explicitly says its end-to-end result is neutral/noisy. Its reported medians are slightly slower. It should not be cited as proof that compilation became faster.

The draft classifies supported performance improvements as FEATURE, as requested. It includes the PR measurements for [#4453](https://github.com/BoundaryML/baml/pull/4453), [#4458](https://github.com/BoundaryML/baml/pull/4458), [#4463](https://github.com/BoundaryML/baml/pull/4463), and [#4604](https://github.com/BoundaryML/baml/pull/4604). These are author-reported A/B results from intermediate commits. They are not newly measured 0.17.0-versus-0.18.0 benchmarks, and their percentage changes are not additive.

### The historical format does not meet the procedure

The historical post has no fenced code examples. It groups five reflection PRs into one feature and four PRs into each of two fix entries. It puts compilation and stream-parsing speedups under fixes. The new draft splits effects into groups of at most three PRs, gives each effect one classification, includes syntax/library examples, and supplies before/after migration guidance. Closely related features and breaking effects from a single PR are separate entries where the user action differs.

### Followups require a broader inventory and partial-fix language

The nine external-authored PR destinations include Ruby groundwork and CI/test-only contributions. Those would be lost if notification planning reused only step 1b. GitHub association metadata identifies seven candidate external handles, with both Ruby accounts preserved rather than presumed identical.

[#4429](https://github.com/BoundaryML/baml/issues/4429) deserves a partial update: CLI logging is verified, while packed-binary logging and `--log-file` are not established as fixed. [#4588](https://github.com/BoundaryML/baml/issues/4588) has an author-confirmed nightly fix and a passing 0.18.0 dead-endpoint reproduction; its separate stderr-content policy question remains open. [#4506](https://github.com/BoundaryML/baml/issues/4506) still reproduces on 0.18.0 and must not receive a “fixed” notification just because the related iterator PR merged.

Recommendation: keep release-note inclusion and contributor notification eligibility separate. Resolve linked issue and source-thread context before drafting a response. Preserve partial scope and deduplicate destinations.

## Suggested procedure changes

1. Accept explicit lower and upper revision bounds. Resolve both to commit IDs before scanning. Use current release discovery only when bounds are absent.
2. Inventory every commit in the range before filtering. Include packaging/workflow fixes that affect shipped artifacts. Keep this complete inventory for followups.
3. Record every exclusion with a reason. Resolve reverted and superseded changes against the upper tag, not merge titles alone.
4. Inspect merged diffs for each public surface and all broad PRs. Verify example API names, return types, configuration keys, and imports against the final tag.
5. Include migration guidance for removed arguments, changed return types, EOF behavior, deadlines, and renamed output paths. A bug-fix classification does not eliminate the need to state required user action.
6. Require benchmark evidence before calling a refactor a performance improvement. Label the measured revision, workload, build profile, and baseline.
7. Validate code examples with the target release. Keep compile-only checks distinct from execution: #4506 passes `baml check` but still fails when run.
8. Keep notification drafts local until publishing is authorized. Track source coverage, account-attribution rules, unavailable sources, and partial fixes explicitly.
9. Preserve the original post during a backtest. Mark any website draft with `isPublished: false` and a unique slug.
