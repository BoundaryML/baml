# BAML TypeScript tooling architecture

**What this adds.** Opt-in v1 tooling for importing BAML directly from TypeScript — `import { b, Resume } from './baml_src/resume.baml'` and `import { b } from 'baml:client'` — with cross-language editor support (definition, references, rename, diagnostics, completion, hover) and build integration for Vite, Rollup, Webpack, esbuild, and Bun. `baml generate`, the generated SDK, and all v0 surfaces are unchanged; the feature activates only by installing the tooling package.

## Package boundaries

Rust, under `baml_language/` — the compiler is the only owner of BAML syntax and semantics; no TypeScript code parses BAML or identifies a declaration by name-matching generated text:

- `crates/baml_tooling_api` (internal): every tooling operation over a long-lived `ProjectDatabase` — saved/unsaved source snapshots, check, in-memory SDK and virtual-module emission with segment maps, definition/references/hover/completions, prepare-rename/rename, `fingerprint()`, `watch_files()`. Advertises capabilities `typescriptImports.v1` and `rename.v1`.
- `crates/bridge_ctypes`: the additive-only `baml.tooling.v1` protobuf contract (`types/baml/tooling/v1/tooling.proto`), generated into `typescript2/pkg-proto`.
- `sdks/typescript/bridge_tooling`: napi crate published as `@boundaryml/baml-bridge-tooling` (umbrella + eight platform packages in `release/platforms.json`); a thin protobuf shim over `baml_tooling_api`, independent of the runtime bridge ABI. `crates/bridge_wasm` exposes the same byte-oriented dispatcher; a protocol operation is complete only when both hosts expose it in the same commit.
- `crates/sdkgen_typescript_shared`: emission factored into pure in-memory `emit_modules` plus a mapped writer that records segment maps; `baml generate` uses it internally, guarded by an SDK byte-parity test.
- `crates/baml_lsp2_actions`: compiler-owned `prepareRename`/`rename` with collision validation — the gate behind `rename.v1`.

TypeScript, under `typescript2/`:

- `pkg-baml-tooling` → **`@boundaryml/baml-tooling`**, the single published package. It discovers the owning `baml.toml`, chooses native then WASM unless configured, maintains source overlays and revision gates, translates segment maps, owns the disk cache and the sidecar predicate, and ships the `baml-ts-gen` bin. All host integrations are subpath entry points over shared cores: `./vite`, `./rollup`, `./webpack`, `./esbuild`, `./bun` share one build core; `./typescript-plugin` (CJS) is the tsserver plugin. Host entry points contain only lifecycle and API wiring; resolver state, sidecar policy, diagnostics, and generated artifacts live in the shared project/build core.
- `pkg-ts-tooling-e2e` (private): real bundler builds and HMR, packed-install fixtures, and a real tsserver protocol harness.

## Projection and source maps

A `.baml` file import is an export filter over the compiled project (BAML compilation is project-wide); `baml:client` exports the full public client. Every entry module imports one shared internal runtime module, `\0baml:<projectId>:runtime`, so serialized bytecode occurs once per bundle.

Declaration projection carries a version-1 segment map. BAML source is never transformed before compilation, so compiler spans are true source spans; the only mapping needed is generated declaration ↔ BAML source. Generated offsets are UTF-16; physical BAML offsets are UTF-8 bytes. Each segment carries a compiler `symbolId`, optional overload `signatureId`, source hash, and semantic role. The tooling package verifies revision text against `sourceHashes`, builds UTF-8/UTF-16 line indexes, and binary-searches in both directions. Synthetic items map to their compiler-designated owner, never an invented span; overloads and same-named symbols are disambiguated by id, never bare names. An untranslatable result is dropped — a virtual filename is never returned to an editor.

## Language-service features

`@boundaryml/baml-tooling/typescript-plugin` (configured in `compilerOptions.plugins`, workspace TypeScript required) creates deterministic in-memory `.d.ts` files under `/.baml/__virtual__/` and feeds unsaved editor buffers to the compiler as overlays, so the compiler always sees what the user sees. Each feature asks TypeScript first, then translates any hit inside a virtual declaration through the segment map to a `symbolId` and a compiler query: definition lands on the exact physical `.baml` token; references merge TS use sites with compiler `references_at`, deduped and grouped under the BAML declaration; rename maps the TS symbol to compiler `prepare_rename`/`rename` and is capability-gated on `rename.v1` (older bridges refuse cleanly); diagnostics merge TS use-site errors with compiler diagnostics at true `.baml` spans, also attached once per affected import specifier; completions and hover are proxied from the literal declarations and augmented with compiler documentation. The native BAML LSP remains authoritative while the cursor is inside a `.baml` document.

## Caching, invalidation, HMR

Three layers: (1) the in-process Salsa database — overlays go through `update_file` so only dependent queries recompute; (2) a disk artifact cache namespaced by tooling protocol version and `fingerprint()` (compiler version + semantic config + target) with per-file raw-byte content hashes, atomic-rename writes, envelope validation on read, and single-flighted concurrent misses — bad or stale entries are misses, never errors; (3) a bounded per-revision projection cache on the TS side.

Build hosts watch everything `watch_files()` reports — all sources, `baml.toml`, and config contributors — from startup. Any BAML or config change invalidates project-wide (correctness before pruning), with per-module output hashing so unchanged modules aren't re-emitted. In a dev server a failed compile keeps last-known-good runtime loaded while diagnostics surface; a production build always fails, with errors carrying physical BAML file/line/col.

## Sidecars and native typechecking (`tsc` / `tsgo`)

`baml-ts-gen` emits sidecars byte-identical to the virtual declarations so plain JS-based `tsc --noEmit` works without the plugin. `<name>.baml.ts` overrides runtime and types; `<name>.baml.d.ts` overrides declarations only; one exported predicate is shared by every host. Generated files embed the compiler fingerprint, so a stale sidecar is detected rather than silently trusted. An invalid build fails sidecar generation; the editor may retain a stale-marked last-known-good declaration alongside the current compiler error.

The Go-native TypeScript compiler (`tsgo`, now published as `typescript@latest`) does not load JS tsserver plugins, so direct `.baml` imports need compiler support. The recommended path is **[smithersai/baml-tsgo](https://github.com/smithersai/baml-tsgo)**: following the pattern established by [Effect's TypeScript-Go fork](https://github.com/Effect-TS/tsgo) (pinned upstream submodule, minimal ordered patches, generated shims, `init()`-registered hooks), a small resolver hook maps relative `.baml` imports onto the fingerprinted `.baml.d.ts` sidecars, so one native binary provides both `--noEmit` checking and the editor language server while BAML semantics stay in Rust. The alternative is [Volar](https://volarjs.dev/): a language plugin projecting `.baml` into mapped virtual TypeScript over stock `tsc` with a `vue-tsc`-style checker wrapper — no compiler fork, but a second Node-based projection/checking stack to keep in sync.

## Running the tests

- Rust, from `baml_language/`: `cargo test --lib`, `cargo test --package baml_tests`, `cargo test --package baml_lsp2_actions_tests`, and `cargo nextest run -p sdk_test_typescript` / `-p sdk_test_typescript_web` (SDK byte parity). Lint gates: `cargo fmt --all -- --config imports_granularity=Crate --config group_imports=StdExternalCrate` and `cargo clippy --workspace --all-targets --all-features -- -D warnings`.
- TypeScript, from `typescript2/`: `pnpm build:wasm && pnpm lint && pnpm typecheck && pnpm test && pnpm build`. The `pkg-baml-tooling` contract suite runs twice (native and WASM backends must agree); `pkg-ts-tooling-e2e` runs real Vite/Rollup/Webpack/esbuild/Bun builds with HMR, the sidecar + `tsc --noEmit` path, packed-tarball installs, and a real tsserver over the packed plugin.
