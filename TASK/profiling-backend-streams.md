# Profiling Backend: Thread-Rooted Executions and Process Streams

Status: prerequisite specification for `baml query` (see
`TASK/baml-query-scope.md` §7, item **P0**). It amends the segmented backend
landed on `paulo/re-profiling-backend` (`TASK/profiling-backend-mvp.md`, "the
MVP"). Where this document and the MVP disagree, this document wins; Section
11 lists every MVP section it amends so the MVP can be edited when this
lands. Code references are to `baml_language/crates/` at `7a351097c`.

This document is written to be implemented without further design inference.
Anything not pinned here is either (a) explicitly unchanged from the MVP, or
(b) listed in Section 12 as an open question. An implementer who finds a
third case must stop and amend this document first. It was reviewed against
the code by three independent passes (accuracy, implementability, MVP
invariants); the resolutions are folded in.

---

## 0. Why

Two independent problems with the MVP store, both observed in code:

1. **Three names for one thing.** The runtime's unit of execution is a
   parentless logical thread: `register_root` receives `root_thread_ref`,
   `run.meta` stores it, the decoder finds the owning boundary by scanning
   slots for `meta().root_thread_ref == thread_ref` (`decoder.rs:2323-2341`),
   and every root call builds a fresh `BexVm` with a fresh thread id
   (`bex_engine/src/lib.rs:3479-3499`; `next_prof_thread_id` =
   `next_bex_thread_id`, `lib.rs:2179-2181`) — no `ThreadRef` ever hosts two
   roots. Yet storage keys everything by a second random identity
   (`BoundaryId`), names the artifact a "run", and the docs say "boundary".
2. **Per-execution files and fsyncs.** Every execution, however small,
   creates 3 directories and 4 files (`run.meta`, ≥1 `.bamlcct`, ≥1
   `.bamlspans`, `run.end`), and every published file costs **4 fsyncs**
   (tmp file `sync_all`, `sync_dir(final dir)`, `usage.state` tmp
   `sync_all`, `sync_dir(root)` — `store.rs:617-648, 1327-1332, 1396-1414`)
   = **16 fsyncs per execution**, the first 4 of them **synchronously on the
   root call path** (`register_root` → `begin_boundary`, `session.rs:1048`),
   all under the project-wide `publish.lock` (cross-process) and the
   in-process `process_publish` mutex (`store.rs:307`). A `baml test`
   process with 1,000 cases is 3,000 directories, 4,000 files, 16,000
   fsyncs. Listing is `readdir` + two file opens per execution.

This specification fixes both with one change: **executions are identified by
their root thread, and a process writes one stream of segments shared by all
its executions.** Thread lifecycle becomes durable (spawn lineage for free).
Capture policy, CCT semantics, evidence fact semantics, CAS, memory
governance, and the ring/decoder substrate do not change.

---

## 1. Decisions at a glance

| # | Decision | Replaces |
|---|---|---|
| T1 | **Execution = parentless logical thread.** `ExecutionId` is the root thread's `ThreadRef` (`baml_thread_1_…`). No `BoundaryId` in any durable profiler format; no "run". | `BoundaryId` as store key; `run.meta`/`run.end`; `runs/<boundary>/` |
| T2 | **Language runtime identity is unchanged.** The host still mints a random 16-byte `baml_id_1_…` token per root call; `baml.id.*`, `boundary.id.current()`, `LocalId`, `$id` behave exactly as today. The profiler stores the token only as the root span's `runtime_id` annotation (ordinal 0) and in `RootStarted.runtime_id`. | MVP §5.1:498 "the boundary ID is also the root span's initial runtime ID" → "the host runtime token is …" |
| T3 | **One stream per process**: `streams/<process_euid hex>/{meta,data}/<seq>.*` + `stream.lock`. Segments are shared by every execution and engine of the process; groups inside a data segment are keyed by root `ThreadRef`. | per-execution directories and sequence spaces |
| T4 | **Two planes.** `meta/` (index: `StreamStarted`, `EngineStarted`, `RootStarted`, `RootEnded`) and `data/` (CCT + evidence groups). The existing CCT and evidence payload codecs are reused inside groups. | `cct/` + `evidence/` planes + two marker files |
| T5 | **No disk I/O on root admission.** Publication is batched by size, age (`publish_interval`, default 1 s), explicit flush, and engine/process close. Admission facts travel through the registry slot, not a queue. | `begin_boundary` committing `run.meta` before the root runs |
| T6 | **Thread lifecycle is durable**: evidence facts `ThreadStart` (parent, spawn call, spawn site, name) and `ThreadEnd` (status), one pair per logical thread. | thread state that lived only in the decoder |
| T7 | **Function/file tables are durable** per engine (`EngineStarted.function_table_cid` → CAS codec 2) and **wall clock is durable** per stream (`StreamStarted.zero_unix_ns`). | nothing (gaps G1/G2 in the scope doc) |
| T8 | **Publication cycle = meta-pre → data → meta-post.** `RootStarted(x)` is committed before or with the first data segment of `x`; `RootEnded(x)` is committed only after every group of `x` has been committed or lost, so its data range and health are final. | `run.end` fence |
| T9 | **Completeness per execution** = index state (from meta only) × data state (from `load()`); **liveness** = `stream.lock` held. | no liveness signal |
| T10 | **Registry slots are released at hand-off** to the stream writer; durable terminal state lives in files. | `Closing → Sealed` waiting on `run.end` |
| T11 | **Format is pre-release**: segment `SCHEMA_VERSION = 2`, CAS keeps its own `CAS_FORMAT_VERSION = 1` (object bytes unchanged), no reader for the v1 layout, root dir stays `.baml/profiles-v1`, legacy `runs/` ignored by readers and removed by `baml clean`. | — |

Unchanged and reaffirmed: capture policy and `CapturePlan` (`domain.rs`);
`ContextKey`, CCT counters and semantics (MVP §5.4/§6); evidence fact
semantics and dependency ordering (§6.5/§7.3 — Section 5.4 shows how the
batch-outcome feedback is preserved); CAS object bytes and `ValueCid`; memory
governor and sizing (§8); ring/decoder substrate; `BAML_PROFILE`; `baml
clean`; wasm = off.

---

## 2. Concepts and identity

### 2.1 Execution

An **execution** is the tree of logical threads rooted at a thread whose
`StartThread` record has `parent_thread_id == 0` (`record.rs:157-172`,
emitted at `bex_engine/src/lib.rs:3602-3609`). Its identity:

~~~rust
pub struct ExecutionId(pub ThreadRef);   // newtype in ids.rs; wire = ThreadRef wire
~~~

`ThreadRef { process_euid, engine_id, thread_id }` (`ids.rs:101`), 32 bytes
on disk (`euid[16] ‖ engine u64 BE ‖ thread u64 BE`), wire
`baml_thread_1_` + base64url-nopad(`0x01 ‖ 32 bytes`) = 58 chars
(`ids.rs:162-171`, `THREAD_REF_LEN = 33`). Uniqueness across processes comes
from the random `ProcessEuid` (see the fork guard, Section 5.8).

Every descendant thread (ordinary or detached spawn) belongs to its parent's
execution; `VmSpawner::spawn_with_function/_callable` (`lib.rs:6850-6887`)
and nested host calls create new executions; `$init_test` roots of the test
runner are ordinary executions (`test_command.rs:310,1112`); internal roots
with `RootProfileIntent::SuppressInternal` (LSP multi-project) produce
nothing. All as today.

### 2.2 Runtime identity (language surface) — unchanged

The host keeps minting a random 16-byte token per root call
(`function_call_context.rs:64`, `with_boundary_id`), the engine keeps
installing it as the entry call's runtime id (`lib.rs:3617-3619`,
`vm.rs:5550-5567`), and `baml.id.current/new/set`, `boundary.id.current()`,
`boundary.id()`/`LocalId` keep their documented behaviour and `baml_id_1_`
format (MVP §4.3, §12.4; `bex_engine/tests/identity.rs` passes unchanged).

`BoundaryId` is **kept as the Rust type** of that token (renaming touches the
legacy history/playground plane, Section 10). Inside `prof/backend` it appears
only as `RuntimeIdAnnotation.runtime_id`, `SpanRuntimeId.runtime_id`,
`RootStarted.runtime_id`, `ExecutionMetadata.runtime_id`,
`ExecutionRuntime.runtime_id` — "host/language runtime token, opaque to the
profiler". Renames (values unchanged): `BoundaryRegistry/Handle/Slot/State/
Phase/EndStatus/Metadata` → `ExecutionRegistry/Handle/Slot/State/Phase/
EndStatus/Metadata`; `RootProfileIntent::UserBoundary { boundary_id }` →
`UserRoot { runtime_id: BoundaryId }`; `InactiveReason::BoundaryStateUnavailable`
→ `ExecutionStateUnavailable`, `BoundaryStoreUnavailable` and
`BoundaryStoreIndeterminate` → folded into one `StoreUnavailable` (admission
no longer touches the store; it reads two atomics, Section 5.5); `MeasuredLayouts::boundary_slot_bytes` →
`execution_slot_bytes` (value 8 KiB unchanged); `BoundaryHealthSnapshot` →
`ExecutionHealthSnapshot` (field order/encoding unchanged).

### 2.3 Program identity — conservative content hash (decided 2026-08-24)

`ProgramId` stops being random. The **compiling host** computes, over the
exact file set handed to the compiler:

~~~text
program_hash = SHA-256( "baml-program-v1"
                      ‖ baml_version::CANONICAL_VERSION (utf8)
                      ‖ for each file, sorted by path bytes ascending:
                          path (utf8) ‖ 0x00 ‖ file bytes ‖ 0x00 )
~~~

and threads it into engine construction; `build_program_metadata`
(`bex_engine/src/lib.rs:1587-1697`) sets `program_id =
ProgramId(program_hash[0..16])` and `source_snapshot_id =
Some(SourceSnapshotId(program_hash))` (the field exists and is always `None`
today, `lib.rs:1693`). If the host provides no hash (an embedding that
compiles from memory and chooses not to), the fallback is
`ProgramId::new_random()` — random over-splits, which is the safe direction.

This is deliberately the **most conservative** identity: any byte change in
any compiled file — comments and whitespace included — or a compiler version
change yields a new `program_id`, hence new `ContextKey`s. Two runs of the
byte-identical build in different processes/engines produce identical
`ContextKey`s, so path-level aggregation (`GROUP BY context_id`) works
across executions of one build; across *builds*, aggregation uses
`fqn`/`definition_key`. Semantic per-function hashing (comment-insensitive
continuity across builds) is a later, separate change; it must not reuse the
`"baml-program-v1"` domain string.

No format changes: `EngineStarted.program_id` stays 16 bytes; identical
builds now also dedupe their `FunctionTableV1` CAS object across processes
for free. Producers of the hash: the `bex_project` compile pipeline
(`SourceState` holds the `FsPath → String` file map,
`bex_project/src/project.rs:1122`) and `bex_engine/tests/common/mod.rs::
compile_for_engine` (hash the single in-memory source the same way, path
`"<test>"`). The pack pipeline does NOT embed the hash yet (landed
behavior): a packed binary materializes with `source_content_hash = None`
and each engine falls back to a random `ProgramId`, so packed runs are not
comparable across processes. Tracked gap; the fix is a pack-manifest hash
embedded at pack time.

Engine id (`EngineId`) is unchanged.

---

## 3. On-disk layout

~~~text
.baml/
  profiles-v1.lock                      # unchanged: shared lease per store; exclusive for `baml clean`
  profiles-v1/
    publish.lock                        # unchanged: project-wide publication/accounting lock
    usage.state                         # unchanged (BAMLUSE1)
    tmp/                                # unchanged
    streams/
      <process_euid hex32>/
        stream.lock                     # exclusive while the owning store is open (liveness, Section 6.4)
        meta/<seq:020>.bamlmeta         # index plane (Section 4.3)
        data/<seq:020>.bamldata         # CCT + evidence groups (Section 4.4)
    cas/sha256/<2 hex>/<64 hex>.bamlvalue   # unchanged bytes; codec 1 = value body, codec 2 = function table
    runs/                               # LEGACY v1 layout: never read; counted by usage; removed by `baml clean`
~~~

Rules:

- One stream directory per `ProcessEuid`. At most one `ProfilerStore` per
  `(root, ProcessEuid)` is open at a time: the store creates/opens
  `stream.lock` and holds it `lock_exclusive` until drop; a second open
  (same or other process) fails with `StoreFailureReason::StreamInUse`.
  Sequential re-open (tests; `ProfilerSession` rebuilt in one process) is
  allowed: `open` scans `meta/` and `data/` for the highest well-formed
  sequence and initialises `StreamHighWater` from it (and resolves a
  crashed-indeterminate tail exactly as the MVP open-scan does for
  segments, §9.1). Tests that simulate several processes pass distinct
  `ProcessEuid`s.
- Sequences are `u64`, start at 1, dense and contiguous **per stream per
  plane**; MVP allocation/commit rules (§9.1): a candidate is consumed only
  by `Committed` or by an indeterminate rename; `Lost` does not consume.
- Every file = body ‖ SHA-256(body) (`with_checksum`/`validate_checksum`,
  unchanged).
- Lock order: `profiles-v1.lock(shared) → stream.lock(exclusive) →
  publish.lock`. Readers take `profiles-v1.lock(shared)` for the duration of
  a call (new; today's reader takes nothing) so `baml clean` cannot remove
  files underneath them; readers never take `publish.lock`.

---

## 4. Formats

Integers are big-endian. `str` = `u32 len ‖ utf8` (encoder fails closed on
> 64 KiB: `EncodeError::StringTooLong`). `opt X` = `0x00` | `0x01 ‖ X`.
`ThreadRef` = 32 bytes. `CallRef` = `ThreadRef ‖ call_id u64` (40).
`ContextKey` = 32. `CallSiteSourceSpan` = `file_id, start_offset,
end_offset, line` (4×u32; inside the unchanged `BAMLERR1` sub-codec the
order is `file_id, line, start, end`, `evidence.rs:195-198`). Segment `SCHEMA_VERSION: u16 = 2`;
`CAS_FORMAT_VERSION: u16 = 1` (new constant; CAS objects currently embed
`SCHEMA_VERSION` at bytes 8..10, `store.rs:1236,1272,1103` — split the
constant so CAS bytes do not change).

### 4.1 Magics and versions

| constant | value | file |
|---|---|---|
| `META_MAGIC` | `b"BAMLMET1"` | `meta/*.bamlmeta` |
| `DATA_MAGIC` | `b"BAMLDAT1"` | `data/*.bamldata` |
| `VALUE_MAGIC` | `b"BAMLVAL1"` (unchanged) | CAS objects |
| `USAGE_MAGIC` | `b"BAMLUSE1"` (unchanged) | `usage.state` |
| removed | `BAMLRUN1`, `BAMLEND1`, `BAMLCCT1`, `BAMLSPN1` | — |
| `SCHEMA_VERSION` | `2` | meta/data segments |
| `CAS_FORMAT_VERSION` | `1` | CAS objects |
| CAS `CodecVersion` | `1` = value body (unchanged), `2` = `FunctionTableV1` | CAS |

Readers reject a segment with `version != 2` with
`SegmentReadError::UnsupportedVersion(v)` (remedy text: "run `baml clean`").

### 4.2 `stream.lock`

Zero-length file, created if absent. `lock_exclusive` by the owner;
readers `try_lock_shared` (Section 6.4).

### 4.3 Meta plane — `BAMLMET1`

~~~text
META_MAGIC(8) ‖ u16 SCHEMA_VERSION ‖ u64 sequence ‖ process_euid[16]
‖ u64 data_high_water ‖ u64 record_count ‖ u64 payload_len ‖ payload ‖ sha256
payload := { u8 tag ‖ u32 body_len ‖ body }*
~~~

`data_high_water` = `StreamHighWater.data` at the moment this meta segment
was encoded. Because meta-pre precedes data in a cycle (Section 5.3), every
group of an execution `x` lies in a data segment with `sequence >
data_high_water` of the meta segment carrying `RootStarted(x)`; readers use
it as the lower bound for unended executions (Section 6.2).

Decoder checks: magic, version, `process_euid` equals the stream directory
name, `record_count` equals decoded records, each `body_len` exact, no
trailing bytes, unknown tag → `MetaUnknownTag(tag)`.

| tag | record | body |
|---|---|---|
| 0 | `StreamStarted` | `u32 pid ‖ u64 zero_unix_ns ‖ str baml_version ‖ str os_arch` |
| 1 | `EngineStarted` | `u64 engine_id ‖ program_id[16] ‖ opt function_table_cid[32] ‖ opt str revision_label ‖ opt str source_label` |
| 2 | `RootStarted` | `ThreadRef(32) ‖ u64 started_ns ‖ runtime_id[16]` |
| 3 | `RootEnded` | `ThreadRef(32) ‖ u64 ended_ns ‖ u8 status ‖ u8 flags ‖ u64 data_first_seq ‖ u64 data_last_seq ‖ u64 data_segment_count ‖ u32 health_len ‖ health` |

- `StreamStarted`: record 1 of meta segment 1, nowhere else
  (`MetaStreamStartedMisplaced` otherwise). `pid = std::process::id()`;
  `zero_unix_ns = clock::started_at_epoch_ns() as u64` (`clock.rs:305`; the
  `TickConverter` zero point — `from_clock` sets `seg_base_ns = 0` at
  `base_ticks`, `clock.rs:479-515` — so `wall_ns = zero_unix_ns + x_ns` for
  every `*_ns` in the stream); `baml_version = baml_version::CANONICAL_VERSION`;
  `os_arch = format!("{}-{}", std::env::consts::OS, std::env::consts::ARCH)`.
- `EngineStarted`: once per `BexEngine` at profiler activation (Section 7.2),
  enqueued before any `RootStarted` of that engine. `function_table_cid =
  None` only if the CAS publication failed; readers then show function labels
  NULL.
- `RootStarted.started_ns = TickConverter::to_ns(admitted_ticks)` where
  `admitted_ticks = now_ticks()` sampled at the top of `register_root`
  (Section 5.5). It is **not** derived from the root `StartThread` record
  (which is emitted after admission, `lib.rs:3596-3609`, and may be lost).
  `runtime_id` = host token. There is no `entry_function_id` (Q4 resolved:
  dropped — the root span's `function_id` is in the data plane).
- `RootEnded.status` = `ExecutionEndStatus` (Succeeded=0, Failed=1,
  Cancelled=2, Panicked=3, Abandoned=4; `> 4` → `MetaInvalidStatus`).
  `flags` bit 0 = `root_started_lost` (the meta batch carrying
  `RootStarted(x)` was `Lost`), bits 1–7 reserved zero. `data_first_seq` /
  `data_last_seq` = lowest/highest data sequence whose segment contains a
  group for `x` (0/0 if none); `data_segment_count` = number of distinct
  committed data segments containing a group for `x`. `health =
  ExecutionHealthSnapshot::encode()` — 26 × u64 = 208 bytes, order unchanged
  (`decoder.rs:24-89`); `health_len != 208` → `MetaInvalidHealth`. Exactly
  one `RootEnded` per execution (`MetaDuplicateRootEnded`); at most one
  `RootStarted` (`MetaDuplicateRootStarted`).

### 4.4 Data plane — `BAMLDAT1`

~~~text
DATA_MAGIC(8) ‖ u16 SCHEMA_VERSION ‖ u64 sequence ‖ process_euid[16]
‖ u64 group_count ‖ u64 payload_len ‖ payload ‖ sha256
payload := group*
group   := root ThreadRef(32) ‖ u8 cct_health
         ‖ u64 cct_record_count ‖ u64 cct_len ‖ cct_payload
         ‖ u64 evidence_record_count ‖ u64 evidence_len ‖ evidence_payload
~~~

- Fixed group prefix = 49 bytes; `evidence_record_count`/`evidence_len`
  follow the CCT payload.
- `cct_payload` is exactly the existing CCT payload (`cct_codec.rs:38-89`),
  decoded by `decode_cct_payload(payload, cct_record_count, &[cct_health])`
  (its `RecordCountMismatch` and key-recompute checks apply; `cct_codec.rs:
  91-95`); empty (`cct_len = 0`, `cct_record_count = 0`, `cct_health = 0`)
  when the group carries no CCT delta. `cct_health` = the 1-byte
  `CounterHealth` bitmask (`cct_codec.rs:254-272`).
- `evidence_payload` is exactly the existing evidence payload
  (`evidence_codec.rs:44-83`) with Section 4.5's changes, decoded by
  `decode_evidence_payload(payload, evidence_record_count)`
  (`evidence_codec.rs:59-62`); or empty.
- Both empty → `DataEmptyGroup`. At most one group per execution per segment
  (`DataDuplicateGroup`); groups ordered by `ThreadRef` bytes ascending
  (`DataGroupOrder`). Within a group the CCT section precedes the evidence
  section; the MVP dependency rule (§6.5/§9.2) reads: "a `ContextRef::
  Normal(key)` must resolve to a definition in this group's CCT section or in
  an earlier committed group **of the same execution**".
- Decoder checks: magic, version, euid = directory, `group_count`, order,
  lengths, no trailing bytes; sub-decoder checks unchanged.
- Readers read a whole file (the trailing SHA-256 requires it) and skip
  foreign groups by slice arithmetic without decoding them.

### 4.5 Evidence fact changes

Tags 0–5 keep their numbers and `fact_count`/`tag`/`len` framing.

| fact | change |
|---|---|
| `SpanStart` (0) | **remove** leading `boundary_id[16]`. Body: `CallRef(40) ‖ opt CallRef parent ‖ ThreadRef(32) ‖ ContextRef ‖ function_id u32 ‖ opt call_site ‖ edge_kind u8 ‖ started_ns u64 ‖ selection_reasons u8 ‖ roles u8 ‖ opt (annotation_ordinal u32 ‖ runtime_id[16])` (order as `evidence_codec.rs:112-134` minus the first field). The ordinal-0 annotation's source is `ExecutionRuntime.runtime_id` (today `publisher.meta().boundary_id`, `decoder.rs:1078-1083`). |
| `SpanEnd` (1), `SpanRuntimeId` (2), `ValueOccurrence` (3), `TerminalErrorRef` (5) | unchanged |
| `ErrorCapture` (4) | **remove** `boundary_id[16]` after `ErrorCaptureId` (`evidence.rs:182-213`); sub-codec magic `BAMLERR1`/version stay. |
| `ContextRef` | `Overflow` = `1 ‖ reason u8 ‖ edge_kind u8` (today `1 ‖ BoundaryRef(40) ‖ reason ‖ edge`, `evidence_codec.rs:305-319`, `evidence.rs:302-317`). `BoundaryRef` type deleted; `ActiveCctEpoch.boundary` field and the `BoundaryRef` parameter of `ActiveCctEpoch::new` (`cct.rs:214-230`) deleted; `record_overflow` (`cct.rs:507-522`) builds the new variant; `BoundaryRuntime::new`/`fresh_epoch` (`decoder.rs:198-233`) adjusted; `backend/mod.rs:33` export removed. |
| **new** `ThreadStart` (6) | `ThreadRef(32) ‖ opt ThreadRef parent ‖ opt CallRef spawn_call ‖ opt call_site spawn_site ‖ u64 started_ns ‖ u8 kind ‖ str name`; `kind` 0 Root, 1 Spawn. Root: `parent/spawn_call/spawn_site = None`, `kind = 0`. `name` ≤ 256 bytes (`MAX_THREAD_NAME_LEN`, `record.rs:18`). |
| **new** `ThreadEnd` (7) | `ThreadRef(32) ‖ u64 ended_ns ‖ u8 status`; `status` = `ThreadEndStatus` (Completed=0, Cancelled=1, Errored=2; `record.rs:96-105`). |

Reader model is **tolerant**: `ThreadEvidence { start: Option<ThreadStart>,
end: Option<ThreadEnd> }`; a second `ThreadStart`/`ThreadEnd` for one
`ThreadRef` is `DuplicateThreadStart`/`DuplicateThreadEnd`; a missing
start, a missing parent start, or a root `ThreadStart` whose `thread_ref`
differs from the group root are **not** errors (they are population loss
already counted by the writer) — `ExecutionProfile` exposes them as
`thread_issues: Vec<ThreadIssue { thread, kind: MissingStart | MissingParent
| RootMismatch }>`.

Emission rule (decoder, Section 7.3): `ThreadStart` is pushed when the
decoder admits a thread; `ThreadEnd` is pushed only if that thread's
`ThreadStart` was pushed (dependency rule, like `SpanEnd` after `SpanStart`);
a child's `ThreadStart` is pushed regardless of its parent's. Both are
ordinary evidence facts charged `evidence_item_min_bytes` under
`Owner::Evidence`; reservation failure counts `evidence_queue_full` and the
thread's population is still folded into the CCT.

### 4.6 CAS object codec 2 — `FunctionTableV1`

Published once per engine (Section 7.2); `cid = ValueCid::for_encoded(
CodecVersion(2), body)` (unchanged hashing), so identical programs dedupe.

~~~text
body := u16 body_version(=1)
      ‖ u32 function_count ‖ function*
      ‖ u32 file_count ‖ file*
function := u32 function_id ‖ str fqn ‖ str display_name ‖ opt str definition_key
          ‖ u8 kind ‖ opt str kind_detail ‖ u8 origin
          ‖ opt str source_file ‖ opt (u32 file_id ‖ u32 start ‖ u32 end)
          ‖ opt str package_name ‖ u16 namespace_count ‖ str*
file     := u32 file_id ‖ str path
~~~

`kind` codes: 0 Bytecode, 1 SysOp (`kind_detail` = sysop name), 2 Native, 3
NativeUnresolved (`RuntimeFunctionKind`, `metadata.rs:38-43`); `origin`: 0
UserDefined, 1 Companion, 2 Internal, 3 Builtin, 4 AutoDerive
(`metadata.rs:46-52`). Code 255 is reserved "unknown" at the codec level
only (no Rust variant; decoders map 255 and any other unlisted value to
`None`). Functions sorted by `function_id` ascending, files by `file_id`
ascending; `files` = every distinct `file_id` referenced by any function's
`source_span`, mapped to that function's `source_file` (`lib.rs:1619-1624`
builds both from the same `Function`). Deterministic: identical
`FunctionMetadataTable` ⇒ identical bytes on every platform (golden test).

---

## 5. Writer

### 5.1 Store API (`prof/backend/store.rs`)

Removed: `BoundaryRunMeta`, `RunEnd`, `RunEndSegmentFence`, `SegmentHighWater`,
`begin_boundary`, `AdmittedBoundary`, `BeginBoundaryResult`,
`FinishBoundaryResult`, `SegmentKind`, `StoreFileKind::{RunMeta, RunEnd,
CctSegment, EvidenceSegment}`, `run_directory`,
`encode_run_meta/run_end/segment`, `decode_run_meta/run_end/cct_segment/
evidence_segment`.

Added / changed:

~~~rust
pub struct StreamId(pub ProcessEuid);
pub enum Plane { Meta, Data }
pub struct StreamHighWater { pub meta: u64, pub data: u64 }   // last COMMITTED sequence per plane; 0 = none;
                                                               // an indeterminate candidate is not reflected
pub enum StoreFileKind { MetaSegment, DataSegment, CasObject }
// StoreOpenError is a struct { reason: StoreFailureReason, source: Option<io::Error> } (store.rs:77-81);
// new reasons: StoreFailureReason::StreamInUse
pub enum MetaRecord { StreamStarted{..}, EngineStarted{..}, RootStarted{..}, RootEnded{..} }  // plain structs, encoded by the store
pub struct DataGroup { pub root: ThreadRef, pub cct_health: CounterHealth, pub cct_record_count: u64, pub cct: Vec<u8>,
                       pub evidence_record_count: u64, pub evidence: Vec<u8> }

impl ProfilerStore {
    pub fn open_native(root: PathBuf, disk: DiskBudget, stream: StreamId) -> Result<Arc<Self>, StoreOpenError>;
    pub fn open(root: PathBuf, disk: DiskBudget, platform: Arc<dyn StorePlatform>, stream: StreamId) -> Result<Arc<Self>, StoreOpenError>;
    pub fn stream(&self) -> StreamId;
    pub fn high_water(&self) -> StreamHighWater;
    /// One meta segment. `terminal` = "contains at least one RootEnded" (one attempt under a latched disk gate, MVP §5.3/§8.2).
    pub fn publish_meta(&self, records: &[MetaRecord], terminal: bool) -> PublishBatchResult;
    /// One data segment. Groups sorted by root ThreadRef, each non-empty.
    pub fn publish_data(&self, groups: &[DataGroup]) -> PublishBatchResult;
    pub fn publish_cas_object(&self, codec: CodecVersion, body: &[u8]) -> (ValueCid, PublishCasResult); // unchanged
    pub fn resolve_indeterminate(&self, token: IndeterminateToken) -> ResolveIndeterminateResult;       // unchanged
    pub fn is_normal_admission_open(&self) -> bool;                                                      // unchanged
}
pub trait StorePlatform { /* existing: available_space, sync_dir, before_rename */
    fn sync_file(&self, file: &File) -> io::Result<()> { file.sync_all() }   // NEW: ALL file fsyncs (segment tmp file AND usage.state tmp file) route here
}
pub enum PublishBatchResult {
    Committed { sequence: u64 },
    Lost(StoreFailureReason),
    /// The store was already indeterminate (another publication's post-rename state): this batch was NOT written.
    /// Keep it pending and retry after `resolve_indeterminate` succeeds.
    Blocked(IndeterminateToken),
    /// This batch IS the post-rename candidate at `sequence`; when the token resolves `Committed` it counts as
    /// `Committed { sequence }`. (Replaces the single `Indeterminate(token)` variant, which conflated the two;
    /// the MVP's `reserve_and_publish` advanced the fence on it even when nothing was written — `store.rs:775-781, 868-871`.)
    Indeterminate { token: IndeterminateToken, sequence: u64 },
}
~~~

`publish_meta`/`publish_data` = MVP `reserve_and_publish` with a per-plane,
per-stream sequence (same allocation/commit/lost/indeterminate rules,
`store.rs:723-789`). `publish_bytes`/`publish_bytes_locked` (`store.rs:
526-648`: gate check, `process_publish`, indeterminate check, `publish.lock`,
final-path-exists, usage read, disk guard, `create_dir_all`, tmp write +
`sync_file`, `before_rename`, re-check, rename, `sync_dir`, usage write)
are unchanged except the final path
`streams/<euid>/{meta,data}/<seq:020>.{bamlmeta,bamldata}` and `sync_file`.
Under a latched disk gate the writer publishes **one** combined meta batch
with `terminal = true` (Section 5.3 step 3′): pending `EngineStarted`/
`RootStarted` of roots admitted before the latch are prepended to the
eligible `RootEnded`s. That is accepted — no new roots are admitted while
latched, and the disk check in `publish_bytes_locked` still bounds the
write.

`open` additionally: `create_dir_all(streams/<euid>/{meta,data})`; create
and `lock_exclusive` `stream.lock` (failure → `StreamInUse`); scan both
planes for the highest well-formed sequence (initial `StreamHighWater`);
record `opened_pid = std::process::id()`; insert the `ProcessEuid` into a
process-global `OPEN_STREAMS: Mutex<HashSet<ProcessEuid>>` (removed on
`Drop`; used by the reader's same-process liveness short-circuit, Section
6.4). `ProfilerStore::is_indeterminate() -> bool` (new, atomic) mirrors the
`indeterminate` slot (set in `retain_indeterminate`, cleared on resolution).

### 5.2 Stream writer (new, `prof/backend/writer.rs`)

One `StreamWriter` per `OnSession`, held behind a `Mutex` (uncontended:
driven **only by the consumer thread** — `maintain_sessions`,
`runtime.rs:153-159`, after `maintain_ready_boundaries` regardless of its
early returns, and from `Flush`/`EngineClosed` handling in `consumer.rs`;
`checkpoint()` callers on other threads only read through the mutex):

~~~rust
struct StreamWriter {
    store: Arc<ProfilerStore>,
    pending_meta_pre: Vec<MetaRecord>,           // StreamStarted, EngineStarted, RootStarted
    pending_meta_post: Vec<MetaRecord>,          // RootEnded awaiting eligibility (5.3 step 4)
    pending_groups: BTreeMap<ThreadRef, PendingGroup>,  // one per execution with unpublished work
    pending_bytes: u64,                          // Σ transferred reservation bytes of pending_groups
    oldest_pending: Option<Instant>,             // enqueue time of the oldest item in any pending_* set
    exec_index: HashMap<ThreadRef, ExecPublication>,   // data_first/last/count + flags; removed at RootEnded encode time (5.3 step 5)
    indeterminate: Option<IndeterminateToken>,   // token the writer must resolve before publishing again (own or foreign)
    inflight: Option<InflightBatch>,             // the batch written under an Indeterminate{token, sequence}; applied on resolution
    meta_queue: Reservation,                     // one fixed reservation, see 5.4
}
struct PendingGroup { cct: Option<SealedCctEpoch>, evidence: Vec<EvidenceFact>, handle: ExecutionHandle,
                      batch_ids: Vec<u64>, stats: EvidenceBatchStats, reservations: Vec<Reservation>, bytes: u64 }
struct ExecPublication { first: u64, last: u64, count: u64, root_started_lost: bool }
enum InflightBatch { Meta { sequence: u64, records: Vec<MetaRecord>, pre: bool }, Data { sequence: u64, groups: Vec<PendingGroup> } }
~~~

Inputs (consumer thread only):

- `enqueue_meta(record)` — `StreamStarted` (session start, **only if
  `store.high_water().meta == 0`**; a re-opened stream never re-emits it),
  `EngineStarted` + `RootStarted` (from `take_admitted`, Section 5.5),
  `RootEnded` (finalization, Section 5.6).
- `hand_off(handle, root, cct: Option<SealedCctEpoch>, evidence: Vec<EvidenceFact>, batch_id: Option<u64>, stats: EvidenceBatchStats)` —
  from the per-execution runtime on epoch rollover (`batch_id = None`,
  CCT only), evidence flush, and finalization. A hand-off with no contexts,
  no overflow and no evidence is a no-op (never creates a group; §4.4
  `DataEmptyGroup`). Reservations transfer to the writer; released at
  `Committed`/`Lost`. `bytes` = Σ accounted reservation bytes (the unit of
  `pending_bytes` and of step-4 packing). **Merge rule** when a
  `PendingGroup` for `root` already exists: `cct` merged by `ContextKey` (tuples must be identical within one
  execution — mismatch is an invariant violation: `debug_assert!`, keep the
  first), counters added saturating with `CounterHealth` OR-ed, overflow
  added by `(reason, edge)`, evidence appended, `batch_ids`/`stats`/
  reservations/`bytes` summed. One execution therefore never has two
  unpublished groups, and a later fact is lost atomically with the start it
  depends on.

### 5.3 Publication cycle

`publish_if_due(now: Instant, force: bool)`:

1. If `indeterminate` is set → `store.resolve_indeterminate(token)`.
   `StillIndeterminate` → return (nothing publishes; MVP §7.2; `high_water`
   does not advance). `Committed` → if `inflight` is `Some`, apply the
   `Committed { sequence }` handling of that batch exactly as in step 3/4/5
   (it *was* written at that sequence), clear `inflight`; clear
   `indeterminate`; continue.
2. `due = force ‖ pending_bytes >= sizing.segment_target_bytes ‖
   oldest_pending.elapsed() >= config.publish_interval`. If not due, return.
3. **meta-pre** (skipped under a latched gate — see 3′): if
   `pending_meta_pre` non-empty → `publish_meta(pre, false)`.
   `Committed` → clear. `Lost(reason)` → process-global `meta_batch_lost +=
   1`; for each `RootStarted` in it set `exec_index[x].root_started_lost`
   (creating the entry with `first = last = count = 0` if absent); drop the
   batch (never retried). `Blocked(t)` → `indeterminate = Some(t)`, keep the
   batch pending, return. `Indeterminate { t, seq }` → `inflight = Meta{..}`,
   `indeterminate = Some(t)`, return.
   3′. If `!store.is_normal_admission_open()` (disk/unavailable latched),
   step 3 is skipped and `pending_meta_pre` is drained into the step-5 batch
   ahead of the `RootEnded`s (order: `StreamStarted`, `EngineStarted`,
   `RootStarted`, `RootEnded`), which is published with `terminal = true`;
   `root_started_lost` is set only if that combined batch is `Lost`.
4. **data**: while `pending_groups` non-empty: take groups in `ThreadRef`
   order while the running sum of `bytes` stays ≤ `segment_target_bytes`
   (a single oversize group goes alone); encode (unchanged CCT/evidence
   encoders); `publish_data`. `Committed { seq }` → for each group:
   `exec_index[root]`: `first = seq` if `first == 0` else unchanged, `last =
   seq`, `count += 1`; `decoder.apply_batch_outcome(handle, &batch_ids,
   Committed)`; `record_evidence_committed(stats)` into the execution's
   health (Section 5.4 "health sink"); release reservations. `Lost(reason)` →
   for each group: into the health sink fold `record_evidence_publish_failed
   (stats)`, `cct_segment_publish_failed += 1` if `cct.is_some()`,
   `evidence_segment_publish_failed += 1` if `!evidence.is_empty()` (one
   increment per lost *group*, not per merged hand-off);
   `decoder.apply_batch_outcome(handle, &batch_ids, Lost)`; release
   reservations. `Blocked(t)` → keep the groups pending, `indeterminate =
   Some(t)`, return. `Indeterminate { t, seq }` → `inflight = Data{..}`
   (groups moved out of `pending_groups`), `indeterminate = Some(t)`,
   return. Repeat while groups remain (once a cycle starts it drains).
5. **meta-post**: a `RootEnded(x)` is *eligible* iff `pending_groups` has no
   entry for `x` and `inflight` carries no group for `x`. Fill
   `data_first/last/count` and `flags` from `exec_index[x]` **at encode
   time** (absent entry ⇒ zeros), remove `exec_index[x]`; publish all
   eligible (plus the 3′ prefix, if any) as one `publish_meta(post, terminal
   = true)`. Outcomes as step 3 except `Lost` → `root_ended_lost += 1` per
   `RootEnded` (never retried).
6. After the cycle: `oldest_pending = Some(now)` if anything remains pending
   (e.g. an ineligible `RootEnded`), else `None` (worst-case latency is
   therefore 2 × `publish_interval`). If `force` and anything remains
   pending that is publishable (not waiting on `inflight`/`indeterminate`),
   repeat from step 3.

**Health sink.** "The execution's health" means `ExecutionRuntime.health`
while `registry.validate(handle)` succeeds (slot still live); otherwise the
pending `RootEnded(root)`'s health (which by the eligibility rule is still
pending whenever a group of that execution can still complete or fail —
`debug_assert!`).

`apply_batch_outcome` is today's `apply_evidence_batch_outcome`
(`decoder.rs:2007-2032`: flips `SpanState::Queued(batch_id)` →
`Durable`/`Lost`, rewrites `error_targets` to
`TerminalErrorTarget::Lost(EvidenceSegmentPublishFailed)`), called on the
consumer thread before the next decode; it is a no-op for a handle whose
slot has been released (`discard_execution` removed that execution's
`calls`/`error_targets`/pending maps, `decoder.rs:2068-2138`, and handles
carry the generation). Because publication runs synchronously on the
consumer thread, no fact is decoded between a publish attempt and its
outcome.

Ordering guarantees that follow: `StreamStarted` precedes every record of its
stream; `EngineStarted` precedes the first `RootStarted` of its engine
(Section 5.5 `take_admitted` ordering); `RootStarted(x)` is committed before
or in the same cycle as the first data segment containing `x` (step 3 before
step 4) unless that meta batch was `Lost` (then `RootEnded.flags` records
it); `RootEnded(x)` is committed only after every group of `x` is committed
or lost, so its data range, count and health are final. A live reader
between step 4 and step 5 sees `x` as `NoRootEnded` → `Running`; that is the
documented live state.

`publish_interval`: new `ProfilerConfig.publish_interval: Duration`, default
1 s, read in `ProfilerConfig::default()` from `BAML_PROFILE_PUBLISH_INTERVAL_MS`
(new, tests) when set; `Duration::ZERO` = publish on every consumer pass that
has pending work; `Duration::MAX` = manual (only `force`). The consumer's
park is already timed (`WAKE_INTERVAL = 50 ms`, `consumer.rs:14,103`), which
bounds the age-trigger latency; the park timeout must be `min(WAKE_INTERVAL,
publish_interval)` when `publish_interval < 50 ms`.

### 5.4 Bounds

- `pending_groups` memory = transferred reservations (already governed);
  `due` fires at `segment_target_bytes`, so pending data is bounded by one
  target plus one in-flight batch.
- `pending_meta_*` is bounded by construction: ≤ 1 `StreamStarted` + engines
  + 2 × `execution_slots` records. The writer takes **one** reservation at
  session start: `meta_queue_bytes = (2 × sizing.execution_slots + 64) ×
  meta_record_bytes` under `Owner::Writer`/`ReservationClass::General`
  (`Owner::Writer` exists with a 64 KiB minimum charge, `memory.rs:17-26,
  142`; `sizing.rs:61`; it is used only as a test pressure filler today),
  where `meta_record_bytes = 320` (new `MeasuredLayouts` field; `RootEnded`
  encodes to ≈ 286 bytes). Reservation failure → session `Off` with a
  `SetupDiagnostic` (as the producer-queue failure, `session.rs:247-266`).
  No per-record reservation and no per-record failure mode; if a process
  activates more than 64 engines the 65th `EngineStarted` forces an
  immediate meta-pre publication instead of growing the vector.
- `exec_index` entries are removed when their `RootEnded` is encoded (step
  5); `RootEnded` enqueue is infallible (the `meta_queue` reservation covers
  2 × slots), so there is no other removal path.
- At most one indeterminate token per store (unchanged).
- `StreamReader::open` is O(executions ever written to the stream) — the
  index plane is ≈ 60 + 290 bytes per execution (Section 6.1).

### 5.5 Admission (replaces `register_root` steps 3–4, `session.rs:1019-1087`)

`ProfilerSession::register_root(intent: RootProfileIntent, root_thread_ref, program_id)`
(`runtime_id` travels inside `RootProfileIntent::UserRoot { runtime_id }`;
`revision_label`/`source_label` move to `EngineStarted`):

1. `admitted_ticks = now_ticks()`.
2. `SuppressInternal` → `Inactive(Suppressed)`; `Off` → `Inactive(Disabled)`
   (unchanged).
3. Fork guard: if `std::process::id() != session.pid` →
   `Inactive(ForkedProcess)` (new reason; Section 5.8).
4. If `!store.is_normal_admission_open() || store.is_indeterminate()` →
   `Inactive(StoreUnavailable)` (two atomic reads; today the gate and the
   indeterminate state are consulted inside `begin_boundary`,
   `store.rs:454-471`; no file is touched either way). Keeping the
   indeterminate check is what bounds `pending_meta_*` (Section 5.4): roots
   are not admitted while the store cannot publish.
5. `registry.reserve_root(ExecutionMetadata { root_thread_ref, runtime_id, admitted_ticks })`
   → slot (with `admitted_pending = true`, new slot flag) or
   `Inactive(ExecutionStateUnavailable)` (unchanged).
6. `publishers[slot] = Some(ExecutionRuntime::new(generation, root_thread_ref, runtime_id, program_id, process_euid, engine_id))`.
7. Return `Active(ActiveRootAdmission { profiler: Active(ActiveRootProfiler { root_thread_ref }), completion })`.

No store call, no lock, no I/O, no queue send. `reserve_root` stores
`metadata` and sets `admitted_pending` under the slot's `metadata` mutex, so
the pair is read atomically. On the consumer thread, at the **top of every
`maintain_sessions` pass** (before `maintain_ready_boundaries`),
`registry.take_admitted() -> Vec<MetaRecord>` (1) scans slots for
`admitted_pending` (acquire) and clears it, (2) **then** drains the
registry-side `engines_started: Vec<EngineStarted>` (Section 7.2), and
returns records ordered so that every `EngineStarted` precedes any
`RootStarted` of the same `engine_id`; the writer enqueues them into
`pending_meta_pre` in one call, so they can never be split across cycles.
(Seeing a slot with acquire guarantees visibility of the earlier engine push;
an engine can only admit roots after its `engine_started` push.)
`take_admitted` reads only the registry slot — `publishers[slot]` (step 6)
need not be set yet. `acknowledge_terminal` asserts `admitted_pending ==
false` (debug) and clears it. `RootStarted { root: meta.root_thread_ref,
started_ns: clock.to_ns(meta.admitted_ticks), runtime_id }`. This path is
infallible after `reserve_root` succeeded; the `DecoderCommand` producer
lane (`session.rs:97-146`, bounded `try_send`, lossy) is **not** used for
admission.

### 5.6 Finalization (replaces `finalize_ready_boundary`, `decoder.rs:2187-2321`)

For a ready execution (phase `Closing`, consumer drain completed — unchanged
detection via `ready_handles`/`consumer_drain_completed`, `boundary.rs:
339-367`, `session.rs:929-983`): `discard_execution` (today
`discard_boundary`, `decoder.rs:2068`), merge producer health (unchanged),
seal the current epoch, `writer.hand_off(handle, root, sealed_epoch,
remaining_evidence, batch_id)` (one call; merged per Section 5.2), then
`writer.enqueue_meta(RootEnded { root, ended_ns, status, .. })` where
`ended_ns = clock.to_ns(closing_ticks)`, `closing_ticks` = `now_ticks()`
recorded by the registry at the one-to-zero lease release (new field set in
`finish_thread`, `boundary.rs:305-337`, exposed via `closing_facts`,
`boundary.rs:369-385`), `status = registry.closing_facts(handle)` (default
`Abandoned`, as `decoder.rs:2303-2305`). Then `registry.acknowledge_terminal
(handle, ExecutionPhase::Released)` immediately (slot cleared, generation
bumped, free-listed; `acknowledge_terminal` accepts only `Released`); the
`ExecutionRuntime` is dropped. `ExecutionPhase` = `Open, RootReturned,
Closing, Released` (`Sealed`/`ReleasedIncomplete` removed). `FinalizationState`
is removed. The nuance of today's `flush_evidence` ("on CCT `Lost`, drop the
evidence batch", `decoder.rs:1957-1965`) disappears: CCT and evidence of one
hand-off are one group in one segment and share one outcome.

Checkpoints: `ProfilerCheckpoint` is replaced by

~~~rust
pub struct StreamCheckpoint { pub high_water: StreamHighWater, pub pending_groups: u32, pub pending_meta: u32,
                              pub oldest_pending_age: Option<Duration>, pub publication_inflight: bool }
pub struct ExecutionCheckpoint { pub root: ThreadRef, pub health: ExecutionHealthSnapshot, pub queued: QueueHealthSnapshot,
                                 pub data_first_seq: u64, pub data_last_seq: u64 }   // None after Released
~~~

`flush_and_join(timeout)` (`consumer.rs:55-65`) → `drain_to_idle` then
`publish_if_due(force = true)`; `engine_closed` (`consumer.rs:67-77`) does the
same for the engine's finalized executions (executions still `Open` at engine
close — LSP cancel — are not ended; they read `Running` while the stream is
alive, `Abandoned` afterwards).

### 5.7 Failure behaviour (delta to MVP §5.3/§7.2/§8.2)

| event | MVP | now |
|---|---|---|
| disk guard trips pre-rename | batch `Lost`, gate latched, later roots profiler-off | same; a lost data batch may carry groups of many executions — each gets its exact counts (still-pending `RootEnded`s absorb them); pending `RootEnded`s are published once under `terminal = true` |
| post-rename `sync_dir` failure | one global indeterminate, `publish.lock` held | same; blocks both planes and CAS; the written batch is `inflight` and is applied as `Committed` on resolution; other batches stay pending (`Blocked`); new roots are not admitted while indeterminate |
| crash within `publish_interval` of admission | `run.meta` existed → "incomplete run" visible | nothing visible for that execution (honest loss window) |
| crash after data committed, before `RootStarted`/`RootEnded` meta | `run.end` synchronous | execution may be `NoRootStarted` (groups exist, not listed — Section 6.5) or `NoRootEnded` → `Abandoned`; bounded by `publish_interval` |
| terminal publication definitely rejected | `ReleasedIncomplete`, missing `run.end` | `RootEnded` `Lost` → `root_ended_lost += 1`, execution `NoRootEnded`; slot already released |
| tail **meta** segment deleted after the fact | n/a | invisible (no fence on meta); interior meta gaps are `IndexCorrupt` |
| `process::exit` without flush | tail loss (consumer async) | `baml run`/`baml test` call `flush_and_join(5 s)` on every exit path (Section 7.5); executions with live detached descendants at exit read `Abandoned` |

The durability window (`publish_interval`, 1 s) is the deliberate trade for
removing per-execution fsyncs. Hosts that want per-execution durability call
`flush_and_join` after each root.

### 5.8 Fork guard

`ProcessEuid::current()` and `ProfilerSession::global()` are process
`OnceLock`s; a `fork()` child inherits the euid, the open store, and the
`stream.lock` open-file-description (flock is shared across fork), so two
processes would allocate the same sequences (`Lost(PathConflict)` →
`GATE_UNAVAILABLE`). The session records `pid` at creation; `register_root`
compares `std::process::id()` (step 3) and returns `Inactive(ForkedProcess)`
in a child. The child profiles nothing (its consumer thread did not survive
the fork anyway); this is the documented behaviour, gated by a Unix test.
Known false positive: while a forked child lives it still holds the
inherited `stream.lock`, so the parent's stream reads `alive` even after the
parent exits.

---

## 6. Reader (`prof/backend/reader.rs`, rewritten)

### 6.1 API

~~~rust
pub fn list_streams(root: &Path) -> Result<Vec<StreamId>, ReadError>;          // readdir streams/, 32-hex names only
pub struct StreamReader { pub stream: StreamId, pub header: Option<StreamStarted>, pub engines: Vec<EngineStarted>,
                          pub roots: Vec<RootIndexEntry>, pub high_water: StreamHighWater, pub alive: bool, pub index_gaps: Vec<u64> }
pub struct RootIndexEntry { pub root: ThreadRef, pub started: Option<RootStarted>, pub ended: Option<RootEnded> }
impl StreamReader {
    pub fn open(root: &Path, stream: StreamId) -> Result<Self, ReadError>;     // reads ALL meta segments; never opens data/
    pub fn execution(&self, id: ExecutionId) -> Result<ExecutionReader, ReadError>;
    pub fn orphan_groups(&self) -> Result<Vec<ThreadRef>, ReadError>;          // EXPENSIVE: scans data/ headers for roots absent from the index
}
pub fn list_executions(root: &Path) -> Result<Vec<ExecutionSummary>, ReadError>; // all streams; meta planes only
pub struct ExecutionSummary { pub id: ExecutionId, pub stream: StreamId, pub engine_id: EngineId, pub program_id: Option<ProgramId>,
    pub runtime_id: Option<BoundaryId>, pub started_ns: Option<u64>, pub started_unix_ns: Option<u64>, pub ended_ns: Option<u64>,
    pub status: ExecutionStatus, pub index_state: IndexState, pub health: Option<ExecutionHealthSnapshot>,
    pub data_first_seq: u64, pub data_last_seq: u64, pub data_segment_count: u64 }
pub enum ExecutionStatus { Running, Abandoned, Succeeded, Failed, Cancelled, Panicked }
pub enum IndexState { Complete, NoRootEnded, RootStartedLost, IndexCorrupt }   // NoRootStarted executions are not listed
pub struct ExecutionReader { .. }
impl ExecutionReader {
    pub fn load(&self) -> Result<ExecutionProfile, ReadError>;                 // folds data segments [first..=last]
    pub fn read_value(&self, cid: ValueCid) -> Result<DecodedCasObject, ReadError>;        // unchanged
    pub fn function_table(&self) -> Result<Option<FunctionTable>, ReadError>;               // via EngineStarted.function_table_cid
}
pub struct ExecutionProfile { pub summary: ExecutionSummary, pub data_state: DataState,
    pub contexts: HashMap<ContextKey, MergedContext>, pub overflow: HashMap<(OverflowReason, EdgeKind), CctCounters>, pub cct_health: CounterHealth,
    pub threads: HashMap<ThreadRef, ThreadEvidence>, pub thread_issues: Vec<ThreadIssue>,
    pub spans: HashMap<CallRef, SpanEvidence>, pub errors: HashMap<ErrorCaptureId, ErrorCapture> }
pub enum DataState { Complete, Incomplete(Vec<DataIssue>) }
pub enum DataIssue { MissingDataSegment(u64), CorruptDataSegment(u64), GroupCountMismatch { expected: u64, found: u64 }, NoRootEnded }
~~~

`DurableRunReader`, `ProfileRun`, `RunReaderCursor`, `RunReadError` are
removed. `ReadError` = the `RunReadError` variants minus `InvalidFence`/
`SegmentBeyondFence`/`MetadataMismatch` (`MissingSegment`/`SequenceMismatch`
now carry `Plane`), plus `StreamNotFound`, `ExecutionNotFound`,
`MetaDuplicateRootStarted`, `MetaDuplicateRootEnded`,
`MetaStreamStartedMisplaced`, `MetaUnknownTag`, `MetaInvalidStatus`,
`MetaInvalidHealth`, `DataDuplicateGroup`, `DataEmptyGroup`, `DataGroupOrder`,
`DuplicateThreadStart`, `DuplicateThreadEnd`, `UnsupportedVersion`.

### 6.2 Status and completeness

For execution `x` in stream `s` (all decidable from the meta plane except
`DataState`):

- `started = RootStarted(x)` present; `ended = RootEnded(x)` present.
- Listed iff `started ‖ ended`. `status` = `ended.status` if `ended`; else
  `Running` if `s.alive`; else `Abandoned`.
- `index_state`: let `s` = meta sequence of `x`'s first record, `e` = meta
  sequence of `RootEnded(x)` if present, `G` = the stream's meta sequence
  gaps. `IndexCorrupt` iff (∃ g ∈ G with `g > s` and (`e` absent or `g <
  e`)) or (`ended && !started && flags bit 0 == 0`); else `RootStartedLost`
  if `ended && flags bit 0`; else `NoRootEnded` if `!ended`; else
  `Complete`. `status` rules apply regardless.
- `DataState` (from `load()` only): `Complete` iff `ended && for every seq in
  [data_first ..= data_last]` the data segment exists and decodes, and the
  number of those segments containing a group for `x` equals
  `data_segment_count`; else `Incomplete(issues)`. Without `RootEnded`,
  `ExecutionSummary.data_first_seq = data_last_seq = data_segment_count = 0`
  and `load()` folds `[m.data_high_water + 1 ..= s.high_water.data]` where
  `m` is the meta segment carrying `RootStarted(x)` (or `[1 ..=
  s.high_water.data]` if `RootStarted` is absent), filtering groups by root,
  and reports `DataIssue::NoRootEnded` (so-far values).
- Listing never opens `data/`. `load()` opens exactly the range above; cost is
  O(bytes the process wrote during that range), not O(execution) — a long
  execution in a busy stream pays for its neighbours' bytes (acknowledged;
  a per-group offset table is a possible later optimisation).

### 6.3 Fold and join validation

`load()` = for each data segment in range, for the group with `root == x`:
merge CCT (unchanged `merge_cct` rules), merge evidence (unchanged
duplicate/missing rules + Section 4.5 thread rules), then
`validate_dependencies` (checks unchanged: every span has a start, every
`ContextRef::Normal` resolves, every `TerminalErrorTarget::Capture` resolves;
**new**: `ContextRef::Overflow{reason, edge}` resolves to
`overflow[(reason, edge)]`). **Severity is new**: these are hard errors
(`Err(ReadError::Missing*)`, today's behaviour) only when `DataState` would
otherwise be `Complete` and `health.cct_segment_publish_failed == 0 &&
health.evidence_segment_publish_failed == 0`; otherwise each dangling
reference becomes `DataIssue::UnresolvedDependency { kind, key_or_call_ref }`
and `DataState::Incomplete` (a lost or missing group legitimately strands
later deltas/facts; "folds the rest" must not panic or error). `error_stack`
unchanged. `started_unix_ns = header.zero_unix_ns + started_ns` when the
stream header exists, else `None`.

### 6.4 Liveness

`alive` = `File::open(streams/<euid>/stream.lock)` succeeds and
`try_lock_shared` returns `Err(WouldBlock)`; on success the reader unlocks
immediately and `alive = false`; missing file → `false`. A reader running in
the same process as an open store for that stream short-circuits
`alive = true` via the process-global `OPEN_STREAMS` set maintained by
`ProfilerStore::open`/`Drop` (Section 5.1) — do not probe the lock: on Linux
NFS `flock` is emulated per-process and would report the process's own
stream dead. `fs2` = `flock(2)`/`LockFileEx`; a fresh `File::open` is a new
open-file-description, so the probe is correct on local filesystems. Point in
time; `ExecutionSummary.status = Running` means "at bind time".

### 6.5 Tolerated damage

| observation | reader result |
|---|---|
| meta sequence gap | `index_gaps` on the stream; `IndexCorrupt` for executions whose records could lie after the gap; listing still returns what was read |
| data segment in an execution's range missing / corrupt | `MissingDataSegment` / `CorruptDataSegment`; `load()` folds the rest, `DataState::Incomplete` |
| groups for `x` exist but no meta record for `x` (meta batch lost before crash) | not listed; discoverable via `orphan_groups()`; `baml query` internal profile may expose it |
| checksum/decode failure at any sequence (publication is tmp+fsync+rename, so a final path is never partially written; a failure is corruption) | `Corrupt*Segment(seq)`; for the data plane folded into `DataState`, for the meta plane a gap entry in `index_gaps` |
| legacy `runs/` directory | ignored |

---

## 7. Engine and session changes

### 7.1 `bex_engine` admission call site (`lib.rs:3560-3610`)

`register_root(RootProfileIntent::UserRoot { runtime_id }, root_thread_ref, program_id)`.
`install_boundary_id_for_current_call` unchanged. Root `StartThread` record
unchanged (parent 0/0).

### 7.2 Engine activation

`activate_profiling` (`lib.rs:2139-2154`; called from the constructor paths
`lib.rs:1717,1737,1777` or from the LSP deferred commit,
`bex_project/src/project.rs:939`): after the session is active, encode
`FunctionTableV1` from `program_metadata()` (`lib.rs:2167`), call
`session.publish_function_table(bytes) -> Option<ValueCid>` (→
`store.publish_cas_object(CodecVersion(2), bytes)`; `Lost`/`Conflict` →
`None` + process-global `function_table_publish_failed += 1`;
`Indeterminate(t)` → `None` and the token is parked in the store and picked
up by the writer's step 1), then `session.engine_started(engine_id,
program_id, cid, revision_label, source_label)` which pushes `EngineStarted`
onto the registry-side `engines_started` vector (under the registry's
existing mutex; drained by `take_admitted`, Section 5.5 — **not** the
`ControlMsg` channel, whose drain order relative to the slot scan would let a
`RootStarted` precede its `EngineStarted`, and not the lossy producer lane). This is the
one deliberate synchronous publication (4 fsyncs under `publish.lock`) and
happens once per engine, before any root of that engine can be admitted.

### 7.3 Decoder

- `find_boundary_by_root` → `find_execution_by_root`, comparing
  `runtime.root`.
- `SpanStart`/`ErrorCapture` no longer read `publisher.meta().boundary_id`;
  the ordinal-0 annotation uses `runtime.runtime_id`.
- `insert_thread(resources, thread_ref, exec, parent: Option<(ThreadRef, CallRef)>, spawn_site, started_ticks, name)`
  (today `decoder.rs:1356`, signature `(resources, thread_ref, boundary,
  spawn_parent: Option<ContextKeyProjection>, spawn_site)`) pushes
  `ThreadStart`; `EndThread` consumption pushes `ThreadEnd` (ts from the
  record). **New retention**: the decoder keeps `ts_ticks` and `name` of
  `StartThread`/`StartThreadSpawn` and `ts_ticks` of `EndThread` through its
  pending tables (`insert_pending_thread` `decoder.rs:1522-1557`,
  `insert_pending_thread_end` `:1559-1600`, both of which drop them today).
- `rollover_if_needed`, `flush_evidence`, and finalization call
  `writer.hand_off` instead of `publisher.publish_*`; the rule "seal and
  include the current CCT epoch whenever evidence is handed off"
  (`flush_evidence`, `decoder.rs:1932-2005`) is preserved.
- `ExecutionRuntime { generation, root: ThreadRef, runtime_id: BoundaryId, cct: Option<ActiveCctEpoch>, evidence: EvidenceBatch, health: ExecutionHealthSnapshot }`.
- `DecoderCommand` variants carry `ExecutionHandle` (rename only).

### 7.4 Registry (`boundary.rs` → `execution.rs`)

Rename; `ExecutionMetadata { root_thread_ref, runtime_id, admitted_ticks }`;
new slot flag `admitted_pending` (set with `metadata` under the slot's
`metadata` mutex, cleared by `take_admitted`, asserted clear by
`acknowledge_terminal`); registry-side `engines_started: Vec<EngineStarted>`;
`take_admitted()` per Section 5.5; `closing_ticks` recorded in `finish_thread`
at the one-to-zero transition and returned by `closing_facts`;
`acknowledge_terminal(handle, Released)`.

### 7.5 CLI

`baml run` and `baml test` call `bex_events::prof::flush_and_join(
Duration::from_secs(5))` before every `process::exit` (`run_command.rs:711,
1081`, `main.rs:37` paths; today nothing in `baml_cli` flushes).
`ProfilerConfig::store_root` is resolved by the CLI from the discovered
project root (`project_load::find_project_root_from`) and passed into the
session (scope doc G4); `BAML_PROFILE_DIR=<path>` (new) overrides for tests.

---

## 8. Migration and cutover

- v1 layout never shipped; no reader, no migration (MVP §12). Segment
  `SCHEMA_VERSION = 2`; CAS bytes unchanged (`CAS_FORMAT_VERSION = 1`).
- A root containing `runs/` opens normally; `scan_physical_usage` counts it;
  readers ignore it; `clean_profiles_v1` removes the whole root. The LSP
  ignore-pattern test string `.baml/profiles-v1/runs/run.meta`
  (`baml_lsp_server/src/lib.rs:1220`) becomes a `streams/…/meta/…bamlmeta`
  path (behaviour unchanged).
- `.baml/profiles-v1` keeps its name ("v1" = store generation).

---

## 9. Acceptance gates and tests

MVP §14 gates that do not mention `run.meta`, `run.end`, per-boundary
directories, `Sealed`/`ReleasedIncomplete`, or `begin_boundary` remain in
force verbatim. The following replace or add gates. Gate numbers assume
`process_memory_bytes = 256 MiB` (`segment_target_bytes = 4 MiB`,
`execution_slots = 2016`) unless stated.

**Formats (unit):**
- Golden byte fixtures + cross-platform SHA-256: meta segment with all four
  records; data segment with two groups (CCT-only; evidence-only including
  `ThreadStart`/`ThreadEnd`/`SpanStart` without boundary id/overflow
  `ContextRef`); `FunctionTableV1`. Regenerate: `evidence.rs` error-record
  goldens (`error_record_codecs_have_cross_platform_goldens`, :642-656, and
  the `BoundaryRef`-constructing test :626-640), `evidence_codec.rs` golden
  (:623-631, encodes `SpanStart.boundary_id` and an overflow `BoundaryRef`),
  and edit the `cct_codec.rs` fixture (:346-354, constructs `ActiveCctEpoch::
  new(.., BoundaryRef, ..)`; its hash is unchanged).
- Property tests: every truncation / trailing-bytes / duplicate-group /
  ordering / record-count violation is the named typed error.
- `open` twice on one `(root, StreamId)` → `StreamInUse`; sequential re-open
  resumes sequences; two stores with distinct `StreamId`s publish
  concurrently without collisions and with correct `usage.state`.
- `Lost` does not consume a sequence; indeterminate blocks both planes and CAS
  until exact resolution; a `RootEnded` meta batch gets one attempt under a
  latched disk gate (ports `store.rs:1707,1732,1749`).

**Writer (session tests, `publish_interval = Duration::MAX` + explicit
`flush` unless stated):**
- Admission performs no file-system call: a `StorePlatform` that panics on
  any I/O; `register_root` returns `Active`; p99 admission latency < 20 µs
  over 10k roots (no I/O, no lock).
- One root with one spawn, then flush: meta segment 1 =
  `StreamStarted, EngineStarted, RootStarted`; data segment 1 = one group
  with `ThreadStart ×2`, `ThreadEnd ×2`, root `SpanStart`/`SpanEnd`; meta
  segment 2 = `RootEnded{data_first = data_last = 1, count = 1, flags = 0}`.
  Exactly 3 segment files; `sync_file + sync_dir` count = `2 (open) +
  4 × (3 + published CAS objects)` (per publication: segment tmp file, final
  dir, `usage.state` tmp file, root dir — unchanged protocol; a `Reused` CAS
  hit costs 0; all counted via the platform).
- 1,000 sequential roots on one engine, then flush: data segments
  `≤ ceil(total_encoded_bytes / segment_target_bytes) + 1`, meta segments
  `= 2`. File count is O(bytes), not O(executions).
- With `publish_interval = 1 s`: a root that completes at t=0 is on disk by
  t ≤ 1.05 s without a flush (age trigger via the 50 ms park).
- Ordering: with pending meta-pre and a size-due data batch, the meta
  sequence commits before the data sequence; inject meta-pre `Lost` → the
  execution's `RootEnded.flags` bit 0 set, reader `RootStartedLost`.
- Batch outcome: inject data `Lost` between two rollovers of a long span;
  `apply_batch_outcome` marks the start `Lost`; the later `SpanEnd` is
  counted `StartUncommitted`; the reader's `validate_dependencies` passes
  (ports MVP §7.3 "no dangling reference"). Inject `Indeterminate` then
  resolve: the in-flight batch is applied once as `Committed`; inject
  `Blocked` then resolve: the pending batch is published once at the next
  sequence (no duplicate records/groups).
- Re-open: a second `ProfilerSession` in the same process (same
  `ProcessEuid`) resumes sequences and emits no second `StreamStarted`.
- Indeterminate admission gate: while the store is indeterminate,
  `register_root` returns `Inactive(StoreUnavailable)` and `pending_meta_*`
  does not grow.
- Two hand-offs of one execution in one cycle merge into one group
  (CCT counters summed, evidence concatenated, one segment).
- `RootEnded` eligibility: a `RootEnded` enqueued while its group is pending
  is not in the next meta-pre; it is in meta-post with the final range (pin
  a 3-data-segment execution: `first = 1, last = 3, count = 3`).
- Slot release: `ready_handles()` empty and the slot reusable immediately
  after finalization, before any publication.
- `flush_and_join` empties `pending_*`; `engine_closed` publishes the
  engine's finalized executions and leaves `Open` ones untouched.
- Fork (Unix): after `fork()`, `register_root` in the child returns
  `Inactive(ForkedProcess)`; the parent's stream is intact.
- Program identity: two engines compiled from byte-identical file sets have
  equal `program_id` and equal root `ContextKey`s; flipping one comment byte
  or the compiler version changes both; an engine constructed without a hash
  gets a random `program_id` (cross-platform golden for the hash formula).
- Ports of `final_drain_keeps_structural_end_when_value_command_was_lost`
  and `reordered_descendant_facts_resolve_and_attribute_losses_to_their_run`
  (`session.rs:1222,1335`) asserting on `ExecutionProfile`;
  `active_boundary_exposes_o1_committed_checkpoint` (:1184) rewritten for
  `StreamCheckpoint`/`ExecutionCheckpoint`.

**Reader:**
- `list_executions` on a fixture with 3 streams × N executions returns
  status/`index_state` per Section 6.2 and opens no `data/` file (counting
  platform / fs shim).
- Deleting an interior data segment in range → `MissingDataSegment`;
  corrupting it → `CorruptDataSegment`; removing a group's segment count
  mismatch → `GroupCountMismatch` (ports `profiling_backend.rs:200-210`).
- Liveness: while a store is open `alive == true` and an unended execution
  reads `Running`; after drop, `Abandoned`; same-process reader short-circuit.
- Torn tail while alive is ignored; while dead it is `Corrupt`.
- Wall clock: `started_unix_ns - zero_unix_ns == started_ns`.
- `orphan_groups()` finds an execution whose meta batch was lost.

**Engine / e2e:**
- `bex_engine/tests/profiling_backend.rs`: rewrite the four tests onto
  `list_executions`/`ExecutionReader` (root id assertions stay on the
  `baml_id_1_` runtime token via `RootStarted.runtime_id`; no directory
  assertions); `read_evidence` helper (:95-113) retargeted. `identity.rs`
  passes **unchanged**.
- `baml_tests/examples/profiling_e2e_verify.rs` + `profiling_e2e/run.py`:
  2 executions in 1 stream, both `IndexState::Complete`, `DataState::Complete`,
  `Succeeded`, health default; exactly 3 segment files under `streams/` for
  the packed single-process run (meta, data, meta).
- `baml_tests/benches/profiling_overhead.rs` (:134-153 hard-codes
  `profiles-v1/runs`): retarget; add: a 200-case `baml test` fixture in one
  process with `BAML_PROFILE_PUBLISH_INTERVAL_MS=60000` → ≤ 3 segment files
  (+ CAS), vs ≥ 800 files today.
- Packed stress (MVP log "36-task production-policy stress") re-run: zero
  loss, invariants on `ExecutionProfile`.
- wasm: `cargo check -p bex_events -p bex_engine -p bridge_wasm --target
  wasm32-unknown-unknown` green.

**Docs:** MVP amendments of Section 11 applied in the same PR.

---

## 10. Explicitly out of scope

- Renaming the `BoundaryId` Rust type and the legacy history/playground
  plane's `boundaryId` wire fields (`run.rs`, `run_wire.rs`, `history/`,
  `playground_ws.rs`, `bridge_wasm`, TS `BoundaryId`) — untouched; it keeps
  keying by the host token, which is still minted.
- Durability stronger than `publish_interval`; per-group checksums / group
  offset tables; retention/GC; an index beyond the meta plane; folding the
  usage ledger write into the publication cycle (would halve fsyncs further —
  later).
- Exposing `ThreadRef` wire strings in language APIs or CLI output (the query
  layer exposes them; `baml.id.*` does not change).
- Profiling in `fork()` children.

---

## 11. Amendments to `TASK/profiling-backend-mvp.md` (apply when landing)

| MVP section | change |
|---|---|
| §1.2 | word "boundary" → "execution (root thread)" only |
| §1.5, §1.6 | lifetime/barrier text: "boundary" → "execution"; durability statement: publication batched per Section 5.3; `run.meta`/`run.end` → meta plane records |
| §5.1 | `StartBoundary{…}` control message (MVP 375-379, 492, 542) → admission facts in the registry slot + `take_admitted` (Section 5.5); `BoundaryHandle/State/Phase` renamed; phases `Open, RootReturned, Closing, Released`; "Second, `begin_boundary` publishes `run.meta`" paragraph (458) and the `Indeterminate` admission text replaced by Section 5.5; line 498 per T2 |
| §5.3 | steps 9–10 replaced by Section 5.6; the `FinishBoundaryBarrier` message (661, 681) is described as what it is in code — the slot `finish_ready` flag + wake — and renamed execution |
| §6.5 | `BoundaryRef` removed; `ContextRef::Overflow{reason, edge_kind}`; dependency rule "earlier committed segment" (also §9.2 2060-2066) → "earlier committed group of the same execution" |
| §7.1 | add `ThreadStart`/`ThreadEnd`; `SpanStart`/`ErrorCapture` lose `boundary_id` |
| §7.2 | state machine unchanged; owner is the stream writer; per-plane per-stream sequences; "New boundaries fail store admission while latched" (1170) → "new roots run profiler-off while latched" |
| §7.3 | "`run.end`" → "`RootEnded`"; "after a successful terminal barrier and `run.end` publication" → "after `RootEnded` is committed"; equations unchanged |
| §8.2 | interface → `publish_meta / publish_data / publish_cas_object`; `begin_boundary`/`finish_boundary` removed; "New boundaries start profiler-off while blocked" unchanged |
| §8.4 | remove `ProfilerBoundaryStoreIndeterminate`; add `ForkedProcess`, `meta_batch_lost`, `root_ended_lost`, `function_table_publish_failed` |
| §9.1 | layout → Section 3; headers → Sections 4.3/4.4; fences → `RootEnded.data_*`; "A crashed or still-active run has no `run.end`" (2007) → Section 6.2; "Per-boundary/per-plane publication is serialized" → per-stream |
| §9.2 | protocol unchanged; paths updated; `stream.lock` in lock order (2090-2093); `sync_file` via platform |
| §10 | `ProfilerCheckpoint` (2108-2115) → `StreamCheckpoint` + `ExecutionCheckpoint`; live readers cursor on `StreamHighWater` and filter by root |
| §12.1 | keep list: add "host runtime token minting"; §12.4 unchanged |
| §14 | gates per Section 9 |
| §15 | add "per-execution synchronous durability", "profiling in fork children" |
| §16 | add rows: Execution identity → root `ThreadRef` (§2); Layout → process streams (§3–5) |

---

## 12. Resolved decisions (2026-08-24)

1. **`publish_interval` = 1 s** (the default in Section 5.3 stands).
2. **Meta-plane loss is tolerated** (flags + counters + `orphan_groups`,
   Sections 5.3/6.5 stand); the store never latches on meta loss alone.
3. **`ProgramId` is a conservative content hash** — Section 2.3. Semantic
   per-function hashing is deferred, separate work.
