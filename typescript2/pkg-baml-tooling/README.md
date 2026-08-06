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
  is unavailable. The `<name>.baml.d.ts` files it writes carry declarations
  only, so they never disable the bundler plugins — the two workflows coexist
  in one checkout. Only a hand-written `<name>.baml.ts` overrides the runtime.

Project ownership follows the nearest `baml.toml` walking up from each import,
so importing `.baml` from a dependency in `node_modules` (or a pnpm-workspace
sibling reached through a symlink) compiles against that package's own project
— every project gets its own session, runtime module, and cache namespace.

All host entry points delegate to the same `BamlProject` and build core, so
project discovery, sidecar precedence, caching, diagnostics, and mappings have
one implementation.

`BamlProject.dispose()` releases the compiler session for real: it sends the
protocol's `close` request, so the bridge drops the project's database and the
allocations reached through it on the native and WASM hosts alike. Call it when
you stop using a project — a long-lived host that replaces sessions (a config
change reopening a lane) would otherwise accumulate superseded databases for
the life of the process. It is idempotent and never throws, and using a project
after disposal fails loudly rather than silently reopening a session.

The native bridge loads from `@boundaryml/baml-bridge-tooling` by default.
`BAML_TOOLING_BRIDGE_PATH` overrides that with an arbitrary module path and is
honored inside `tsserver` as well — treat it as a trusted, development-only
escape hatch (CI pinning, local bridge builds), never as user input.
