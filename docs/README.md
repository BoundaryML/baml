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

The selected default toolchain must also provide its browser runtime. Curated
index builds resolve it automatically. Explicit builds use
`BAML_DOCS_RUNTIME_FILE` or `BAML_DOCS_RUNTIME_URL`; plural forms accept a
comma-separated runtime set when needed. The consumer validates the runtime's
payload hash, version, source revision, JS/WASM hashes, and delivery budget,
then materializes only the selected default into `public/baml-runtime`.

The same release freezes the matching browser runtime at
`/manifest/v1/docs/v<version>/runtime.json`, with its JavaScript and WASM files
under content-addressed paths beside that manifest. The version index binds both
`stdlib.json` and `runtime.json` by payload checksum. Release CI runs every
registered example through the exact native CLI and exact packaged WASM before
advertising either artifact.

Pull-request verification and its exact-metadata Vercel preview pass the
source-built JSON and WASM together with a captured copy of the curated version index.
The source-built toolchain becomes the default while the other immutable
published toolchains remain available in the selector. Set
`BAML_DOCS_VERSIONS_INDEX_FILE` to reproduce that overlay locally.

The build eagerly renders authored pages, the default-version aliases, and each
version landing page. Versioned symbol and command pages remain normal public
routes, but render on their first request instead of multiplying release build
work by the number of retained toolchains.

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

Prepare a deterministic editorial bundle from a clean checkout at that pinned
revision before approving anything:

```sh
pnpm --filter @baml/developer-docs review:book -- \
  --source /path/to/clean/baml-book \
  --output /tmp/baml-book-review
```

The bundle discovers the ordered chapters from `src/SUMMARY.md`, verifies that
every include, listing, runnable project, and quiz converts to valid MDX, and
records exact source and converted hashes. Its Markdown summary and converted
pages are review aids only: every candidate is explicitly unapproved until a
human copies its approval entry into `book-import.json` after auditing it.

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

Runnable examples execute in one isolated Web Worker per page. The worker is
checked in; the source-pinned WASM is a generated deployment input and is never
checked in. Release and pull-request CI compile it once, while Vercel only
validates and copies the frozen artifact with TypeScript-only build tooling.

To produce a local runtime after building `bridge_wasm` with `wasm-pack`:

```sh
version="$($PWD/baml_language/target/debug/baml-cli --version | awk '{print $2}')"
revision="$(git rev-parse HEAD)"
wasm-pack build baml_language/crates/bridge_wasm --target web --release \
  --out-dir /tmp/baml-docs-runtime-build --out-name bridge_wasm
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" \
  pnpm --filter @baml/developer-docs produce:runner-artifact -- \
    --input /tmp/baml-docs-runtime-build \
    --version "$version" \
    --source-revision "$revision" \
    --output /tmp/baml-docs-runtime
BAML_DOCS_RUNTIME_FILE=/tmp/baml-docs-runtime/runtime.json \
  pnpm --filter @baml/developer-docs generate:derived
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" \
  pnpm --filter @baml/developer-docs verify:runner
```

The release manifest pins the source commit, runtime identity, hashes, and
compressed sizes. The same-origin serving manifest and content-addressed files
are derived from it during the docs build. CI executes every registered example
with the native CLI and exact browser artifact, requiring identical output.

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
