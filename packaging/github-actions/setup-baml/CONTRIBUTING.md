# Contributing

This action lives in `packaging/github-actions/setup-baml` in `BoundaryML/baml` and is mirrored to `BoundaryML/setup-baml`; open pull requests against the monorepo.

Run `npm test` and `shellcheck scripts/setup.sh` before opening a pull request. Changes to installer pins must update both the immutable BAML commit and the matching SHA-256 digest after reviewing the upstream installer diff.

Pull requests exercise exact-version installation on Linux, macOS, and Windows, plus `canary`, `nightly`, and project-selector flows. The action itself must remain limited to installing and selecting BAML; project checks, generation, and tests belong to caller workflows.
