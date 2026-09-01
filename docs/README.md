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
