# Observability implementation status

This worktree implements the P0-P8 phase ledger in `TASK/design.md`, including
the host-wiring phase. P9 was evaluated and intentionally stopped at its
mandatory migration fence: the design forbids deleting legacy profile/v1
paths until paired baselines have existed for one release cycle and the full
C2/C3/C6/C7 equivalence set is green.

| Phase | Delivered |
|---|---|
| P0 | Complete `obs-bench` command surface, bounded streaming runner, machine manifests, stats/replay/crashfuzz, deterministic cct-only/full-trace corpus synthesis and scanning, plain-BAML hotloop/agent/transcript/idle/deep/path workloads, per-platform baselines, refresh workflow, two-platform reusable gate workflow, and one-device-pixel UI floor |
| P1 | Dense compile-time function IDs, immutable load-seam validation, legacy-bytecode identity finalization, program/revision/source identities, deterministic dictionaries, definition/lambda metadata, capture flags, envelope/cache ABI bumps, and relink/emit oracles |
| P2 | Causal CCT aggregation, defer/resync, thread lifecycle, suspend/resume, LLM enrichment, spawn instances, recursion folding, recent calls, compact partition retention, exact raw/CCT counter oracle, and six-shape CPU-pinned integrated benchmarks |
| P3 | All 13 BCCT block kinds, session/boundary metadata, checkpoints, watermarks, off-thread durability, sealing/recovery, snapshots, dual v1/v2 layout, exact-index/RSS/fsync gates, and quiescent 256 MiB/24 h session epochs independent of segment rotation |
| P4 | Sans-I/O bounded query engine, real native file/range/live sources, 256 MiB native cache gate, BQF1, authenticated `/api/obs`, runs/timeline/Left Heavy, live ACK gating, and the Runs UI |
| P5 | Canonical value DAG/CIDs, byte-exact versioned goldens, per-size/transcript/hash curves, packs and indexes, writer/GC locks, root ordering, continuous drain, staging/promotion/audit, manifests, reachability GC, retention, and `baml clean` |
| P6 | Bounded flight recorder, exact dumps with real boundary CID discovery and pin manifests, full-trace budgets, exact index, error/manual/per-call latency triggers, loss diagnostics, and deterministic shed saturation |
| Host | Default CLI/test/CFFI/pack history wiring, ULID boundaries, completion barriers, SDK shed defaults, `BAML_HISTORY`, project `[observability]` policy, embedded pack policy, and privacy documentation |
| P7 | BQL parser/planner/catalog, CLI query/schema/explain/files/params/cursors/snapshots, completeness metadata, pinned reads, MCP-neutral adapters, failure/diff/compare/vdiff/stats, byte-identical-input matched-I/O output-multiset comparison, and Studio language service |
| P8 | `baml studio`, Sandwich/search/cross-revision diff, native value DAG inspection and Merkle diff, wasm ObserveEngine, 32 MiB wasm/HTTP Range cache gates, and ClickHouse aggregate compiler/golden corpus |
| P9 | Gate evaluated; C2/C3/C6/C7 candidates are green and the baseline-refresh lifecycle is installed, while guarded legacy projection, caps, JSON projection, and v1 writers remain preserved for the mandatory one-release paired-evidence window |

Measured results and validation evidence are in
`TASK/observability-benchmark-results.md`. Local-data and opt-out behavior are
documented in `TASK/observability-privacy.md`.
