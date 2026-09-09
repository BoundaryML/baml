# Interface lifetime and mutation review

Open [index.html](index.html) directly in a browser. It is a self-contained, offline HTML artifact: no install, server, external scripts or network requests are required. Relative evidence links work when this directory stays beside the design documents and probes. For an in-app preview, serving the repository locally is also supported.

The artifact contains 33 cases (20 lifetime, 13 mutation), 136 step-through states, and code for every step. Each case shows ownership edges, receiver/storage identity, field values, copied local values, origin-scope state, expected model invariants and the real bridge tests still required. Code stays visible throughout a case and highlights the block corresponding to the selected step. Selected cases offer Python, TypeScript, Rust and BAML alternatives; protocol cases explicitly use bridge pseudocode. Fixture functions/imports are omitted, and proposed SDK examples are not claimed to run on the current SDK.

Cases can be marked **Looks right** or **Needs discussion**, with notes saved in browser-local storage where available. **Export review notes** displays downloadable/copyable JSON; it sends nothing externally. URL fragments such as `#M04/3` select a case and step. Saved assessments are user review notes, not test results or changes to the repository.

## What this does and does not prove

This is an executable review model, not the new BAML bridge implementation. Edges represent logical ownership obligations. VM roots/field references, native ownership and transferred bridge leases are distinguished; their counts are not actual allocator/registry measurements. Scope authority is separate from strong ownership. The simplified collector repeatedly releases objects with no incoming modeled edges; real runtimes have tracing, release queues and delayed collection. The modeled cross-runtime cycle represents the case where each runtime sees a bridge root that it cannot trace through.

The model is intentionally honest about retained state: a cross-runtime cycle survives until a bridge edge is retired after drain, and a permanently running native worker retains its closing runtime/receiver indefinitely. Explicit shutdown cannot safely reclaim memory still accessible by running native code. Nor does bridge reference release imply application resource cleanup. The final state is not universally zero: mutation demonstrations leave their caller-owned values live, the stale-generation case creates a new unrelated receiver, and the hung-worker case deliberately remains retained.

Every case includes an implementation gate. Selected current BAML/Python behavior is backed by existing probes; generic interface ABI transfer, field storage, cancellation races and shutdown still require real bridge implementation and fault-injected conformance tests. Finite modeled scenarios cannot prove leak freedom across all schedules, values or languages.

## Policy review points

| Case | Recommendation / remaining decision |
| --- | --- |
| L10 | Two-phase scope closure. A callback cannot await a drain containing itself. Recommend nonwaiting revocation plus a deterministic self-drain error; an external owner awaits actual closure. Define descendant/reentrant-call admission during closing. |
| L18 | A single atomic ledger/staging protocol decides adoption versus cancellation. Duplicate completion/ACK must not duplicate ownership. Specify recovery and bounded transaction-record retirement for any transport with ambiguous acknowledgements. |
| L19 | Await real work drain. An optional timeout reports incomplete closure and retains safe runtime/worker state; do not silently detach and free it. A hard termination boundary requires separately specified isolation. |
| M12 | Recommend an owner-side linearization point for each individual storage mutation. Multi-operation sequences, iteration and data export do not implicitly become transactions/snapshots; their consistency needs explicit semantics. |

Additional conformance work includes clone/invoke/close races, concurrent close waiters, malformed nested result/error cleanup, weak-cache resurrection prevention, multiple explicit registrations of one host object, scope-authority preservation, recursively aliased map/class/list storage, iterators/streams, and generation-safe release queues after executor shutdown. See the per-case gates and the technical design.

## Edit and verify

- `cases.js`: executable model and scenario states.
- `code.js`: per-case, per-language source with step mappings.
- `app.js`: interaction, source highlighting and graph rendering.
- `template.html`: standalone page structure and theme-aware responsive styling.
- `build.mjs`: embeds all source into `index.html`.
- `verify.mjs`: model invariants, expected terminal states, code coverage, source links, script syntax, offline bundle and generated-file freshness.

From `baml_language/`:

```sh
node interface_review/build.mjs
node interface_review/verify.mjs
```

The verification currently covers 33 cases, 136 frames and 385 expected-state assertions, plus failure guards and artifact checks. These are **model checks**, not BAML ABI tests. Browser review separately exercises step navigation, source-language switching, notes, reload persistence, JSON export preview and responsive layout. The in-app browser did not report a download event for the automatic Blob download, so export also exposes the JSON directly for copying.
