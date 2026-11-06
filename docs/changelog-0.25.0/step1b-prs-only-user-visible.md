# BAML 0.25.0 release audit

Prepared on 2026-11-05 Pacific time for the 2026-11-06 UTC release. All inventories, effects, examples, and follow-ups use these fixed boundaries.

- Lower: published `baml-language-0.24.0`, `dec4713a0e7c9699442bf5855b4f327063f769c5`.
- Upper: 0.25.0 release source, `a2f6d5403f399b0b3cc1a0902b0b3dd208c6043a`, selected by [release run 42150548387](https://github.com/BoundaryML/baml/actions/runs/42150548387). The tag was not yet created when these references were pinned. Verify its final target before publication.
- The range contains 14 squash-merged PRs across the whole repository. All are v1, including website, workflow, tests, and metadata PRs excluded from the public post.
- The 0.24.0 registry release did not promote the canary channel. #5551 explicitly replaces it with 0.25.0 instead of rerunning the old cut. The new post links the 0.24.0 migrations for users coming from the last promoted 0.23.0 canary.
- The existing changelog branch was rebased onto this upper source. The 0.24.0 notes and audits remain separate. This directory records the additional 0.25.0 inventory without overwriting the earlier audit.

## Retained PRs

- [#5532](https://github.com/BoundaryML/baml/pull/5532) — Add a package_name generator setting; Python SDKs import under any name.
- [#5533](https://github.com/BoundaryML/baml/pull/5533) — Let package_name name Go, Rust and TypeScript SDKs.
- [#5543](https://github.com/BoundaryML/baml/pull/5543) — Redacted snapshot value in read-only BTELCAS format v6.
- [#5546](https://github.com/BoundaryML/baml/pull/5546) — fix(release): trust Homebrew formula before auditing it.
- [#5547](https://github.com/BoundaryML/baml/pull/5547) — Show valid catch_all syntax in cleanup and catch_all docs.
- [#5501](https://github.com/BoundaryML/baml/pull/5501) — fix(stdlib): remove unreachable code after diverging loops.

All eight exclusions, including shipped-effect checks for packaging and generator-test changes, are recorded in [the full inventory](step1a-release-inventory.md).
