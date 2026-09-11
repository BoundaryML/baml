# BAML heap metrics

This harness requires `baml.sys.heap_stats()` from [PR #4837](https://github.com/BoundaryML/baml/pull/4837), still open at migration time. Pass a checkout containing that change to `scripts/build.py --baml-source /path/to/checkout/baml_language` until it lands. Preserved local artifacts came from the separate `baml-e2e-bench` worktree; the runtime change is not bundled with this harness. See [README.md](README.md) for migration state and reproducibility instructions.

## API and sampling

`baml.sys.heap_stats() -> baml.sys.HeapStats` is a VM builtin usable from packed BAML programs and generated SDKs. It samples the calling VM’s shared heap while its active heap permit excludes collection, then allocates the result. It does not force GC. Concurrent allocations can occur during sampling; fields are not an atomic snapshot.

The implementation delegates to `BexHeap::stats()` in `baml_language/crates/bex_heap/src/heap.rs`, the same hook used by Rust’s `BexEngine::heap_stats()`. It requires no metrics listener in the pack host and no process-global allocator replacement.

## Exported gauges

All three BAML variants serve these metrics at their private `:9091/metrics` endpoints:

| Metric | HeapStats field | Meaning |
| --- | --- | --- |
| `baml_heap_total_object_slots` | `total_objects` | Compile-time plus runtime slots |
| `baml_heap_compile_time_object_slots` | `compile_time_objects` | Permanent compile-time slots |
| `baml_heap_runtime_object_slots` | `runtime_objects` | Runtime slots, including unused reservations and uncollected objects |
| `baml_heap_active_handles` | `active_handles` | Registered heap handles acting as GC roots |
| `baml_heap_tlab_chunks` | `tlab_chunks` | Current nursery allocation reservations |

These values are gauges: collection can reduce runtime slots and nursery reservations. A fresh runtime can report zero runtime slots before the stats result itself is allocated. Total slots equal compile-time plus runtime slots within a sample.

Node and Python call the generated `heap_stats` wrapper using the same process-wide runtime as `hello_world`. Their existing V8/tracemalloc/RSS gauges remain available. The native variant starts a second BAML HTTP listener in the existing packed program. No sampling happens on `GET /`; each metrics scrape allocates its result, and native metrics also execute a BAML HTTP handler. Sampling can therefore influence subsequent measurements.

Prometheus scrapes the endpoints every five seconds. Grafana adds three panels with count units below the five existing comparison rows. `scripts/verify.py` requires all three exporters to be healthy, the five gauges to be finite and nonnegative, and their totals to agree, alongside the existing response, quota, and load checks.

## Measurement limits

Object slots are not live objects or heap-used bytes. `runtime_objects` sums the lengths of the three generation spaces. `alloc_tlab_chunk()` extends the nursery with placeholder objects for a whole reservation before all slots are used. The current default reservation is 1,024 slots; backing storage grows in chunks of 4,096 slots.

Multiplying slots by `size_of::<Object>()` would omit separately allocated string/array/map payloads, additional backing capacity, allocator overhead, and other native allocations. This demo therefore does not present that product as heap-used bytes. RSS, V8 heap, Python traced allocations, and BAML slots have different, overlapping scopes and must not be added.

A full BAML heap-byte measurement would require additional allocation accounting or process-allocator instrumentation with a clearly broader scope. Without PR #4837 the base checkout exposes these counters only to Rust; simply enabling profiling or adding a sidecar cannot access the existing process’s BAML heap through a supported external API.

## Verified local run

Release artifacts from `77c5356622582b5832033871cc7f0e9a03ee7bd1` passed `python3 scripts/verify.py --seconds 60`: all five experiments delivered 6,000 successful requests at approximately 100 RPS, with zero new transport errors or restarts. All 11 Prometheus targets were healthy, all 15 BAML gauge series passed the field checks, and all 23 Grafana panels were provisioned. The three new charts were also inspected in Grafana. The report is saved at `results/baml-local-hello-world/verification.json`; artifact hashes and the build fingerprint are in `.build/manifest.json`.

At the end of that interval, runtime slots were 16,384 for Node+BAML, 15,360 for Python+BAML, and 7,873,536 for native BAML. These are reserved object-slot counts, not bytes or live objects. The interval demonstrates working instrumentation under load; it does not establish sustained memory stability or fix the native runtime’s existing memory growth.
