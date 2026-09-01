# BAML/WASM runner spike

Status: browser component and a source-pinned preview artifact are implemented.

## Decision

The documentation portal will run BAML examples in a dedicated Web Worker. One
worker owns one WASM instance per page, and project sessions are cached by a
content-derived key. The page will never fetch or instantiate the runtime during
initial rendering. A runnable listing may warm its project when it approaches
the viewport, and a click must also work without a successful warmup.

The language release pipeline must produce an immutable, versioned runtime
artifact. A docs build consumes that artifact; it must not compile Rust as an
implicit part of every preview deployment. The manifest records at least the
monorepo commit, BAML toolchain version, raw size, compressed size, and artifact
digest.

Every runnable listing is checked twice in CI:

1. Run it with the native CLI used to validate the authored listing.
2. Run it through the exact WASM artifact the preview will serve and compare the
   formatted result.

A version string alone is not an acceptance test.

## Measurements

Measured locally on 2026-08-31 from commit `56e04b6` with
`CARGO_PROFILE_RELEASE_OPT_LEVEL=z`:

| Metric | Result |
| --- | ---: |
| Source build | 56.2 s |
| Raw WASM | 17,823,635 bytes |
| gzip level 9 | 4,598,896 bytes |
| Brotli quality 11 | 2,948,496 bytes |
| Node WASM initialization | 41 ms |
| First project session | 3,959 ms |
| First zero-network run after session creation | 7.5 ms |

These are development-machine measurements, not browser performance claims.
The provisional regression budgets are 5 MB gzip, 250 ms initialization, 5 s
first project readiness, and 100 ms for a repeat zero-network run. Browser p50
and p95 gates must replace the local numbers before launch.

CI recomputes the gzip size of the exact checked-in artifact and enforces the
5 MB download budget. The deployment explicitly serves content-addressed
runtime files with a one-year immutable cache lifetime, while the mutable
manifest and worker entrypoint revalidate on every navigation.

The docs workflow also opens the production build in headless Chromium. Five
isolated browser contexts prove that the WASM is not requested before the
reader selects **Run BAML**, verify the rendered result, and measure cold and
warm click-to-result p50/p95 latency. The local-CI ceilings are deliberately
coarse regression alarms; preview-region browser telemetry must replace them
before launch.

## Protocol finding

The book's BAML 0.17 driver expected a successful result to contain a
`result.valueRef`. BAML 0.18 returned the same `BamlOutboundValue` protobuf
inline as base64 in `result.value`. The run reached `succeeded`, but the old
driver rendered `null`; the native CLI rendered `"world"`.

`lib/baml-runner/result.mjs` supports both protocol forms and fails on unknown
renderers. Its tests use the payload captured from the current runtime. This
adapter belongs in the future shared WASM-runner package rather than being
copied into every consumer.

## Reliability requirements

- Terminate and recreate the worker after a WASM panic or worker-level error.
- Time out a run and ask the runtime to cancel it instead of leaking work.
- Cache rejected session promises only for the duration of the attempt so the
  next click can retry.
- Keep outbound network and host capabilities disabled for Stage 1 examples.
- Serve the WASM from an immutable, content-hashed URL with Brotli/gzip and
  long-lived caching.
- Report load, initialization, session, and execution timings separately.
- Fail CI when the runtime manifest, compiler, grammar, or runnable listings
  describe incompatible versions.

## Implemented vertical slice

The portal ships one content-addressed runtime artifact and a worker bundle. The
`BamlRunner` MDX component warms a project near the viewport, keeps the runtime
off the main thread, reports download/initialization/project/run timings, and
restarts the worker after a crash or outer timeout. Project sessions are keyed
by a SHA-256 digest of their complete file set.

CI verifies the artifact manifest and generated worker bundle, then executes
every registered runnable example with both the source-built native CLI and the
exact checked-in WASM artifact. The outputs must agree.

## Remaining implementation work

1. Publish the runtime artifact for every stable/canary language release.
2. Extract the docs-owned RunStore client, VFS, and result decoder into a small
   published browser-runtime package shared with the book.
3. Run browser benchmarks in the preview environment and replace the
   provisional budgets with observed p50/p95 targets.
