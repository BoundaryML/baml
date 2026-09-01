# Boundary developer docs

This package serves the Fumadocs portal planned for `developer.boundaryml.com`.

## Local development

From the repository root:

```sh
pnpm --filter @baml/developer-docs dev
```

The portal imports the canonical BAML TextMate grammar from `typescript2/pkg-grammar` so local highlighting stays aligned with the language tooling.

## Generated references

Language and CLI reference pages are build artifacts. They are never checked
in. A nightly or canary release asks its exact stamped CLI for the authoritative
standard-library package list, exports every package plus the CLI command tree,
and publishes one immutable JSON document at:

```text
https://pkg.boundaryml.com/manifest/v1/docs/v<version>/stdlib.json
```

The Fumadocs build is a TypeScript-only consumer. By default it resolves the
curated release index at
`https://pkg.boundaryml.com/manifest/v1/docs/versions.json`, fetches every
listed immutable artifact, and renders the versions side by side. Unversioned
reference routes remain aliases for the index's default version; exact routes
include `v<version>` after `/baml/language/reference` or `/cli/commands`.

Use `BAML_DOCS_VERSIONS` (comma-separated), `BAML_DOCS_METADATA_URLS`, or
`BAML_DOCS_METADATA_FILES` to build an explicit set. Their singular forms stay
supported for deterministic one-version and pull-request builds.
`BAML_DOCS_DEFAULT_VERSION` selects the unversioned alias and must be present
in the set. Each release JSON contains all toolchain-discovered packages—not a
docs-maintained list—and generated symbols use fully qualified names such as
`baml.http.Request`.

To exercise the complete producer/consumer path locally:

```sh
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli --bin baml-cli
version="$($PWD/baml_language/target/debug/baml-cli --version | awk '{print $2}')"
metadata="$PWD/.tmp/baml-docs-metadata-$version.json"
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" \
  pnpm --filter @baml/developer-docs produce:docs-metadata -- \
    --version "$version" \
    --channel canary \
    --source-revision "$(git rev-parse HEAD)" \
    --released-at "$(git show -s --format=%cI HEAD)" \
    --output "$metadata"
BAML_DOCS_VERSION="$version" BAML_DOCS_METADATA_FILE="$metadata" \
  pnpm --filter @baml/developer-docs build
```

## BAML book imports

The book remains at `/baml/book`, but publication is approval-gated. Add only
human-audited chapters to `book-import.json` with `status: "approved"`, the
source path, route output, and SHA-256 of the exact reviewed Markdown. Drafts
that merely exist in the `baml-book` working tree are not publishable input.
The source checkout must be clean and at the revision pinned by the manifest,
which also pins every included listing and quiz.

Import approved chapters from a local checkout:

```sh
pnpm --filter @baml/developer-docs generate:book -- --source /path/to/baml-book
pnpm --filter @baml/developer-docs check:book
```

The importer converts anchored mdBook includes, semantic notes, numbered code
listings, opt-in runnable projects, and TOML quizzes into native Fumadocs MDX.
Generated pages, navigation, and provenance are ignored build outputs. CI
regenerates them from the pinned, clean source checkout and verifies their
hashes and that no unmanaged chapter slipped around the approval manifest. Set
`BAML_BOOK_SOURCE` (or pass `--source`) once approved chapters are present.

## Runnable examples

Runnable examples execute in one isolated Web Worker per page. The worker and
source-pinned WASM runtime are checked in, so Vercel previews never compile Rust
as part of a docs build.

To rebuild the runtime after a language release or compatible canary update:

```sh
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli --bin baml-cli
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" pnpm --filter @baml/developer-docs runner:artifact
pnpm --filter @baml/developer-docs runner:bundle
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" pnpm --filter @baml/developer-docs verify:runner
```

The manifest pins the source commit, runtime identity, hashes, and compressed
sizes. CI also executes every registered example with the native CLI and the
exact browser artifact, requiring identical formatted output.

## Deployment

The repository is connected to the Vercel project `baml/developer-docs` with these settings:

- Root Directory: `docs`
- Framework Preset: Next.js
- Production Branch: `canary`
- Preview deployments: enabled for pull requests

The project currently uses its generated `vercel.app` domain. Assigning `developer.boundaryml.com` requires access to the existing `boundaryml.com` domain or DNS account.

Configure `DEVELOPER_DOCS_VERCEL_DEPLOY_HOOK` so the release workflow rebuilds
production only after its channel manifest has been promoted. Configure
`DEVELOPER_DOCS_VERCEL_TOKEN`, `DEVELOPER_DOCS_VERCEL_ORG_ID`, and
`DEVELOPER_DOCS_VERCEL_PROJECT_ID` for the docs workflow to build pull request
previews from the exact metadata it just verified. That workflow preview is the
authoritative review surface for generated references; a standalone Vercel Git
integration build can only resolve the latest published channel artifact.
