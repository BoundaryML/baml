# BAML/WASM runner spike

Status: architecture proven; browser component and release artifact are not yet shipped.

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

## Remaining implementation work

1. Publish the runtime artifact for every stable/canary language release.
2. Extract the current RunStore client, VFS, and result decoder into a small
   browser-runtime package with no playground UI dependency.
3. Add the worker and `BamlRunner` MDX component to this portal.
4. Run browser benchmarks in the preview environment and replace the
   provisional budgets with observed p50/p95 targets.
