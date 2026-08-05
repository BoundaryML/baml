# BAML Observability & Profiling — Canonical Design

**Status:** Canonical. Supersedes `stale-profiling-design.md` (2026-07-30) and incorporates the 2026-08-04 query-surface decisions.
**Companion:** `studio-design.md` (the hosted product and capture-upload architecture). This document owns everything that happens on the machine where a BAML program runs: capture, aggregation, storage, local querying, and the local UI.
**State:** the architecture described in Parts I–III is **implemented and shipped** (phases P0–P9 complete; evidence in Part IV). The SQL query tier (§10) is the one major component still to build; its phases are listed in §16.

---

# Part I — The problem and the principles

## 1. Why this exists

A BAML program is an orchestration of LLM calls, tools, and ordinary functions. When it misbehaves — wrong answer, slow run, surprise bill — the developer's honest position without observability is *"we don't really know what happened."* The industry-standard fix, per-call event logging (one record per function call, OpenTelemetry-style), was measured against real BAML workloads before this design existed, and the numbers ended the argument:

- A 3.8-second hot loop produced **1.69 GB** of profile events at **446 MB/s** — extrapolating to **38.5 TB/day** for one busy process.
- Replaying 4,096 events through the legacy run-store projection took **65 s** and generated **2.21 GB** of JSON patches (quadratic).
- Transcript-style value capture grew **N(N+1)/2**: each LLM turn re-captured the whole conversation.

Per-call event logging is the disease. This design's central move is to stop writing one record per call and instead maintain **aggregates that grow with the program's shape, plus bounded exact-evidence windows, plus content-addressed values** — while keeping the always-on cost so low it never needs to be turned off.

The results, measured end-to-end on the shipped implementation (full table in §13):

- **74.4 ns/call** total profiling overhead — 35% *cheaper* than the per-event profiler it replaced (113.6 ns), while writing ~52,000× less.
- **4.5 KB** on disk for a 5-million-call run (was 235 MB): a **52,224×–70,200×** reduction.
- **2.62 ms** to open a run and render the first frame (was 1,654.7 ms): **632×**.
- Transcript storage **≥20×** smaller at 64 turns via content-addressed dedupe.

## 2. Product commitments (first principles)

1. **Observability is on by default.** No spans, no SDK calls, no config. `baml run` records; the data is there when you need it. The corollary is a hard cost budget: if it isn't nearly free, it can't be default-on.
2. **Cost grows with unique behavior, never with traffic.** Storage and memory are proportional to *distinct calling contexts × active time windows × distinct content* — never to call count. Five million calls through one code path cost the same few counters as five thousand.
3. **No silent truncation.** Every bound in the system (rings, windows, budgets, retention) has a counter, a marker record, or an explicit error. An answer computed from partial evidence must be *identifiable* as partial. Bounded never means silent.
4. **The producer never blocks.** The program being observed pays ~10 ns/call to append fixed-width records to a lock-free ring; everything else happens on a background consumer thread. Backpressure degrades observability (visibly), never the program.
5. **Reads are O(pixels), never O(events).** The UI and query paths fold aggregates sized to the viewport; opening a multi-gigabyte history must not require materializing it.
6. **Identity is integers, assigned at compile time.** Function identity is a dense `u32` stamped by the compiler; names live in a per-revision dictionary written once. No strings on any per-call path.
7. **One fold engine, one public query language.** A single Rust fold engine (`bex_query`) privately powers the UI over the native storage. The one *public* query surface is the **ClickHouse SQL dialect over versioned, grain-named views** — locally via `baml query` (clickhouse-local over Parquet projections of sealed artifacts), hosted via a `(version, sql)` endpoint (studio doc §16.6). There is no bespoke query language. (History: a pipeline DSL, "BQL", was designed and a v1 was built; it was removed by the 2026-08-04 decision — §10.1.)
8. **Local-first, cloud-shaped.** Everything lands under the project's `.baml/` directory in formats designed to be projected, uploaded, and served by the hosted product without re-modeling.

## 3. The three planes and the grain principle

All observability data lives in one of three planes, distinguished by *what question they can answer honestly*:

| Plane | Contains | Bounded by | Answers |
|---|---|---|---|
| **Tally** (the CCT) | Per-calling-context counters: calls, errors, self/await time, duration histograms, LLM tokens | Unique contexts (p99 ≈ 3,537 nodes) | *How much, how often, how slow, how many errored* — population-true, always |
| **Tape** (exact windows) | Exact events: recent-call ring (last 4,096), flight-recorder dumps (16 MiB, on trigger), opt-in full traces, opt-in raw firehose | Fixed byte/slot budgets | *What exactly happened, in order* — within a declared window |
| **Values** | Captured inputs/outputs/errors as content-addressed DAGs | Capture policy + dedupe | *What data actually flowed* — for captured calls |

**The grain principle** is the single most important consumer-facing rule: the tally is **population** data (one row per code path, exact totals over *all* calls); the tape and values are **instance** data (rows per *recorded* call — a deliberate sample). A count computed over instance data answers "how many *recorded*…", never "how many…". Every query surface in this design — the UI, the SQL views, the documentation — is organized so the two grains are named apart and cannot silently impersonate each other (§10.2). Nothing true is ever dropped: the population totals are complete in the tally; what is *never materialized* is per-call detail outside the windows, because producing it is the 38.5 TB/day disease.

## 4. What this enables (user stories)

Stories are grouped by persona; each is served by a named mechanism. (P0 = launch-blocking, P1 = fast-follow, P2 = later. All P0/P1 mechanisms below are shipped except the SQL tier, whose stories are marked ▸.)

**The app developer (local loop).** See the exact LLM input/output for a wrong answer (values; on by default for root+LLM calls). Find an accidental hot loop in seconds (a 36 M-call run collapses to a 3-node CCT naming the runaway path). Diagnose a failed run: error payload + the failing helper's actual arguments + the exact events just before the failure (error trigger → staged-value promotion + flight-recorder dump). Open yesterday's run (durable history + retention). Is it my code or the model? (self vs await accounting per context). Count every LLM call including hidden retries; detect byte-identical duplicate prompts (LLM counters + value CIDs ▸ `GROUP BY cid`). Iterate on a long agent transcript without drowning the disk (value DAG dedupe). Compare two runs after a prompt edit (UI diff view; ▸ SQL joins over dictionary-aligned grains).

**The production operator.** Reconstruct what the service was doing at 03:04 (CCT time series + flight dumps). Attribute a production error to exact failing inputs (error-trigger promotion). Find a latency regression by *calling context*, not just function name (context-keyed histograms). Correlate a deploy with a behavior change (cross-revision alignment via `definition_key`, §6.3). Detect a runaway loop before it detonates the bill (live CCT deltas). Trust the data: explicit loss markers, watermarks, never silent truncation (engine facts surfaced in the UI; ▸ telemetry self-accounting views). Enforce retention and deletion (retention engine + CAS GC + audit records).

**The AI agent.** Agents are the sharpening persona: they need bounded results, stable IDs, and machine-checkable evidence. Enumerate recent runs as a bounded index with stable ULID ids (runs grain). Fetch the exact input that caused an error within a byte budget, with child-CID handles for selective descent (budgeted hydration, §9). Find the hottest calling context (CCT folds). Diff a function's outputs across runs by CID equality — the Merkle short-circuit (▸ SQL over CID columns). **Verify my own fix**: same inputs (matched by input CID), did outputs change? (▸ SQL join across two runs on input CID). The 2026-08-04 decision trades the previous design's fail-closed query-language guardrails for full SQL expressiveness plus documented schema semantics; the agent reads the schema documentation and checks completeness facts explicitly (§10.2).

**The engineering lead.** LLM spend by root feature and model (LLM counters ▸ SQL JOIN against the org's own price table — dollar cost is computed at query time, never stored). Which features changed behavior after a model upgrade (revision alignment + value drift by CID). Privacy audit: what value classes are captured where (capture-flags bitfield in the dictionary + audit records ▸ audit grain view).

**Requirements these stories force into the data model** (all shipped): LLM enrichment persisted per context (model, tokens, error class); duration histograms in the storage schema from day one (p95/p99 are unanswerable retroactively from sums); the telemetry observable as data (watermarks, loss counters, shed markers); cross-revision identity as a *join* (`definition_key` + `def_content_hash`) because dense ids are per-revision; value CIDs as a first-class query primitive; ULID run ids whose *raw 16-byte payload* sorts chronologically (the base64url string form does not — pagination must decode cursors or sort by `created_ms`).

---

# Part II — Architecture (as built)

## 5. The capture plane

### 5.1 Producer: records and rings

The VM emits fixed-width, little-endian records into a per-thread lock-free ring (`bex_events::prof::{record,ring}`). Nine record types (tag byte + payload):

| Tag | Record | Size | When |
|---|---|---|---|
| 0x01 | `CallFunction` | 54 B | function entry: thread, call_id, parent_call_id, function_id, ts, optional call-site span |
| 0x02 | `EndFunction` | 26 B | function exit: status Ok/Errored/Cancelled/Exited |
| 0x03 | `StartThread` | 36 B + name ≤256 | logical thread birth: parent thread + spawning call (the spawn edge's raw material) |
| 0x04 | `EndThread` | 18 B | thread end + status |
| 0x05 | `SetFunctionId` | 41 B | `$id` boundary override annotation (ring/dump visibility only; not CCT identity — shipped contract per §3-OQ7 of the prior design) |
| 0x06 | `SuspendThread` | 22 B | await park: reason (SysOp/Await/AwaitAny/EarlyYield) + suspend_seq |
| 0x07 | `ResumeThread` | 30 B | self-contained resume (carries suspend_ts → reorder-immune) |
| 0x08 | `LlmCallMeta` | 38 B | per-LLM-call enrichment: model_id, tokens in/out, provider/parse/retry flags |
| 0x09 | `ModelBirth` | 8 B + name | model-name interning (once per model per engine) |

`MAX_RECORD_LEN = 292`. Ready-inline sysops emit no suspend/resume (correctly counted as running). The producer cost is an append + fences: ~10 ns. Ring overflow policy is `BAML_RING_OVERFLOW_POLICY = abort | shed` — abort in dev (loud, debuggable), the shedding ladder in servers (§5.6).

### 5.2 Consumer: the background thread

One background consumer thread per process drains all rings (~250 ms cadence plus high-water wakes) and feeds every downstream plane in a single pass per drained byte-range: the CCT engine (§7), the recent-call ring, the flight recorder, the raw firehose (when enabled), and session storage (§8). Control messages (`bind_boundary`, `complete_boundary`, `flight_dump`, snapshot/live-segment taps) arrive over an mpsc channel with a forced wake. Engines shorter than one window (<250 ms CLI runs) still mint their session at close — no run is too fast to record.

### 5.3 Capture contracts and defaults

Four contracts, tabulated so every surface can state exactly what is promised:

| Contract | Default | Promises | Does NOT promise |
|---|---|---|---|
| **Aggregate CCT** | ON everywhere | counts, total/self/await ns, status counts, histograms, LLM counters, spawn aggregates — per context per window, with watermarks | exact ordering, per-call timestamps, values |
| **Values** | ON for root + LLM functions; opt-in per call via `$id = boundary.id().capture(inputs=, output=, error=)`; trigger-promoted for `promote_on_error` | every selected value captured or an explicit `CaptureLoss`; content-addressed; carries `function_id` | values for unselected calls |
| **Flight recorder** | ON (16 MiB native / 4 MiB wasm) | exact events within a declared, queryable window | events older than the window |
| **Full trace** | OFF (opt-in, bounded) | exact per-thread event segments up to budget, then explicit `TraceBudgetExhausted` | unbounded history |

**Triggers** connect always-on to exact-evidence: `OnError` (root or policy-matched errored/cancelled close → flight dump + promotion of staged values in the failing subtree), `OnLatencyMs(t)` (default 30 s), `Manual`. Rate limits: ≥5 s between dumps, ≤16 per boundary, dropped dumps counted. A firing trigger may also raise the boundary's capture flags for the rest of the run.

**Host defaults**: CLI/SDK — CCT on, root I/O + LLM Auto + error promotion, recorder on, full trace off. Playground — same plus log capture. wasm — in-memory, inline-only values, 4 MiB recorder. CI — `BAML_HISTORY=0` disables durable capture wholesale (the named privacy switch; sessions still record unless profiling is off).

### 5.4 Value capture mechanics

Captured values are deep-copied out of the VM heap into a trace-local arena (`TraceHeap`) at capture time (reserve-before-copy: a failed reservation does zero work), then drained asynchronously. Each capture carries its `(thread_id, call_id)` key **and its `function_id`** — stamped at the capture site where the VM knows the callee — so values are attributable to functions with zero joins and zero env flags. Root input/output/error captures come from the boundary context; per-call captures from the `$id = boundary.id().capture(...)` side channel; log bodies via the log capture hook.

**Trigger-promoted staging** (the retroactive story — "when a helper errors, I want the args it actually received"): `promote_on_error` captures land in a byte-bounded staging ring (32 MiB native / 8 MiB wasm) tagged speculative; released free at normal frame close (no serialization, no hashing, no I/O); on a trigger, drafts in the failing subtree are promoted to the durable queue with `role: promoted` + the trigger id. Evictions are counted and reported (`staged_evicted`) so "the buffer was too small" is visible and tunable.

**Continuous drain**: values drain while the boundary is open (high-water wake at ~½ budget + coarse interval); the pending budget is a flow-control window, not a per-run cap. Canonicalization, hashing, and pack appends run on a dedicated per-process **value drain service** thread — never on the prof consumer — with its CPU measured separately (C10).

### 5.5 Flight recorder

A bounded ring of **raw drained bytes** — one memcpy on the drain path, zero transcode until a trigger fires. 16 MiB ≈ 200k call pairs ≈ 11 s of a working-agent trace (and 21 ms of the pathological hot loop, which is exactly what the CCT is for). Whole-chunk FIFO eviction with counted, queryable eviction state. A dump transcodes retained chunks into `sessions/<sess>/flight/<ts>-<trigger>.bamlprof` — the *legacy event framing*, so every existing reader works — and appends a `BoundaryTrigger` record to bound boundaries so the UI can jump from a CCT node to its exact-event evidence.

### 5.6 The shedding ladder

Capacity model: one consumer core sustains ~20 M pairs/s at the 50 ns gate; a single hot loop generates ~9.5 M pairs/s, so two pathological producers can exceed one core. Under `shed` policy the consumer degrades in strict order, each step counted and marked in the stream: (1) stop flight-recorder memcpys → (2) stop full-trace transcode → (3) defer value canonicalization (bounded queue, then `CaptureLoss`) → (4) shed structural ranges (drop drained ranges; affected threads degrade via the resync path §7.2; `shed_ranges`/`shed_events` counters). **CCT aggregation is never disabled, and shed mode never aborts.**

### 5.7 Raw firehose (opt-in)

`BAML_PROFILE_RAW=1` writes every drained byte-range verbatim to `sessions/<sess>/raw/raw-NNNNNN.bamlprof` (`BAMLRAW1` container: 64 B header with euid/engine/clock calibration + u32-framed ranges, 64 MiB rotation). It is the correctness oracle's ground truth (§12) and the maximal exact-event source; it reintroduces per-call disk cost by design, which is why it is opt-in.

## 6. Compile-time identity

### 6.1 Dense function ids

`function_id` is a dense `u32` assigned by a single compiler finalizer over the final function pool: 0 = unknown/unattributable (the sentinel every degraded path folds into), 1 = spawn-closure, 2–15 reserved, real functions from 16. The finalizer (`finalize_program_identity`) runs at compile tail, on incremental re-link, and at pack load; `Function.function_id` is never serialized (borsh-skipped) — identity is always recomputed against the revision, so a pack and its dictionary can never disagree.

### 6.2 Revisions and dictionaries

`RevisionId` = BLAKE3-256 over (source snapshot × toolchain × compile options), wire form `baml_rev_1_<base64url>`; `SourceSnapshotId` similarly over sorted file content hashes. Every artifact header carries the revision. The **revision dictionary** (`.baml/dict/baml_rev_1_….bamldict`, ~180 KB, written once per revision, idempotent tmp+rename) maps `function_id → {fqn, file, span, kind, definition_key, owner type, lambda identity, package/namespace, capture_flags, def_content_hash}`. The consumer guarantees dictionary-before-first-referencing-artifact ordering; a missing dictionary degrades to explicit `fn#<id>` labels, never silent renames.

### 6.3 Definition keys: the cross-revision join

Dense ids are meaningful only within `(revision_id, ·)`. Cross-revision queries join on **`definition_key`** (e.g. `function:user.extract_invoice`, stable across recompiles) and annotate with **`def_content_hash`** — a BLAKE3 over the definition's *behavioral* content (types + bytecode projection that excludes spans/names/docstrings and canonicalizes pool references to definition keys), so unrelated edits leave hashes byte-identical (golden-pinned) and "code changed here" is a computable badge. Renames break the join by design; hash equality across a rename is a hint, not identity. This contract underpins deploy-correlation and diff views, and is the join spine of the SQL tier's cross-revision views (§10.2).

### 6.4 No pre-enumerated paths

Calling contexts are *not* enumerated at compile time (dynamic dispatch and recursion make it impossible; measured cardinality is tiny — corpus p99 3,537 nodes). Compile time contributes the dense id and function count so runtime interning is a string-free `(parent u32, function u32) → u32` map. Context node ids are dense, **session-epoch-scoped**, and meaningless outside their stream; every consumer re-keys (boundary snapshots re-densify; the cloud re-keys under its own dictionary).

## 7. The CCT engine

The calling-context tree is the always-on aggregate: one node per unique call path, columnar (SoA) storage, interned via a single FxHash map — the measured ~22 ns shape.

### 7.1 Structures

A **partition** is the spawn tree rooted at one root logical thread — inherited O(1) at `StartThread`, and the unit that binds to a boundary. Per thread: partition, uncapped call stack of `ActiveCall {call_id, node, start, flags}`, last-charge timestamp, suspend state, spawn-context node. Node columns: identity (parent, function, depth — immutable), counters (enters, ends by status, total/self/await ns), 16-bucket duration histogram, LLM side table keyed (node, model), delta bookkeeping (dirty epoch, last flushed).

### 7.2 Causal correctness: defer, retry, resync

Cross-thread record ordering is not guaranteed (per-thread rings drain independently). Any record whose causal parent hasn't arrived — a call whose parent call is unknown, an end without its call, a thread start without its parent thread, a thread end with pending state — is **deferred** (≤54 B copy) and replayed when its dependency lands; thread migration is a synchronizing await, so parents arrive within a sweep. Hot paths never defer. **Resync bounds the wait**: after `DEFER_MAX_SWEEPS = 1024` (or an explicitly dropped/corrupt range), the consumer synthesizes the missing parent as a `function_id 0` unattributable node, replays dependents against it, flags the partition degraded, and writes a loss-marker block. Aggregation never wedges; the failure is visible, attributed, and bounded.

### 7.3 Time accounting: charge-to-current

On every event, elapsed time since the thread's last charge goes to the current stack top — into `await_ns` if the thread is suspended, else `self_ns`; `EndFunction` additionally accrues `total_ns` and buckets the duration histogram. Window closes charge against per-thread *drained-event watermarks*, not consumer wall time (clamped reorders counted separately from clock anomalies). Suspend/resume records make awaiting-vs-running exact per context — the "is it my code or the model?" split — and resume records are self-contained (carry the suspend timestamp) so they tolerate reordering. Open calls accrue awaiting per window and are visible live; a 60-second LLM call is never invisible until it ends.

### 7.4 Recursion and spawn aggregation

Stacks are never truncated and counts are always exact. Past depth 512, the engine folds paths: it scans up to 8 ancestors for the same function and reuses that node (a back-edge, flagged, with folded-frame counters) — path uniqueness coarsens *visibly*; time and counts stay exact. Spawn edges are keyed `(spawn-context node, child entry function)` so 10,000 equivalent workers cost one edge plus one shared subtree, with aggregate status/time columns plus a bounded instance table (first 64 + up to 256 exceptional instances — errored, cancelled, slow, pinned — then a counted overflow).

### 7.5 Epochs and lifecycle

Node ids are scoped to a **session epoch**; the engine rotates epochs at 256 MiB of CCT bytes or 24 h, restarting ids and re-birthing live nodes, which bounds every reader's id space. Partitions are **freed at boundary completion**: final charge, last delta, checkpoint, spawn settle, snapshot fold (§8.4), then the partition's ring/instance/defer state is dropped — server memory is O(live boundaries), gated by C11.

### 7.6 The recent-call ring

Per partition, the last 4,096 completed calls plus all open calls in 56-byte slots (thread, call, node, parent, start/end, status, dump ref) — the exact-recency tier of the timeline. Eviction is counted; the UI renders "showing last 4,096 of N", never an unlabeled sample.

### 7.7 Measured cost

Integrated engine benchmark (pinned, release): hot loop **47.8–48.6 ns/pair** against the ≤50 gate; adversarial p99-cardinality shape (3,543 nodes — a synthetic modeled on the corpus p99 of 3,537) 52.7–54.4 (within the ≤60 never-exceed; flagged for CI-hardware confirmation); decode alone 9.6, recorder memcpy 2.3. End-to-end paired (5 M calls, quiet box, best-of-3): **74.4 ns/call** — versus 113.6 for the deleted per-event pipeline. Consumer CPU: 680–752 ms per 10 M records (−35% vs the transitional dual pipeline).

## 8. Storage: the session/history hierarchy

### 8.1 Layout

```
.baml/
  dict/<revision>.bamldict          # per-revision identity dictionaries (§6.2)
  sessions/<session>/               # per-engine-process working state
    cct/seg-NNNNNN.bamlseg          # 250ms delta segments (BCCT container)
    meta.bamlmeta                   # boundary/session lifecycle records (BMET)
    flight/<ts>-<trigger>.bamlprof  # flight-recorder dumps (+ .bamlcids pins)
    raw/raw-NNNNNN.bamlprof         # opt-in raw firehose (BAMLRAW1)
    trace/…                        # opt-in full-trace segments
  history/<boundary-ulid>/          # per-run durable results
    boundary.bamlmeta               # begin/bound/complete + diagnostics
    cct.bamlcct                     # folded final CCT snapshot (re-densified)
    thread-*/value-*.bamlvalue      # value capture roots
  store/                            # content-addressed value store (CAS)
    packs/pack-*.bpk1 (+ .bpki)     # chunk packs + indexes
    staging/                        # pre-promotion draft spill
  proj/v1/                          # SQL-tier Parquet projections (§10.4) — rebuildable
  index.jsonl                       # append-only run index
  retention.log                     # tombstones (audit of deletions)
```

Sessions are *working state* (subject to seal-and-collect GC); history is *results* (subject only to retention policy); `store/` is shared content (GC'd by liveness sweep, §9.5); `proj/` is a rebuildable projection cache — deleting it costs a re-projection, never data.

### 8.2 The BCCT segment container

All CCT streams share one framed container: magic `BCCT` v1 header (32 B, with session epoch + base timestamp), self-describing blocks (`DBLK`: kind u16, flags, byte len, record count, crc32c), and a seal footer (`BCCTFOOT` + `TSEG` trailer with block count, total len, footer crc). Thirteen block kinds: node_birth (1), node_delta (2), window_close (3), watermark (4), partition_bind (5), boundary_trigger (6), hist_delta (7), llm_delta (8), spawn_edge_birth (9), spawn_delta (10), spawn_instance (11), loss_marker (12), instance_range (13). Columnar-in-block layout matches the engine's SoA memory. **Torn-tail recovery is a golden-pinned contract**: a reader accepts every intact block up to the first bad frame, reports `torn=true` + valid prefix length, and never errors on a crashed process's tail. Unknown block kinds are skipped by length (forward compat); unknown *versions* fail loudly (v2 readers must be explicit).

Write cadence: 250 ms delta flush; durability D1 = 1 s group-commit fdatasync (bounded loss window for live segments), D2 = fsync-before-rename at every seal/finalize (sealed artifacts are never silently torn). Segments rotate at 4 MiB; boundary close writes the folded `cct.bamlcct` snapshot (~226 KB at p99 shape, 4.5 KB typical) with re-densified node ids and embedded birth columns — self-contained, no segment replay needed to read a completed run.

### 8.3 Meta stream and run lifecycle

`meta.bamlmeta` (`BMET\0` v1, 9 record kinds) carries session open/close, boundary begin/bind/complete, trigger records, capture-policy stamps, and diagnostics. A boundary ULID is minted at begin; `history/<ulid>/` is created at completion (or by the janitor for crashed sessions — begin-without-complete + dead heartbeat ⇒ status `crashed`, torn prefix preserved). `index.jsonl` appends one line per completed run: the O(1) discovery surface for "list recent runs".

### 8.4 Snapshot fold and the unbound lane

Partitions bind to boundaries via `partition_bind` blocks; work that never binds (background threads, pre-bind spans) folds into the session's **unbound lane**, visible in session-scoped queries and counted — never silently dropped. Boundary completion folds the partition's deltas into the final snapshot; window-aligned deltas remain in segments for time-series queries until GC'd (sealed segments are collected once every referencing boundary is complete and snapshotted).

### 8.5 Retention

Retention runs at session open + on demand: age/count/byte budgets over `history/`, tombstones appended to `retention.log` (audit: what was removed, when, by which policy), then CAS GC (§9.5). `BAML_HISTORY=0` disables durable history entirely. Deleting a run directory by hand is detected by the projection manifest (§10.4) and surfaced, not papered over.

## 9. The value plane (CAS)

### 9.1 Canonical value DAG

A captured BAML value is encoded as a **canonical Merkle DAG**: deterministic byte encoding (sorted map keys, normalized floats — NaN canonicalized, −0.0 → +0.0 — length-prefixed, type-tagged), chunked, each node/chunk hashed with BLAKE3-256 under domain separators (`baml-value-node-v1\0`, `baml-value-chunk-v1\0`), yielding a root **CID** (wire form `bamlv_1_<base64url>`). Equal values — across calls, runs, sessions, machines — produce equal CIDs; a 64-turn transcript that grows by one message per turn shares every unchanged subtree (measured ≥20× storage at 64 turns; the win grows with depth). The CID is simultaneously: dedupe key, integrity check, equality primitive (diff = compare roots, descend only unequal children — the Merkle short-circuit), and query join key.

The encoder (`bex_events::store::canon`) is **FROZEN**: golden fixtures pin bytes and CIDs; any change is a new domain version. The decoder is its exact inverse (golden decode∘encode-identity proven) with **budgeted decoding**: `DecodeBudget {max_bytes, max_depth}` walks the DAG and replaces over-budget subtrees with explicit elision markers (`ELIDED_REASON`, carrying the child CID) — a truncated read is always *labeled* and always *resumable* by CID. `to_json` renders schema-erased JSON for transport.

### 9.2 Capture roots and the store

Each capture produces a `.bamlvalue` root record: `(thread_id, call_id, function_id, role ∈ {input, output, error, log, promoted(+trigger)}, root CID, codec version, logical length, timestamp)` — small, fixed-shape, one per captured value; bodies live in the CAS. Small values may be inlined (below a threshold, the root carries the bytes); everything else is chunks in packs.

### 9.3 Packs and index

`store/packs/pack-*.bpk1`: append-only pack files (magic `BPK1`, record framing `CK`: cid, len, crc32c, zstd-compressed payload) with sidecar index `.bpki` (sorted cid → (pack, offset)); an in-memory bloom + index map serves lookups. Writes are batched by the value drain service; a chunk already present anywhere in the store is never written twice (the dedupe path is a hash lookup, not a compare).

### 9.4 Reading and hydration

`bex_query::values` provides run-scoped listing (roots joined to function identity via capture-carried `function_id`, with raw-firehose fallback for legacy captures) and budgeted hydration (Dag → inline → blob resolution). This same path backs the UI values panel, the CLI, and the SQL tier's hydration step (§10.6) — one read implementation.

### 9.5 GC and retention interaction

CAS GC is a liveness sweep: roots referenced by retained history are live; live roots pin their DAG closure (plus `.bamlcids` pin files for flight dumps that reference values); unreferenced chunks are collected by pack rewrite (copy-live-forward, atomic swap). Tombstoned runs release their pins; cryptographic content-addressing means deletion is real (no orphan copies) and audit is possible (the tombstone names what was released).

## 10. The query architecture (SQL tier) — TO BUILD

This is the one major component of this design that is **not yet implemented**. Everything it consumes (sealed artifacts, formats, read paths) is shipped; the tier itself — projector, view DDL, `baml query`, hosted endpoint — is new work (phases in §16).

### 10.1 The decision and its history

Three query surfaces were designed or built during this initiative: a pipeline DSL ("BQL", v1 built — 2,558 lines: lexer/parser/planner/executor, `baml q`, a BQF1 wire frame, value stages), a JSON query AST for the hosted product ("StudioQueryV1", designed only), and ad-hoc UI RPC. On 2026-08-04, after an adversarial research pass (archived in `old-references/bql-vs-sql.md` + research reports), the decision inverted: **one user-facing query language everywhere — the ClickHouse SQL dialect over versioned, grain-named views.** BQL and StudioQueryV1 are deleted, not frozen.

The honest reasoning chain, recorded because the losing arguments were good ones:

- BQL's real advantages were never syntax; they were (a) fail-closed grain honesty (`E_NO_EXACT_SOURCE` instead of a silently-wrong count), (b) a mandatory completeness footer on every result, (c) a sans-io engine that runs identically over mmap/wasm/HTTP. The counter-realization: (a) and (b) can be *approximated* by schema design — grain-named views + queryable coverage ledgers + documentation — at the cost of enforcement; (c) survives intact as the *private* UI engine, which is not a query language.
- The multitenant objections to hosted SQL passthrough dissolve under standard ClickHouse machinery: **RLS row policies** for tenancy, **role settings profiles + quotas** for budgets, a **versioned `(version, sql)` endpoint** for schema churn — all proven, none bespoke.
- The capability objections dissolve too: values **hydrate at query time** (nothing pre-materialized), near-live is **flush → project → query (~1–2 s)**, and the engine is **downloaded on first use** (~100–150 MB compressed / ~0.5 GB on disk — the honest price of dialect identity with hosted).
- The one thing physically lost is **in-browser querying** (no wasm ClickHouse). Accepted with eyes open: the VSCode playground talks to its native server; promptfiddle-class browser hosts become server-backed (one `baml` binary per session — studio doc); `bex_query`'s wasm build ceases to be a query surface.
- What is *knowingly* given up on honesty: nothing forces an agent to check the coverage ledger before counting instance rows. BQL would have failed that query closed. The mitigation budget goes into naming, in-schema documentation, and trap-case docs (§10.3) — "explain the schema; models are smart enough to figure it out" is the product position, and it is recorded here as an accepted risk, not an oversight.

### 10.2 The view contract

**Grain naming rule:** any noun countable at two grains carries an explicit suffix — `*_population_v1` (complete over the always-on aggregate contract; `SUM(ends_err)` is the true error count) vs `*_instances_v1` (rows exist only where an exact-evidence source covered the scope; `COUNT(*)` is a lower bound *by construction*). Registries that exist at one grain (runs, functions, revisions) go unsuffixed. Views are versioned `_vN`; additive column changes don't bump; grain/meaning changes do; N and N−1 supported concurrently, N−2 fails loudly.

**View catalog v1** (physical layout private; this is the public contract):

| View | Grain | Key columns |
|---|---|---|
| `runs_v1` | one row per boundary | run_id, created_ms, status (ok/errored/cancelled/crashed/running), revision_id, duration, total_calls, total_errors, llm/token totals, degraded, diagnostics |
| `cct_population_v1` | (run, context node) — folded totals | node/parent/depth, function_id + **denormalized fqn/definition_key/def_content_hash/path**, enters, ends by status, total/self/await ns, hist Array(UInt64)[16] |
| `cct_windows_v1` | (session, epoch, node, 250 ms window) | time-series deltas; projected lazily; **never summed with population views** (different fold state) |
| `llm_population_v1` | (run, node, model) | llm_calls, tokens in/out, provider/parse errors. Dollar cost = query-time `JOIN` against the user's own price file — never stored |
| `spawn_edges_v1` / `spawn_instances_v1` | population / bounded instances | aggregate + first-64/exceptional instances, `instances_dropped` surfaced |
| `call_instances_v1` | instances from exact windows only | source (flight_dump/full_trace/spawn_instance), window_id, thread/call/parent, start/end/status |
| `exact_windows_v1` | **the honesty ledger** — one row per exact-evidence window | source, trigger, time bounds, event_count, evicted_upto, budget_exhausted |
| `value_roots_v1` | one row per capture root | value_ord, thread/call, function identity, role (input/output/error/log/promoted), **cid**, logical_len, status |
| `value_scalars_v1` | bounded previews (≤4 KiB, policy-respecting) | cid, kind, preview, preview_truncated — the everyday "grep the prompts" surface |
| `capture_losses_v1` | one row per loss event | kind (staging_evicted/shed/drain_budget/ring_evicted/trace_budget/retention_tombstone), count, ts |
| `functions_v1` / `revisions_v1` | dictionary registries | full identity columns per §6 |
| derived: `errors_population_v1`, `error_instances_v1`, `hot_contexts_v1` | pure SQL over the above | shipped in the same DDL file |

`value_bodies_v1` is a scoped companion, not a standing view: it exists only for an explicitly hydrated scope with an explicit budget (§10.6).

Design rules baked into the catalog (each closes a named trap): identity columns are **denormalized** (fqn/definition_key materialized into every fact row, so the naive cross-revision `GROUP BY fqn` is *correct* rather than a disjoint-id-space bug); histograms are 16-element arrays with versioned SQL-lambda UDFs (`cct_hist_quantile_v1` — **integer bucket-upper-bound math, no interpolation**, so the Rust fold engine and the SQL UDF are bit-identical); run ids sort by `created_ms`, never `ORDER BY run_id` (base64 doesn't sort); `node_id` is run-scoped; window rows carry their (session, epoch) scope. Every view and grain-sensitive column carries a machine-parseable `COMMENT` (first line: `grain: instances — COUNT(*) is a lower bound; totals live in errors_population_v1`), rendered by `baml query --schema` and visible to introspecting agents.

### 10.3 Honesty via documented schema

Four layers replace BQL's enforcement: (1) **names** — the grain travels inside every query text; (2) **in-database docs** — comments on views/columns, served by `--schema`; (3) **the ledger** — `exact_windows_v1` + `capture_losses_v1` make coverage *queryable data*; every documented instance-grain example includes the join; (4) **trap-case docs** — explicit wrong/right pairs for each known hazard (instances-as-population, run_id ordering, cross-revision function_id grouping, hosted-vs-local CID spaces). The CLI additionally prints a non-SQL freshness footer ("projected through T; hot tail included; N runs in scope; M capture losses") — deliberately outside the result set so it can't be mistaken for part of the SQL contract. The residual hazard (nothing *forces* the ledger join) is accepted and named; an agent eval over the view catalog with trap cases runs before the v1 freeze (§16 gate).

### 10.4 Local execution: `baml query`

`clickhouse local` over **Parquet projections** of sealed artifacts:

- **Projection**: one Parquet file per (sealed source artifact, view), written tmp+rename to `.baml/proj/v1/<view>/run_id=<id>/part.parquet` (hive-partitioned for predicate pruning), zstd + row-group stats. Idempotent because sources are immutable. An append-only **manifest** (`proj/v1/manifest.jsonl`: source path, length, seal crc32c, projector + schema versions, outputs) makes refresh an O(#runs) diff and doubles as the drift detector — vanished sources are checked against `retention.log` (tombstoned ⇒ drop projection, surface as `capture_losses_v1(kind='retention_tombstone')`; not tombstoned ⇒ loud warning). Compaction rewrites old per-run files into consolidated monthly files at ~500 files/view.
- **Hot tail (near-live)**: active segments are readable via the committed-block scan; the projector regenerates `proj/v1/hot/*.parquet` up to the last watermark per invocation. Flush cadence 250 ms + 1 s D1 ⇒ end-to-end freshness ~1–2 s. Hot rows feed `cct_windows_v1` only, never population views.
- **Invocation**: stateless — generated init SQL (explicit Parquet schemas, view DDL, UDFs, `max_memory_usage` cap) concatenated with the user statement into one `clickhouse local` run. `--path` DDL persistence is a measured optimization, not the baseline. Engine binary: pinned LTS, downloaded on first use into `~/.cache/baml/clickhouse/<version>/`, sha256-verified against checksums baked into the `baml` release; `BAML_CLICKHOUSE_BINARY` override for air-gapped installs. **Known gaps stated plainly**: no native Windows clickhouse-local (WSL or nothing — a product decision to document); startup latency ~100–500 ms warm (the dominant cost for every query; measured as an acceptance gate, p50 target < 1 s end-to-end).
- The fold engine remains the **interactive** plane (2.62 ms run-open); SQL is the **question-answering** plane. Both read the same sealed artifacts; the conformance corpus holds them to the same answers where they overlap.

### 10.5 Hosted execution

Same view DDL applied as migrations to ClickHouse Cloud; the `(version, sql)` API endpoint names the **contract version** = (view schema vN + documented SQL subset + pinned canonical engine version). Tenancy and budgets (full mechanics in the studio doc): per-tenant CH users; permissive row policies **on base tables** (with an explicit admin allow-all — the no-policy-means-zero-rows trap); serving views declared `SQL SECURITY INVOKER` (a DEFINER view would bypass invoker policies — a one-line tenancy hole); settings profiles with MAX-constrained limits + `readonly` + quotas keyed by user; grants only on the serving database; `system.query_log` and friends never granted (they leak other tenants' SQL). Dialect drift between pinned-local and auto-upgrading Cloud is structural; mitigations: `SETTINGS compatibility = '<pinned>'` in the hosted profile, and the **conformance corpus** (catalog queries + trap cases with asserted outputs, incl. NaN fixtures) run in CI against pinned binary × Cloud staging — divergence is a release blocker. CID note: hosted `value_roots_v1.cid` is a tenant-scoped token, not the raw local CID; the two columns are documented as non-comparable (studio doc §7 decision).

### 10.6 Value hydration at query time

Nothing about bodies is pre-materialized beyond bounded previews. Three tiers: (1) **CID columns** — equality/dedupe/drift queries (`GROUP BY cid`, verify-my-fix joins) need no hydration at all; (2) **`value_scalars_v1` previews** (≤4 KiB, redaction-respecting) — the 80% "show me the prompt that…" case, projected at projection time; (3) **explicit budgeted pre-hydration** — `baml query --hydrate run=<id> role=output --max-bytes 256mb` resolves distinct CIDs once each through the standard budgeted read path into a temp `value_bodies_v1` Parquet bound into the query. Budgets enforced *outside* SQL with the existing contract semantics; hydration cost ∝ distinct content (dedupe honored). Executable-UDF hydration was evaluated and rejected as the primary mechanism — ClickHouse Cloud doesn't support executable UDFs, so it would fork the dialect on exactly the feature users touch most; it remains a documented local-only power tool at most. The same contract sentence holds locally and hosted: "`value_bodies_v1` exists for the scope you hydrated, with an explicit budget."

## 11. The UI plane (playground)

One product: **`baml playground`** (no separate `baml studio`). The playground UI reads observability through **~6 private RPC methods** served by the fold engine (`bex_query`) — run list, run snapshot + patches, CCT graph/profile, values list/read, source — over the same server the playground already runs. This is internal plumbing, not a query surface: no stability contract beyond the UI, free to churn with the UI. Two data paths feed it:

- **Files**: mmap + fold over `.baml/` (history and sealed sessions) — 2.62 ms open, O(pixels) rendering, BQF1 wire frames to the webview (the `BqlTable` frame kind is deleted with BQL).
- **In-process RAM tap**: live runs executing in the same process stream engine deltas straight to the UI (snapshot + monotone patch cursor), giving sub-window latency for the local dev loop without touching disk.

Current shipped UI: runs list, CCT tree/flame/timeline, per-node detail (status, timings, histogram, LLM meta), captured-values panel (roots by function/role, budgeted JSON hydration). An in-app SQL box (echoing `baml query`) is an open question (§17), not a commitment. The full product surface — five screens, live cursors, comparison — is the studio doc §16.5.

---

# Part III — Correctness, performance, maintainability

## 12. Correctness strategy

**The oracle.** The raw firehose (§5.7) is byte-exact ground truth. The differential harness replays recorded raw streams through the CCT engine and independently through a naive reference fold (simple, obviously-correct, unoptimized); totals, per-context counters, histogram sums, and status counts must match exactly. Property tests generate adversarial interleavings (cross-thread reorderings, torn ranges, duplicate drains, clock anomalies) and assert the same equivalence plus the resync invariants (never wedge; degraded partitions flagged; unattributable time lands on function 0, not nowhere).

**Golden pinning.** Every on-disk format has committed golden fixtures asserting exact bytes: BCCT blocks + torn-tail recovery, BMET records, `.bamlcct` snapshots, raw container, canonical value encoding (bytes *and* CIDs), decode∘encode identity, pack/index framing, dictionaries, BQF1 frames. A golden diff is a format version bump, never an accident. Test inventory as of this writing: 38 prof-gate tests, 13 golden (v1+v2), 16 canon, plus per-crate units.

**Loss accounting invariants.** Ring shed, staging eviction, drain-budget drops, recorder eviction, defer-resync synthesis, clamped clocks — every loss path increments a named counter that lands in a queryable marker/watermark block. The test suite asserts the *counters fire* under induced pressure, not merely that the system survives.

**Crash consistency.** Kill-at-every-boundary tests: torn segment tails recover to the last intact block; sealed artifacts are never torn (D2); the janitor terminalizes crashed sessions as `crashed` with prefix preserved; pack writes are atomic per chunk with index rebuild-on-mismatch.

**SQL-tier correctness (to build, §16)**: projector reads committed blocks only; manifest CRC drift detection; conformance corpus (local pinned binary × hosted staging) with asserted numeric outputs including integer-quantile and NaN fixtures; cross-engine parity by construction (all stored metrics are integers; the canonical quantile is integer bucket-bound math; derived ratios computed in queries with the documented `x / nullIf(y,0)` idiom).

## 13. Performance ledger

All numbers measured on the shipped implementation (single dev machine, release builds, best-of-3 unless noted; CI-hardware re-confirmation flagged where noted):

| Metric | Legacy | This design | Factor |
|---|---|---|---|
| Producer+consumer overhead, paired 5M-call run | 113.6 ns/call | **74.4 ns/call** | 1.53× cheaper |
| CCT integrated hot-loop cost | — | 47.8–48.6 ns/pair (gate ≤50) | — |
| CCT adversarial p99 shape (3,543-node synthetic, §7.7) | — | 52.7–54.4 ns/pair (never-exceed ≤60; >50 target on this VM — re-measure on CI hardware) | — |
| Disk per 5M-call run | 235.7 MB | **4.5 KB** | 52,224×–70,200× |
| Open run → first frame | 1,654.7 ms | **2.62 ms** | 632× |
| Transcript storage, 64 turns (C5) | N(N+1)/2 growth | dedupe ≥20× | grows with depth |
| Peak consumer RSS, sustained load (C7) | grew | **34.3 MiB constant** | — |
| Consumer CPU per 10M records | ~1,050–1,160 ms | 680–752 ms | −35% |
| wasm bridge size | — | 4.4 MiB gzip vs 4.5 MiB gate (~100 KiB headroom) | — |

(C-numbered labels here and in §5–§7 are gate ids from the completed implementation plan's criteria list, kept for traceability with the benchmark ledger.) Budget structure: producer ~10 ns (append + fences) + engine ~38 ns (decode 9.6 + intern/charge/defer bookkeeping) + recorder memcpy 2.3. Capacity: one consumer core ≈ 20 M pairs/s. SQL-tier targets (gates, not results): `baml query` p50 < 1 s end-to-end; projection cost is provably negligible at corpus volumes (p99 snapshot 226 KB; worst source rate ~1.9 MB/s); the latency budget is clickhouse-local startup, to be measured against the pinned binary.

## 14. Maintainability

**Crate map** (one responsibility each): `bex_events` — record formats, rings, containers, canon codec, CAS, GC (the format authority; everything golden-pinned lives here); `bex_engine` — VM-side capture glue (boundary contexts, value capture, TraceHeap); `bex_vm` — emission sites; `bex_query` — fold engine, run/value reads, UI RPC backing; `baml_cli` — commands; `baml_lsp_server` — playground server; `bridge_wasm` — wasm host (size-gated). To add: `bex_proj` (or a module in `bex_query`) — the Parquet projector; `db/clickhouse/views/` — versioned view DDL + UDFs, single source deployed verbatim to local queries-file and hosted migrations.

**Format discipline**: formats are append-only within a version (unknown block kinds/fields skipped by length); breaking changes are new magics/domains; readers support N and N−1, refuse N−2 loudly. The same rule now governs view versions (§10.2).

**What the 2026-08-04 decision deleted from the maintenance surface**: the BQL lexer/parser/planner/executor (2,558 lines + 16 tests), `baml q`, the BqlTable BQF1 frame + TS decoder, the planned BQL→ClickHouse compiler, the planned StudioQueryV1 AST + language service — i.e., all *language* maintenance. **What it added**: the projector, view DDL per version, the conformance corpus + CI, pinned-binary distribution (checksums, bump process, Windows story), tenancy-as-code, and schema documentation that must be excellent because it replaced the type system. The trade is favorable *iff the conformance corpus is CI-enforced*; that is a stated release gate, not an aspiration.

**Codebase deletions still pending** (part of Phase Q0, §16): `bex_query/src/bql.rs` + `tests/bql.rs`, `baml_cli/src/q_command.rs`, BqlTable frame kind (Rust + `bqf1.ts` + `observe-client.ts` `bql` op), demo docs referencing `baml q` (`/root/dev/demo/baml-q.md` teaches the deleted surface and must be rewritten against `baml query`).

---

# Part IV — State, phases, open questions

## 15. Implementation state (verified 2026-08-04)

Phases P0–P9 of the original implementation plan are **complete**, verified against the codebase with the final benchmark ledger recorded (commit `fa1fd3091` and successors on `paulo/cct-1`):

| Phase | Scope | Status |
|---|---|---|
| P0 | Record formats, rings, producer emission | ✅ complete |
| P1 | Consumer, drain loop, control channel | ✅ complete |
| P2 | CCT engine: intern, charge, defer/resync, suspend/resume | ✅ complete |
| P3 | Histograms, LLM enrichment, spawn aggregation, recursion fold | ✅ complete |
| P4 | BCCT/BMET storage, D1/D2 durability, torn-tail recovery, sessions/history | ✅ complete |
| P5 | Flight recorder, triggers, shedding ladder, raw firehose | ✅ complete |
| P6 | Value plane: canon codec, CAS packs, staging/promotion, drain service, GC | ✅ complete (residuals: `.bamlidx` sidecar, full-trace CID pins — tracked, non-blocking) |
| P7 | Compile-time identity: function_ids, dictionaries, definition_key, def_content_hash | ✅ complete |
| P8 | Fold engine + UI: run open, CCT views, BQF1, playground panels | ✅ complete (values panel + function_id-carrying captures added post-`fa1fd30`) |
| P9 | Legacy pipeline deletion, final gates, benchmark ledger | ✅ complete |
| — | BQL v1 (built, then superseded) | ⚰️ slated for deletion (Q0) |

Residual small items carried from the phase reviews (none blocking): SDK/pack-host dictionary emission parity; `baml.toml` knob surface for budgets; audit-record polish; flight-dump `.bamlcids` pinning edge cases; CI-hardware re-measurement of the p99-shape leg.

## 16. Phases to build (the SQL tier)

**Q0 — Deletion and rename (small).** Delete bql.rs/tests, `baml q`, BqlTable frames + TS decode; unify command surface under `baml playground`; rewrite demo agent docs against the coming `baml query`. Gate: no `bql` symbol in tree; playground unaffected.

**Q1 — Projector + manifest.** Parquet projection of sealed artifacts per the §10.4 layout (runs, cct_population, llm_population, spawn, value_roots, value_scalars, capture_losses, functions, revisions, exact_windows, call_instances from dumps), manifest with seal-CRC drift detection, retention/tombstone handling, compaction. Gate: projections rebuild byte-stable from fixtures; drift/tombstone scenarios covered.

**Q2 — View DDL v1 + `baml query`.** Pinned-LTS download-on-first-use with baked checksums; generated init SQL (schemas, views, UDFs, memory caps); statement passthrough; freshness footer; `--schema` rendering view/column comments; hot-tail projection for near-live; `--hydrate` budgeted pre-hydration into `value_bodies_v1`. Gate: the user-story query catalog (§4) passes end-to-end; p50 < 1 s measured; docs contain the trap-case pairs.

**Q3 — Conformance corpus + agent eval.** Catalog + trap queries with asserted outputs (integer quantiles, NaN fixtures, empty instance windows, cross-revision grouping); CI against the pinned binary; agent eval (SQL-over-views task success, trap cases included) before the v1 schema freeze. Gate: corpus green in CI; eval results recorded.

**Q4 — Hosted endpoint.** `(version, sql)` API over the same DDL on ClickHouse Cloud; per-tenant users, base-table row policies + admin policy, INVOKER views, profiles/quotas, serving-db-only grants; corpus extended to Cloud staging (tenancy probes through views *and* base tables). Owned jointly with the studio doc's P0-C. Gate: cross-tenant attack tests pass; corpus parity local×hosted.

**Q5 — Time-series + windows projection (fast-follow).** `cct_windows_v1` lazy projection for session time series; `spawn_instances_v1`; optional in-app SQL box if §17-Q1 resolves yes.

## 17. Open questions

1. **In-app SQL box** in the playground (echo `baml query` results in the UI)? Default: not in v1; CLI + hosted first. Revisit after Q2.
2. **Typed hydration surface** beyond JSON (schema-aware rendering of hydrated values in CLI output)? Default: JSON + previews suffice for v1.
3. **wasm capture hosts**: browser-*executed* runs still capture in-memory (4 MiB recorder, inline values); with browser-local querying gone, does the wasm live-view fold path in `bridge_wasm` stay (for same-page live rendering) or go (server-backed everywhere)? The capture-host scope is resolved (studio doc §11: diagnostic-only, embedded wasm SDK users); what remains open here is the live-fold path — default: keep it (shipped and size-gated) until promptfiddle server-backing lands, then re-evaluate against the size gate's ~100 KiB headroom.
4. **OQ6 (carried)**: stdlib introspection surface (`baml.profile.*`) stays dead by default — confirm no product need before deleting the stubs.
5. **OQ7 (carried, shipped contract)**: `$id`/`SetFunctionId` overrides are visible in rings/dumps only, not CCT identity. Confirmed as the durable contract; revisit only with a concrete demand.
6. **Windows**: no native clickhouse-local. Ship `baml query` as WSL-only on Windows with a documented statement, or gate the feature? Needs a product call before Q2 docs.

## Appendix A — Format magics quick reference

| Artifact | Magic / domain | Version rule |
|---|---|---|
| CCT segment | `BCCT` / blocks `DBLK` / footer `BCCTFOOT`+`TSEG` | v1; unknown block kinds skipped |
| Meta stream | `BMET\0` | v1; 9 record kinds |
| Raw firehose | `BAMLRAW1` | v1 |
| Flight dump / full trace | legacy `.bamlprof` framing | frozen |
| Value node/chunk domains | `baml-value-node-v1\0` / `baml-value-chunk-v1\0` | frozen; new domain = new version |
| CID wire | `bamlv_1_` | frozen |
| CAS pack / index | `BPK1` (rec `CK`) / `BPKI` | v1 |
| Dictionary | `.bamldict` protobuf | v1 |
| Revision id | `baml_rev_1_` | frozen |
| UI wire | BQF1 frames (kinds 1–8; kind 9 BqlTable deleted in Q0) | additive |
| Projections | Parquet + `proj/v1/manifest.jsonl` | `proj_schema_version`; rebuildable |

## Appendix B — Glossary

**Boundary/run** — one observed root execution, ULID-identified. **Partition** — spawn tree of one root thread; binds to a boundary. **Session** — one engine process's working directory. **Epoch** — node-id scope within a session. **CCT** — calling-context tree (the tally). **Tape** — bounded exact-event windows (ring, flight recorder, full trace, raw). **CID** — BLAKE3 content id of a canonical value DAG. **Grain** — population (per code path, complete) vs instance (per recorded call, windowed). **Fold engine** — `bex_query`'s sans-io reader powering the UI. **Projection** — rebuildable Parquet derived from sealed artifacts. **View contract** — versioned, grain-named SQL views; the public query surface. **Watermark** — highest contiguous proven position, never merely highest seen.
