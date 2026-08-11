# Profiler

**Status:** The core capture, CCT, canonical value store, local history, fold reader, retention, and GC are built. The public DataFusion SQL layer is not. The server shedding policy and full-trace writer described in older documents are not implemented.

This document is the refresher for how profiling works and what the current branch actually does.

## Why the profiler is shaped this way

The legacy “one event row per function call” model scaled with traffic:

- one measured hot loop produced 1.69 GB in 3.8 seconds;
- replaying 4,096 legacy events produced quadratic patch work; and
- transcript capture repeatedly stored the same growing conversation.

The replacement avoids traffic-proportional structural storage for repeated calls within one boundary: those calls collapse into calling-context aggregates. Aggregate volume follows distinct program shape, value volume follows selected distinct content, and exact tape is bounded or policy-selected. Total retained history still grows with boundary count, and the opt-in raw firehose remains traffic-proportional.

| Plane | What it stores | Honest question |
|---|---|---|
| **Tally / CCT** | One aggregate node per unique calling context, with counters, timing, histograms, LLM totals | How often, how slow, how much, how many errors? |
| **Tape** | Bounded recent calls, flight dumps, and optional raw firehose | What exactly happened inside the retained window? |
| **Values** | Selected inputs/outputs/errors/logs as canonical content-addressed DAGs | What data flowed through a retained call? |

Every call contributes to the CCT. Only calls retained by an exact window or capture policy are individually discoverable. This distinction is the foundation of the query model. Current resident-memory reclamation does not yet fully match the shape-bound goal: completed-partition and dead-thread metadata persist for the engine lifetime. Epoch rotation rebuilds the node namespace but is not full reclamation.

## Hot-path data flow

~~~mermaid
flowchart LR
  CALL["Function/thread/LLM lifecycle"]
  RECORD["Fixed-width record"]
  RING["Per-thread lock-free ring"]
  CONSUMER["One background consumer"]
  ENGINE["CCT engine"]
  RECENT["Recent-call ring"]
  FLIGHT["Flight recorder"]
  RAW["Raw firehose, opt-in"]
  SESSION["Session BCCT/BMET"]
  VALUEQ["Value capture queue"]
  CAS["Boundary-finish drain + canonical CAS"]

  CALL --> RECORD --> RING --> CONSUMER
  CONSUMER --> ENGINE --> SESSION
  CONSUMER --> RECENT
  CONSUMER --> FLIGHT
  CONSUMER --> RAW
  CALL --> VALUEQ --> CAS
~~~

The producer does no SQL, filesystem write, network request, canonical encoding, or value hashing on function entry. A reusable off-thread ValueDrainService exists, but the current CLI does not use it: it drains and canonicalizes captured drafts synchronously at boundary finish.

## Structural records

The current wire has nine tag types. All integers are little-endian.

| Tag | Record | Encoded size | Essential fields |
|---:|---|---:|---|
| 0x01 | CallFunction | 54 B | logical thread, call, parent call, function, timestamp, optional call-site span |
| 0x02 | EndFunction | 26 B | logical thread, call, timestamp, terminal status |
| 0x03 | StartThread | 36 B + name up to 256 B | child thread, parent thread, spawning call, timestamp, name |
| 0x04 | EndThread | 18 B | thread, timestamp, status |
| 0x05 | SetFunctionId | 41 B | per-call **$id** override UUID annotation |
| 0x06 | SuspendThread | 22 B | thread, timestamp, reason, suspend sequence |
| 0x07 | ResumeThread | 30 B | thread, resume timestamp, original suspend timestamp |
| 0x08 | LlmCallMeta | 38 B | thread/call enrichment, model ID, token counts, provider/parse/retry flags; consumer resolves the node |
| 0x09 | ModelBirth | 8 B + name | model ID and interned name |

The maximum record size is 292 bytes. The authoritative codec is [record.rs](../../baml_language/crates/bex_events/src/prof/record.rs).

## Current defaults

These are code-verified local defaults, not hosted policy:

| Control | Current behavior |
|---|---|
| Profiling | On by default on native; wasm requires an adapter to opt into cooperative profiling |
| Durable CLI history | On when profiling is on; **BAML_HISTORY=0** or **false** disables it |
| Ring segment | 256 KiB, clamped to 64 KiB–16 MiB |
| Live ring memory cap | 1 GiB per process |
| Ring recycled-segment cap | 4 per ring |
| Consumer wake timeout | 50 ms |
| CCT window flush | 250 ms |
| Recent completed calls | 4,096 completed calls per partition; open calls live separately in CCT thread state |
| Flight recorder | One consumer-global 16 MiB ring shared across engines |
| Flight dump rate | At least 5 seconds apart, at most 16 per engine |
| Speculative value staging | 32 MiB native, 8 MiB wasm |
| Raw firehose | Off; **BAML_PROFILE_RAW=1** enables it |

The current hard ring-memory cap aborts the process when exceeded. Marker types for shedding exist, but the documented abort-or-shed policy and multi-step server shedding ladder are not wired. The desired **fail_run / abort_process / continue_incomplete** product policy remains target work; current behavior must not be described as graceful shedding.

The bounded full-trace writer is also not implemented. Today exact tape comes from the recent-call ring, flight-recorder dumps, captured values, and the opt-in raw firehose.

## Capture behavior

### Structural capture

Structural profiling is default-on locally. The CLI creates a boundary directory and writes a durable begin record before execution. After the call, it flushes the profiler, drains values, binds the root partition, requests completion—which folds the snapshot before appending completion metadata—then performs a final flush. CLI boundary persistence/control failures are currently best-effort and verbose-gated. Structural ring exhaustion is different: the current 1 GiB cap aborts the process.

This current host behavior is intentionally recorded because the v1 hosted guarantee work will strengthen some failure semantics.

### Value capture

The CLI call context enables values and disables logs by default, recording the **llm_boundary** capture-default label. Compiler defaults are:

- ordinary user/companion/auto-derived functions: inputs and outputs disabled, errors and promote-on-error set to Auto;
- built-in/internal functions: all capture disabled;
- LLM functions: input, output, error and promote-on-error set to Auto.

Auto becomes enabled only when the host enables value capture. Root input/output/error capture is direct and independent of the per-function mask.

A capture draft contains:

- boundary identity;
- process/engine/thread/call identity;
- profiling function ID;
- role: root/call input, output, error, or log body;
- optional log metadata;
- a handle into the copied trace snapshot; and
- an optional promotion trigger.

Values are reserved and copied into a trace-owned heap before later processing. Canonical encoding never runs in the profiler consumer. The reusable value drain service is implemented, but the current CLI drains once, synchronously, after the boundary call resolves.

### Speculative promotion

The byte-bounded staging ring, promotion API and **staging_evicted** accounting exist. Root error handling calls promotion. However, the production VM currently has no caller that stages helper drafts, so “a failing helper’s arguments are retroactively promoted” is not shipped behavior. Wiring this path is required implementation work.

## CCT engine

### Identity and shape

A CCT node is interned by **(parent_node_id, function_id)**. Node IDs are dense **u32** values scoped to a session epoch. They are not stable across runs, sessions, epochs, or projection rebuilds.

Node columns include:

- parent, function, flags, depth and partition;
- enters and terminal counts by status;
- total, self and await nanoseconds;
- 16 duration-histogram buckets;
- dirty/flushed shadows; and
- per-node/per-model LLM counters.

### Time accounting

On each event, elapsed time since the thread’s last charge is attributed to the active stack top:

- suspended → await time;
- otherwise → self time.

EndFunction also records total duration and a histogram bucket. Resume records carry their suspend timestamp, so time accounting is resilient to cross-thread drain reordering.

This is runtime self/await accounting, not operating-system CPU sampling.

### Causal reordering

Per-thread rings do not impose a global order. A record whose causal parent has not arrived is deferred. After 1,024 defer sweeps, the engine synthesizes an unattributable parent under function 0, replays dependents, and marks the affected partition degraded. An explicit lost/corrupt range is a different path: it marks currently live partitions degraded and discards the remainder of that range; it does not itself synthesize the missing parent.

The v1 target invariant is: aggregation may lose attribution after declared structural loss, but it must not wedge or silently drop time. Current corruption degradation is not consistently persisted, so the implementation does not yet satisfy that invariant on every path.

### Recursion and spawning

- Normal stacks are uncapped.
- Beyond depth 512, the engine scans the nearest eight ancestors for the same function and reuses a node when possible. In-memory counts/time remain exact within their **u64** range; path uniqueness becomes visibly coarser.
- Spawn edges aggregate by spawning context and child entry function.
- The instance table retains the first 64 instances plus up to 256 exceptional instances per edge; in-memory aggregate counters remain **u64** and complete within their representable range.

Current folded BCCT enters/terminal/LLM/spawn totals saturate to **u32::MAX** without an overflow marker. Duration histogram buckets are already **u32** in memory and unchecked increments wrap in release builds or panic in debug builds at overflow. Thus “population-true” is the logical contract, but extraordinarily large boundaries can lose exact folded counts today. Widening or explicitly marking overflow/saturation is a v1 correctness gate.

### Epochs and files

- Session segments rotate at 4 MiB or 15 minutes.
- CCT epochs rotate at 256 MiB of CCT bytes or 24 hours.
- Rotation restarts the node-ID namespace and rebuilds state, including roots for historical partitions plus carried spawn/LLM state; it is not a full memory-reclamation boundary.

## Compile-time and cross-revision identity

### Function IDs

The compiler finalizer assigns dense **u32** function IDs:

- 0 = unknown/unattributable;
- 1 = spawn closure;
- 2–15 reserved;
- real functions begin at 16.

Function IDs are meaningful only with their revision.

Current boundary IDs are UUIDv4 payloads encoded as **baml_id_1_** plus base64url. They are not ULIDs and do not sort chronologically. Use **created_ms** or the history-directory prefix for chronology.

### Revision dictionary

One protobuf dictionary per revision maps dense IDs to:

- FQN and display/declaration name;
- project-relative file and source span;
- kind and origin;
- stable definition key;
- owner/lambda/package/namespace identity;
- capture flags; and
- definition content hash.

The full schema is [bamldict.proto](../../baml_language/crates/bex_events/src/dict/proto/bamldict.proto).

### Cross-revision joins

Use:

- **definition_key** for stable semantic identity across recompiles;
- **def_content_hash** to indicate whether that definition’s behavior changed; and
- FQN/source only for display and secondary diagnostics.

A rename intentionally changes the definition key. Equal content hashes across a rename are a hint, not identity.

## Canonical value store

### Encoding and identity

A BAML value becomes a canonical Merkle DAG:

- BLAKE3-256 CIDs with versioned node/chunk domains;
- deterministic field/key ordering;
- canonical NaN representation;
- **positive and negative zero remain distinct** in the current codec;
- 128 KiB fixed chunking for long string/byte bodies;
- 128-way collection fanout;
- small leaves kept inline where the codec permits; and
- indirect encoding for map keys longer than 4,096 bytes.

The public CID wire form is **bamlv_1_** plus unpadded base64url. Codec changes require a new version/domain. The authoritative implementation and golden tests are [canon.rs](../../baml_language/crates/bex_events/src/store/canon.rs).

### Pack layer

The local store writes append-only **.bamlpack** files. Pack magic remains **BPK1**; the filename is not **.bpk1**. V1 pack records are currently raw, not zstd-compressed. Packs seal at 64 MiB or explicit/graceful writer shutdown. A crash can leave an unsealed, unindexed pack that open-time scanning recovers. **.bamlpack.idx** sidecars use **BPKI** and are rebuildable.

The local CAS is shared across runs and deduplicates logically by CID. A writer skips CIDs already visible in its opened store or active writer. Concurrent processes can still append semantically harmless duplicate physical records before either sees the other’s new index. Current GC keeps every physical copy of a live CID; live-duplicate compaction is not implemented.

The intended invariant is that a root is not reclaimable before its pack bytes and boundary pin are durable. The current CLI syncs the pack and then appends **manifest.bamlcids**, but that manifest append is not fsynced in the same barrier. Treat the stronger “never durable without its root pin” statement as an implementation gap, not an as-built guarantee.

Current **.bamlvalue** capture records may also retain an inline legacy body alongside the canonical DAG reference; a general boundary writer externalizes larger legacy bodies to per-boundary SHA-256 blobs. Query readers prefer canonical DAG/CAS, then inline, then legacy blob. “Bodies live only in CAS” is the v1 target, not a complete description of existing artifacts.

### Budgeted reads

The canonical decoder supports byte/depth budgets and returns explicit elision plus child CIDs for resumable descent. Query-time value resolution must reuse this codec and store; the DataFusion work must not introduce the prototype PR’s SHA-256 JSON blob store or a second value model.

## Retention and GC

### Local retention

Current local defaults:

- history: 30 days, 2 GiB total, preserve at least the newest 20 boundaries;
- sessions: 7 days, 1 GiB total;
- raw firehose: 512 MiB per session;
- legacy profiles: 7 days.

The current retention pass independently prunes: oldest raw files above the per-session cap; whole history directories by age/size/floor; whole session directories by age/size; then old legacy profiles. It does not independently prune flight or trace files despite counters/comments that anticipated that behavior. Every material removal appends a tombstone to **retention.log**.

### CAS GC

The mark roots are:

- every retained boundary’s **manifest.bamlcids**;
- session/flight CID pin manifests;
- root-level **uploads.pin** for content still owed to a hosted upload.

GC takes the writer lock exclusively. If a writer is live, it skips. The mark closes over DAG children. Packs younger than 24 hours are protected. Entirely dead packs are unlinked; partially live packs are compacted.

Local retention is distinct from hosted retention. Accepted hosted S3 evidence is indefinite by default and is removed only through explicit authorized erasure.

## Crash and corruption behavior

- BCCT and pack readers accept the intact checksummed/CRC-valid prefix of a torn tail.
- Sealed BCCT snapshots carry their footer/checksum. Pack records carry CRCs; pack indexes are separately rebuildable and atomically published. Do not generalize one container’s seal protocol to every artifact.
- A boundary begin without completion under a dead/stale session is read as crashed/partial; no daemon invents an outcome.
- Unknown additive BCCT/BMET kinds can be skipped; unsupported major versions fail explicitly. An unknown raw profiler tag terminates/discards the remainder of that decoded range.
- Missing dictionaries degrade labels to numeric identities, not fabricated names.
- Several declared-loss paths emit counters/markers today, but corruption degradation is not consistently persisted into boundary diagnostics. V1 requires every material loss/degradation path to become queryable evidence.

## Measured evidence, not protocol guarantees

The implementation ledger reported, on one development machine:

| Measurement | Reported result |
|---|---:|
| End-to-end profiler overhead | 74.4 ns per call |
| CCT hot-loop slice | 47.8–48.6 ns per pair |
| Five-million-call disk result | 4.5 KiB |
| Completed-run first-frame open | 2.62 ms |
| Consumer RSS under sustained load | 34.3 MiB |
| Transcript dedupe at 64 turns | at least 20× |

These numbers justify the architecture and seed regression gates. They are not cross-machine guarantees or user-facing SLOs. Re-measure on controlled CI hardware and maintain a dated benchmark ledger.

## Current code map

| Responsibility | Source |
|---|---|
| Profiling configuration and defaults | [prof/config.rs](../../baml_language/crates/bex_events/src/prof/config.rs) |
| Record codec | [prof/record.rs](../../baml_language/crates/bex_events/src/prof/record.rs) |
| Ring | [prof/ring.rs](../../baml_language/crates/bex_events/src/prof/ring.rs) |
| Background consumer | [prof/consumer.rs](../../baml_language/crates/bex_events/src/prof/consumer.rs) |
| CCT engine | [prof/cct/engine.rs](../../baml_language/crates/bex_events/src/prof/cct/engine.rs) |
| BCCT segment format | [prof/cct/segment.rs](../../baml_language/crates/bex_events/src/prof/cct/segment.rs) |
| BCCT row codecs | [prof/cct/blocks.rs](../../baml_language/crates/bex_events/src/prof/cct/blocks.rs) |
| BMET lifecycle stream | [prof/cct/meta.rs](../../baml_language/crates/bex_events/src/prof/cct/meta.rs) |
| Session writer | [prof/cct/session.rs](../../baml_language/crates/bex_events/src/prof/cct/session.rs) |
| Value capture queue | [bex_engine/value_capture.rs](../../baml_language/crates/bex_engine/src/value_capture.rs) |
| Canonical value codec | [store/canon.rs](../../baml_language/crates/bex_events/src/store/canon.rs) |
| Pack/index | [store/pack.rs](../../baml_language/crates/bex_events/src/store/pack.rs), [store/index.rs](../../baml_language/crates/bex_events/src/store/index.rs) |
| Retention/GC | [store/retention.rs](../../baml_language/crates/bex_events/src/store/retention.rs), [store/gc.rs](../../baml_language/crates/bex_events/src/store/gc.rs) |
| CLI boundary wiring | [run_observability.rs](../../baml_language/crates/baml_cli/src/run_observability.rs) |
| Fold/run/value readers | [bex_query](../../baml_language/crates/bex_query/src) |

## Remaining profiler work relevant to v1

- Implement the predeclared structural-exhaustion policy; do not claim the current abort is **fail_run**.
- Decide and implement the full-trace contract, or keep it explicitly absent.
- Preserve SDK/pack/playground host parity for boundary/dictionary/value wiring.
- Add the policy/config surface without changing hot-path semantics.
- Ensure flight/CAS pin edge cases and automatic cleanup invocation are correct.
- Write flight CID sidecars if flight artifacts can reference values; GC recognizes such sidecars, but the current flight writer does not create them.
- Make boundary root-pin durability match the stated CAS invariant.
- Reclaim completed partition/thread metadata or otherwise prove the server memory bound; current free_partition does not remove the retained slabs/nodes that old prose claimed it did.
- Widen folded totals/histograms or persist explicit saturation/overflow evidence before claiming population-exact results at extreme scale.
- Keep the raw oracle, golden fixtures, differential fold tests, crash tests, and performance gates active while the SQL/provider layer is added.
