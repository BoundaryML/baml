# Boundary developer docs

This package serves the Fumadocs portal planned for `developer.boundaryml.com`.

## Local development

From the repository root:

```sh
pnpm --filter @baml/developer-docs dev
```

The portal imports the canonical BAML TextMate grammar from `typescript2/pkg-grammar` so local highlighting stays aligned with the language tooling.

## Generated references

Build the repository CLI, then point both generators at that exact binary:

```sh
cargo build --manifest-path baml_language/Cargo.toml -p baml_cli --bin baml-cli
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" pnpm --filter @baml/developer-docs generate:reference
BAML_BIN="$PWD/baml_language/target/debug/baml-cli" pnpm --filter @baml/developer-docs generate:cli-reference
```

The generated Markdown and source snapshots are committed. CI rebuilds the repository CLI and runs both corresponding `check:*` commands, so language or command-tree drift fails with a regeneration instruction.

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
Generated pages and provenance are committed so previews do not depend on a
second repository. CI verifies their hashes, navigation, and that no unmanaged
book chapter slipped around the approval manifest.

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
