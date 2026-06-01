# Releasing `@boundaryml/baml-core` (Node.js SDK)

The Node.js runtime package `@boundaryml/baml-core` and its per-platform
native sub-packages are built and published by
`.github/workflows/release-sdk.yaml` (shared with the Python SDK release).

## What ships

- **`@boundaryml/baml-core`** — the umbrella package: compiled TypeScript
  runtime (`index.js`, `proto.js`, `typemap.js`, `define_function.js`, the
  media/handle/stream wrappers), the protobuf runtime under `proto/`, and an
  `optionalDependencies` map pointing at the per-platform native packages.
- **`@boundaryml/baml-core-<triple>`** — one per build target (8 total:
  linux gnu/musl × x64/arm64, darwin x64/arm64, windows-msvc x64/arm64),
  each carrying a single `*.node` native addon. `napi prepublish` assembles
  these from the build matrix's uploaded artifacts.

Generated `baml_sdk/` projects (`baml-cli generate --from <project>` with a
`generator { output_type "typescript/node" }` block) import the runtime
from `@boundaryml/baml-core`.

## Version

The version is the `version` field in
`sdks/nodejs/bridge_nodejs/package.json`. Bump it in-tree and merge before
dispatching a release run (mirrors the Python `pyproject.toml` flow).

## Release procedure

1. Bump `bridge_nodejs/package.json` `version` (and the generated SDK's
   expected runtime version if pinned) on a branch; merge to the default
   branch.
2. Dispatch **BAML SDK Release** (`release-sdk.yaml`) via
   `workflow_dispatch`:
   - `publish_npm: false` first — a build-only rehearsal that runs the full
     8-target matrix and assembles the packages without publishing.
   - Once green, dispatch again with `publish_npm: true` to publish the
     umbrella + sub-packages to npm.
3. Verify on a clean machine per primary platform:
   ```sh
   npm install @boundaryml/baml-core
   node -e "const b = require('@boundaryml/baml-core'); console.log(Object.keys(b).length)"
   ```
   Only the matching native sub-package is pulled in via
   `optionalDependencies`.

## Notes

- Native builds use `napi-rs` (`@napi-rs/cli`), the Node analog of the
  Python `maturin` flow. musl targets cross-compile via the zig linker.
- The `napi.binaryName` stays `baml_node`; only the npm package name is
  `@boundaryml/baml-core`. The auto-generated `native.js` loader prefers the
  per-platform package and falls back to the co-located `*.node`.
- npm publish uses `NPM_TOKEN` (repo secret) with `--provenance`. If
  `@boundaryml` is onboarded to npm trusted publishing (OIDC), drop the
  token and mirror the PyPI OIDC binding instead.
- `baml-cli generate` reaches the emitter through `codegen_nodejs` (wired in
  `crates/baml_cli/src/generate.rs`); `output_type "typescript/node"`.
