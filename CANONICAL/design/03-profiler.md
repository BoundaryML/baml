# Profiler

**Status:** The core capture, CCT, canonical value store, local history, fold reader, retention, and GC are built, and the C1 hardening pass closed the durability, exhaustion-policy, saturation, diagnostics-persistence, and memory-bound gaps recorded below. The public DataFusion SQL layer is not built. The full-trace writer remains deliberately absent.

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

Every call contributes to the CCT. Only calls retained by an exact window or capture policy are individually discoverable. This distinction is the foundation of the query model. Completing a boundary now recycles its dead thread slots through a free list (`thread_slab_occupancy` exposes the bound); node rows remain session-scoped and are bounded by epoch rotation, and a small per-partition stub persists per boundary for the engine lifetime.

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

The producer does no SQL, filesystem write, network request, canonical encoding, or value hashing on function entry. The CLI drains continuously off-thread: a per-boundary drain worker encodes and persists captured drafts every 250 ms and runs the root-commit barrier at finish. (The reusable mpsc ValueDrainService also exists for hosts that prefer a store-owning service thread.)

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
| Structural exhaustion | **fail_run** default; **BAML_PROFILE_EXHAUSTION** selects fail_run / abort_process / continue_incomplete |

Exceeding the ring-memory cap applies the selected structural-exhaustion policy: **fail_run** (default) latches capture off for the process and the application continues; **continue_incomplete** sheds while over the cap and resumes; **abort_process** (strict opt-in) keeps the historical hard abort. Every shed is a counted drop that becomes a session SHED marker, degraded partitions, boundary diagnostics, and a BoundaryLoss record — never a silent gap. Exact per-environment defaults remain X1 policy work.

The bounded full-trace writer is also not implemented. Today exact tape comes from the recent-call ring, flight-recorder dumps, captured values, and the opt-in raw firehose.

## Capture behavior

### Structural capture

Structural profiling is default-on locally. The CLI creates a boundary directory and writes a durable begin record before execution. After the call, it flushes the profiler, drains values, binds the root partition, requests completion—which folds the snapshot before appending completion metadata—then performs a final flush. CLI boundary persistence/control failures are currently best-effort and verbose-gated. Structural ring exhaustion follows the configured policy above (fail_run by default); only the strict abort_process opt-in terminates the process.

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

Values are reserved and copied into a trace-owned heap before later processing. Canonical encoding never runs in the profiler consumer or on application threads: the CLI's drain worker encodes and persists drafts continuously at a 250 ms cadence and finishes with the durable root-commit barrier.

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

The v1 invariant holds: aggregation may lose attribution after declared structural loss, but it does not wedge or silently drop time. Corrupt ranges and shed records are counted in engine diagnostics, persisted as DEGRADED/SHED markers in the session stream, and reported as per-boundary deltas in the completion record. The defer list itself is bounded (**DEFER_MAX_PENDING**); hitting the bound resolves through the same declared synthesis as the timeout path.

### Recursion and spawning

- Normal stacks are uncapped.
- Beyond depth 512, the engine scans the nearest eight ancestors for the same function and reuses a node when possible. In-memory counts/time remain exact within their **u64** range; path uniqueness becomes visibly coarser.
- Spawn edges aggregate by spawning context and child entry function.
- The instance table retains the first 64 instances plus up to 256 exceptional instances per edge; in-memory aggregate counters remain **u64** and complete within their representable range.

Folded BCCT enters/terminal/LLM/spawn totals still saturate to **u32::MAX** on the wire, but every engaged clamp is now counted and a fold that clamped anything embeds an explicit **SATURATED** marker (kind-12, marker 6) in the sealed snapshot: totals become declared lower bounds, never silently wrong exact counts. In-memory histogram buckets saturate (counted in **hist_saturated_drops**) instead of wrapping/panicking, and window deltas subtract saturating so a held bucket cannot underflow its shadow.

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
- local definition content hash.

The full schema is [bamldict.proto](../../baml_language/crates/bex_events/src/dict/proto/bamldict.proto).

### Cross-revision joins

Use:

- **definition_key** for stable semantic identity across recompiles;
- the artifact's **def_content_hash** to indicate whether that function's own
  compiled signature or bytecode changed; and
- FQN/source only for display and secondary diagnostics.

A rename intentionally changes the definition key. Equal content hashes across
a rename are a hint, not identity.

The hash is deliberately local and layout-independent. Named types contribute
their nominal identity and referenced functions contribute their definition
key; their definition contents are not hashed recursively. Changing a class
schema or callee body always changes the whole-program revision, but it need
not change the hash of a function that consumes that class or calls that
callee. The public query catalog therefore names this field
**local_definition_hash**. It must not be described as a dependency-aware
behavior version.

The source snapshot is BLAKE3-256 over sorted project-relative source paths and
their content hashes plus `baml.toml`. The revision is a domain-separated
BLAKE3-256 hash over that snapshot, compiler identity, optimization level and
`emit_test_cases`. V1 must audit the revision constructor whenever another
behavior-affecting compiler input is introduced.

### Call-site identity

A call site is a static source expression, not a runtime invocation. A compact
call-site ID is revision-local and maps to a file and source span. The protobuf
shape exists, but the current dictionary builder emits an empty call-site
section. Producer emission, dictionary population and retained-call
consumption must land together before source navigation relies on it.

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

The root-pin barrier is crash-safe as built: the pack fsync (data and, once, the packs-directory entry) precedes the manifest append, and the append itself fsyncs the manifest and the boundary directory. A pinned root either survives a crash together with its pack bytes or is absent, in which case the durable unpinned chunks age out through the GC grace window. Dedupe additionally trusts only provably durable chunks: a sealed idx implies durability, a crashed writer's pack is recovered durably at open, and a live foreign writer's unsealed pack stays readable but never absorbs another writer's put.

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
- Every material loss/degradation path is queryable evidence: shed, corrupt-range, saturation, and synthesis counters persist as session markers, per-boundary completion diagnostics, and BoundaryLoss records, and the fold reader carries them into the mandatory query footer.

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

- Done in C1: the structural-exhaustion policy ladder (fail_run default), the crash-safe root-pin durability barrier, thread-slot reclamation at partition free plus the bounded defer list, explicit saturation/overflow evidence, and consistent loss/degradation persistence through markers, boundary diagnostics, and the fold reader.
- The full-trace writer stays explicitly absent (deferred); recent, flight, values, and opt-in raw remain the exact evidence paths.
- Speculative helper staging/promotion machinery exists but production wiring stays deferred (see [Deferred](10-deferred.md#languageruntime-dependent-depth)); no surface may imply it ships.
- Preserve SDK/pack/playground host parity for boundary/dictionary/value wiring (the continuous drain worker currently serves the CLI host; wasm/playground drain inline).
- Flight dumps write durably (tmp+rename) and carry no CID references today; if the transcoder ever emits value references it must write the `.bamlcids` pin (GC already honors it) in the same barrier.
- Keep the raw oracle, golden fixtures, differential fold tests, crash tests, and performance gates active while the SQL/provider layer is added (hot-loop gate re-measured at 48.6 ns/pair after C1).
