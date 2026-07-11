# Phase 1 Plan: Set Up the Bridge (bridge_nodejs)

## Overview

Bring `bridge_nodejs` (napi-rs + hand-written TypeScript) to the runtime surface that Node codegen will need in Phase 2+, taking `bridge_python` as the reference impl but following the language-idiomatic API in `00a-spec-codegen-mappings.md` rather than mirroring Python's package or naming choices. The remaining Phase 1 runtime gaps are: the runtime npm package **rename** to `@boundaryml/baml-core-node`, the handle-table helper functions (`takeHandleFromTable` / `putHandleIntoTable` / seed helpers), the four media value classes (`Image`/`Audio`/`Video`/`Pdf`), the `BamlStream` typed wrapper, and the process-global runtime accessor (`initializeRuntime` / `getRuntime`).

Much of the runtime layer already exists and is at parity — `callFunctionSync` / `callFunction`, the `BamlOutboundResult` envelope decode (ok/error/panic + process-exit), error classes, collector, host span manager, ctx manager, the abort controller, and the full host-callable (`HostValue`) round-trip. See "Current State Analysis" for the precise line-level inventory. This plan only covers the **runtime** layer (`bridge_nodejs`); the `sdkgen_nodejs` emitter (a `unimplemented!` stub today — `sdkgen_nodejs/src/lib.rs:18-26`) and the generated SDK shape are Phase 2+.

**Runtime package name (locked decision): `@boundaryml/baml-core-node`.** All four `00*` spec docs (re-edited 2026-05-29) now consistently import runtime symbols — `defineFunction`, `defineInstanceFunction`, `initializeRuntime`, the media classes, and `Stream` — from `@boundaryml/baml-core-node` (`00a-spec-codegen-mappings.md:30, :276, :292`; `00a-example-ts-codegen-type-shapes.md:76, :555, :581`). The crate as it ships today is named `@boundaryml/baml-node` (`bridge_nodejs/package.json:2`, `binaryName: "baml_node"`). **Phase 1 owns renaming the package to `@boundaryml/baml-core-node`** so that Phase 2 generated code can import from the canonical name — see Phase 1.0 for the `package.json` rename step. The runtime exposes `initializeRuntime`, `defineFunction` / `defineInstanceFunction` (the factories themselves are emitted/owned in Phase 2, but the package they import from is settled now), the media value classes, and `Stream` under this package name. Throughout this plan, "the runtime package" means `@boundaryml/baml-core-node`.

**Runtime export names for the stdlib value classes (locked).** The Phase 1 napi/TS classes are named `BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf` / `BamlStream` (mirroring Python's `baml_core.baml_py.BamlImage`, `baml_core.BamlStream`), and `@boundaryml/baml-core-node` exports them under exactly those `Baml*` names — it does **not** alias them. Codegen does the aliasing on re-export: the generated `baml_sdk/baml/media/index.ts` emits `export { BamlImage as Image } from "@boundaryml/baml-core-node"` (and `BamlStream as Stream` in `baml/llm/index.ts`), exactly as the Python fixture re-binds `from baml_core.baml_py import BamlImage as Image` (`00a-spec…:68`, `00a-example…:531-573`). So Phase 1 just exports the `Baml*` names; the public `Image`/`Stream` surface is produced by Phase 2/4 codegen. `defineFunction` and `defineInstanceFunction` are likewise separate named exports of `@boundaryml/baml-core-node`. The `10a-todo-items.md` B3 fix (codegen-side media re-export, still referencing the stale `@boundaryml/baml-node` name) is forward-looking and pre-dates this rename.

## Goal

Delivery criteria. The runtime layer must expose this surface (verified by Jest assertions under `bridge_nodejs/tests/` where a test is named; existing behavior is noted as already-passing):

0. The runtime npm package is renamed from `@boundaryml/baml-node` to `@boundaryml/baml-core-node` (`package.json:2`); the `binaryName` / native artifact name stays `baml_node` (renaming the `.node` artifact is out of scope and unrelated to the npm package name). See Phase 1.0.
1. `BamlRuntime.initializeRuntime()` initializes the process-global singleton (renaming/aliasing the existing `fromFiles` factory — see discrepancy note in 1.4); `getRuntime()` returns a handle to it.
2. `callFunctionSync` / `callFunction` (async) round-trip primitive, class, enum, union, list, map, and nested-class results. **Already passing** (`tests/call_function.test.ts`, `tests/test_engine.test.ts`).
3. `AbortController` cancels in-flight calls. **Already implemented** (`src/abort_controller.rs`).
4. `BamlHandle` lifecycle works: construct from `{key, handleType}`, clone, GC-driven `HANDLE_TABLE.release` via `ObjectFinalize` (**already implemented**, `src/handle.rs`), plus new module-level `takeHandleFromTable` / `putHandleIntoTable` / `_seedFunctionRefHandle` / `_seedGenericMediaHandle` helpers.
5. `BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf` napi classes with `fromUrl` / `fromFile` / `fromBase64` constructors and `url()` / `file()` / `base64()` / `mimeType()` accessors, backed by `CffiHandleTableEntry::Adt(BexExternalAdt::Media(...))`, exported from `@boundaryml/baml-core-node` under their `Baml*` names only (codegen aliases them as `Image`/`Audio`/`Video`/`Pdf` on re-export — see the locked note in Overview). These are runtime-owned value classes; codegen emits no class body for them (`00a-spec-codegen-mappings.md:68`, `00a-example…:531-573`). **Not yet present** (`src/media.rs` does not exist).
6. `BamlStream<TStream, TFinal>` TypeScript class with `next` / `nextAsync` / `final` / `finalAsync` round-tripping through `baml.llm.Stream.next` and `baml.llm.Stream.final` via the global runtime, exported from `@boundaryml/baml-core-node` as `BamlStream` only (codegen aliases it as `Stream` on re-export). Only **async** streaming is a real function call — codegen emits `_stream_async`; the reserved sync `_stream` name must throw a clear runtime error or be omitted from the type surface (`00b-overview.md:15`, `00a-spec-codegen-mappings.md:80-81`). `next`/`final` here are the per-chunk pulls on the wrapper, distinct from the function-level stream naming. **Not yet present.**
7. `BamlError` / `BamlInvalidArgumentError` / `BamlClientError` / `BamlCancelledError` are real JS classes. **Already present** (`typescript_src/errors.ts`). BAML-function failures already flow through the structured `BamlOutboundResult` envelope (`decodeCallResult` raises class-name/message/trace and `process.exit`s on exit-panics — `typescript_src/proto.ts:324-346`); `wrapNativeError` only re-wraps raw napi errors from the non-call paths (decode errors, missing function). See 1.5 for the remaining `BamlPanic` gap.
8. `Collector` / `FunctionLog` / `Timing` / `Usage` / `LLMCall` work as in Python; `FunctionLog.result` decodes the proto. **Already implemented** (`src/types/collector.rs`, `typescript_src/index.ts`).
9. `HostSpanManager` + `CtxManager` provide async-isolated tracing. **Already implemented** (`src/types/host_span_manager.rs`, `typescript_src/ctx_manager.ts`).
10. `flushEvents` runs on process exit (**already wired** in `index.ts:128` and `ctx_manager.ts:23-28`) and is exposed to user code.

(Not a Phase 1 deliverable but worth noting: the host-callable / `HostValue` round-trip — `registerHostCallable` / `completeHostCall` / `releaseHostCallable` plus the dispatch wrapper — is **already fully implemented** in `src/host_value.rs` and `typescript_src/proto.ts`, with coverage in `tests/host_callable.test.ts`. This surface is not mentioned in the original plan; it is done and stays as-is.)

**Automated runtime-layer verification command (run from `bridge_nodejs/`):**

```
pnpm install && pnpm build:debug && pnpm test
```

or equivalently `cargo nextest run --package bridge_nodejs -- --ignored jest` (which is what `tests/run_jest.rs` drives — note its build step is `pnpm build:debug`, not a release build).

The full `sdk_test_nodejs` nextest suite is explicitly **not** the Phase 1 anchor — it will keep failing until codegen (Phase 2-4) and the emitted SDK land.

## Current State Analysis

Ground-truth inventory of `bridge_nodejs/` as it stands today (paths under `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/`), verified against the source on 2026-05-29. The runtime layer is substantially further along than the previous draft of this plan assumed — most "gap" items below are already done. The five real remaining gaps are: **the `@boundaryml/baml-core-node` package rename, handle helpers, media classes, `BamlStream`, and the global-runtime accessor.**

### Already implemented and at parity

- **Crate scaffolding** — `Cargo.toml` declares cdylib + bex deps; `build.rs` calls `napi_build::setup()`; `package.json` wires `pnpm build:debug` (proto → napi-debug → tsc → tag-generated, `package.json:20-26`). The native module init (`src/lib.rs:15-27`) registers the host-dispatch/host-release callbacks into `bridge_cffi`. The only scaffolding change needed is the npm `"name"` rename to `@boundaryml/baml-core-node` (Phase 1.0); the `binaryName` / native `.node` artifact name stays `baml_node`.
- **`AbortController`** — `src/abort_controller.rs` matches `bridge_python/src/abort_controller.rs`: `new()`, `abort()`, `aborted` getter, `token()` accessor. ✓
- **`BamlRuntime` + `callFunctionSync` + `callFunction`** — `src/runtime.rs`. `BamlRuntime` is now a **zero-sized handle**: it does not cache an `Arc<dyn Bex>`; both call paths fetch the singleton via `bridge_cffi::get_runtime()` at each call site (`runtime.rs:51, :100`), mirroring `bridge_python/src/runtime.rs` after its 31e-phase4 refactor. The call paths delegate to `bridge_cffi::call_and_encode`, which performs the whole `Result → BamlOutboundResult` translation (incl. the `catch_unwind → SdkPanic` boundary) in Rust and returns envelope bytes; the TS side just decodes. The async variant uses napi's `env.spawn_future`. ✓
- **`BamlOutboundResult` envelope decode (ok/error/panic)** — `typescript_src/proto.ts:324-346` (`decodeCallResult`) already implements the full envelope contract from `00a`'s "Node error handling" section: `ok` decodes and returns the value; `error` raises with decoded class name + message + trace; `panic` raises likewise, except `isExitPanic` calls `process.exit(code)` (`proto.ts:334-336`), which fires the registered `process.once('exit', flushEvents)` hook to flush telemetry on the way out — see the exit-panic flush Spec note in 1.5. ✓
- **`Collector`/`FunctionLog`/`Timing`/`Usage`/`LLMCall`** — `src/types/collector.rs` (napi shells) + `typescript_src/index.ts:46-85` (TS wrappers). `FunctionLog.result` returns proto bytes which the TS wrapper decodes via `decodeCallResult`. ✓
- **`HostSpanManager`** — `src/types/host_span_manager.rs`: six methods (`enter`/`exitOk`/`exitError`/`upsertTags`/`deepClone`/`contextDepth`). The Node side accepts `serde_json::Value` directly via napi's `serde-json` feature (no Python `py_to_json` helper needed). ✓
- **`CtxManager`** — `typescript_src/ctx_manager.ts`: `AsyncLocalStorage`-backed `get`/`reset`/`cloneContext`/`upsertTags`/`traceFn`/`traceFnAsync`/`flush`. ✓
- **Host-callable (`HostValue`) round-trip** — `src/host_value.rs` (napi `registerHostCallable` / `releaseHostCallable` / `completeHostCall`) + the `makeHostCallableDispatch` tsfn wrapper in `typescript_src/proto.ts`. Wired into `bridge_cffi` via the module-init callbacks. **Fully implemented and tested** (`tests/host_callable.test.ts`). Not in the original plan; needs no Phase 1 work. ✓
- **TypeScript `index.ts`** — Re-exports `BamlRuntime`, `AbortController`, `BamlHandle`, `HostSpanManager`, `getVersion`, `flushEvents` (`index.ts:17`), `Timing` / `Usage` / `LLMCall` (`:18`), `encodeCallArgs` / `decodeCallResult` (`:19`), `CtxManager` (`:20`), the four error classes + `wrapNativeError` (`:22-28`); and defines the TS-side `FunctionResult` (`:30-44`) / `FunctionLog` (`:46-64`) / `Collector` (`:66-85`) wrappers plus explicit-runtime `callFunctionSync` / `callFunction` helpers (`:87-125`). ✓ (needs the new exports listed under gaps below: `getRuntime`, the handle helpers, the `Baml*` media classes, and `BamlStream` — all under their `Baml*` names; codegen adds the `Image`/`Stream` aliases on re-export.) Note: it does **not** re-export the host-callable napi functions (`registerHostCallable` / `releaseHostCallable` / `completeHostCall`) — those are internal to `proto.ts` and not surfaced; no Phase 1 change.
- **TypeScript `errors.ts`** — `BamlError` / `BamlInvalidArgumentError` / `BamlClientError` / `BamlCancelledError` + `wrapNativeError` (string-prefix classifier on `BamlError: BamlX:` messages). ✓ (`BamlPanic` is missing — see gaps; the classifier's `includes()` over-match is a minor 1.5 cleanup.)
- **TypeScript `proto.ts`** — `encodeCallArgs` / `setInboundValue` and `decodeCallResult` / `decodeValueHolder` handle primitives, bigint, lists, maps, classes, enums, unions, `Uint8Array`, `BamlHandle`, and host callables. Per the JS-encoder simplification, `setInboundValue` emits non-builtin objects as `mapValue` (no FQN tagging) and lets `coerce_arg_to_declared_type` reshape on the Rust side. ✓ for the runtime layer. It does **not** yet recognize media wrappers or `BamlStream` (those classes don't exist yet) — added in 1.2/1.3.
- **`BamlHandle` napi class with `HandleKey` low/high split** — `src/handle.rs` implements `new(HandleKey, handleType)`, `key`/`handleType` getters, `clone()`, and `ObjectFinalize` calling `HANDLE_TABLE.release`. The `Long`-compatible `{low, high}` shape (layout-compatible with protobufjs `Long`) replaces Python's bare `u64` — a deliberate node-idiomatic divergence, documented inline at `handle.rs:8-22`. ✓
- **Tests scaffold** — Six Jest test files: `test_engine.test.ts`, `test_collector.test.ts`, `test_tracing.test.ts`, `call_function.test.ts`, `host_callable.test.ts`, plus `run_jest.rs` driving them under `cargo nextest`. ✓

### Remaining Phase 1 work (gaps)

#### Rust (napi) side gaps

- **No `media.rs`** — `bridge_python/src/media.rs` defines a `define_media_pyclass!` macro used four times for `BamlImage` / `BamlAudio` / `BamlVideo` / `BamlPdf`, each wrapping a `BamlPyHandle` over a `CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc))`. There is no equivalent in `bridge_nodejs/src/` (confirmed: no `media.rs`, no `BamlImage`/`fromUrl`/`_fromHandle` in `native.d.ts`). The four media classes are required because they are runtime-owned stdlib values that codegen re-exports rather than emitting a class body for (`00a-spec-codegen-mappings.md:67`, `00a-example…:531-573`). Phase 1.2.
- **Handle-table helper functions missing** — `src/handle.rs` has the class lifecycle (clone, finalize-release) but **not** the module-level helpers `take_pyhandle_from_table` / `put_pyhandle_into_table` and the seed helpers from `bridge_python/src/py_handle.rs:81-134`. The Node `BamlHandle` already plays the full `BamlPyHandle` role for the *class* (clone + release-on-finalize); what's missing is the free functions for the decoder's validate-on-take and encoder's clone-on-put. Phase 1.1 adds `takeHandleFromTable(key, handleType)` / `putHandleIntoTable(handle)` and test-only `_seedFunctionRefHandle(globalIndex)` / `_seedGenericMediaHandle()`.
- **No global-runtime accessor / no `initializeRuntime`** — `bridge_python/src/runtime.rs` exposes `BamlRuntime.initialize_runtime` (a `#[staticmethod]` — the **only** constructor, *renamed from* `from_files`, not an alias) and a module-level `get_runtime()` free function (`runtime.rs:243-258`) that validates the `bridge_cffi` singleton and returns a fresh zero-sized handle. The Node side still exposes only `BamlRuntime.fromFiles` and has **no** `getRuntime` and **no** `initializeRuntime`. `getRuntime()` is what `BamlStream` (1.3) and Phase-2 generated callsites depend on. Phase 1.4. (⚠ see discrepancy on `fromFiles` vs `initializeRuntime` naming in 1.4.)
- **No `FunctionResult` napi class** — intentional. `bridge_python` exposes `FunctionResult` as a pyclass only for stub-gen advertising; it never crosses the FFI boundary. Node keeps `FunctionResult` as the existing TS-only class (`index.ts:30-44`). No code change; documented divergence.
- **`BamlPanic` not represented on the Rust side** — `bridge_python` raises a `BamlPanic` for SDK-setup-failure call sites (handle-returning sites like `initialize_runtime` / `get_runtime`), built via the pure-Python `make_sdk_panic` (`bridge_python/src/errors.rs`, `baml_core/__init__.py:25`). The Node `errors.rs` only produces `BamlError: BamlX:`-prefixed napi errors; there is no `BamlPanic` JS class. Phase 1.5 decides whether to add `BamlPanic` now (recommended for parity, since `getRuntime` failures are SDK-setup panics) or defer.

#### TypeScript side gaps

- **No `BamlStream` class** — Python's is pure-Python at `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/_stream.py` (the Python SDK source lives under `sdks/python/src/baml_core/`, **not** under a `bridge_python/python_src/` tree — that path does not exist). The Node analog goes at `typescript_src/stream.ts`. Surface: `next()` / `nextAsync()` / `final()` / `finalAsync()` (JS-idiomatic camelCase for the async pair), plus internal `_fromHandle(handle)` factory and `_toHandle()` accessor. The wrapped handle's table row is `Adt(TaggedHeapHandle{ ty, .. })` → `ADT_TAGGED_HEAP_HANDLE` (proto tag 14). Generic over `BamlStream<TStream, TFinal>` (params erased at runtime). Phase 1.3.
  - ⚠ Streaming surface note (`00a`): only **async** streaming is a real call — codegen emits `_stream_async`. The reserved sync name `b.extract_resume_stream()` is *not* a real call; if generated it must throw a clear runtime error, and is preferably omitted from the type surface. This is a codegen (Phase 2/4) concern, but the runtime `BamlStream` wrapper underpins it: `next`/`final` are the per-chunk pulls, distinct from the function-level stream-vs-sync naming. The wrapper class itself exposes both sync and async pulls (`next`/`nextAsync`) as Python does.
- **Exit-hook double-registration** — registered in `index.ts:128` and in the `CtxManager` constructor (`ctx_manager.ts:23-28`). Both guard against duplicates (`process.once` / an `exitHookInstalled` flag), so this is harmless. Optional 1.5 cleanup: consolidate into one `exit_hook.ts` helper.
- **New handle/media/stream symbols not re-exported** — once added, `index.ts` must re-export `takeHandleFromTable` / `putHandleIntoTable` / `_seedFunctionRefHandle` / `_seedGenericMediaHandle`, the four media classes, and `BamlStream`. `proto.ts` decode currently constructs `new BamlHandle(...)` directly (`proto.ts:268`); 1.3 adds media-kind dispatch so a media `handle_value` decodes to the right wrapper.
- **No `BamlTypeMap` / typemap** — Python has `typemap.py`; Node's equivalent is Phase 2 codegen scaffolding (the `sdkgen_nodejs` emitter is still a stub). Phase 1 does not add it and `index.ts` does not import it.

#### Test-coverage gaps

- **No media tests** (Python has `tests/test_media.py`). Add `tests/test_media.test.ts` — constructors round-trip through accessors.
- **No decode-handle tests** (Python has `tests/test_decode_handle.py`). Add `tests/test_decode_handle.test.ts` exercising `_seedFunctionRefHandle` / `_seedGenericMediaHandle` + `takeHandleFromTable` + the media-kind decode dispatch.
- **No BamlStream tests** — add `tests/test_stream.test.ts`. End-to-end `next`/`final` needs a streaming BAML fixture and is deferred to a later phase; the Phase 1 unit test validates only the wrapper-class structure and `_toHandle` round-trip through `encodeCallArgs`.

## What We're NOT Doing

These belong to Phase 2+ (phase numbering follows `00b-overview.md`). The `sdkgen_nodejs` emitter is a `unimplemented!` stub today (`sdks/nodejs/sdkgen_nodejs/src/lib.rs:18-26`); none of the generated-SDK surface exists yet.

- **Phase 2 (codegen scaffolding):** the `sdkgen_nodejs::to_source_code` body, generated `.ts`/`.d.ts` files, the `baml_sdk` package/file layout, the typemap, `_inlinedbaml`, the `defineFunction` / `defineInstanceFunction` factory *implementations* in the runtime package, and the codegen-side emission of import statements (`import { defineFunction } from "@boundaryml/baml-core-node"`). The package *name* is settled (`@boundaryml/baml-core-node`, Phase 1.0) and the runtime exposes the media/`Stream` value classes under their `Baml*` names; Phase 2/4 generated code re-binds `BamlImage as Image` (etc.) on re-export (see the locked note in Overview).
- **Phase 3 (`translate_ty`):** any BAML→TS type expression translation (the `Ty` conversion table in `00a`).
- **Phase 4 (filled-in codegen):** generated class/enum/type-alias bodies, stream-companion type bodies under `stream_types`, function bindings (sync + `_async`, `_stream_async`, `__build_request` etc.), docstrings, and the codegen-side stdlib re-export of the media classes + `Stream` (which IS the runtime class — see `10a-todo-items.md` B3; note that doc describes the *codegen* fix and references files like `sdkgen_nodejs/src/leaf.rs` and `typemap.ts` that do **not exist yet** — they are forward-looking).
- **Phase 5 (proto encode/decode for codegen):** outbound class-value decoding routed via the typemap, inbound encoding of codegen-emitted user classes / streams / generics. The runtime `proto.ts` stays as-is in Phase 1 apart from the small media-wrapper decode dispatch added in 1.3.
- **Phase 6 (release pipeline):** `.github/workflows/release-sdk.yaml` integration, prebuild matrix, npm publish.

## Implementation Approach

We split Phase 1 into sub-phases 1.0 through 1.5. Each is independently testable and commit-worthy. The order is chosen so the dependency chain runs forward:

- **Phase 1.0** (package rename to `@boundaryml/baml-core-node`) is a one-line `package.json` change with no code dependency; do it first so every later sub-phase's docs/tests reference the canonical name.
- **Phase 1.1** (handle infrastructure) is foundational: media classes (1.2) and stream (1.3) both depend on it.
- **Phase 1.2** (media classes) and **Phase 1.3** (stream) can be parallelized but we run media first because tests are simpler.
- **Phase 1.4** (global runtime + proto-handle integration) glues things together.
- **Phase 1.5** (error/ctx-manager polish) finishes parity and locks in the test target.

Use existing tests as the red-green anchor. For each sub-phase: identify failing tests (red), implement (green), commit.

---

## Phase 1.0: Rename the runtime package to `@boundaryml/baml-core-node`

### Goal

Rename the published npm package from `@boundaryml/baml-node` to `@boundaryml/baml-core-node` so Phase 2 generated code can import runtime symbols (`defineFunction`, `defineInstanceFunction`, `initializeRuntime`, the media classes, `Stream`) from the canonical name the spec docs settled on. This is a package-identity change only; the napi `binaryName` and the `.node` artifact name stay `baml_node`.

### Changes Required

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/package.json`**

```diff
- "name": "@boundaryml/baml-node",
+ "name": "@boundaryml/baml-core-node",
```

Leave `"napi": { "binaryName": "baml_node", ... }` (`package.json:6-7`) unchanged — renaming the native artifact is a separate, larger change (prebuild matrix, loader paths) and is not required for the import-name decision.

**Audit for the old name elsewhere.** Grep the tree for `@boundaryml/baml-node` and update any references that should follow the npm package (workspace manifests, codegen import strings already written, docs):

```
rg -n "@boundaryml/baml-node" baml_language/
```

Known stale reference: `10a-todo-items.md` B3 still shows `import { BamlImage } from "@boundaryml/baml-node"` — that doc is forward-looking codegen guidance and should be read as `@boundaryml/baml-core-node`. Do not chase references inside generated-SDK fixtures that don't exist yet (Phase 2+).

### Success Criteria

**Automated:**
- `pnpm install && pnpm build:debug && pnpm test` still passes after the rename (the bridge's own tests import from relative paths / `./native`, not the package name, so they are unaffected).

**Manual:**
- `rg '@boundaryml/baml-node' baml_language/` returns no live references that should have moved to `@boundaryml/baml-core-node`.

---

## Phase 1.1: Handle Table Integration

### Goal

Add the missing handle-table helpers — module-level `takeHandleFromTable` / `putHandleIntoTable` / `_seedFunctionRefHandle` / `_seedGenericMediaHandle` napi free functions — and re-export them from TypeScript. This unblocks media (1.2), stream (1.3), and decode tests. (The `BamlHandle` class lifecycle — clone, finalize-release — is already done in `src/handle.rs`; this sub-phase only adds the free functions.)

### Changes Required

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/src/handle.rs`**

Append the free functions (mirrors `bridge_python/src/py_handle.rs:81-134`). Note: `BamlHandle`'s fields are **private** (`key: u64`, `handle_type: i32`), so these helpers must construct it via a `pub(crate)` constructor rather than a struct literal. Add `pub(crate) fn from_parts(key: u64, handle_type: i32) -> Self` and `pub(crate) fn key_u64(&self) -> u64` to the existing `impl BamlHandle` first (these are also needed by media in 1.2). Then:

```rust
use bridge_ctypes::CffiHandleTableEntry;
use bex_project::{BexExternalAdt, MediaKind, MediaValue};

/// Validate that `key` exists in `HANDLE_TABLE`, then wrap as a `BamlHandle`.
/// Used by the proto decoder's handle path. Does **not** drain —
/// the entry stays in the table and is owned by the returned `BamlHandle`.
#[napi]
pub fn take_handle_from_table(key: HandleKey, handle_type: i32) -> napi::Result<BamlHandle> {
    let key_u64 = key.to_u64();
    if HANDLE_TABLE.resolve(key_u64).is_none() {
        return Err(napi::Error::new(
            napi::Status::GenericFailure,
            format!("BAML handle key {key_u64} is not in HANDLE_TABLE"),
        ));
    }
    Ok(BamlHandle::from_parts(key_u64, handle_type))
}

/// Allocate a fresh `HANDLE_TABLE` row sharing the same `Arc` as `handle`.
/// Returns the new key so the caller can stage a wire `BamlHandle`.
#[napi]
pub fn put_handle_into_table(handle: &BamlHandle) -> napi::Result<HandleKey> {
    let new_key = HANDLE_TABLE.clone_handle(handle.key_u64()).ok_or_else(|| {
        napi::Error::new(
            napi::Status::GenericFailure,
            format!("BamlHandle key {} is not in HANDLE_TABLE", handle.key_u64()),
        )
    })?;
    Ok(HandleKey::from_u64(new_key))
}

/// Test-only: seed a `FunctionRef` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedFunctionRefHandle")]
pub fn seed_function_ref_handle(global_index: u32) -> (HandleKey, i32) {
    let entry = CffiHandleTableEntry::FunctionRef { global_index: global_index as usize };
    let ht = entry.handle_type();
    let key = HANDLE_TABLE.insert(entry);
    (HandleKey::from_u64(key), ht as i32)
}

/// Test-only: seed an `Adt(Media(generic))` entry into `HANDLE_TABLE`.
#[napi(js_name = "_seedGenericMediaHandle")]
pub fn seed_generic_media_handle() -> (HandleKey, i32) {
    let media = MediaValue::from_url(MediaKind::Generic, "https://example.com/", None);
    let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(media));
    let ht = entry.handle_type();
    let key = HANDLE_TABLE.insert(entry);
    (HandleKey::from_u64(key), ht as i32)
}
```

Tuple returns work in napi-rs but are surfaced as `[HandleKey, number]`. If tuple support is awkward, refactor to return a `#[napi(object)] struct SeedResult { key: HandleKey, handleType: i32 }`.

**File: `native.d.ts` (and `typescript_src/native.d.ts`)**

This file is auto-generated by `pnpm build:napi-debug`, which writes `./native.d.ts` and then `cp`s it to `typescript_src/native.d.ts` (`package.json:21`). Both checked-in copies regenerate on `pnpm build:debug`; no manual edits.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`**

Add to the `export { … } from './native'` block:

```ts
export {
    takeHandleFromTable,
    putHandleIntoTable,
    _seedFunctionRefHandle,
    _seedGenericMediaHandle,
} from './native';
```

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_decode_handle.test.ts` (new)**

Mirror `bridge_python/tests/test_decode_handle.py:1-40`. Two tests:

```ts
import { BamlHandle, _seedFunctionRefHandle, _seedGenericMediaHandle, takeHandleFromTable } from '../index';

describe('handle table dispatch', () => {
    test('function ref handle round-trips', () => {
        const [key, ht] = _seedFunctionRefHandle(123);
        const h = takeHandleFromTable(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
        expect(h.handleType).toBe(ht);
    });

    test('generic media handle round-trips', () => {
        const [key, ht] = _seedGenericMediaHandle();
        const h = takeHandleFromTable(key, ht);
        expect(h).toBeInstanceOf(BamlHandle);
    });
});
```

### Success Criteria

**Automated:**
- `pnpm build:debug && pnpm test` passes including `test_decode_handle.test.ts`.
- All existing tests still pass.

**Manual:**
- `native.d.ts` after `pnpm build:napi-debug` includes the four new function declarations.
- Code-review the napi `handle.rs` against `bridge_python/src/py_handle.rs`; confirm semantics match (release-on-finalize, validate-on-take, clone-on-put).

---

## Phase 1.2: Media Classes (BamlImage / BamlAudio / BamlVideo / BamlPdf)

### Goal

Add four napi classes mirroring `bridge_python/src/media.rs`. Each holds a `BamlHandle` (via `napi::bindgen_prelude::Reference<BamlHandle>` or a `u64`+ht pair stored inline — see assumption below) and exposes `fromUrl` / `fromFile` / `fromBase64` static constructors plus `url()` / `file()` / `base64()` / `mimeType()` accessors. Each also exposes internal `_fromHandle(BamlHandle)` and `_toHandle()` for proto encode/decode.

**Assumption** (documented in `media.rs` header): the four media classes store their `BamlHandle` by-value as a `napi::Reference<BamlHandle>` (napi-rs's analog of pyo3's `Py<BamlPyHandle>`). If `Reference` proves awkward, fall back to storing the raw `(key: u64, handle_type: i32)` inline and re-constructing the `BamlHandle` JS class on `_toHandle()` calls — the wire shape is the same and the `HANDLE_TABLE` row ownership is preserved either way as long as `Drop` releases. Default: store the raw pair inline (simpler, no napi::Reference lifetime complications).

### Changes Required

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/src/media.rs` (new)**

Translate the `define_media_pyclass!` macro from `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/media.rs:47-175` (four invocations at `:185-200`, `register` at `:204-207`) to napi. Four media classes via a `define_media_napi_class!` macro that expands to a `#[napi]` struct + `#[napi]` impl block. Skip the `__get_pydantic_core_schema__` hook (no JS equivalent — TypeScript has structural typing).

Naming divergence: Python names the internal factory/accessor `_from_pyhandle` / `_to_pyhandle`; Node uses `_fromHandle` / `_toHandle` (no "py", camelCase). These are decode/encode internals (`00a` BexExternalValue table: media travels the **handle** path — `handle_value` with `ADT_MEDIA_*` — not `class_value`, on the default FFI path), so the runtime decoder dispatches on `handle_type` to pick the wrapper rather than reconstructing a class from fields.

Skeleton:

```rust
//! napi types for BAML media (`baml.media.{Image,Video,Audio,Pdf}`).
//! Mirrors bridge_python/src/media.rs.

use bex_project::{BexExternalAdt, MediaKind, MediaValue};
use bridge_ctypes::{CffiHandleTableEntry, HANDLE_TABLE, baml_core::cffi::BamlHandleType};
use napi::bindgen_prelude::*;
use napi_derive::napi;
use std::sync::Arc;

use crate::handle::{BamlHandle, HandleKey};

macro_rules! define_media_napi_class {
    ($name:ident, $kind:expr, $expected_ht:expr) => {
        #[napi]
        pub struct $name {
            key: u64,
            handle_type: i32,
        }

        #[napi]
        impl $name {
            #[napi(factory, js_name = "fromUrl")]
            pub fn from_url(url: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_url($kind, &url, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self { key, handle_type: $expected_ht as i32 }
            }

            #[napi(factory, js_name = "fromFile")]
            pub fn from_file(file: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_file($kind, &file, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self { key, handle_type: $expected_ht as i32 }
            }

            #[napi(factory, js_name = "fromBase64")]
            pub fn from_base64(base64: String, mime_type: Option<String>) -> Self {
                let inner = MediaValue::from_base64($kind, &base64, mime_type.as_deref());
                let entry = CffiHandleTableEntry::Adt(BexExternalAdt::Media(inner));
                let key = HANDLE_TABLE.insert(entry);
                Self { key, handle_type: $expected_ht as i32 }
            }

            #[napi]
            pub fn url(&self) -> napi::Result<Option<String>> { Ok(self.media_arc()?.url()) }
            #[napi]
            pub fn file(&self) -> napi::Result<Option<String>> { Ok(self.media_arc()?.file()) }
            #[napi]
            pub fn base64(&self) -> napi::Result<String> { Ok(self.media_arc()?.base64()) }
            #[napi(js_name = "mimeType")]
            pub fn mime_type(&self) -> napi::Result<Option<String>> { Ok(self.media_arc()?.mime_type()) }

            /// Internal: build from an existing BamlHandle. Used by proto decode.
            #[napi(factory, js_name = "_fromHandle")]
            pub fn from_handle(handle: &BamlHandle) -> napi::Result<Self> {
                if handle.handle_type() != $expected_ht as i32 {
                    return Err(napi::Error::new(napi::Status::InvalidArg, format!(
                        "BamlHandle.handleType is {}, expected {} for {}",
                        handle.handle_type(), $expected_ht as i32, stringify!($name),
                    )));
                }
                // Clone the table row so the input handle stays usable.
                let new_key = HANDLE_TABLE.clone_handle(handle.key_u64()).ok_or_else(|| {
                    napi::Error::new(napi::Status::GenericFailure, "media handle key no longer valid")
                })?;
                Ok(Self { key: new_key, handle_type: $expected_ht as i32 })
            }

            /// Internal: produce a fresh BamlHandle pointing at the same table row.
            #[napi(js_name = "_toHandle")]
            pub fn to_handle(&self) -> napi::Result<BamlHandle> {
                let new_key = HANDLE_TABLE.clone_handle(self.key).ok_or_else(|| {
                    napi::Error::new(napi::Status::GenericFailure, "media handle key no longer valid")
                })?;
                Ok(BamlHandle::from_parts(new_key, self.handle_type))
            }
        }

        impl $name {
            fn media_arc(&self) -> napi::Result<Arc<MediaValue>> {
                let entry = HANDLE_TABLE.resolve(self.key).ok_or_else(|| {
                    napi::Error::new(napi::Status::GenericFailure,
                        format!("media handle key {} no longer in HANDLE_TABLE", self.key))
                })?;
                match &*entry {
                    CffiHandleTableEntry::Adt(BexExternalAdt::Media(arc)) if arc.kind == $kind => Ok(Arc::clone(arc)),
                    _ => Err(napi::Error::new(napi::Status::GenericFailure,
                        "media handle no longer points to a media value of the expected kind")),
                }
            }
        }

        impl ObjectFinalize for $name {
            fn finalize(self, _env: Env) -> napi::Result<()> {
                HANDLE_TABLE.release(self.key);
                Ok(())
            }
        }
    };
}

define_media_napi_class!(BamlImage, MediaKind::Image, BamlHandleType::AdtMediaImage as u64);
define_media_napi_class!(BamlAudio, MediaKind::Audio, BamlHandleType::AdtMediaAudio as u64);
define_media_napi_class!(BamlVideo, MediaKind::Video, BamlHandleType::AdtMediaVideo as u64);
define_media_napi_class!(BamlPdf,   MediaKind::Pdf,   BamlHandleType::AdtMediaPdf   as u64);
```

Note: `#[napi(custom_finalize)]` may be required on the struct definitions for `ObjectFinalize` to be honored; check existing `handle.rs:41` usage. Add the attribute if needed.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/src/handle.rs`**

The `pub(crate) fn from_parts(...)` and `pub(crate) fn key_u64(&self)` helpers needed here were already added in Phase 1.1. No further `handle.rs` change in 1.2.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/src/lib.rs`**

Add `pub mod media;` to the module list (`lib.rs:6-11` currently declares `abort_controller`, `errors`, `handle`, `host_value`, `runtime`, `types`). napi-derive registers the classes via inventory on module init — no manual `m.add_class` needed. Verify by reading the regenerated `native.d.ts`.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`**

Re-export the media classes under their `Baml*` names only. The runtime does **not** export bare `Image`/`Audio`/`Video`/`Pdf` aliases — codegen does the aliasing on re-export (`export { BamlImage as Image } from "@boundaryml/baml-core-node"`, Python-style), so the public `baml_sdk.baml.media.Image` resolves there, not here (`00a-spec…:68`, `00a-example…:531-573`):

```ts
export { BamlImage, BamlAudio, BamlVideo, BamlPdf } from './native';
```

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_media.test.ts` (new)**

Mirror `/Users/sam/baml3/baml_language/sdks/python/tests/test_media.py`. Tests for each of the four classes:

```ts
import { BamlImage, BamlAudio, BamlVideo, BamlPdf } from '../index';

describe('BamlImage', () => {
    test('fromUrl', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png');
        expect(img.url()).toBe('https://example.com/cat.png');
        expect(img.file()).toBeNull();
        expect(img.mimeType()).toBeNull();
    });
    test('fromUrl with mime', () => {
        const img = BamlImage.fromUrl('https://example.com/cat.png', 'image/png');
        expect(img.mimeType()).toBe('image/png');
    });
    test('fromFile', () => {
        const img = BamlImage.fromFile('/tmp/cat.png');
        expect(img.file()).toBe('/tmp/cat.png');
        expect(img.url()).toBeNull();
    });
    test('fromBase64', () => {
        const img = BamlImage.fromBase64('aGVsbG8=');
        expect(img.base64()).toBe('aGVsbG8=');
    });
});

// Repeat for BamlAudio, BamlVideo, BamlPdf (same shape, different fixtures).
```

### Success Criteria

**Automated:**
- `pnpm test test_media.test.ts` passes all sixteen-ish cases (4 kinds × 4 constructors).
- All existing tests still pass.

**Manual:**
- `native.d.ts` contains four new declared classes with the right method shapes.
- `BamlImage.fromUrl(...)` is constructible from a Jest test without throwing on missing peer dependencies.

---

## Phase 1.3: BamlStream

### Goal

Add a pure-TypeScript `BamlStream<TStream, TFinal>` class at `bridge_nodejs/typescript_src/stream.ts`, mirroring `sdks/python/src/baml_core/_stream.py`. The class wraps a `BamlHandle` whose table row is a `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ... })` (allocated by the engine when a streaming function returns). Surface: `next()` / `nextAsync()` / `final()` / `finalAsync()` round-trip via `getRuntime()` and the FQNs `baml.llm.Stream.next` / `baml.llm.Stream.final`. Re-export from the runtime `index.ts` under its `BamlStream` name only; codegen aliases it as `Stream` on re-export (`00a-example…:576-592`).

⚠ **Spec note — function-level streaming is async-only.** This `BamlStream` wrapper's `next`/`final` are per-chunk pulls and are unrelated to the function-level stream-vs-sync distinction. At the codegen layer (Phase 2/4), only `_stream_async` is a real call; the reserved sync `_stream` name must throw a clear runtime error or be omitted from the type surface (`00a-spec-codegen-mappings.md:80-81`, `00b-overview.md:15`). Phase 1 only ships the wrapper; it does not emit or reserve the function-level stream names.

### Changes Required

**Dependency:** Phase 1.4's `getRuntime()` must exist OR the stream class can take the runtime as a constructor arg for Phase 1. Assumption (documented): make `BamlStream` construction take the runtime explicitly in Phase 1 and update it to use `getRuntime()` once Phase 1.4 lands. This avoids cross-dependency between 1.3 and 1.4.

Revised: actually, run 1.4 *before* 1.3 because the prior-art design from `_stream.py` strongly assumes a `getRuntime()`. Mark this in the sub-phase order.

(Reordering: Phase 1.1 → Phase 1.4 → Phase 1.2 → Phase 1.3 → Phase 1.5. We keep the section numbers but execute in that order.)

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/stream.ts` (new)**

```ts
// stream.ts — pure-TS analog of sdks/python/src/baml_core/_stream.py.
//
// BamlStream wraps a BamlHandle whose HANDLE_TABLE row is a
// `CffiHandleTableEntry::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle })`
// (handle_type ADT_TAGGED_HEAP_HANDLE). next/final round-trip through
// getRuntime().callFunction* against the well-known FQNs
// `baml.llm.Stream.next` and `baml.llm.Stream.final`.

import { BamlHandle, getRuntime } from './native';   // getRuntime added in Phase 1.4
import { encodeCallArgs, decodeCallResult } from './proto';

const STREAM_NEXT_FN = 'baml.llm.Stream.next';
const STREAM_FINAL_FN = 'baml.llm.Stream.final';

export class BamlStream<TStream, TFinal> {
    private _handle: BamlHandle;

    constructor(handle: BamlHandle) {
        this._handle = handle;
    }

    /** Internal: produce a fresh BamlStream from a BamlHandle. Used by proto decode. */
    static _fromHandle<TStream, TFinal>(handle: BamlHandle): BamlStream<TStream, TFinal> {
        return new BamlStream<TStream, TFinal>(handle);
    }

    /** Internal: expose the inner BamlHandle for inbound encode. */
    _toHandle(): BamlHandle {
        return this._handle;
    }

    next(): TStream {
        return this._callSync(STREAM_NEXT_FN) as TStream;
    }
    async nextAsync(): Promise<TStream> {
        return (await this._callAsync(STREAM_NEXT_FN)) as TStream;
    }
    final(): TFinal {
        return this._callSync(STREAM_FINAL_FN) as TFinal;
    }
    async finalAsync(): Promise<TFinal> {
        return (await this._callAsync(STREAM_FINAL_FN)) as TFinal;
    }

    private _callSync(fqn: string): unknown {
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = rt.callFunctionSync(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
    private async _callAsync(fqn: string): Promise<unknown> {
        const rt = getRuntime();
        const argsProto = encodeCallArgs({ self: this });
        const resultBytes = await rt.callFunction(fqn, argsProto, null, null, null);
        return decodeCallResult(resultBytes);
    }
}
```

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/proto.ts`**

Extend `setInboundValue` to recognize `BamlStream` and the media classes (each calls `value._toHandle()` → emits a `handle` like the existing `BamlHandle` branch at `proto.ts:75-90`). Mirror the dispatch order from `sdks/python/src/baml_core/proto.py` `_set_inbound_value` (null, bool, number, string, Uint8Array, BamlHandle, BamlStream, media types, then objects):

```ts
// after the `value instanceof BamlHandle` branch (proto.ts:75-90):
} else if (value instanceof BamlStream) {
    const h = value._toHandle();
    iv.handle = { key: h.key, handleType: h.handleType };
} else if (value instanceof BamlImage || value instanceof BamlAudio
        || value instanceof BamlVideo || value instanceof BamlPdf) {
    const h = value._toHandle();
    iv.handle = { key: h.key, handleType: h.handleType };
```

(Imports for `BamlStream`, `BamlImage`, etc. added at the top of `proto.ts`.)

For decode: add a dispatch on `handle_type` in `decodeValueHolder` for media kinds, at the existing `handleValue` branch (`proto.ts:267-268`, which today does `return new BamlHandle(holder.handleValue.key, holder.handleValue.handleType ?? 0)`), mirroring Python's `_decode_handle`. Stream / `ADT_TAGGED_HEAP_HANDLE` decode needs the typemap (Phase 5) — leave it falling through to bare `BamlHandle` for now and document this. Decode media:

Decoder obligations per the authoritative `BexExternalValue conversions` table (`00a-spec-codegen-mappings.md:222-256`; the parallel Python table is `00a-prior-art-python-type-mappings.md:162-196`):
- `handle_value` dispatches on `handle_type`: media handles (`ADT_MEDIA_IMAGE/AUDIO/VIDEO/PDF`) decode to the typed wrapper; `FUNCTION_REF`, `ADT_COLLECTOR`, `ADT_TYPE`, `ADT_PROMPT_AST`, `UNTAGGED_RUST_DATA`, etc. fall through to a bare `BamlHandle`.
- `HANDLE_UNSPECIFIED` (`= 0`) must be **rejected** — it is never a valid decoded handle (Python's `_decode_handle` raises on it).
- Inline `media_value` and `prompt_ast_value` proto fields must be **rejected** by default on the Node FFI path — media/prompt AST are expected via `handle_value`, not inline (the encoder ships them as handles unless `serialize_media` / `serialize_prompt_ast` are opted in, which the default FFI path does not).

```ts
// inside decodeValueHolder, where it currently returns `new BamlHandle(holder.handleValue.key, ...)`:
const ht = holder.handleValue.handleType ?? 0;
if (ht === BamlHandleType.HANDLE_UNSPECIFIED) {
    throw new BamlError('decoded handle has HANDLE_UNSPECIFIED handle_type');
}
const handle = new BamlHandle(holder.handleValue.key, ht);
if (ht === BamlHandleType.ADT_MEDIA_IMAGE) return BamlImage._fromHandle(handle);
if (ht === BamlHandleType.ADT_MEDIA_AUDIO) return BamlAudio._fromHandle(handle);
if (ht === BamlHandleType.ADT_MEDIA_VIDEO) return BamlVideo._fromHandle(handle);
if (ht === BamlHandleType.ADT_MEDIA_PDF) return BamlPdf._fromHandle(handle);
// TODO Phase 5: ADT_TAGGED_HEAP_HANDLE dispatch via BamlTypeMap → BamlStream / user classes.
return handle;
```

(`ADT_MEDIA_GENERIC` has no typed wrapper today — per the `Ty::Media(Generic)` row it stays a bare `BamlHandle` / `unknown`; do not route it to a media class.) The inline-`media_value` / `prompt_ast_value` rejection belongs wherever `decodeValueHolder` switches on the `BamlOutboundValue.value` oneof; add explicit `throw new BamlError(...)` arms for those two fields if they aren't already unreachable.

`BamlHandleType` is the proto-generated enum; it is **already imported** in `proto.ts:16` (`const BamlHandleType = baml_core.cffi.v1.BamlHandleType`), so no new import is needed for the decode dispatch.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`**

```ts
export { BamlStream } from './stream';   // runtime exports the `Baml*` name only; codegen aliases it as `Stream` on re-export
```

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_stream.test.ts` (new)**

Unit-level test that constructs a `BamlStream` from a seeded TaggedHeapHandle and exercises `_toHandle()` round-trip via the proto encoder. End-to-end `next`/`final` testing requires a streaming BAML fixture function; assumption: skip the e2e test in Phase 1 (engine streaming setup is out of scope for the bridge layer) and add a TODO marker referencing Phase 4 for end-to-end. The unit test does:

```ts
import { BamlStream, BamlHandle, _seedFunctionRefHandle, encodeCallArgs } from '../index';

describe('BamlStream', () => {
    test('_toHandle round-trips through encodeCallArgs', () => {
        // Seed any handle (function ref is fine for the encoder test).
        const [key, ht] = _seedFunctionRefHandle(42);
        const h = new BamlHandle(key, ht);
        const stream = BamlStream._fromHandle<number, string>(h);
        // Verify the inner handle is reachable.
        const innerH = stream._toHandle();
        expect(innerH).toBeInstanceOf(BamlHandle);
        // Encoding should not throw on a stream-typed value.
        expect(() => encodeCallArgs({ self: stream })).not.toThrow();
    });
});
```

Assumption (documented): the engine's `TaggedHeapHandle` decoder dispatch is in Phase 5; Phase 1's stream test only validates the wrapper class structure.

### Success Criteria

**Automated:**
- `pnpm test test_stream.test.ts` passes.
- All other tests still pass.

**Manual:**
- `BamlStream` is type-parameterized correctly: `BamlStream<{ partial: string }, { final: string }>` compiles cleanly under `tsc`.
- `proto.ts` decode of a media `handle_value` produces an instance of the right media class (verifiable by extending `test_decode_handle.test.ts` with a media seed).

---

## Phase 1.4: Global Runtime Singleton (getRuntime + initializeRuntime)

### Goal

Expose the process-global runtime singleton that `bridge_cffi` already maintains (`bridge_cffi::initialize_runtime` stores it; `bridge_cffi::get_runtime()` fetches it). The Node `BamlRuntime` is already a **zero-sized handle** (`src/runtime.rs:22-23`, `struct BamlRuntime {}`) whose call paths fetch the singleton per call. So the only new surface is a `getRuntime()` free function returning a fresh zero-sized handle, plus renaming/aliasing the constructor to `initializeRuntime`. Codegen-emitted callsites (Phase 2+) and `BamlStream` (1.3) call `getRuntime()` to avoid threading a runtime reference everywhere.

**Order note:** in execution order, run Phase 1.4 *before* Phase 1.3 because the stream depends on `getRuntime()`.

⚠ **Spec/parity discrepancy — `fromFiles` vs `initializeRuntime`.** The original plan said the two "coexist as aliases", but `bridge_python` *renamed* the constructor: there is only `BamlRuntime.initialize_runtime` (a `#[staticmethod]`), no `from_files` (`bridge_python/src/runtime.rs:44-57`). The Node side today exposes only `BamlRuntime.fromFiles` (`src/runtime.rs:28-39`) and existing tests (`test_engine.test.ts`, `call_function.test.ts`) call `fromFiles`. Decision for this plan: **rename** the napi factory to `initializeRuntime` for parity with Python and with `00a`'s `initializeRuntime(...)` import, and update the existing test call sites in the same change. If keeping `fromFiles` as a back-compat alias is preferred, document it explicitly — but do not silently keep both as the "canonical" entrypoint. Whichever is chosen, the `getRuntime()` accessor below is unchanged.

### Changes Required

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/src/runtime.rs`**

Rename the existing `from_files` factory to `initialize_runtime` (napi `initializeRuntime`) and add a module-level `get_runtime()` free function. Because `BamlRuntime` is zero-sized and carries no `bex`, the accessor just validates the singleton and returns a fresh `BamlRuntime {}` (exactly like `bridge_python/src/runtime.rs:243-258`):

```rust
#[napi]
impl BamlRuntime {
    /// Initialize the process-global runtime from in-memory BAML files.
    /// `bridge_cffi::initialize_runtime` is a single-slot singleton, so a
    /// second call replaces the prior runtime; the result is reachable via
    /// getRuntime().
    #[napi(factory, js_name = "initializeRuntime")]
    pub fn initialize_runtime(
        root_path: String,
        files: std::collections::HashMap<String, String>,
    ) -> napi::Result<Self> {
        match bridge_cffi::initialize_runtime(&root_path, files) {
            Ok(_bex) => Ok(BamlRuntime {}),
            Err(e) => Err(bridge_error_to_napi(e)),
        }
    }
    // ... existing call_function_sync / call_function stay unchanged ...
}

/// Return the process-global BamlRuntime, or a BamlError-shaped napi::Error
/// if initializeRuntime has not run yet. The handle is zero-sized; the Arc
/// lives in bridge_cffi.
#[napi(js_name = "getRuntime")]
pub fn get_runtime() -> napi::Result<BamlRuntime> {
    bridge_cffi::get_runtime().map_err(|e| match e {
        bridge_cffi::BridgeError::NotInitialized => napi::Error::new(
            napi::Status::GenericFailure,
            "BamlError: BAML runtime has not been initialized — call BamlRuntime.initializeRuntime first.",
        ),
        other => bridge_error_to_napi(other),
    })?;
    Ok(BamlRuntime {})
}
```

(Note: `bridge_cffi::get_runtime()` is the real API — not `bridge_cffi::engine::get_runtime()`. Phase 1.5 may instead route the `NotInitialized` case through a `BamlPanic`, matching `bridge_python`, which raises an SDK-setup panic at this site via `py_sdk_panic`.)

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/index.ts`**

```ts
// Add getRuntime to the existing `export { ... } from './native'` block (index.ts:17).
export { BamlRuntime, AbortController, BamlHandle, HostSpanManager, getRuntime, getVersion, flushEvents } from './native';
```

`stream.ts` (1.3) imports `getRuntime` directly from `./native` — no separate `runtime.ts` re-export file is needed.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_engine.test.ts` (+ `call_function.test.ts`)**

If the constructor is renamed, update the existing `BamlRuntime.fromFiles(...)` call sites in both files to `BamlRuntime.initializeRuntime(...)`. Then add a `getRuntime` test:

```ts
test('getRuntime returns initialized runtime', () => {
    BamlRuntime.initializeRuntime('.', { 'main.baml': BAML_SOURCE });
    const rt = getRuntime();
    expect(callFunctionSync(rt, 'ReturnOne', {}).result()).toBe(1);
});
```

(Assumption: the bridge_cffi singleton is set-once-per-process and a second `initializeRuntime` replaces it; a "throws before initialize" test would need process-level isolation, which is out of scope for Phase 1. Skip the negative test or assert only `typeof getRuntime === 'function'`.)

### Success Criteria

**Automated:**
- `pnpm test test_engine.test.ts` passes the new `getRuntime returns initialized runtime` test.
- All other tests still pass.

**Manual:**
- `getRuntime()` symbol appears in `native.d.ts` after `pnpm build:napi-debug`.
- Calling `getRuntime()` before any `initializeRuntime()` throws an error whose message contains `"has not been initialized"`.

---

## Phase 1.5: Error Wrapping, Ctx Manager Cleanup, Exit Hook Consolidation

### Goal

Polish the error surface and lock in the Phase 1 finish line. Scope note: the **BAML function-result** error/panic path is **already done** — `decodeCallResult` decodes the `BamlOutboundResult` envelope and raises a class-name/message/trace error or `process.exit`s on exit-panics (`proto.ts:324-346`). `wrapNativeError` only covers **raw napi errors** from the non-call entry points (arg-decode failure, function-not-found surfaced as `napi::Error` before the envelope exists). The remaining work is: (1) decide whether to add a `BamlPanic` JS class + route `getRuntime`/`initializeRuntime` setup failures through it (parity with `bridge_python`); (2) tighten `wrapNativeError`'s classifier; (3) optionally consolidate the exit hook. Also verify ctx_manager tests match `bridge_python/tests/test_tracing.py`.

### Changes Required

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/exit_hook.ts` (new)**

```ts
// exit_hook.ts — single-registration helper for flushEvents on process exit.
// Both index.ts and CtxManager constructor used to call process.once('exit',…)
// independently. Consolidate to one registration.

import { flushEvents } from './native';

let installed = false;

export function installFlushOnExit(): void {
    if (installed) return;
    installed = true;
    process.once('exit', () => {
        try { flushEvents(); } catch { /* ignore */ }
    });
}
```

**Edits to existing files:**

- `typescript_src/index.ts`: replace the `process.once('exit', …)` block at `index.ts:127-130` with `import { installFlushOnExit } from './exit_hook'; installFlushOnExit();`.
- `typescript_src/ctx_manager.ts`: replace the `if (!exitHookInstalled)` block at `ctx_manager.ts:23-28` (and drop the module-level `exitHookInstalled` flag at `:7`) with `import { installFlushOnExit } from './exit_hook'; installFlushOnExit();`.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/typescript_src/errors.ts`**

Make `wrapNativeError` more precise: split on the prefix `BamlError: BamlInvalidArgumentError:` (etc.) — currently it uses `String.includes(...)` which can match user messages. New form:

```ts
const PREFIX_MAP: Array<[string, new (m: string) => BamlError]> = [
    ['BamlError: BamlCancelledError:', BamlCancelledError],
    ['BamlError: BamlInvalidArgumentError:', BamlInvalidArgumentError],
    ['BamlError: BamlClientError:', BamlClientError],
];

export function wrapNativeError(err: unknown): BamlError {
    if (!(err instanceof Error)) return new BamlError(String(err));
    const msg = err.message;
    for (const [prefix, Ctor] of PREFIX_MAP) {
        if (msg.startsWith(prefix)) return new Ctor(msg);
    }
    if (msg.startsWith('BamlError:')) return new BamlError(msg);
    return new BamlError(msg);
}
```

The existing `callFunctionSync` / `callFunction` helpers in `index.ts:101-106, :119-124` already wrap their native-call try/catch in `wrapNativeError`. The new entry points from this Phase (media constructors, `getRuntime`, `BamlStream` pulls) should each catch-and-wrap the same way, so callers always see `instanceof BamlError`. Constructor wrapping for `initializeRuntime` is **not** required: the existing `function not found` tests assert with `toThrow()` / `rejects.toThrow()` without checking the subclass, and BAML-function failures already arrive structured via the envelope.

**`BamlPanic` parity (decision point).** `bridge_python` has a `BamlPanic` class and raises it at SDK-setup sites (`initialize_runtime`, `get_runtime`) via `make_sdk_panic`; `decodeCallResult`'s `panic` branch is the runtime equivalent for in-call panics, but it currently throws a generic error built by `makeThrownError('panic', …)` (`proto.ts:332-339`) rather than a typed `BamlPanic`. Per `00a`'s "Node error handling" (`00a-spec-codegen-mappings.md:219`: panics raise "the Node runtime's `BamlPanic` equivalent, except for process-exit panics where the runtime intentionally exits after flushing telemetry"), Phase 1.5 should add a `BamlPanic extends BamlError` class in `errors.ts`, have `makeThrownError('panic', …)` and the `getRuntime` `NotInitialized` path construct it, and re-export it from `index.ts`. (If deferred, note it explicitly — but it is small and is the one missing error class versus Python.)

⚠ **Spec note — exit-panic telemetry flush.** `00a` says exit-panics flush telemetry *before* exiting. The current `proto.ts:332-336` exit-panic branch calls `process.exit(code)` directly with **no** explicit `flushEvents()` first; the flush happens via the registered `process.once('exit', flushEvents)` hook (`index.ts:128`), which `process.exit` fires synchronously. This satisfies the contract as long as that hook is installed (it is, and 1.5's `installFlushOnExit` keeps it installed). If the spec intends an explicit flush at the panic site (independent of the exit hook), add a `flushEvents()` call before `process.exit(code)` in the exit-panic branch. The plan's earlier "flush is handled engine-side before the envelope is produced" assertion is unverified against the engine — treat the exit-hook flush as the load-bearing mechanism on the Node side.

**File: `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/tests/test_tracing.test.ts`**

Compare against `bridge_python/tests/test_tracing.py` and add any missing parity tests:

- contextDepth ✓
- enter/exitOk ✓
- deepClone ✓
- upsertTags ✓
- CtxManager.traceFn / traceFnAsync ✓

Likely sufficient. Add one more if Python has it:

```ts
test('upsertTags merges into current span', () => {
    const ctxMgr = new CtxManager(rt);
    const mgr = ctxMgr.cloneContext();
    mgr.enter('test', {});
    expect(() => ctxMgr.upsertTags({ env: 'prod' })).not.toThrow();
    mgr.exitOk();
});
```

### Success Criteria

**Automated:**
- Full `pnpm test` passes (all four existing test files + three new files: `test_decode_handle.test.ts`, `test_media.test.ts`, `test_stream.test.ts`).
- `cargo nextest run --package bridge_nodejs -- --ignored jest` exits 0.

**Manual:**
- `installFlushOnExit` is called exactly once per process (smoke-test by running two `import './index'` lines in a Node REPL and checking that `process.listenerCount('exit')` increases by exactly 1).
- `wrapNativeError` correctly classifies a thrown `BamlError: BamlCancelledError: …` napi message into `BamlCancelledError`.
- A `panic` envelope (and the `getRuntime` `NotInitialized` path, if routed through it) surfaces as `BamlPanic` if that class is added in 1.5.
- Code-review the resulting TS surface against `sdks/python/src/baml_core/__init__.py:235-269` (`__all__`) — every Python `__all__` entry has an equivalent export from `bridge_nodejs/index.ts` except: typemap / `define_function` (Phase 2 codegen), and `BamlPyHandle` (Node names it `BamlHandle`). Confirm `BamlPanic`, `BamlStream`, and the media classes are exported.

---

## Testing Strategy

### Red-green tracking

| Test file | Phase 1 status | Anchor |
|---|---|---|
| `tests/test_engine.test.ts` | already passes; update `fromFiles`→`initializeRuntime` call sites + add `getRuntime` test in 1.4 | green |
| `tests/call_function.test.ts` | already passes; update `fromFiles`→`initializeRuntime` call sites in 1.4 | green (kept passing) |
| `tests/test_collector.test.ts` | already passes | green (kept passing) |
| `tests/test_tracing.test.ts` | already passes; possibly extend in 1.5 | green |
| `tests/host_callable.test.ts` | already passes (host-callable round-trip, already implemented) | green (kept passing) |
| `tests/test_decode_handle.test.ts` | **new** in Phase 1.1 | red → green |
| `tests/test_media.test.ts` | **new** in Phase 1.2 | red → green |
| `tests/test_stream.test.ts` | **new** in Phase 1.3 | red → green |

### Running the test suite

From `/Users/sam/baml3/baml_language/sdks/nodejs/bridge_nodejs/`:

```
pnpm install              # one-time
pnpm build:debug          # rebuilds proto + napi + tsc; takes ~30s after warmup
pnpm test                 # runs Jest
```

Or via cargo nextest:

```
cd /Users/sam/baml3 && cargo nextest run --package bridge_nodejs -- --ignored jest
```

The nextest variant drives `tests/run_jest.rs:8-34` which itself runs `pnpm install && pnpm build:debug && pnpm test`.

### What we explicitly do NOT run in Phase 1

- `cargo nextest run -E 'package(/^sdk_test_nodejs_/)'` — this is the Phase 4 anchor. It will keep failing because no SDK code is generated.
- `pnpm` workspace-wide tests on `sdkgen_nodejs/` (the `sdkgen_nodejs` lib is a panicking stub through Phase 1; Phase 2 fills it in).

### Manual smoke

After Phase 1 lands, the following one-liner should work in a fresh Node REPL inside `bridge_nodejs/`:

```js
const { BamlRuntime, callFunctionSync, BamlImage, BamlStream, BamlError, getRuntime } = require('./');
BamlRuntime.initializeRuntime('.', { 'main.baml': 'function One() -> int { 1 }' });
console.log(callFunctionSync(getRuntime(), 'One', {}).result());  // → 1
console.log(BamlImage.fromUrl('https://example.com/a.png').url()); // → https://example.com/a.png
```

## References

All prior-art file:line pointers (Python). Use these as the source of truth when porting:

- **Crate registration:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/lib.rs:34-56` (`#[pymodule] fn baml_py`).
- **Runtime + call_function:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/runtime.rs:45` (`initialize_runtime` `#[staticmethod]`, call_function, call_function_sync) and `:245` (`get_runtime` free function).
- **BamlPyHandle:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/py_handle.rs` (whole file, ~135 lines). Drop releases at `:70-74`. `__copy__` at `:43-54`. `take_pyhandle_from_table` at `:81-90`. `put_pyhandle_into_table` at `:96-108`. Seed helpers at `:113-134`.
- **Media classes:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/media.rs` (whole file, ~210 lines). Macro `define_media_pyclass!` at `:47-175`. Four invocations at `:185-200`. Register (`m.add_class::<…>()`) at `:204-207`.
- **Errors:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/errors.rs:10-13` (declare via `create_exception!`); `bridge_error_to_py` at `:58-100`; `runtime_error_to_py` at `:103-124`. Python re-export at `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/errors.py:1-18`.
- **AbortController:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/abort_controller.rs` (whole file, 54 lines).
- **Collector + FunctionLog + Timing + Usage + LLMCall:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/types/collector.rs` (302 lines). Python wrapper at `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/__init__.py:50-117`.
- **HostSpanManager:** `/Users/sam/baml3/baml_language/sdks/python/rust/bridge_python/src/types/host_span_manager.rs` (whole file, 131 lines). `py_to_json` helper at `:91-127` (Node skips this — napi auto-converts).
- **BamlStream:** `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/_stream.py` (whole file, 100 lines). Constants at `:26-27`. `_call_sync` / `_call_async` at `:75-91`.
- **CtxManager:** `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/ctx_manager.py` (whole file, 206 lines). `trace_fn` decorator at `:108-172`. `_build_params` helper at `:197-206`.
- **Module `__init__`:** `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/__init__.py:1-269`. `__all__` at `:240-269`. `atexit.register(flush_events)` at `:42`. `call_function_sync` / `call_function` helpers at `:138-151`.
- **Proto encode/decode (for context — Phase 5 will rewrite the Node proto.ts in line with this):** `/Users/sam/baml3/baml_language/sdks/python/src/baml_core/proto.py:75-192` (`_set_inbound_value`), `:213-218` (`encode_call_args`), `:384-426` (`_decode_handle`), `:440-485` (`decode_value`), `:488-492` (`decode_call_result`).
- **BamlHandleType enum (proto):** `/Users/sam/baml3/baml_language/crates/bridge_ctypes/types/baml_core/cffi/v1/baml_inbound.proto:31-46`.

## Assumptions Recap (Decisions Made Without Asking)

1. **`BamlHandle` (Node) plays the role of `BamlPyHandle` (Python).** No separate "PyHandle"-equivalent class in Node — the existing `BamlHandle` already has `clone`, `ObjectFinalize`-release, and the `{low, high}` wire shape. We extend it (Phase 1.1) with module-level `takeHandleFromTable` / `putHandleIntoTable` / seed helpers.
2. **Media classes store `(key: u64, handle_type: i32)` inline**, not `napi::Reference<BamlHandle>`. Simpler lifetime; `Drop`/`ObjectFinalize` on the media class itself releases.
3. **`FunctionResult` stays TS-only**, not a napi class. Trivial value wrapper; no FFI need.
4. **`BamlTypeMap` / `setTypeMap` / `getTypeMap` are Phase 2 work**, not Phase 1. The TS surface in Phase 1 doesn't import them.
5. **`define_function` factory** (Python `_init_.py:199-237`) is Phase 2 work — generated SDK code emits `defineFunction(...)` calls; the helper itself lives in the future generated `baml_sdk/__init__.ts` or a Phase 2 runtime module.
6. **End-to-end stream tests are deferred to Phase 4.** Phase 1's `BamlStream` test verifies only the wrapper class shape + `_toHandle` round-trip via `encodeCallArgs`.
7. **`BamlRuntime.initializeRuntime` is the canonical factory** (renamed from `fromFiles`), matching Python's sole `initialize_runtime` and the `initializeRuntime(...)` import the spec docs use (`00a-example…:89`). Existing `fromFiles` call sites in `test_engine.test.ts` / `call_function.test.ts` are updated in 1.4. A back-compat `fromFiles` alias may be kept but is not the canonical entrypoint; if kept, document it explicitly (see the 1.4 discrepancy note).
8. **Process-global singleton:** Node-side codegen will need `getRuntime()` for the same reason Python does (avoid threading `rt` through every call site). We expose it identically to Python.
9. **Order of Phase 1 sub-phases for execution** is 1.1 → 1.4 → 1.2 → 1.3 → 1.5. Section numbering preserved in this document; the dependency chain runs 1.4 before 1.3 because `BamlStream` consumes `getRuntime()`.
10. **Test isolation around the global singleton** is not addressed in Phase 1. Multiple `initializeRuntime` calls overwrite the singleton; tests rely on the cffi engine's idempotent behavior. If conflicts arise in Phase 4+, a `_resetRuntime()` test helper can be added then.
11. **Code-emoji policy:** keep filenames and inline comments emoji-free.
12. **Exit hook consolidation** (Phase 1.5) is a quality-of-life cleanup, not a correctness fix. The existing dual registration is harmless thanks to `process.once`.
13. **The npm package is renamed to `@boundaryml/baml-core-node`** (Phase 1.0), matching all four `00*` spec docs. The native `binaryName` / `.node` artifact stays `baml_node`; only the npm `"name"` changes. Generated code (Phase 2) imports runtime symbols from `@boundaryml/baml-core-node`.
14. **The runtime exports the stdlib value classes under their `Baml*` names only** (`BamlImage`/`BamlAudio`/`BamlVideo`/`BamlPdf`/`BamlStream`) and does not alias. Phase 2/4 codegen aliases them to the public `Image`/`Audio`/`Video`/`Pdf`/`Stream` on re-export (`export { BamlImage as Image } from "@boundaryml/baml-core-node"`). `defineFunction` and `defineInstanceFunction` are separate named exports of `@boundaryml/baml-core-node`.
