# Observability implementation benchmark ledger

Measured on 2026-07-31 from Linux release builds in this worktree. Commands
are reproducible from `baml_language/`. Every saved row was accepted by
`obs-bench validate`, the committed Linux baseline policy, and the reusable
workflow's direct acceptance predicates. The macOS policy also parses and
evaluates against the same row schema; the reusable workflow is responsible
for collecting its real arm64 measurements.

## CCT consumer hot path

Command:

```text
cargo bench -p bex_events --bench cct_update
```

The pre-optimization integrated CCT implementation measured 65.881–69.117
ns/call-pair across its original cardinality sweep. The final benchmark
includes raw-record decoding, an equal-cost flight-recorder range copy,
depth-14 stacks, and eight round-robin producer rings. On Linux the harness
pins itself to one CPU from its assigned cpuset before warmup, preventing
scheduler migration from invalidating absolute nanosecond gates.

| Shape | Median ns/call-pair | Sample range |
|---|---:|---:|
| 1 function | 46.436 | 45.349–47.520 |
| 16 functions | 48.514 | 48.015–48.571 |
| 1,024 functions | 48.796 | 48.718–49.142 |
| 4,096 functions | 49.783 | 49.558–50.265 |
| Depth 14, one ring | 48.818 | 48.626–48.890 |
| Depth 14, eight rings | 49.336 | 49.293–50.683 |

The six-shape median is 48.807 ns/call-pair. It clears the 50 ns integrated
median gate, and every sample clears the 60 ns absolute ceiling.

The dual-pipeline oracle also decodes the retained legacy stream and compares
raw function-enter and every terminal status total exactly against the CCT
snapshot. This guards the speedup against silently dropping work.

## Value CAS

Command:

```text
cargo bench -p bex_events --bench value_cas
```

The permanent per-size curve clears both the 300 MB/s/core floor and
`max(2 ms, size / 250 MB/s)` latency gate:

| Input | Median encode | Throughput |
|---:|---:|---:|
| 1 KiB | 2.244 µs | 456.328 MB/s |
| 4 KiB | 6.119 µs | 669.390 MB/s |
| 16 KiB | 10.877 µs | 1,506.298 MB/s |
| 64 KiB | 25.288 µs | 2,591.585 MB/s |
| 256 KiB | 88.253 µs | 2,970.369 MB/s |
| 1 MiB | 606.830 µs | 1,727.957 MB/s |

The transcript curve captures every prefix at N=16/32/64/128 with a repeated
64 KiB prompt. At N=64, repeated whole bodies account for 136,314,880 bytes;
the project-scoped CAS stores 232,261 unique bytes, a 586.904x reduction.

- Growth exponent, N=16 to N=64: 0.801 (gate <=1.2).
- Incremental bytes at append 64: 5,125 (gate <=73,728).
- Prefix encoder throughput: 2,337.687 MB/s (gate >=300 MB/s/core).
- Median terminal-transcript encode: 1.738 ms.
- Terminal hash: 32.270 µs / 2,189.836 MB/s.

Canonical values, BCCT headers, revision dictionaries, and BQF1 each have
versioned, byte-exact frozen fixtures with owning tests.

## Compact storage, index, and durability

Command:

```text
cargo bench -p bex_events --bench bcct_storage
```

- Three-row compact CCT block: 3.412 µs/block.
- Conservative encoded size including a fresh container header: 328
  bytes/window, or 1,312 bytes/s at four windows/s (gate <=6 KB/s).
- Recovery scan: 2.113 µs/block over 1,000,000 scanned blocks.
- Exact 100k-event index: 800,872 bytes over a 4,800,000-byte segment,
  ratio 0.166848 after four LOD levels are shed (gate <=0.25).
- 10,000 completed boundary partitions: 7,122,944-byte RSS delta while
  partition/recent-call/spawn-instance state returns to zero (gate <=64 MiB).
- Off-thread process-crash durability: 128/128 fsync completions, 12.420 µs
  producer-stall p99 (gate <=20 ms).

Session epochs independently rotate at 256 MiB or 24 hours only after all
engine threads, partitions, deferred work, boundary bindings, and pending
latency triggers are quiescent. Segment rotation does not reset the epoch
deadline.

## Bounded query and wire path

Command:

```text
cargo bench -p bex_query --bench query_bounds
```

The C6 rows use a real temporary BCCT file and `FileSource`; they are not
in-memory-source estimates.

| Workload | Time | Bounded resources |
|---|---:|---:|
| Open + fold + live frame, 4,096 events | 2.803 ms | 197,032 B source; 66,940 B frame; 360,736 B cache; 225,280 B RSS delta |
| Open + fold + live frame, 100,000 events | 62.539 ms | 4,800,424 B source; 66,777 B frame; 8,800,288 B cache; 8,482,816 B RSS delta |
| Timeline, 1,000,000 calls | 1.200 ms/query | 66,940 B frame |
| Timeline, 36,000,000 calls | 1.359 ms/query | 66,940 B frame |
| Live subscription, 30 Hz for 10 s | 300 frames | 13,200 B total |

Viewport bytes are exactly invariant between 1M and 36M logical calls and
stay below the 200 KiB cap. Tests pin the native query cache at 256 MiB and
both wasm query/HTTP-range caches at 32 MiB, including byte-budget eviction.

`obs-bench corpus synth --target-bytes 10737418240` was also exercised:
both cct-only and full-trace modes reported 10 GiB logical corpora, with 2,550
hard-linked shard paths backed by about 16.9 MiB of deterministic physical
templates. The generator bounds artifacts at 10,000 and validates every
template. This proves the release corpus construction and scan path; the
interactive C7 rows above exercise bounded viewport work rather than claiming
a linear 10 GiB full-file scan is sub-500 ms.

## P9 migration fence

Legacy profile projection and v1 writers remain intentionally present. P9
requires paired legacy/CCT baselines retained for one release cycle plus all
C2/C3/C6/C7 equivalence gates. This work establishes the candidate benchmark
and baseline-refresh lifecycle, but cannot manufacture a completed release
cycle. Deleting the fallback before that evidence exists would violate the
canonical design.

## Deterministic acceptance coverage

The performance rows above are complemented by deterministic acceptance
fixtures:

- 239 `bex_events` library tests passed, including causal defer/resync,
  thread lifecycle, session epochs, exact raw/CCT equivalence, saturation,
  trigger/pin publication, recovery, GC, and retention.
- C11 lifecycle retention opens and releases 10,000 root/spawn boundary
  partitions and proves flat backing state as well as the measured RSS bound.
- C12 saturation bounds live/peak ring bytes, keeps CCT aggregation active,
  marks degradation explicitly, and reconciles marker/stat drop counts.
- Trigger coverage includes exact-once error/manual/latency firing, a
  five-second per-boundary rate limit, a 16-dump cap, durable diagnostics, and
  real promoted-CID pin manifests for flight and exact dumps.
- Snapshot pinning keeps append-only growth invisible at a committed prefix;
  replacement and truncation fail closed.
- BQL matched-I/O compare joins byte-identical input CIDs, compares output CID
  multisets, preserves left/right run identity, and reports unmatched rows.
- Value DAG hydration/diff frames are byte-bounded and expose child CIDs or
  resume rows rather than partially decoding a value.

## Final integration checks

- The Linux x86-64 policy accepts all 28 saved measured gates. The macOS arm64
  policy parses and accepts the row schema; CI collects and enforces native
  macOS measurements instead of relabeling this Linux run.
- The reusable workflow YAML parses, and its C2/C3/C6/C7/C10/C11/C12/C13 jq
  predicates all evaluate true over the final rows.
- Native and wasm cache-budget matrices pass.
- Cross-crate checks cover CLI, native observe server, pack host, CFFI, wasm,
  benchmark tooling, LLM/sysop integration, and compiler tests.
- Rust formatting, focused multi-crate test matrices, the wasm target check,
  and `git diff --check` complete the final validation pass.
