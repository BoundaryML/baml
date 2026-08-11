# Local artifacts

**Status:** Code-verified current layout. Future query-provider caches are listed separately and must remain rebuildable.

## Literal project tree

~~~text
.baml/
  .gitignore
  dict/
    baml_rev_1_<base64url>.bamldict

  sessions/
    <started_secs>-<process_euid_hex>-e<engine_id>/
      session.bamlmeta
      cct/
        seg-000000.bamlseg
        seg-000001.bamlseg
      flight/
        <timestamp_ms>-<trigger>.bamlprof
        <matching CID pin manifests; GC recognizes, writer not implemented>
      raw/
        raw-000001.bamlprof
      trace/
        <reserved path; directory/writer not implemented>

  history/
    <created_ms>-<target_slug>-<baml_id_1_wire>/
      boundary.bamlmeta
      cct.bamlcct
      manifest.bamlcids             # only when canonical roots were persisted
      thread-<thread_id>/
        stack-<segment>.bamlprof    # reader compatibility; current CLI does not emit
        value-<segment>.bamlvalue
      blobs/
        sha256/<first-two-hex>/<digest>.blob
        # active large-body fallback plus reader compatibility

  store/
    writers.lock
    gc.lock
    packs/
      pack-<process_euid_hex>-<seq6>.bamlpack
      pack-<process_euid_hex>-<seq6>.bamlpack.idx
      pack-<process_euid_hex>-<seq6>.lease

  uploads.pin                       # GC-recognized; new hosted sync writer absent
  retention.log

  <future rebuildable provider state>
    # SQLite, Parquet, or direct-artifact indexes selected per logical table
~~~

Not every optional directory exists in every run. In particular:

- raw exists only when the firehose is enabled;
- flight appears only after a trigger/manual dump;
- flight dumps carry only structural events (no CID references), so no pin manifest is owed today; GC honors `flight/*.bamlcids` the moment a future transcoder emits value references, and the dump write itself is durable tmp+rename;
- the full-trace writer is not implemented;
- stack files are reader compatibility artifacts; current live writers do not emit them;
- the CLI's continuous drain worker creates **value-0.bamlvalue** lazily at the first captured draft (under that draft's thread dir), so an uncaptured run leaves no header-only file;
- **manifest.bamlcids** appears only when canonical roots were actually persisted;
- the active general boundary writer can still place a legacy body larger than 4 KiB under **blobs/sha256** as a fallback; and
- GC recognizes **uploads.pin**, but the new Project Studio hosted synchronizer that would write it does not exist on this branch.

There is no implemented root **index.jsonl** in the current branch. Run discovery scans **history/*/boundary.bamlmeta**.

## Directory identities

### Session

~~~text
<started_secs>-<32-hex-char process euid>-e<engine_id>
~~~

A session is one engine process’s working CCT/meta state. **session.bamlmeta** is the actual filename; older prose saying **meta.bamlmeta** is wrong.

### Boundary

~~~text
<created_ms>-<sanitized target up to 80 chars>-<baml_id_1_wire>
~~~

The directory is created at boundary begin, before execution, not only at completion. Once a boundary is bound, the reader uses that session’s liveness to classify a begin-only boundary as crashed/partial. Before binding, the current reader conservatively treats any live session as possible ownership; this is a heuristic, not exact identity.

## Binary formats

### BCCT: .bamlseg and .bamlcct

Both live segments and folded snapshots use the BCCT v1 container.

| Element | Literal contract |
|---|---|
| Segment magic | **BCCT** |
| Header | 112 bytes |
| Block magic | **DBLK** |
| Block header | 32 bytes |
| Block trailer | 16 bytes |
| Block alignment | 64 bytes |
| Footer magic | **BCCTFOOT** |
| End magic | **TSEG** |
| Seal trailer | 48 bytes |

Block kinds:

| ID | Kind | Primary row fields |
|---:|---|---|
| 1 | CctDelta | node, enter/end deltas, total/self/await deltas |
| 2 | NodeBirth | node, parent, function, logical thread, partition |
| 3 | SpawnEdge | edge/context/function, counts, running/awaiting deltas |
| 4 | Watermark | wall time, drained-through timestamp, events, durability/reason |
| 5 | PartitionBind | partition, boundary-local ID, 16-byte boundary ID, created time |
| 6 | FooterIndex | block index metadata |
| 7 | Reserved | none |
| 8 | NodeTotal | final node totals |
| 9 | CctHist | node plus 16 **u32** buckets |
| 10 | LlmDelta | node/model, call/token/error deltas |
| 11 | ModelBirth | model ID and name |
| 12 | Marker | loss/degraded/shed/budget/epoch marker and detail |
| 13 | Instance | bounded spawn/exact instance metadata |

Committed-prefix readers stop at the first incomplete/bad block and report torn bytes. Unknown kinds are skipped by length; an unsupported format version fails.

### BMET: session.bamlmeta and boundary.bamlmeta

| Element | Contract |
|---|---|
| Header | 8 bytes: **BMET**, NUL, v1 bytes |
| Record frame | **u32 payload length + u8 kind + JSON payload + u32 CRC32C** |
| Session kinds | begin, heartbeat, epoch close, end |
| Boundary kinds | begin, bound, complete, trigger, loss |

The JSON payload is intentional: meta traffic is small/cold and benefits from additive self-description.

### Raw/flight .bamlprof

- Flight dumps reuse the legacy event framing so existing readers can open them.
- Raw files use the **BAMLRAW1** container and rotate at 64 MiB.
- The raw stream is the profiler correctness oracle and opt-in because it restores traffic-proportional disk cost.

### Value capture .bamlvalue

The protobuf schema is [bamlvalue.proto](../../../baml_language/crates/bex_events/src/value/proto/bamlvalue.proto). Important records include:

- file header with boundary ID;
- value metadata and availability;
- trace call key;
- capture role and profiling function ID;
- canonical DAG root reference;
- log metadata;
- capture loss;
- run status/target and time anchor.

### Revision dictionary .bamldict

One length-delimited **RevisionDictionaryV1** protobuf per revision. See [bamldict.proto](../../../baml_language/crates/bex_events/src/dict/proto/bamldict.proto).

### CAS .bamlpack

Pack:

- filename suffix **.bamlpack**;
- magic **BPK1**;
- 48-byte header;
- record magic **CK**;
- kind, storage code, 32-byte CID, logical/stored lengths, payload, CRC32C;
- v1 writer uses raw storage;
- seals at 64 MiB or explicit/graceful writer shutdown; a crash may leave a recoverable unsealed/unindexed pack.

Index:

- suffix **.bamlpack.idx**;
- magic **BPKI**;
- 16-byte header;
- 256-entry fanout;
- sorted 48-byte CID entries;
- CRC trailer;
- entirely rebuildable from the pack.

## Durability and mutability

| Artifact | Mutability rule |
|---|---|
| Active ring | In memory, producer/consumer synchronization |
| Active BCCT segment | Append committed blocks; reader sees whole committed prefix |
| Sealed BCCT/snapshot | Immutable |
| BMET | Append-only, per-record CRC; torn last record tolerated |
| BAML pack | Append-only while leased; sealed/indexed at explicit/graceful shutdown; crash recovery scans an unsealed pack |
| Pack index | Rebuildable, published atomically |
| Boundary CID manifest | Appended after CAS sync, fsynced (file + boundary dir) in the same barrier |
| Retention log | Append-only tombstones |
| Provider cache/index | Rebuildable; never evidence |

## Cleanup interaction

1. Local retention prunes oldest raw files above the per-session cap, then whole history directories, whole session directories, and old legacy profiles. It does not independently prune flight/trace files.
2. Removing a boundary releases its root pins.
3. CAS GC marks live roots from retained history, flight/session pins, and uploads.pin.
4. GC closes over child CIDs.
5. Young packs remain protected by a 24-hour grace.
6. Fully dead packs are removed; partially live packs are compacted.

## Future query-provider state

The architecture deliberately does not prescribe one literal local analytical tree. A provider may use:

- rebuildable SQLite for small resident catalogs;
- Parquet for scan-heavy aggregate tables;
- direct artifact/fold adapters for already efficient native formats; or
- a mixture selected per logical relation.

Whatever is chosen:

- it lives outside the evidence authority;
- deletion followed by rebuild yields the same normalized semantics;
- a manifest/generation binds ordinary queries to a fixed universe;
- logical schema mappings declare types, nullability, grain, availability, resident/hydrated status, and capabilities;
- it never introduces a second CID/codec; and
- **control.sqlite** remains a distinct non-rebuildable control database.

Future ordinary SQL readers must bind a stable file/provider snapshot and handle generation changes or file replacement without assuming filenames live forever. This is the target D10 contract, not a guarantee of every current fold reader.

Related: [Profiler capture and CAS](../03-profiler.md#canonical-value-store), [Profiler retention and GC](../03-profiler.md#retention-and-gc), [Query system](../04-query-system.md), and [Decision register](../08-decisions.md).
