# BAML 0.25.0 release audit

Prepared on 2026-11-05 Pacific time for the 2026-11-06 UTC release. All inventories, effects, examples, and follow-ups use these fixed boundaries.

- Lower: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Upper: 0.25.0 release source, `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`, selected by [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387). The tag was not yet created when these references were pinned. Verify its final target before publication.
- The range contains 14 squash-merged PRs across the whole repository. All are v1, including website, workflow, tests, and metadata PRs excluded from the public post.
- The 0.24.0 registry release did not promote the canary channel. #5551 explicitly replaces it with 0.25.0 instead of rerunning the old cut. The new post links the 0.24.0 migrations for users coming from the last promoted 0.23.0 canary.
- The existing changelog branch was rebased onto this upper source. The 0.24.0 notes and audits remain separate. This directory records the additional 0.25.0 inventory without overwriting the earlier audit.

## Complete inventory and dispositions

- `82946115c8dc5b3f4140c72fc024b62e462e28d5` [#5538](https://github.com/BoundaryML/baml/pull/5538) — Excluded: Tests and translator documentation only. Promise callback return support already shipped in #5526 before the lower boundary; inspected translate_ty diff to avoid a duplicate feature claim.
- `3fd14e9c7b0c1060a423c79eaa3baf4f504c52b9` [#5534](https://github.com/BoundaryML/baml/pull/5534) — Excluded: Homepage/blog presentation only, independent of the language toolchain.
- `ec06ef62cccf0621bd51f548cd35a8639d574349` [#5543](https://github.com/BoundaryML/baml/pull/5543) — Retained: Read BTELCAS v6 redacted values in CLI queries and the playground. State/kind report redacted, rendered data uses a $redacted envelope, and equality depending on withheld data is unknown. Writers remain v5; no new application redaction declaration is shipped.
- `1d9f45cc438aae240f39ff0f9b6926668ed5298f` [#5532](https://github.com/BoundaryML/baml/pull/5532) — Retained: Configure the Python SDK package and output directory with a dotted package_name. Self-relative imports and type maps follow relocation. Defaults keep their public location; internal generated bookkeeping can change. Renaming requires updating imports/packaging and removing stale generated output after verification. One process still has one program/type map.
- `0983d5d37216de6b1be73ad5a726cb5d79aa1bab` [#5545](https://github.com/BoundaryML/baml/pull/5545) — Excluded: Homepage presentation only. Marketing examples do not establish new generated-SDK behavior.
- `845c1240bdf7058a947c518a4f4b03d3beed2345` [#5546](https://github.com/BoundaryML/baml/pull/5546) — Retained: Homebrew publishing trusts the wrapper formula before strict audit, fixing the earlier trust refusal. The dry-run verifier exercises trust/audit too. Successful live promotion must be checked separately; this diff does not by itself prove completion.
- `952c24c691cf58edc7ad293d3f4d3dbc1719b474` [#5548](https://github.com/BoundaryML/baml/pull/5548) — Excluded: Restores the standalone agent article preview and adds website preview validation. No language/SDK change.
- `936cc32f27bad1f50a197050c1c241ef2ad9aaa6` [#5547](https://github.com/BoundaryML/baml/pull/5547) — Retained: Bundled cleanup/catch_all docs show actual pattern=>handler arms and a compilable finalizer example. No syntax behavior changes.
- `8a0350dd1737b1cc5c90159c1cba2a00f45e0ca2` [#5533](https://github.com/BoundaryML/baml/pull/5533) — Retained: Extend package_name to Go package/directory, Rust Cargo package/lib/directory, and TypeScript directory. Explicit Go names require the last sdk_import_path segment to match. Named destinations use ownership checks. Swift/Java/C++/C# reject this option.
- `3194495608ccde61f0067eda418a55a051b1dadf` [#5549](https://github.com/BoundaryML/baml/pull/5549) — Excluded: Website build workflow and PostHog dependencies only. No installed language SDK or installer behavior changes.
- `853072242cf951cca227759a130b9cb91d60e9a0` [#5523](https://github.com/BoundaryML/baml/pull/5523) — Excluded: Website early-access analytics only.
- `8de396097f7cfda74764070fada3b13f1c29bc78` [#5550](https://github.com/BoundaryML/baml/pull/5550) — Excluded: Standalone agent-skills article charts only. These measurements are not runtime performance changes in this canary.
- `a06b63521a6909dbdfbd60d9ccf1da7e6dfdb77e` [#5501](https://github.com/BoundaryML/baml/pull/5501) — Retained: Remove ten unreachable standard-library fallbacks remaining after the wire sanitizer changed. Clean user projects no longer get those embedded-stdlib unreachable warnings. User-code unreachable warnings remain. The original PR description said eleven; the final author comment and merged diff correct this to ten.
- `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a` [#5551](https://github.com/BoundaryML/baml/pull/5551) — Excluded: Mechanical version/release metadata only. Its source commit is the upper boundary; package/manifest publishing is validated separately.
