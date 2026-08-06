# BAML Observability & Profiling — Canonical Design

**Status:** Canonical. Supersedes `stale-profiling-design.md` (2026-07-30) and incorporates the 2026-08-04 query-surface decisions.
**Companion:** `studio-design.md` (the hosted product and capture-upload architecture). This document owns everything that happens on the machine where a BAML program runs: capture, aggregation, storage, local querying, and the local UI.
**Execution order:** `TASK/PLAN.md` (M0–M5).
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

1. **Observability is on by default.** No spans, no SDK calls, no config. `baml run` records; the data is there when you need it. The corollary is a hard cost budget: if it isn't nearly free, it can't be default-on. Why capturing root and LLM I/O by default is the right risk: the data never leaves the machine by default (`.baml/` is local; hosted upload is a separate explicit opt-in with its own policy — studio doc Part III); seeing the exact prompt and output *is* the product in the debugging loop; and the exits are cheap and layered (`BAML_HISTORY=0`, per-class capture flags, redaction policy, retention). Decision owner: product, 2026-07-30, reaffirmed with the on-by-default initiative.
2. **Cost grows with unique behavior, never with traffic.** Storage and memory are proportional to *distinct calling contexts × active time windows × distinct content* — never to call count. Five million calls through one code path cost the same few counters as five thousand.
3. **No silent truncation.** Every bound in the system (rings, windows, budgets, retention) has a counter, a marker record, or an explicit error. An answer computed from partial evidence must be *identifiable* as partial. Bounded never means silent.
4. **The producer never blocks.** The program being observed pays ~10 ns/call to append fixed-width records to a lock-free ring; everything else happens on a background consumer thread. Backpressure degrades observability (visibly), never the program.
5. **Reads are O(pixels), never O(events).** The UI and query paths fold aggregates sized to the viewport; opening a multi-gigabyte history must not require materializing it.
6. **Identity is integers, assigned at compile time.** Function identity is a dense `u32` stamped by the compiler; names live in a per-revision dictionary written once. No strings on any per-call path.
7. **One fold engine, one public query language.** A single Rust fold engine (`bex_query`) privately powers the UI over the native storage. The one *public* query surface is the **ClickHouse SQL dialect over versioned, grain-named views** — locally via `baml query` (an **embedded ClickHouse engine — chDB** — over Parquet projections of sealed artifacts; clickhouse-local retained as a fallback), hosted via a `(version, sql)` endpoint (studio doc §37). There is no bespoke query language. (History: a pipeline DSL, "BQL", was designed and a v1 was built; it was removed by the 2026-08-04 decision — §10.1.)
8. **Local-first, cloud-shaped.** Everything lands under the project's `.baml/` directory in formats designed to be projected, uploaded, and served by the hosted product without re-modeling.

## 3. The three planes and the grain principle

All observability data lives in one of three planes, distinguished by *what question they can answer honestly*:

| Plane | Contains | Bounded by | Answers |
|---|---|---|---|
| **Tally** (the CCT) | Per-calling-context counters: calls, errors, self/await time, duration histograms, LLM tokens | Unique contexts (p99 ≈ 3,537 nodes) | *How much, how often, how slow, how many errored* — population-true, always |
| **Tape** (exact windows) | Exact events: recent-call ring (last 4,096), flight-recorder dumps (16 MiB, on trigger), opt-in full traces, opt-in raw firehose | Fixed byte/slot budgets | *What exactly happened, in order* — within a declared window |
| **Values** | Captured inputs/outputs/errors as content-addressed DAGs | Capture policy + dedupe | *What data actually flowed* — for captured calls |

**The grain principle** is the single most important consumer-facing rule: the tally is **population** data (one row per code path, exact totals over *all* calls); the tape and values are **instance** data (rows per *recorded* call — a deliberate sample). A count computed over instance data answers "how many *recorded*…", never "how many…". Every query surface in this design — the UI, the SQL views, the documentation — is organized so the two grains are named apart and cannot silently impersonate each other (§10.2). Nothing true is ever dropped: the population totals are complete in the tally; what is *never materialized* is per-call detail outside the windows, because producing it is the 38.5 TB/day disease.

**A boundary (= a run)** — the term everything below leans on — is one observed root execution: `baml run` invoking `main()`, one served request handled by an entrypoint, one test case. Boundaries are ULID-identified, bind the partitions (spawn trees) they cause, and are the unit of history, retention, and upload.

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
| 0x05 | `SetFunctionId` | 41 B | `$id` boundary override annotation (ring/dump visibility only, not CCT identity — the shipped contract; §17 item 5) |
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

**The user-facing surface**: per-call capture is BAML syntax (`$id = boundary.id().capture(inputs=…, output=…, error=…)` and `promote_on_error` — shipped); global defaults and budgets are env-var-configured today (`BAML_PROFILE*`, `BAML_HISTORY`, ring/recorder sizes), with the `baml.toml [profile]` knob surface a tracked residual (§15).

**Host defaults**: CLI/SDK — CCT on, root I/O + LLM Auto + error promotion, recorder on, full trace off. Playground — same plus log capture. wasm — in-memory, inline-only values, 4 MiB recorder. CI — `BAML_HISTORY=0` disables durable capture wholesale (the named privacy switch; sessions still record unless profiling is off).

### 5.4 Value capture mechanics

Captured values are deep-copied out of the VM heap into a trace-local arena (`TraceHeap`) at capture time (reserve-before-copy: a failed reservation does zero work), then drained asynchronously. Each capture carries its `(thread_id, call_id)` key **and its `function_id`** — stamped at the capture site where the VM knows the callee — so values are attributable to functions with zero joins and zero env flags. Root input/output/error captures come from the boundary context; per-call captures from the `$id = boundary.id().capture(...)` side channel; log bodies via the log capture hook.

**Trigger-promoted staging** (the retroactive story — "when a helper errors, I want the args it actually received"): `promote_on_error` captures land in a byte-bounded staging ring (32 MiB native / 8 MiB wasm) tagged speculative; released free at normal frame close (no serialization, no hashing, no I/O); on a trigger, drafts in the failing subtree are promoted to the durable queue with `role: promoted` + the trigger id. Evictions are counted and reported (`staged_evicted`) so "the buffer was too small" is visible and tunable.

**Continuous drain**: values drain while the boundary is open (high-water wake at ~½ budget + coarse interval); the pending budget is a flow-control window, not a per-run cap. Canonicalization, hashing, and pack appends run on a dedicated per-process **value drain service** thread — never on the prof consumer — with its CPU measured separately (C10).

### 5.5 Flight recorder

A bounded ring of **raw drained bytes** — one memcpy on the drain path, zero transcode until a trigger fires. 16 MiB ≈ 200k call pairs ≈ 11 s of the design corpus's median working-agent trace (~19k pairs/s — a corpus estimate, so the wall-clock window scales inversely with the workload's pair rate; the same 16 MiB holds 21 ms of the pathological hot loop, which is exactly what the CCT is for). Whole-chunk FIFO eviction with counted, queryable eviction state. A dump transcodes retained chunks into `sessions/<sess>/flight/<ts>-<trigger>.bamlprof` — the *legacy event framing*, so every existing reader works — and appends a `BoundaryTrigger` record to bound boundaries so the UI can jump from a CCT node to its exact-event evidence.

### 5.6 The shedding ladder

Capacity model: the CCT-engine slice alone would sustain ~20 M pairs/s (the ≤50 ns/pair gate inverted), but the full drain loop — decode, engine, recorder memcpy, session storage — measures 680–752 ms per 10 M records ≈ 140 ns/pair, i.e. **~7 M pairs/s sustained per consumer core**. A single pathological hot loop generates ~9.5 M pairs/s: one such producer already exceeds a core's sustained rate, which is exactly why the ladder exists. Under `shed` policy the consumer degrades in strict order, each step counted and marked in the stream: (1) stop flight-recorder memcpys → (2) stop full-trace transcode → (3) defer value canonicalization (bounded queue, then `CaptureLoss`) → (4) shed structural ranges (drop drained ranges; affected threads degrade via the resync path §7.2; `shed_ranges`/`shed_events` counters). **CCT aggregation is never disabled, and shed mode never aborts.**

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

### 6.5 Identity bounds and overflow semantics (compile-time vs runtime)

Every identity space in the system is bounded; here is where each bound lives, what enforces it, and what a user would see — because "what happens past u32?" must have a designed answer, not an accident.

**Function ids (compile time).** The pool is `u32`: 0 = unknown, 1 = spawn-closure, 2–15 reserved, reals from 16 (`FIRST_POOL_FUNCTION_ID`). `assign_function_ids` stamps sequentially in pool order (deterministic — the linker's layout contract) and is idempotent; `verify_function_ids` re-checks at every Program materialization (debug: every row; release: cheap tail probe). Exhausting this space would require ~4.29 billion function *definitions* in one compiled program; each definition costs at minimum hundreds of bytes of pool and bytecode, so compilation fails on memory terabytes before id exhaustion — the bound is unreachable by construction, not guarded by a dedicated diagnostic. Hardening note (cheap, non-blocking): the finalizer can assert `next < u32::MAX` and surface a compile error; recorded as a nicety, not a risk.

**Calling-context node ids (runtime — the "number of paths" question).** Paths are a *runtime* phenomenon: dynamic dispatch and recursion make the context set unenumerable at compile time (§6.4), so **the compiler never errors on path counts — there is nothing for it to count.** At runtime, node ids are dense `u32` scoped to a **session epoch**, and the bound is *managed*, not checked per call: the engine rotates epochs at `EPOCH_ROTATE_BYTES = 256 MiB` of encoded CCT output (or 24 h), starting a fresh node table with ids restarting from zero and live nodes re-birthed (`rotate_epoch`; a `SessionEpochClose` meta record carries the reason and byte count). A node costs well over 100 bytes of encoded output across its birth and deltas, so one epoch tops out around ~10⁶ distinct contexts — three orders of magnitude below `u32::MAX`. Overflow is therefore *unrepresentable in a healthy engine*; as belt-and-braces every id/counter conversion onto the u32 wire is saturating (`u32::try_from(..).unwrap_or(u32::MAX)`; window-delta counters clamp at `u32::MAX` while in-memory totals stay u64), so even a pathological state degrades to a pinned sentinel instead of wrapping into a *wrong identity*. Neither case is a user-visible error: path-cardinality pressure shows up as epoch rotations (queryable in the meta stream), never as a failed run.

**Recursion depth** folds at 512 with flagged back-edges (§7.4) — path *length* is bounded independently of path *count*, and both bounds are visible, not silent.

**Call ids / thread ids** are `u64` per engine — monotone, never recycled within a session; wrapping a u64 at any plausible rate exceeds the lifetime of the hardware. Spawn instance tables bound *stored instances* (first 64 + ≤256 exceptional), not the counts, which are exact u64.

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

Integrated engine benchmark (pinned, release): hot loop **47.8–48.6 ns/pair** against the ≤50 gate; adversarial p99-cardinality shape (3,543 nodes — a synthetic modeled on the corpus p99 of 3,537) 52.7–54.4 (within the ≤60 never-exceed; flagged for CI-hardware confirmation); decode alone 9.6, recorder memcpy 2.3. End-to-end paired (5 M calls, quiet box, best-of-3): **74.4 ns/call** — versus 113.6 for the deleted per-event pipeline. Consumer CPU: 680–752 ms per 10 M records — 68–75 ns/record, ~140 ns/pair, the sustained-capacity figure of §5.6 (−35% vs the transitional dual pipeline).

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
    manifest.bamlcids               # value-root CID pins (the GC mark set, §8.6)
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

`meta.bamlmeta` (`BMET\0` v1, 9 record kinds) carries session open/close, boundary begin/bind/complete, trigger records, capture-policy stamps, and diagnostics. A boundary ULID is minted at begin; `history/<ulid>/` is created at completion. Crashed sessions are terminalized **at read time**, not by a daemon: `bex_query` classifies a run `crashed` when its boundary has begin-without-complete and the session heartbeat is dead (pid gone, or heartbeat stale beyond 30 s default) — a read-side judgment that preserves the torn prefix and invents nothing. `index.jsonl` appends one line per completed run: the O(1) discovery surface for "list recent runs".

### 8.4 Snapshot fold and the unbound lane

Partitions bind to boundaries via `partition_bind` blocks; work that never binds (background threads, pre-bind spans) folds into the session's **unbound lane**, visible in session-scoped queries and counted — never silently dropped. Boundary completion folds the partition's deltas into the final snapshot; window-aligned deltas remain in segments for time-series queries until GC'd (sealed segments are collected once every referencing boundary is complete and snapshotted).

### 8.5 Retention (what gets cleaned, in what order)

Retention (`bex_events::store::retention::clean`) enforces byte/age budgets with a **fixed degradation order**. Invocation today is explicit: `baml clean` (with `--dry-run` preview) runs retention then GC, in that order; automatic invocation at session open is a tracked residual (§15), not yet wired. Every deletion is tombstoned to `retention.log` (jsonl: kind, path, bytes, timestamp) so "removed by retention on <date>" stays queryable, and a `dry_run` mode reports without deleting:

1. **Raw firehose first** — per-session byte cap, oldest files first. Raw is the first casualty by contract: it is the opt-in, per-call-cost tier, and everything it proves is re-derivable from nothing (it *is* the ground truth, but only for debugging the profiler itself).
2. **History budgets** — `history/<run>/` directories by age + total-size budget, oldest first, with a **newest-floor**: the most recent N runs are protected regardless of budget, so retention can never leave a project with zero inspectable history.
3. **Flight dumps and sealed session leftovers** under their own caps.

`BAML_HISTORY=0` disables durable history wholesale (the named privacy/CI switch). Hand-deleting a run directory is also safe: the projection manifest detects the vanished source, checks `retention.log`, and either drops the projection quietly (tombstoned) or warns loudly (evidence disappeared outside retention) — §10.4.

### 8.6 The full cleanup lifecycle (sessions, CAS GC, projections)

Cleanup is three cooperating mechanisms with distinct triggers:

**Session working-state collection (seal-and-collect).** Sealed CCT segments exist to serve time-series reads and boundary folds; once every boundary referencing a segment is complete and snapshotted (its `cct.bamlcct` is self-contained), the segment is collectable. Crashed sessions are terminal by read-time classification (§8.3); their sealed prefix is treated as history-grade evidence before their working state becomes eligible for collection.

**CAS garbage collection** (`store::gc` — mark → sweep with coarse exclusive locking):
- *Locking:* writers hold `writers.lock` shared; GC takes it exclusive. If any writer is live, **GC skips with a notice** — the delete→dedupe→sweep adversarial interleaving reduces to "GC waits," which is the entire concurrency proof.
- *Mark:* the live root set = every `history/*/manifest.bamlcids` (roots committed inside the same group-commit barrier as their pack sync — a value is never durable without its manifest entry), plus `sessions/*/flight/*.bamlcids` pins (flight dumps that reference values), plus **`uploads.pin`** (chunks referenced by not-yet-receipt-accepted uploads — the hosted transport pins content it still owes the cloud, so reclamation-before-receipt is impossible by construction). The mark closes over the canonical DAG via `node_refs` (node → child CIDs).
- *Sweep:* packs older than a **24 h grace window** (`DEFAULT_GRACE_MS`) whose chunks are all unmarked are unlinked whole; partially-live packs are **compacted** — live records rewritten to a fresh pack, old pack unlinked, index rebuilt. Young packs are untouched (grace absorbs in-flight races cheaply). Every deletion appends a tombstone.
- *Report:* roots/marked/kept/unlinked/compacted/bytes-reclaimed — queryable, so "why is my disk not shrinking" has a data answer (usually: live writers, grace window, or a flight pin).

**Projection invalidation.** `.baml/proj/` is an index, never evidence: the manifest's seal-CRC diff reprojects changed sources, drops projections of tombstoned sources (surfaced via `capture_losses_v1(kind='retention_tombstone')`), and a wholesale `rm -rf .baml/proj` costs exactly one re-projection pass.

**Reader-concurrency contract** (stated 2026-08-06; enforcement lands M4): sealed *bytes* are immutable, but their *files* are not permanent — pack compaction rewrites live records into a fresh pack and unlinks the old one, projection compaction consolidates per-run Parquet, and retention unlinks tombstoned sources. Short readers are safe by POSIX unlink semantics (an open handle keeps bytes alive; note this does not hold on Windows filesystems). Long-running readers — scans, resident query engines — must: (a) snapshot their file set from the projection manifest / pack index at start; (b) tolerate vanish-mid-read by re-taking the manifest diff and checking `retention.log` (the projector's existing rule, generalized); (c) invalidate any cached catalogs/footers on manifest-generation change (the resident-engine rule). GC's 24 h grace and writers-lock cover the racy young end; compaction of old packs keeps originals until the atomic swap so re-open-on-stale-handle always finds either the old or the new file.

Ordering contract: retention runs **before** GC (retention releases roots; GC then collects the newly-unreachable closure). Cryptographic content addressing makes deletion real — no orphan copies survive by accident, and the tombstone names what was released.

### 8.7 The cloud delivery boundary

Sealed artifacts are the upload unit: the hosted transport reads the same immutable files this section defines — it never taps rings, never adds hot-path work, and never blocks sealing. Everything from "sealed bytes exist" onward (drain adapters per host, spools, chunk envelopes, upload authorization, receipts, and the per-environment differences — Lambda vs edge vs native) is owned by the studio doc: Part III (capture and delivery), §21–§22 (chunks and envelopes), and §27 (the ingest protocol). The one contract this doc exports to that pipeline: artifacts are immutable once sealed, torn tails are recoverable, and `uploads.pin` (§8.6) keeps CAS content alive until the cloud's receipt-backed contiguous watermark covers it.

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

Value-plane specifics of the §8.6 lifecycle: roots referenced by retained history are live via `manifest.bamlcids` (written inside the same durability barrier as the pack append — a root is never durable without its manifest line); flight dumps pin value closures via `.bamlcids` sidecars; pending uploads pin via `uploads.pin`; everything else is collected by the mark/sweep with the 24 h grace. Dedupe means deletion is *shared-fate*: a chunk dies only when the last referencing root is tombstoned, so deleting one run reclaims only that run's unique content — exactly the storage the run added.

## 10. The query architecture (SQL tier) — TO BUILD

This is the one major component of this design that is **not yet implemented**. Everything it consumes (sealed artifacts, formats, read paths) is shipped; the tier itself — projector, view DDL, `baml query`, hosted endpoint — is new work (phases in §16).

### 10.1 The decision and its history

Three query surfaces were designed or built during this initiative: a pipeline DSL ("BQL", v1 built — 2,558 lines: lexer/parser/planner/executor, `baml q`, a BQF1 wire frame, value stages), a JSON query AST for the hosted product ("StudioQueryV1", designed only), and ad-hoc UI RPC. On 2026-08-04, after an adversarial research pass (archived in `old-references/bql-vs-sql.md` + research reports), the decision inverted: **one user-facing query language everywhere — the ClickHouse SQL dialect over versioned, grain-named views.** BQL and StudioQueryV1 are deleted, not frozen.

The honest reasoning chain, recorded because the losing arguments were good ones:

- BQL's real advantages were never syntax; they were (a) fail-closed grain honesty (`E_NO_EXACT_SOURCE` instead of a silently-wrong count), (b) a mandatory completeness footer on every result, (c) a sans-io engine that runs identically over mmap/wasm/HTTP. The counter-realization: (a) and (b) can be *approximated* by schema design — grain-named views + queryable evidence ledgers + documentation — at the cost of enforcement; (c) survives intact as the *private* UI engine, which is not a query language.
- The multitenant objections to hosted SQL passthrough dissolve under standard ClickHouse machinery: **RLS row policies** for tenancy, **role settings profiles + quotas** for budgets, a **versioned `(version, sql)` endpoint** for schema churn — all proven, none bespoke.
- The capability objections dissolve too: values **hydrate at query time** (nothing pre-materialized), near-live is **flush → project → query (~1–2 s)**, and the engine library is **downloaded on first use** (libchdb: ~150 MB compressed / ~508 MB on disk — verified 2026-08-06, slightly *smaller* than the clickhouse-local binary it replaced). Dialect identity with hosted is inherited from the shared engine family rather than maintained by us: chdb-core and the hosted target are pinned to one version family, and the conformance corpus tests the documented catalog; drift outside the catalog is best-effort, not a treaty.
- The one thing physically lost is **in-browser querying** (no wasm ClickHouse). Accepted with eyes open: the VSCode playground talks to its native server; promptfiddle-class browser hosts become server-backed (one `baml` binary per session — studio doc); `bex_query`'s wasm build ceases to be a query surface.
- What is *knowingly* deferred on capability: **interactive row-granular early-stop full-body predicates** ("stream candidates, hydrate, stop at N matches" inside one statement). The interim answer is the two-phase form — preview/CID-tier prefilter, then `--hydrate --where` or the M4 small-scope scan with early-stop-at-N; recorded here so the gap is a decision, not an oversight.
- What is *knowingly* given up on honesty: nothing forces an agent to check the evidence ledgers before counting instance rows. BQL would have failed that query closed. The mitigation budget goes into naming, in-schema documentation, and trap-case docs (§10.3) — "explain the schema; models are smart enough to figure it out" is the product position, and it is recorded here as an accepted risk, not an oversight.

### 10.2 The view contract

**Grain naming rule:** any noun countable at two grains carries an explicit suffix — `*_population_v1` (complete over the always-on aggregate contract; `SUM(ends_err)` is the true error count) vs `*_instances_v1` (rows exist only where an exact-evidence source covered the scope; `COUNT(*)` is a lower bound *by construction*). Registries that exist at one grain (runs, functions, revisions) go unsuffixed. Views are versioned `_vN`; additive column changes don't bump; grain/meaning changes do; N and N−1 supported concurrently, N−2 fails loudly.

**View catalog v1** (physical layout private; this is the public contract):

| View | Grain | Key columns |
|---|---|---|
| `runs_v1` | one row per boundary | run_id, created_ms, status (ok/errored/cancelled/crashed/running), revision_id, duration, total_calls, total_errors, llm/token totals, degraded, diagnostics |
| `cct_population_v1` | (run, context node) — folded totals | node/parent/depth, function_id, revision_id + **denormalized fqn/definition_key/def_content_hash/path**, enters, ends by status, total/self/await ns, hist Array(UInt64)[16] |
| `cct_windows_v1` | (session, epoch, node, 250 ms window) | time-series deltas; projected lazily; **never summed with population views** (different fold state) |
| `llm_population_v1` | (run, node, model) | llm_calls, tokens in/out, provider/parse errors. Dollar cost = query-time `JOIN` against the user's own price file — never stored |
| `spawn_edges_v1` / `spawn_instances_v1` | population / bounded instances | aggregate + first-64/exceptional instances, `instances_dropped` surfaced |
| `call_instances_v1` | instances from exact windows only | source (flight_dump/full_trace/spawn_instance), window_id, thread/call/parent, start/end/status |
| `exact_windows_v1` | **the evidence ledger** — one row per exact-evidence window | source, trigger, time bounds, event_count, evicted_upto, budget_exhausted |
| `value_roots_v1` | one row per capture root | value_ord, thread/call, function identity, role (input/output/error/log/promoted), **cid**, logical_len, captured_ts, status |
| `value_scalars_v1` | bounded previews (≤4 KiB, policy-respecting) | cid, kind, preview, preview_truncated — the everyday "grep the prompts" surface |
| `capture_losses_v1` | one row per loss event | kind (staging_evicted/shed/drain_budget/ring_evicted/trace_budget/retention_tombstone), count, ts |
| `functions_v1` / `revisions_v1` | dictionary registries | full identity columns per §6 |
| derived: `errors_population_v1`, `error_instances_v1`, `hot_contexts_v1` | pure SQL over the above | shipped in the same DDL file |

`value_bodies_v1` is a scoped companion, not a standing view: it exists only for an explicitly hydrated scope with an explicit budget (§10.6).

Design rules baked into the catalog (each closes a named trap): identity columns are **denormalized** (fqn/definition_key materialized into every fact row, so the naive cross-revision `GROUP BY fqn` is *correct* rather than a disjoint-id-space bug); histograms are 16-element arrays whose bucket bounds are pinned by the engine's `hist_bucket`: a ×4 stride from 1 µs — bucket 0 < 1 µs, bucket b ∈ [4^(b−1), 4^b) µs for 1 ≤ b ≤ 14, bucket 15 the open tail ≥ 4¹⁴ µs ≈ 268 s. Two versioned SQL-lambda UDFs ship in the DDL: `cct_hist_quantile_v1(h, q)` — the index of the bucket containing the quantile rank, **pure integer math, no interpolation**, bit-identical between the Rust fold engine and SQL — and `cct_bucket_upper_ns_v1(i)` = 4^i µs in ns, the canonical rendering of a bucket index (the open tail renders as `≥` its lower bound); run ids sort by `created_ms`, never `ORDER BY run_id` (base64 doesn't sort); dollar cost is never stored — the documented join key is `llm_population_v1.model` (the provider-reported model name interned at capture) against a user-supplied price table, locally `JOIN file('prices.csv', CSVWithNames, 'model String, input_per_mtok Float64, output_per_mtok Float64')`, hosted a small user-uploaded dimension table (the schema doc ships this example); `node_id` is run-scoped; window rows carry their (session, epoch) scope. Every view and grain-sensitive column carries a machine-parseable `COMMENT` (first line: `grain: instances — COUNT(*) is a lower bound; totals live in errors_population_v1`), rendered by `baml query --schema` and visible to introspecting agents.

### 10.3 Honesty via documented schema

Four layers replace BQL's enforcement: (1) **names** — the grain travels inside every query text; (2) **in-database docs** — comments on views/columns, served by `--schema`; (3) **the evidence ledgers** — `exact_windows_v1` + `capture_losses_v1` make evidence coverage *queryable data*; every documented instance-grain example includes the join; (4) **trap-case docs** — explicit wrong/right pairs for each known hazard (instances-as-population, run_id ordering, cross-revision function_id grouping, hosted-vs-local CID spaces). The CLI additionally prints a non-SQL freshness footer ("projected through T; hot tail included; N runs in scope; M capture losses") — deliberately outside the result set so it can't be mistaken for part of the SQL contract. The residual hazard (nothing *forces* the ledger join) is accepted and named; an agent eval over the view catalog with trap cases runs before the v1 freeze (§16 gate).

### 10.4 Local execution: `baml query`

An **embedded ClickHouse engine (chDB)** over **Parquet projections** of sealed artifacts — in-process via chdb-rust over the stable C ABI, with clickhouse-local retained as a fallback behind a thin engine seam (`--engine=clickhouse-local`):

- **Projection**: one Parquet file per (sealed source artifact, view), written tmp+rename to `.baml/proj/v1/<view>/run_id=<id>/part.parquet` (hive-partitioned for predicate pruning), zstd + row-group stats. Idempotent because sources are immutable. An append-only **manifest** (`proj/v1/manifest.jsonl`: source path, length, seal crc32c, projector + schema versions, outputs) makes refresh an O(#runs) diff and doubles as the drift detector — vanished sources are checked against `retention.log` (tombstoned ⇒ drop projection, surface as `capture_losses_v1(kind='retention_tombstone')`; not tombstoned ⇒ loud warning). Compaction rewrites old per-run files into consolidated monthly files at ~500 files/view.
- **Hot tail (near-live)**: active segments are readable via the committed-block scan; the projector regenerates `proj/v1/hot/*.parquet` up to the last watermark per invocation. Flush cadence 250 ms + 1 s D1 ⇒ end-to-end freshness ~1–2 s (a derivation, not yet a gate — Q2 measures and pins it against the named corpus). Two reader-visibility models exist deliberately: the fold engine reads committed blocks at flush cadence (≈250 ms) for interactive views, while the projector consumes at the D1 durability horizon (≤1.25 s) so projections never contain bytes a crash could revoke. Hot rows feed `cct_windows_v1` only, never population views.
- **Invocation**: a generated init script (explicit Parquet schemas, view DDL, the two integer-math UDFs, `max_memory_usage` cap) followed by the user statement, executed in-process through a chDB session. Engine library: pinned chdb-core, downloaded on first use into `~/.cache/baml/chdb/<version>/`, sha256-verified against checksums baked into the `baml` release (never fetched unchecksummed at build time — the tarball is vendored); `BAML_CHDB_LIBRARY` override for air-gapped installs. Embedding hardening (all M2 work items, empirically scoped 2026-08-06): the engine's signal handlers are disabled (`chdb_set_signal_handlers_enabled(false)`); `max_memory_usage` breach fails the *query*, never the process (CI-tested); one connection per process is a documented engine constraint (irrelevant to the one-shot CLI; the resident playground server serializes through its single session); the engine spawns its own thread pool on first query.
- **Measured latency** (2026-08-06, this replaces the old startup budget): warm in-process query **0.4 ms** (~300× the old subprocess floor); a real 2M-row Parquet WHERE+GROUP BY 31 ms vs 161 ms; truly cold first query ~305 ms (a wash with the old subprocess cold path). One-shot CLI cost is now dominated by process exec + Parquet footer reads + scan, not the engine; a resident server pays ~0.4 ms marginal. **Known gaps stated plainly**: no native Windows anywhere in the ClickHouse engine family (upstream ships none) — `baml query` on Windows is WSL, documented, resolved as product posture; and the first-use library download remains a real cost whose incidence lands hardest on fresh CI runners and container images (cache accordingly).
- The fold engine remains the **interactive** plane (2.62 ms run-open); SQL is the **question-answering** plane. Both read the same sealed artifacts; the conformance corpus holds them to the same answers where they overlap.

### 10.5 Hosted execution

Same view DDL applied as migrations to ClickHouse Cloud; the `(version, sql)` API endpoint names the **contract version** = (view schema vN + documented SQL subset + pinned canonical engine version). Tenancy and budgets (full mechanics in the studio doc §37.3–§37.4): CH identities provisioned at the **authorization grain** (per grant-profile, not per tenant — sub-tenant project/environment/value-read fences are real); permissive row policies **on base tables** filtering tenant+project+environment via a control-plane mapping table (with an explicit admin allow-all — the no-policy-means-zero-rows trap); serving views declared `SQL SECURITY INVOKER` (a DEFINER view would bypass invoker policies — a one-line tenancy hole) over **column-scoped base grants** (every value-derived column gated on value-read); settings profiles with CONST/MAX-constrained limits + quotas; grants only on the serving database; `system.query_log` and friends never granted (they leak other tenants' SQL). Local and hosted are one engine family — chdb-core and the Cloud target pinned to the same version stream — so dialect agreement is inherited, not maintained; residual version drift (Cloud auto-upgrades) is handled by `SETTINGS compatibility = '<pinned>'` in the hosted profile and the **conformance corpus** (catalog queries + trap cases with asserted outputs, incl. NaN fixtures) run in CI against pinned binary × Cloud staging — divergence is a release blocker or a documented erratum (identical policy statement in studio §56). CID note: hosted `value_roots_v1.cid` is a tenant-scoped token, not the raw local CID; the two columns are documented as non-comparable (studio doc §6.5 decision).

### 10.6 Value hydration at query time

Nothing about bodies is pre-materialized beyond bounded previews. Three tiers: (1) **CID columns** — equality/dedupe/drift queries (`GROUP BY cid`, verify-my-fix joins) need no hydration at all; (2) **`value_scalars_v1` previews** (≤4 KiB, redaction-respecting) — the 80% "show me the prompt that…" case, projected at projection time; (3) **explicit budgeted pre-hydration** — `baml query --hydrate run=<id> role=output --max-bytes 256mb`, or predicate-scoped: `--hydrate --where <sql>` (a cheap CID/preview-tier pass computes the scope) — resolves distinct CIDs once each through the standard budgeted read path into a temp `value_bodies_v1` Parquet bound into the query. Budgets enforced *outside* SQL with the existing contract semantics; hydration cost ∝ distinct content (dedupe honored). Executable-UDF hydration was evaluated and rejected as the primary mechanism — ClickHouse Cloud doesn't support executable UDFs, so it would fork the dialect on exactly the feature users touch most; it remains a documented local-only power tool at most. The same contract sentence holds locally and hosted: "`value_bodies_v1` exists for the scope you hydrated, with an explicit budget."

### 10.7 Query performance engineering

Local-tier query cost is dominated by fixed overheads, and each is engineered down: **engine cost** (measured 0.4 ms warm in-process with embedded chDB — no longer the dominant term; cold first query ~305 ms, once per process); **file opens** (hive-partitioned `run_id=` paths so predicate pushdown prunes whole files; Parquet row-group min/max statistics prune within files; compaction rewrites old per-run files into consolidated monthly files at ~500/view so open-cost stays bounded); **scan width** (explicit column schemas, zstd, denormalized identity so the common queries touch no dictionary join); **memory** (`max_memory_usage` capped in init SQL — a runaway join fails loudly instead of OOMing the laptop). The deepest optimization is upstream of SQL entirely: population rows arrive pre-aggregated by the runtime (one row per calling context, not per call), so the data volume SQL ever sees is the *shape* of the program, not its traffic. Interactive UI reads stay off SQL altogether on the 2.62 ms fold-engine path. Hosted-tier optimization (ORDER BY design, ClickHouse projections, scheduled rollups, query-shape discipline, quotas) is the studio doc's §35.

## 11. The UI plane (playground)

One product: **`baml playground`** (no separate `baml studio`). The playground UI reads observability through **~6 private RPC method families** served by the fold engine (`bex_query`) — run list, run snapshot + patches, CCT graph/profile, values list/read, source — over the same server the playground already runs (the hosted API's §41 block is the route-level expansion, a superset of these six). This is internal plumbing, not a query surface: no stability contract beyond the UI, free to churn with the UI. Two data paths feed it:

- **Files**: mmap + fold over `.baml/` (history and sealed sessions) — 2.62 ms open, O(pixels) rendering, BQF1 wire frames to the webview (the `BqlTable` frame kind is deleted with BQL).
- **In-process RAM tap**: live runs executing in the same process stream engine deltas straight to the UI (snapshot + monotone patch cursor), giving sub-window latency for the local dev loop without touching disk.

Current shipped UI: runs list, CCT tree/flame/timeline, per-node detail (status, timings, histogram, LLM meta), captured-values panel (roots by function/role, budgeted JSON hydration). An in-app SQL box (echoing `baml query`) is an open question (§17), not a commitment. The full product surface — five screens, live cursors, comparison — is the studio doc §44–§45.

---

# Part III — Correctness, performance, maintainability

## 12. Correctness strategy

**The oracle.** The raw firehose (§5.7) is byte-exact ground truth. The differential harness replays recorded raw streams through the CCT engine and independently through a naive reference fold (simple, obviously-correct, unoptimized); totals, per-context counters, histogram sums, and status counts must match exactly. Property tests generate adversarial interleavings (cross-thread reorderings, torn ranges, duplicate drains, clock anomalies) and assert the same equivalence plus the resync invariants (never wedge; degraded partitions flagged; unattributable time lands on function 0, not nowhere).

**Golden pinning.** Every on-disk format has committed golden fixtures asserting exact bytes: BCCT blocks + torn-tail recovery, BMET records, `.bamlcct` snapshots, raw container, canonical value encoding (bytes *and* CIDs), decode∘encode identity, pack/index framing, dictionaries, BQF1 frames. A golden diff is a format version bump, never an accident. Test inventory as of this writing: 38 prof-gate tests, 13 golden (v1+v2), 16 canon, plus per-crate units.

**Loss accounting invariants.** Ring shed, staging eviction, drain-budget drops, recorder eviction, defer-resync synthesis, clamped clocks — every loss path increments a named counter that lands in a queryable marker/watermark block. The test suite asserts the *counters fire* under induced pressure, not merely that the system survives.

**Crash consistency.** Kill-at-every-boundary tests: torn segment tails recover to the last intact block; sealed artifacts are never torn (D2); crashed sessions terminalize via read-time classification (§8.3) with the intact prefix preserved; pack writes are atomic per chunk with index rebuild-on-mismatch.

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

(C-gate legend — ids from the completed implementation plan's criteria list, kept for ledger traceability: C3 disk-per-run, C5 transcript dedupe, C6 open-path latency, C7 consumer peak RSS under sustained load, C10 value-drain CPU isolated from the prof consumer, C11 server memory O(live boundaries).)

Units, precisely: **74.4 ns/call is producer-visible wall overhead** (paired runs, profiling on vs off — what the observed program pays); the consumer runs concurrently on its own core, and its cost is the separate 10 M-record row (68–75 ns/record ≈ 140 ns/pair sustained ≈ 7 M pairs/s/core). Within the consumer, the CCT-engine slice is ~50 ns/pair (decode 9.6 + intern/charge/defer ~38 + recorder memcpy 2.3); the remainder is session storage and bookkeeping. SQL-tier targets (gates, not yet results): `baml query` p50 < 1 s end-to-end against the pinned Q2 corpus; projection cost for CCT/meta/runs views is provably negligible at corpus volumes (p99 snapshot 226 KB; worst source rate ~1.9 MB/s) — value previews are the one projection scaling with captured roots rather than shape (one pack read + budgeted ≤4 KiB decode per root, bounded per run by a preview byte cap, default 64 MiB, then run-level `preview_truncated`, counted; priced in the Q1 gate); the per-query engine cost is measured at 0.4 ms warm in-process (chDB, 2026-08-06) — the latency budget is now footer reads + scan, gated per M2.

**Enforcement is continuous, not commemorative:** a nightly perf-CI job on a dedicated runner class re-runs the integrated engine benchmark, the paired end-to-end benchmark, and the consumer-CPU benchmark against committed baselines with a ratio noise band — a breach of the ≤60 ns never-exceed or a >10% regression on any ledger row fails the build. The p99-shape leg's CI-hardware confirmation is a **blocking** Q3 item, not a residual.

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

Residual small items carried from the phase reviews (none blocking): SDK/pack-host dictionary emission parity; `baml.toml` knob surface for budgets; audit-record polish; flight-dump `.bamlcids` pinning edge cases; automatic retention/GC invocation at session open (§8.5). (The p99-shape CI-hardware re-measurement is **not** on this list — it is a blocking Q3 gate, §13/§16.)

## 16. Phases to build (the SQL tier)

(Namespace note: Q0–Q5 here are *this doc's phases*. The studio doc's Q1–Q5 are *product questions* — an unrelated namespace; when this doc means the studio's open decision it says "product decision Q1".)

**Q0 — Deletion and rename (small; executes only after Q2 reaches demo parity — the demo never breaks).** Delete bql.rs/tests, `baml q`, BqlTable frames + TS decode; unify command surface under `baml playground`; rewrite demo agent docs against the shipped `baml query`. Gate: no `bql` symbol in tree; playground unaffected; demo end-to-end on the new surface.

**Q1 — Projector + manifest.** Parquet projection of sealed artifacts per the §10.4 layout (runs, cct_population, llm_population, spawn, value_roots, value_scalars, capture_losses, functions, revisions, exact_windows, call_instances from dumps), manifest with seal-CRC drift detection, retention/tombstone handling, compaction. Gate: projections rebuild byte-stable from fixtures; drift/tombstone scenarios covered; preview-projection cost measured (per-root and per-run bounds, §13).

**Q2 — View DDL v1 + `baml query` on embedded chDB.** chdb-rust over the stable C ABI with vendored, checksummed libchdb (§10.4 hardening list: signal handlers off, memory-cap CI test, one-connection-per-process, version pinned to the hosted family, `BAML_CHDB_LIBRARY` override, thin seam keeping `--engine=clickhouse-local` alive); generated init script (schemas, views, UDFs, memory caps); statement passthrough; freshness footer; `--schema` rendering view/column comments; hot-tail projection for near-live; `--hydrate` (incl. `--hydrate --where <sql>`) into `value_bodies_v1`; `--hosted`/`--both` routing; Ctrl-C cancellation with stated semantics; WSL documented for Windows. Gate: the user-story query catalog (§4) passes end-to-end featuring the flagship values→functions→aggregates pattern; warm p50 and cold first-run measured against the pinned corpus (1,000-run history, pre/post-compaction, download excluded — recorded in the studio §59 engine envelope); a runaway query fails loudly under the memory cap without killing the CLI; local near-live freshness measured and pinned (§10.4); docs contain the trap-case pairs.

**Q3 — Conformance corpus + agent eval.** Catalog + trap queries with asserted outputs (integer quantiles, NaN fixtures, empty instance windows, cross-revision grouping); CI against the pinned binary; agent eval (SQL-over-views task success, trap cases included) before the v1 schema freeze. Gate: corpus green in CI; eval results recorded; **CI-hardware confirmation of the ≤60 ns never-exceed p99-shape leg (blocking)**.

**Q4 — Hosted endpoint.** `(version, sql)` API over the same DDL on ClickHouse Cloud; grant-profile-grain CH identities (studio §37.3), base-table row policies + admin policy, column-scoped grants, INVOKER views, CONST/MAX profiles + quotas, serving-db-only grants; corpus extended to Cloud staging (tenancy probes through views *and* base tables). Owned jointly with the studio doc's P0-C. Gate: cross-tenant attack tests pass; corpus parity local×hosted.

**Q5 — Time-series + windows projection (fast-follow).** `cct_windows_v1` lazy projection for session time series; `spawn_instances_v1`; optional in-app SQL box if §17-Q1 resolves yes.

## 17. Open questions

1. **In-app SQL box** in the playground: promoted to an **M4 stretch** — a resident chDB session makes it interactive-grade (0.4 ms/query); pulled in if Q2/M2 lands smoothly.
2. **Typed hydration surface** beyond JSON (schema-aware rendering of hydrated values in CLI output)? Default: JSON + previews suffice for v1.
3. **wasm capture hosts**: browser-*executed* runs still capture in-memory (4 MiB recorder, inline values); with browser-local querying gone, does the wasm live-view fold path in `bridge_wasm` stay (for same-page live rendering) or go (server-backed everywhere)? The capture-host scope is resolved (studio doc §11.4: diagnostic-only, embedded wasm SDK users); what remains open here is the live-fold path — default: keep it (shipped and size-gated) until promptfiddle server-backing lands, then re-evaluate against the size gate's ~100 KiB headroom.
4. **OQ6 (carried)**: stdlib introspection surface (`baml.profile.*`) stays dead by default — confirm no product need before deleting the stubs.
5. **OQ7 (carried, shipped contract)**: `$id`/`SetFunctionId` overrides are visible in rings/dumps only, not CCT identity. Confirmed as the durable contract; revisit only with a concrete demand.
6. **Windows** — resolved 2026-08-06: no native Windows exists anywhere in the ClickHouse engine family (verified upstream), so this was never an engine-choice question. `baml query` on Windows is WSL, stated plainly in the docs; revisit only if upstream ships native Windows.

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

**Boundary/run** — one observed root execution, ULID-identified. **Partition** — spawn tree of one root thread; binds to a boundary. **Session** — one engine process's working directory. **Epoch** — node-id scope within a session. **CCT** — calling-context tree (the tally). **Tape** — bounded exact-event windows (ring, flight recorder, full trace, raw). **CID** — BLAKE3 content id of a canonical value DAG. **Grain** — population (per code path, complete) vs instance (per recorded call, windowed). **Fold engine** — `bex_query`'s sans-io reader powering the UI. **Projection** — rebuildable Parquet derived from sealed artifacts. **View contract** — versioned, grain-named SQL views; the public query surface. **Watermark** — highest contiguous proven position, never merely highest seen. **Sealed** — immutable-by-contract; seal events per artifact: CCT segments at 4 MiB rotation or session close (footer + D2 fsync-rename); the boundary snapshot and meta finalization at completion (a dead session's intact prefix is history-grade via read-time crashed classification, §8.3); flight dumps and raw files at write/rotation; CAS packs at group-commit sync, with `manifest.bamlcids` in the same barrier. **BEX** — the BAML execution engine (the `bex_*` crate family); a BEX thread is one of the engine's logical threads.
