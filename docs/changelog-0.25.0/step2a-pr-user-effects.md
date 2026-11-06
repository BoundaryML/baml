# BAML 0.25.0 release audit

Prepared on 2026-11-05 Pacific time for the 2026-11-06 UTC release. All inventories, effects, examples, and follow-ups use these fixed boundaries.

- Lower: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Upper: 0.25.0 release source, `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`, selected by [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387). The tag was not yet created when these references were pinned. Verify its final target before publication.
- The range contains 14 squash-merged PRs across the whole repository. All are v1, including website, workflow, tests, and metadata PRs excluded from the public post.
- The 0.24.0 registry release did not promote the canary channel. #5551 explicitly replaces it with 0.25.0 instead of rerunning the old cut. The new post links the 0.24.0 migrations for users coming from the last promoted 0.23.0 canary.
- The existing changelog branch was rebased onto this upper source. The 0.24.0 notes and audits remain separate. This directory records the additional 0.25.0 inventory without overwriting the earlier audit.

## Per-PR effects

- **FEATURE**, [#5532](https://github.com/BoundaryML/baml/pull/5532): Configure the Python SDK package and output directory with a dotted package_name. Self-relative imports and type maps follow relocation. Defaults keep their public location; internal generated bookkeeping can change. Renaming requires updating imports/packaging and removing stale generated output after verification. One process still has one program/type map.
- **FEATURE**, [#5533](https://github.com/BoundaryML/baml/pull/5533): Extend package_name to Go package/directory, Rust Cargo package/lib/directory, and TypeScript directory. Explicit Go names require the last sdk_import_path segment to match. Named destinations use ownership checks. Swift/Java/C++/C# reject this option.
- **FEATURE**, [#5543](https://github.com/BoundaryML/baml/pull/5543): Read BTELCAS v6 redacted values in CLI queries and the playground. State/kind report redacted, rendered data uses a $redacted envelope, and equality depending on withheld data is unknown. Writers remain v5; no new application redaction declaration is shipped.
- **BUGFIX**, [#5546](https://github.com/BoundaryML/baml/pull/5546): Homebrew publishing trusts the wrapper formula before strict audit, fixing the earlier trust refusal. The dry-run verifier exercises trust/audit too. Successful live promotion must be checked separately; this diff does not by itself prove completion.
- **BUGFIX**, [#5547](https://github.com/BoundaryML/baml/pull/5547): Bundled cleanup/catch_all docs show actual pattern=>handler arms and a compilable finalizer example. No syntax behavior changes.
- **BUGFIX**, [#5501](https://github.com/BoundaryML/baml/pull/5501): Remove ten unreachable standard-library fallbacks remaining after the wire sanitizer changed. Clean user projects no longer get those embedded-stdlib unreachable warnings. User-code unreachable warnings remain. The original PR description said eleven; the final author comment and merged diff correct this to ten.

Each effect has one classification. #5532/#5533 aggregate into one package-name feature. #5543 is a separate reader feature. The three bug fixes remain separate. There are no new mandatory syntax/API migrations beyond the 0.24.0 changes; opting into a custom name requires the documented path/import updates. No performance claims are taken from the standalone marketing article.
