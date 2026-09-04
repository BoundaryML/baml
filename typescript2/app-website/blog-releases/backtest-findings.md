# Backtest findings for the `0.18.0` changelog process

## Result

The process produced 67 PRs with user-visible effects from 89 commits touching `baml_language/`. The historical `0.18.0` changelog references 56 of those PRs. The backtest therefore found 11 additional PRs that satisfy the proposed inclusion rule. The other 22 PRs are excluded explicitly in `step1-prs-only-user-visible.md`.

The historical changelog is a strong semantic oracle for the 56 PRs it includes. Its grouping is generally good, but its cutoff and completeness are not mechanically auditable from the rendered notes.

## Backtest inputs

- Base tag: `baml-language-0.17.0` at `36545fde3913aa3699a27aed11365541c8123821`.
- Head tag: `baml-language-0.18.0` at `7622555396a99db466afaea09dea2cad259d4033`.
- Historical changelog comparison head: `f3e1e25210b1ac551a4d1ffd0238db5d85824789`.
- Scoped history command: `git log --reverse --format='%h %s' baml-language-0.17.0..baml-language-0.18.0 -- baml_language/`.

## Additional user-visible PRs found by the backtest

- [#4460](https://github.com/BoundaryML/baml/pull/4460) rejects unsupported runtime-dependent generic checks at compile time instead of omitting checks or panicking.
- [#4529](https://github.com/BoundaryML/baml/pull/4529) fixes method calls on global bindings and normalizes session `let` binding types.
- [#4531](https://github.com/BoundaryML/baml/pull/4531) checks session assignments against the original binding type.
- [#4560](https://github.com/BoundaryML/baml/pull/4560) changes observable runtime type identity behavior, including runtime-defined types crossing reflection and host boundaries.
- [#4568](https://github.com/BoundaryML/baml/pull/4568) rejects corrupt or incompatible generated artifacts with actionable regeneration errors.
- [#4571](https://github.com/BoundaryML/baml/pull/4571) keeps reflection sessions usable after rejected compilation artifacts and prevents invalid artifact reuse.
- [#4581](https://github.com/BoundaryML/baml/pull/4581) ships a new language server implementation with user-facing editor behavior, including standard-library navigation and testset code lenses.
- [#4603](https://github.com/BoundaryML/baml/pull/4603) prevents invalid compiler types from silently degrading into `unknown` and rejects invalid numeric compound assignments.
- [#4619](https://github.com/BoundaryML/baml/pull/4619) rejects class constructors that omit required fields.
- [#4621](https://github.com/BoundaryML/baml/pull/4621) rejects unspecialized generic functions stored as values and gives concrete migration guidance.
- [#4593](https://github.com/BoundaryML/baml/pull/4593) rejects `throws unknown` when the implementation does not actually expose an unknown error boundary.

The last three user-visible PRs landed after `f3e1e25210`, the source commit named in the historical `0.18.0` changelog comparison URL, but before the `baml-language-0.18.0` tag. That mismatch explains those three omissions. It does not explain the other eight.

## Process weaknesses exposed by the backtest

1. The release boundary is ambiguous. The instructions discover a canary tag and compare it with `origin/canary`, while the published release may use a source commit that differs from the eventual stable tag. The process should record `BASE_SHA`, `HEAD_SHA`, and the artifact source SHA in every draft.
2. Commit subjects are not sufficient for sparse titles such as `LSP (#4581)` or internally worded changes such as `Version compiler artifact envelopes (#4568)`. The process should require diff inspection for every commit whose subject does not state a user action or observable behavior.
3. A net-zero public change can appear as two apparently user-visible commits. The process correctly needs a range-level check so additions reverted before the release, such as #4464/#4469, are excluded.
4. The requested workflow does not name a file for the aggregated Step 2 output or the Step 3 classifications. This backtest uses `step2b-aggregated-user-effects.md` for both.
5. The blog template says “Thanks to everyone who contributed!” but does not define whether contributor names are required. The historical package changelog records authors, while this blog draft does not invent an attribution policy.
6. “User-visible” needs an explicit rule for developer-facing correctness changes. This backtest includes new diagnostics, prevention of compiler crashes, artifact compatibility errors, and IDE behavior because users can directly observe them.

## Recommended amendments to `RELEASING.md`

- Pin and print the exact comparison before analysis: `BASE_TAG`, `BASE_SHA`, `HEAD_REF`, `HEAD_SHA`, and the release artifact source SHA.
- Run `git log --reverse --format='%H%x09%s' "$BASE_SHA..$HEAD_SHA" -- baml_language/` so later inspection never depends on abbreviated hashes.
- Add a range-level verification pass: “For every included PR, confirm its user-visible effect remains present at `HEAD_SHA`.”
- Add an explicit sparse-message rule: “Inspect the PR diff when the subject does not name the affected command, API, diagnostic, performance characteristic, or behavior.”
- Define Step 2b and Step 3 output filenames.
- Require every aggregate entry to carry PR links and exactly one classification token.
- Add a reconciliation table against any package changelog generated elsewhere, so omissions and cutoff mismatches are visible before publishing.
