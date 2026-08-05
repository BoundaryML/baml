# @boundaryml/baml-tooling

One compiler-backed package for direct BAML imports. BAML syntax and semantics
remain exclusively in the Rust compiler bridge.

- Import `@boundaryml/baml-tooling/vite`, `/rollup`, `/webpack`, or `/esbuild`
  for the corresponding unplugin adapter. The direct Bun adapter is exported
  from `@boundaryml/baml-tooling/bun`.
- Configure TypeScript with
  `compilerOptions.plugins: [{ "name": "@boundaryml/baml-tooling/typescript-plugin" }]`
  and select workspace TypeScript in VS Code.
- Run `baml-ts-gen` before plain `tsc --noEmit` when build-time virtualization
  is unavailable.

All host entry points delegate to the same `BamlProject` and build core, so
project discovery, sidecar precedence, caching, diagnostics, and mappings have
one implementation.
