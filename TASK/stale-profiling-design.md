# BAML Observability -- Canonical Design

**Date:** 2026-07-30.

## Summary

### The data structures

| Structure                                 | Key / identity                                                                | Fields carried                                                                                                         | Written                                                 | Dedup / growth law                                                                                               |
| ----------------------------------------- | ----------------------------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------- |
| **CCT node** (`node_birth`, 24 B)         | `(parent_node, function_id)` interned → dense `node_id` (session-epoch scope) | parent, function_id, thread, partition, flags, depth — **no strings**; name/source resolve via the revision dictionary | once, at first sight of a new calling context           | one row per **unique context ever** (corpus p99: 3,537) — never per call                                         |
| **Counter deltas** (`cct_delta`, 48 B)    | node_id                                                                       | enters, ends_ok/err/cancel/exit, total_ns, self_ns, await_ns                                                           | per **dirty node per 250 ms window**                    | grows with active-nodes × windows; a window of 9.5 M calls to one node = one row                                 |
| **Duration histogram** (`cct_hist`, 68 B) | node_id                                                                       | 16 × u32 buckets, ×4 stride (1 µs → ≥ ~18 min) — the source of p50/p95/p99; no mean/median stored                      | per node **with ≥1 close** per window                   | same law as deltas; idle node = zero rows                                                                        |
| **LLM counters** (`llm_delta`)            | (node_id, model_id)                                                           | llm_calls, tokens_in/out, provider_errs, parse_errs                                                                    | per dirty LLM node per window                           | model names interned once (`model_birth`)                                                                        |
| **Spawn edges** (`spawn_edge`)            | (parent_node, child_entry_fn)                                                 | spawn/live/completed/errored/cancelled, running/awaiting ns                                                            | per dirty edge per window                               | 10k equivalent workers = **one** edge + one shared child subtree; instances kept only first 64 + 256 exceptional |
| **Checkpoints** (`node_total`)            | node_id                                                                       | same 48 B, absolute values                                                                                             | when delta bytes since last ≥ checkpoint size           | bounds read cost; ≤2× write amplification                                                                        |
| **Watermarks**                            | —                                                                             | drained-through ts, durable kind                                                                                       | at D1 syncs after new blocks; ≥10 s heartbeat when idle | the completeness anchor; idle ≈ 7 B/s                                                                            |
| **Recent-call ring** (RAM only)           | (thread_idx, call_id)                                                         | last 4096 exact calls + all open calls, 56 B slots                                                                     | never (feeds live UI)                                   | fixed; eviction counted, never silent                                                                            |
| **Flight recorder** (RAM → dump)          | raw drained bytes                                                             | exact events, 16 MiB / 4 MiB window                                                                                    | `.bamlprof` dump **on trigger only**                    | bounded window; dumps pin their values via `.bamlcids`                                                           |
| **Values** (capture roots + DAG)          | root: `value_N` (never renumbered); content: BLAKE3-256 **CID**               | role, call key, timestamps + canonical value DAG chunks in packs                                                       | roots per capture; chunks **only if CID unseen**        | identical prompts/transcript prefixes stored once project-wide; growth ∝ distinct content                        |
| **Revision dictionary** (`.bamldict`)     | revision_id (BLAKE3-256 of source×toolchain)                                  | per function: fqn, source span, definition_key, def_content_hash, capture flags                                        | **once per revision** (~180 KB)                         | replaces the 129 KB table per file; idempotent write                                                             |
| **Meta** (`boundary/session.bamlmeta`)    | boundary_id (ULID) / session dir                                              | begin/bound/complete records, counts, diagnostics                                                                      | at milestones (D2)                                      | O(1) run listing; crash = begin without complete                                                                 |

### What each event changes

| Event                            | RAM effect                                                                                                            | Disk effect                                                            |
| -------------------------------- | --------------------------------------------------------------------------------------------------------------------- | ---------------------------------------------------------------------- |
| `CallFunction`                   | charge parent's self-time up to ts; intern child node (new context → birth queued); push stack; enters++; dirty-mark  | none per call — everything reaches disk via window flush               |
| `EndFunction`                    | charge self-time; ends_status++; total_ns += duration; hist bucket++; recent-ring slot                                | none per call                                                          |
| `SuspendThread` / `ResumeThread` | thread marked parked / awaiting duration credited to innermost node's await_ns (resume is self-contained)             | none per event                                                         |
| `StartThread` / `EndThread`      | new thread state inherits partition; spawn edge interned at first root call / final charge + edge completion counters | none per event                                                         |
| `LlmCallMeta`                    | (node, model) counters bumped; new model interned                                                                     | `model_birth` row at next flush                                        |
| `SetFunctionId` (`$id`)          | recent-ring annotation                                                                                                | durable only in dumps/full trace (OQ7)                                 |
| **Error / latency trigger**      | flight recorder transcodes → dump; staged values promoted; boundary capture flags raised                              | `flight/<ts>-<trigger>.bamlprof` + `.bamlcids`; promoted value records |
| **Window close (250 ms)**        | dirty set swept, `last_flushed` reset                                                                                 | delta + hist + llm + spawn rows for dirty nodes only                   |
| **D1 commit (1 s / 1 MiB)**      | watermark advances (fsync off-thread)                                                                                 | watermark row                                                          |
| **Boundary complete**            | partition folded then **freed** (server memory stays O(live))                                                         | `cct.bamlcct` snapshot + complete record                               |
| **Value capture**                | TraceHeap copy → canonicalize → CID                                                                                   | pack append only for unseen CIDs + one root record                     |

**Growth law:** bytes ∝ unique contexts × active windows + distinct content + triggers — never ∝ call count or repeated-capture count.

### Don't FRET!!

Every bound below has a counter or marker; **bounded never means silent** (principle #2).

> **"The node table grows forever in a long-running server."**
> Nodes grow with _unique calling contexts_, not calls — measured p99 is 3,537 nodes. Pathological recursion is capped by the depth-512 fold (§5.6), and the whole table is rotated by session epochs at 256 MiB / 24 h (§6.1).

> **"Per-boundary state (recent-call ring, instance tables, defer buffers) accumulates."**
> Partitions are sealed and **freed at boundary completion**, not engine close — server memory is O(live boundaries), enforced by the C11 RSS gate on a 10k-boundary workload (§5.7).

> **"The producer ring can still grow to 1 GiB and abort."**
> Dev keeps abort (loud, debuggable). Servers default to the shed ladder: recorder → full trace → value encoding → structural ranges, each step counted, CCT aggregation never disabled, no abort in `shed` mode (§5.10, gated by C12).

> **"The CCT stream on disk grows forever."**
> It grows with _time_, not call rate (one row per dirty node per 250 ms window — the 36 M-call hot loop writes ~3.5 KB/s, §6.3). Checkpoints bound read cost (§6.3), epochs rotate the stream (§6.1), and retention ages sessions out at 7 d / 1 GiB — after materializing the folded snapshot for any crashed boundary first (§6.8, §6.1).

> **"Histograms every 250 ms is a lot of rows."**
> Only nodes that _closed a call_ that window emit one; idle costs ~7 B/s (heartbeat watermarks only). When segments age out, the permanent record collapses to one folded histogram per node in `cct.bamlcct` (§6.3, §6.8).

> **"Value capture will eat the disk."**
> Content addressing makes repeated content free (transcripts: quadratic → linear, C5 gate ≥20× at N=64); the staging ring is byte-capped with evictions reported (§7.2); drain budgets degrade to explicit `CaptureLoss`, never blocking (§7.3); and the store has a 4 GiB soft budget backed by boundary eviction + GC (§6.8, §6.7).

> **"The flight recorder is another buffer that grows."**
> Hard byte cap (16 MiB native / 4 MiB wasm), whole-chunk FIFO, and the retained-window boundary is queryable; dumps are rate-limited (≥5 s apart, ≤16/boundary) (§5.9).

> **"Deferred records could pile up if a parent never arrives."**
> Deferral is bounded (1024 sweeps); on timeout the parent is synthesized as the unattributable node, dependents replay, and the partition is marked degraded — a wedge is structurally impossible (§5.2).

> **"Opening multi-GB history will OOM the viewer."**
> The query engine is sans-io with byte-budgeted caches (256 MiB native / 32 MiB wasm) and every response ≤ `max_bytes` with visible LOD degradation — viewport cost is O(pixels), proven by the C7 invariance gate (±10% bytes between a 1 M-call and 36 M-call run) (§9.2, §9.3).

> **"What about full trace / the raw firehose?"**
> Both are opt-in and bounded: full trace ends in an explicit `TraceBudgetExhausted` marker (§3), the raw sink hides behind `BAML_PROFILE_RAW` with rotation, and both are the _first_ casualties of the retention degradation order (§6.8).

## Contents

1. Vision and principles
2. What this enables (user stories)
3. Capture contracts
4. Compile-time identity
5. The CCT engine
6. On-disk: layout, formats, durability, retention
7. Values: policy, staging, and the content-addressed store
8. The query surface: BQL
9. The local web app
10. Benchmarks and acceptance criteria
11. Implementation plan (merged phase ledger)
12. Risks and open questions
13. Appendix A: Reconciliation register
14. Appendix B: Relationship to prior documents

---

## 1. Vision and principles

BAML programs are observable **by default**. A user (or an AI agent acting for them) can ask what their program did -- in the dev loop and in production -- without having planted log statements, timers, or flags. When something goes wrong, the evidence already exists, bounded and queryable.

The current implementation proved the capture layer (a ~10 ns/call producer, rich metadata, content-addressed blobs) and exposed the representation problem: one disk event per call means bytes grow with call rate. A measured 3.8-second hot loop produced 1.69 GB at 446 MB/s -- 38.5 TB/day extrapolated -- while the same execution collapses to a three-node calling context. Value capture is separately significant: transcript-style captures grow quadratically (N(N+1)/2) even though distinct information grows linearly.

The design principles, in priority order:

1. **Cost grows with unique behavior, not with work done.** Profiling cost is proportional to unique calling contexts x time resolution, never to call rate. Value cost is proportional to distinct content, never to capture count.
2. **No silent truncation, ever.** Every bound has a counter, a marker, or an explicit error. "Lossless" is contractual per capture mode (Section 3), and missing data is always detectable. An empty query result explains itself.
3. **The producer never blocks and never regresses.** The VM hot path stays at ~10 ns/call. All aggregation, hashing, and I/O happens on cold paths.
4. **O(pixels), not O(events).** No UI or query response is proportional to event count. Every response is bounded by request parameters with visible LOD degradation.
5. **Integer-keyed identity, fixed at compile time.** Function identity is a dense `u32` assigned by the compiler; names, spans, and metadata live in a per-revision dictionary written once. The runtime never interns or passes strings on any per-call path.
6. **Local-first, cloud-shaped.** Everything lands under `.baml/` in formats designed to upload incrementally later (content-addressed values, revision-scoped dictionaries, time-bucketed aggregate deltas). Nothing in v1 requires a server.
7. **One engine, one wire.** A single Rust query engine (`bex_query`, native + wasm) serves the CLI, the web app, the playground, VSCode, and agents. There are not two data planes.

---

## 2. What this enables (user stories)

Stories were generated from four personas and pressure-tested against the capture contracts (Section 3). Each is answerable by a specific contract; where a story needs data the defaults do not capture, the answer is an explicit trigger or opt-in -- never a silent gap. Priorities: P0 = launch-blocking, P1 = fast-follow, P2 = later.

### 2.1 The app developer (local dev loop)

| P   | Story                                                                                                             | Served by                                               |
| --- | ----------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------- |
| P0  | See the exact LLM input/output for a wrong answer -- click any LLM call, see the rendered prompt and raw response | values (default: LLM functions)                         |
| P0  | Compare two runs after a prompt edit -- prompt diff, output diff, per-function counts/latency/errors              | values + CCT + revision dictionaries                    |
| P0  | Find an accidental hot loop in seconds -- the 36M-call run collapses to a 3-node CCT naming the runaway path      | CCT                                                     |
| P0  | Diagnose a failed run: error payload, the failing helper's actual args, exact events just before the failure      | error trigger -> value promotion + flight-recorder dump |
| P0  | Open yesterday's run I forgot to save -- browse history, inspect call tree + prompts + outputs                    | durable history + retention window                      |
| P1  | Is it my code or the model? -- running vs awaiting breakdown per calling context                                  | CCT (self/await accounting)                             |
| P1  | Count every LLM call including hidden retries; detect byte-identical duplicate prompts                            | CCT + value CIDs                                        |
| P1  | Find the straggler in a parallel fan-out                                                                          | CCT spawn-edge aggregates                               |
| P1  | Set a latency trigger to capture the rare slow call                                                               | trigger config + flight recorder                        |
| P1  | Promote a helper function's values to debug wrong-but-not-erroring behavior                                       | `@capture` opt-in                                       |
| P2  | Bounded full trace for an exact-ordering bug                                                                      | full trace (opt-in)                                     |
| P2  | Iterate on a long agent transcript without drowning my disk                                                       | value DAG dedupe                                        |

### 2.2 The production operator

| P   | Story                                                                                           | Served by                                 |
| --- | ----------------------------------------------------------------------------------------------- | ----------------------------------------- |
| P0  | Reconstruct what the service was doing at 03:04                                                 | CCT time series + flight recorder         |
| P0  | Attribute a production error to the exact failing inputs                                        | error-trigger value promotion             |
| P0  | Find a latency regression by calling context, not just function name                            | CCT (context-keyed, dur_hist)             |
| P0  | Correlate a deploy with a behavior change -- one continuous timeline across a revision boundary | CCT + cross-revision alignment            |
| P0  | Trust the data: explicit loss markers, never silent truncation                                  | watermarks + CaptureLoss + `health()`     |
| P0  | Detect a runaway loop / retry storm before it detonates the bill                                | CCT live deltas                           |
| P1  | Track LLM token spend and retry cost by calling context                                         | CCT LLM counters                          |
| P1  | Enforce retention and honor deletion for captured values                                        | retention + CAS GC + `audit()`            |
| P1  | Distinguish "stuck on provider" from "burning CPU"                                              | self vs await counters                    |
| P1  | Exact event dump when a request breaches SLO                                                    | latency trigger -> flight recorder        |
| P1  | Keep telemetry itself within budget and observable                                              | self-accounting (`health()`, `storage()`) |
| P2  | Bounded full trace against one production instance                                              | full trace (opt-in, bounded)              |

### 2.3 The AI agent (autonomous debugging and optimization)

The agent persona sharpens requirements humans tolerate: results must be structured and bounded (an agent cannot skim a million rows), IDs stable across queries, completeness machine-checkable before the agent commits to a conclusion, and the highest-stakes workflow -- _verify my own fix_ -- a first-class operation.

| P   | Story                                                                                                        | Served by                                    |
| --- | ------------------------------------------------------------------------------------------------------------ | -------------------------------------------- |
| P0  | Enumerate recent runs as a bounded, paginated index with stable IDs                                          | run index (bamlmeta scan; ULID boundary ids) |
| P0  | Fetch the exact input that caused an error within a byte budget, with child CIDs for selective descent       | value DAG + bounded hydration                |
| P0  | Find the hottest calling context (top-k CCT nodes with full paths)                                           | CCT                                          |
| P0  | Diff a function's outputs across runs with Merkle short-circuit                                              | value DAG (CID equality)                     |
| P0  | Verify my own fix: before/after revision comparison with matched inputs and an explicit completeness verdict | `diff` + `compare(match_io)`                 |
| P1  | Bisect when a regression appeared across time and revisions                                                  | CCT series + revision alignment              |
| P1  | Pull the exact-event window around a failure                                                                 | flight-recorder dumps                        |
| P1  | Check capture completeness before trusting any conclusion                                                    | mandatory result meta footer                 |
| P2  | Request an explicit bounded full trace for exact-ordering questions                                          | full trace                                   |

### 2.4 The engineering lead

| P   | Story                                                                                         | Served by                          |
| --- | --------------------------------------------------------------------------------------------- | ---------------------------------- |
| P0  | LLM spend by feature and model over a month; count-driven vs token-driven                     | CCT LLM counters + `lookup()`      |
| P0  | Error and retry rates by function across revisions                                            | CCT + revision alignment           |
| P0  | Parse-failure hotspots by prompt/schema, with raw output on failures                          | CCT + value capture join           |
| P0  | Privacy/consent audit: what was captured, by role/trigger/scope; promotions lacking redaction | audit records                      |
| P1  | Output drift review after a prompt change                                                     | value CIDs across runs             |
| P1  | Which agent tools are actually called                                                         | CCT                                |
| P1  | Latency regression: ours vs the provider's                                                    | self vs await split                |
| P1  | Forecast telemetry cost at 10x traffic                                                        | self-accounting                    |
| P2  | Concurrency/spawn-fanout capacity review                                                      | spawn aggregates + flight recorder |

### 2.5 Requirements the stories force into the data model

Walking the stories back through the capture contracts yields requirements a pure profiling design would miss. These are **binding on the formats in Section 6**:

1. **LLM enrichment is persisted.** Today an LLM call is an anonymous sysop; nothing records model, token usage, or provider errors durably. The P0 cost and parse-failure stories require per-call LLM metadata (model, tokens in/out, error class) rolled into CCT counters (Section 6.3 `llm_delta` blocks) and stamped on value records.
2. **Latency histograms are in the storage schema from day one.** Tail-latency stories (p95/p99 per context) cannot be answered retroactively if delta blocks carry only sums. A fixed-width log2-bucket histogram column family (Section 6.3 `cct_hist`) is a now-or-never schema decision.
3. **Telemetry is itself a first-class observable.** `health()`, `storage()`, `audit()` queries require the pipeline to record watermarks, capture loss, shedding, backlog age, and termination cleanliness as data, not log lines.
4. **Completeness is part of every answer.** Every query result carries what was and wasn't covered (Section 8.4). "No data" is always distinguishable from "no events".
5. **Cross-revision identity is a join, not an accident.** Dense function ids are per-revision; every "across deploys" story goes through dictionary alignment (`definition_key` + `def_content_hash`, Section 4.4).
6. **Value identity (CIDs) is a query primitive**, not just a storage optimization: duplicate-prompt detection, drift review, and Merkle-short-circuit diffs group and compare by CID.
7. **Run ids sort by time.** `BoundaryId` is minted as a ULID (48-bit ms timestamp + 80 random bits) in the same 16-byte, `baml_id_1_...`-encoded shape. The **raw 16-byte payload** sorts chronologically; the base64url string form does _not_ (the URL_SAFE alphabet's value order differs from ASCII order), so keyset pagination decodes cursors and compares payloads (or uses the run index's `created_ms`) rather than comparing strings. No format change; old random ids remain valid and merely sort arbitrarily among themselves.

---

## 3. Capture contracts

Four explicit capture modes. "Lossless" is defined per contract; silent truncation is not an implementation of any of them.

| Contract            | Default                                                           | Promises (complete information)                                                                                                                                         | Intentionally not promised                                                                                                                                   |
| ------------------- | ----------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------ |
| **Aggregate CCT**   | ON, everywhere, every function                                    | Counts, total/self/awaiting ns, status counts, duration histograms, LLM counters, spawn aggregates -- per calling context per time window, with completeness watermarks | Exact invocation order; exact per-call timestamps; per-call values; per-call `$id` override records (durable only in dumps/full trace -- see Section 12 OQ7) |
| **Values**          | ON for root function + LLM functions; trigger-promoted for others | Every selected value, or an explicit `CaptureLoss` record; content-addressed, deduplicated                                                                              | Values of unselected calls                                                                                                                                   |
| **Flight recorder** | ON, bounded (16 MiB native / 4 MiB wasm)                          | Exact events within the retained window; dumps bound to triggers                                                                                                        | Events older than the declared window                                                                                                                        |
| **Full trace**      | OFF; explicit opt-in, bounded                                     | Exact events for the bounded session, or an explicit `TraceBudgetExhausted` terminal marker                                                                             | Anything past the declared bound; wrap-around is deliberately not offered in v1                                                                              |

### 3.1 Triggers and promotion

Triggers connect the always-on modes to the exact-evidence modes:

- **`OnError`** -- a root-level or policy-matched `Errored`/`Cancelled` close: dumps the flight recorder and promotes staged values (Section 7.2) in the failing subtree to durable storage.
- **`OnLatencyMs(t)`** -- a call closes with duration > t (sysop default 30 s).
- **`Manual`** -- host/CLI/query-surface request.
- **Trigger side effect:** a firing trigger may also raise the boundary's value-capture flags for the remainder of the run (capture more once something is wrong).

Rate limits: >=5 s between dumps per boundary, <=16 dumps per boundary, with a `dropped_dumps` counter. Trigger configuration lives in `baml.toml [observability]` and is adjustable at runtime through the host API.

### 3.2 Defaults by host

| Host                           | CCT              | Values                               | Flight recorder | Full trace                  |
| ------------------------------ | ---------------- | ------------------------------------ | --------------- | --------------------------- |
| `baml run` / `baml test` (CLI) | on               | root io + LLM Auto + error promotion | on              | off                         |
| SDK (cffi) / pack host         | on               | root io + LLM Auto + error promotion | on              | off                         |
| Playground / `baml studio`     | on               | same + logs                          | on              | off (one-click arm per run) |
| wasm (VSCode/browser)          | on (in-memory)   | same, inline-only                    | on (4 MiB)      | off                         |
| CI (`BAML_HISTORY=0`)          | on, session-only | off                                  | on              | off                         |

This table is a **product change with a privacy consequence**: CLI and SDK runs, which today persist nothing, will persist captured root/LLM inputs and outputs under `.baml/`. That change is named, documented, and controlled by `BAML_HISTORY=0` plus per-function capture attributes; Section 7.5 covers redaction and audit.

## The wiring that makes this real on CLI/SDK/pack hosts -- minting a `BoundaryId`, binding it to the consumer, writing `boundary.bamlmeta`, enabling capture defaults, and draining values continuously -- does not exist today and is a named implementation phase (Section 11, Phase H). Without it, "on by default" would be vacuous: today only the playground and wasm hosts mint boundaries or enable capture.

## 4. Compile-time identity

Everything in this section makes integer-keyed identity a _compiler output_, leaves the VM hot path byte-for-byte unchanged, and turns the 129 KB-per-file metadata table into a once-per-revision dictionary.

### 4.1 `function_id` is assigned by the compiler

`function_id` is derived state: the dense enumeration of `Object::Function` entries in final object-pool order, stamped by exactly one finalizer, never at construction sites and never at engine init.

New module `crates/bex_vm_types/src/identity.rs`:

```rust
pub const FUNCTION_ID_UNKNOWN: u32 = 0;        // unattributable (existing sentinel, kept)
pub const FUNCTION_ID_SPAWN_CLOSURE: u32 = 1;  // spawn-closure child roots
// 2..=15 reserved (host-call frame, GC frame, native frame, ...)
pub const FIRST_POOL_FUNCTION_ID: u32 = 16;

/// Stamp dense ids onto every Object::Function in pool order. Idempotent.
pub fn assign_function_ids(program: &mut Program) -> u32;
```

Pool order stays the rule because it is exactly what the engine walk does today (`bex_engine/src/lib.rs:1581-1595`), it is deterministic (the linker's layout contract), and the VM already reads `f.function_id` off the heap object at call time -- **no VM change**.

The finalizer (`finalize_program_identity` = `assign_function_ids` + revision hashing + `Program.identity` attachment) is called at every site that materializes a runnable `Program`:

1. Full / stdlib-seeded compile -- tail of the public entries in `baml_compiler2_emit`.
2. Incremental reuse compile -- on the **linked** program (partial dirty-only programs are never finalized).
3. **Pack load -- after the `PackEnvelope` borsh deserialization in `baml_pack_host`** (and any future `Program` deserialization site). Note: the envelope is loaded directly via borsh, not through `link::link` -- the finalizer hooks the load site. _Prerequisite PR:_ `PackEnvelope` today is a bare versionless borsh struct; it gains a magic + version prefix first. Because packed binaries embed the envelope next to their own matching host (libsui), cross-version envelope reads essentially cannot occur, so no legacy `FunctionV1` decoder is built -- the version prefix exists to fail loudly, and identity-less programs fall back to the content hash of Section 4.3.

The engine walk keeps only `lower_to_compact` and asserts ids are stamped (`debug_assert` full check; release checks last id only). The five `function_id: 0 // assigned at engine init (interim provider)` construction sites keep `0` with an updated comment. `Function.function_id` **stays `#[borsh(skip)]`** -- units cannot carry final ids (a unit doesn't know its pool position until link), re-derivation is one walk, and the B-693 byte-identity oracle continues to hold unchanged.

Synthetic rows move to the reserved low range: the dictionary always emits rows for ids 0 and 1 first; the engine and VM stamp spawn-closure roots with the constant instead of `pool_count + 1/+2`.

### 4.2 The revision dictionary

One file per compiled revision, written once, referenced by every segment header.

```rust
pub struct RevisionId(pub [u8; 32]);        // "baml_rev_1_" + base64url(32B)
pub struct SourceSnapshotId(pub [u8; 32]);  // "baml_src_1_" + base64url(32B)

pub struct ProgramIdentity {
    pub revision_id: RevisionId,
    pub source_snapshot_id: SourceSnapshotId,
    pub compiler_id: String,      // canonical version + channel + commit; dev: "dev+" + blake3(exe)
    pub function_count: u32,
}
// Program gains: #[borsh(skip)] pub identity: Option<ProgramIdentity>
```

**Ruling (32 vs 16 bytes):** `RevisionId` is BLAKE3-256 (32 bytes) everywhere -- segment headers reserve 32 bytes (Section 6.2). Content-addressed identity that will eventually be a cross-fleet, cross-tenant cloud key gets full-width hashes; 16 extra header bytes are noise.

`RevisionDictionary` (pure walk over the finalized `Program`, ~single-digit ms for ~1000 functions) contains: `identity`, `capture_policy_version`, `files: Vec<FileRow>` (file_id -> path + content hash), `functions: Vec<FunctionDictRow>`, and `call_sites: Vec<CallSiteRow>` (emitted now; consumed by a later record-slimming milestone that swaps the ~16 span bytes in `CallFunction` for a u32 `call_site_id`).

`FunctionDictRow` (subsumes `bex_events::FunctionMetadata`; per-row `revision_id`/`source_snapshot_id` Options are deleted -- the dictionary is revision-scoped):

```
function_id, fqn, display_name, declared_name?,
file_id, span_start, span_end, line,
kind (Bytecode | SysOp(name) | Native), origin (UserDefined | Companion | Internal | Builtin | AutoDerive),
definition_key,          // "function:user.Extract" -- THE cross-revision join key, emitted from HIR ItemRefs
owner_type_key?,         // "class:user.Foo"
lambda: Option<LambdaIdentity>,   // structured, Section 4.5 -- never parsed from fqn
package_name?, namespace[],
capture_flags: u32,      // Section 7.1 bitfield -- "does this function capture values" without scanning
def_content_hash: [u8; 32],       // Section 4.4
semantic_lanes?          // BEP-053, lands separately
```

**File:** `<project>/.baml/dict/baml_rev_1_<b64url>.bamldict` -- content-addressed by revision id, so writes are idempotent (atomic tmp+rename; rename-race loser is a no-op). Encoding: length-delimited protobuf (`RevisionDictionaryV1`), one message per section so readers skip what they don't need. ~180 KB once per revision, versus 129 KB x every file today; the p50 profile artifact drops from 132 KB (table-dominated) to ~3 KB.

**Who writes it:** the engine passes `Arc<RevisionDictionary>` through `register_engine_metadata` at `activate_profiling`; the **consumer** writes the dict file before opening the first segment that references the revision (`ensure_dict_written`: existence check -> tmp -> rename). A segment header referencing `revision_id` is never created before the rename returns. Failure handling: dict write fails => fall back to embedding the legacy `FunctionMetadataTable` in that engine's headers (degraded but complete, warned); reader missing a dict => explicit `DictionaryMissing { revision_id }` (ids render as `fn#<id>`; recompiling the same source regenerates the byte-identical file). wasm keeps the embedded-table path (no filesystem).

**Header changes** (`EventFileHeaderV1` + the BCCT header):

```proto
message RevisionRefV1 { bytes revision_id = 1; uint32 dict_format_version = 2; uint32 function_count = 3; }
optional RevisionRefV1 revision_ref = 12;
optional bytes boundary_id = 13;   // N3: populated by boundary-scoped writers (full-trace
                                   // stack segments, flight dumps) so a segment copied out
                                   // of its directory self-identifies; session CCT segments
                                   // intentionally use partition_bind blocks instead (Section 6.1).
// field 5 function_table: kept; new native writers leave it EMPTY unless degraded/wasm.
// field 3 program_id: DELETED (random-per-engine ProgramId dies per the TASK/2 id ruling;
//   engine-instance identity is already (process_id, engine_id)).
// fields 10/11 source_snapshot_id / revision_id: now ALWAYS populated (string forms).
```

Identity precedence for boundary artifacts (absorbing N3's ladder): `boundary.bamlmeta` -> segment header `boundary_id` (cross-checked; mismatch = corruption) -> dir-name suffix fallback for legacy dirs only.

**Salsa interaction:** per-file BLAKE3 hashes are salsa queries; `source_snapshot_id` combines memoized per-file hashes (microseconds on edit); the dictionary is rebuilt per finalized Program (LSP keystrokes never reach emit); the file is written at most once per revision.

### 4.3 Revision identity: exactly what is hashed

Neither TIR nor the emitted program. Revision identity = _source x toolchain x options_:

```
source_snapshot_id = BLAKE3("baml.snapshot.v1\0"
    || u64_le(file_count)
    || for each user source file, sorted by project-relative path:
        u32_le(len(path)) || path_utf8 || blake3_32(content)
    || u8(has_manifest) || [ blake3_32(baml.toml content) ])

revision_id = BLAKE3("baml.revision.v1\0"
    || source_snapshot_id
    || u32_le(len(compiler_id)) || compiler_id
    || u8(opt_level) || u8(emit_test_cases) || u16_le(0))
```

Why not hash `borsh(Program)`: it serializes megabytes just to hash on every compile; source identity is what users mean by "revision"; and the compiler is deterministic (emit_determinism tests), so same inputs => same program. The `borsh(Program)` hash survives only as the fallback for identity-less legacy packs (domain-separated `"baml.revision.fallback.v1\0"`), computed once at engine init. Result: `revision_id` and `source_snapshot_id` are **never None on any path**, with zero hot-path hashing.

### 4.4 Per-function content hash and the cross-revision join contract

```
def_content_hash = BLAKE3("baml.def.v1\0"
    || u8(kind_tag) || u32_le(arity)
    || borsh(param_types) || borsh(return_type) || borsh(throws_type)
    || borsh(HashProjection(bytecode)))
```

`HashProjection` excludes line tables, spans, local names, debug locals, docstrings, and display strings -- **and canonicalizes inter-object references**: `ConstValue::Object(pool_index)` operands are replaced by the referent's `definition_key` string before hashing. This is a review-mandated fix: pool indices are whole-program layout, so hashing them raw would churn every unchanged function's hash on any unrelated add/remove. The pinned golden test includes the case _"edit an unrelated file => all other def_content_hashes byte-identical"_.

The join contract for "latency of `user.Extract` across the last 10 revisions":

- **Join key:** `definition_key` (compiler-emitted from HIR ItemRefs; the engine's capital-letter FQN sniffing dies).
- Per revision: open its dictionary, resolve `definition_key -> function_id`, query segments by `(revision_id, function_id)`, group by `definition_key`, annotate each revision slice with `def_content_hash` so UIs and agents can mark "code changed here" boundaries.
- **Renames break the join by design.** `def_content_hash` equality across a rename is a _hint_ ("possibly renamed from X"), never silent identity.
- No local alignment-table artifact -- alignment is computed on read (10 dicts x 1000 rows is trivial). The cloud materializes the definitions table at ingest.
- `(revision_id, function_id)` is the only scope in which `function_id` means anything.

### 4.5 Lambda and closure identity

MIR already knows the parent function item, the per-parent ordinal, and the span at `lower_lambda`; today it flattens them into `"<lambda(parent, N)>"` which the engine re-parses. Fix: carry it structurally.

```rust
pub struct DefinitionMeta {           // NEW borsh fields on Function (deliberate wire bump)
    pub definition_key: String,
    pub owner_type_key: Option<String>,
    pub lambda: Option<LambdaIdentity>,
}
pub struct LambdaIdentity {
    pub parent_definition_key: String,  // "function:user.hello.retry"
    pub ordinal: u32,                   // lowering order within the parent body
    pub kind: LambdaKind,               // Lambda | SpawnedClosure | Adapter
}
```

Lambda `definition_key` = `lambda:{parent_key}#{ordinal}` -- stable for unchanged source (bodies relower whole; clean units are reused verbatim). Editing a parent body changes its lambdas' hashes and possibly ordinals -- reported as code-change, which is true. The debug name string is display-only; nothing parses it anymore.

Wire compat: `def_meta` + the capture-props extension change `Function`'s borsh encoding -- bex_cache images key on compiler fingerprint (old images miss harmlessly; bump `FORMAT_VERSION`); the pack envelope bump is the Section 4.1 prerequisite PR.

### 4.6 CCT path identity: runtime-interned, compile-enabled

**Ruling: no compile-time path pre-allocation.** The static call graph cannot enumerate dynamic contexts (closures as values, dispatch, spawn, recursion), and measured CCT cardinality is tiny (corpus p99 = 3,537 nodes) with interning already inside the measured 22 ns/call. What compile time contributes is the property that makes interning string-free: dense `function_id` + `function_count` in the header lets the consumer pre-size flat arrays and intern nodes with a `(parent_node: u32, function_id: u32) -> u32` integer-keyed map. Zero strings, zero Arc -- the requirement verbatim.

Node definitions are append-only rows inside the segment stream itself (Section 6.3 `node_birth`), scoped `(revision_id, process_euid, engine_id, session_epoch)` -- one session directory (Section 6.1); node ids restart with each epoch. Cloud upload re-keys under `(tenant, revision_id, path_dict_id, path_id)` with full paths recoverable by parent-chase; counters referencing an undefined node are an explicit corruption signal.

---

## 5. The CCT engine

The consumer-side replacement for "one disk event per call". Runs inside the existing `bex-prof-consumer` transcode loop (native) and the cooperative drain (wasm), consuming **raw ring records** -- protobuf transcode leaves the always-on path entirely.

Module tree: `crates/bex_events/src/prof/cct/{mod, engine, stacks, nodes, spawn, delta, segment, recorder}.rs`. Everything except `segment.rs` is target-neutral (no fs, no threads).

### 5.1 Structures

Per-engine state (`EngineCct`), threads keyed by logical `thread_id`. A thread's **partition** is the root logical thread of its spawn tree (O(1) at `StartThread` by inheriting the parent's partition); partitions are the unit that binds to a `BoundaryId`.

```rust
struct ThreadState {
    partition: u32,
    stack: Vec<ActiveCall>,          // NEVER capped; live depth is real program state
    last_charge_ticks: u64,
    suspended: Option<Suspend>,
    spawn_ctx_node: u32,
    entry_edge: u32,
}
struct ActiveCall { call_id: u64, node: u32, start_ticks: u64, flags: u8 }
```

Node storage is structure-of-arrays, interned by `(parent_node_id, function_id)` through one FxHash map -- the shape of the measured 22 ns/call prototype:

```
identity:  parent u32[] | function u32[] | flags u8[] | depth u16[]   (immutable after intern)
counters:  enters u64[] | ends_ok/err/cancel/exit u64[] | total_ns u64[] | self_ns u64[] | await_ns u64[]
hist:      16 x u32 duration buckets per node with closes this window (matches Section 6.3 kind 9)
llm:       side table keyed (node, model_id) -> {calls, tokens_in, tokens_out,
           provider_errs, parse_errs}   (LLM nodes only; sysop-enriched, Section 5.4;
           flushes as Section 6.3 kind-10 rows)
delta:     last_flushed (parallel SoA) | dirty_epoch u32[]
```

Node ids are `u32`, dense, **session-epoch-scoped** (Section 6.1 ruling): unique within one session directory; each node belongs to exactly one partition (subtrees are disjoint per partition since parent chains root at per-partition pseudo-nodes), so per-boundary export re-densifies trivially. CSR child indexes are built lazily per snapshot, never on the hot path.

### 5.2 Ordering: causal defer, not timestamp sort

Task migration at await points means one logical thread's records arrive via multiple rings, and a sweep can drain the post-await ring before the pre-await ring. The engine handles this causally:

- `CallFunction` with an unknown `parent_call_id` is deferred (copied, <=54 B) keyed on the missing `(thread_id, call_id)`; the parent's push happened-before the child's (migration is a synchronizing await), so it arrives no later than the next sweep. On arrival, dependents replay. The hot loop (single ring) never defers.
- `EndFunction` fast-paths against stack top; a miss walks the stack (cancel drains close innermost-first); absent entirely => defer.
- **`EndThread` defers while the thread has pending deferrals or open expected records**, finalizing only after one quiescent follow-up sweep; **`StartThread` whose `parent_thread_id` has no `ThreadState` yet defers likewise.** (Review fix: thread lifecycle records reorder across rings exactly like call records.)
- `Suspend`/`Resume` are order-independent by construction (Section 5.3).

**Resync after loss (review fix -- this is the difference between a counter and a wedge):** a deferral surviving `DEFER_MAX_SWEEPS = 1024` sweeps, or a corrupt-range drop by the consumer (which today discards the rest of a drained range), triggers **synthesized recovery**: the missing parent is materialized as the `function_id = 0` unattributable node, dependents replay under it, the partition is flagged degraded, and a loss-marker block is written into the segment stream. Aggregation continues; attribution beyond the loss point coarsens visibly. A two-ring migration fixture plus a corrupt-range fixture pin this in CI.

### 5.3 Time accounting: charge-to-current, and two new raw records

Every thread carries `last_charge_ticks`; on every event for the thread:

```
elapsed = clamp0(event_ticks - last_charge_ticks)
target  = stack.top (or the thread-root pseudo-node)
if suspended { target.await_ns += elapsed } else { target.self_ns += elapsed }
last_charge_ticks = event_ticks
```

`EndFunction` additionally accrues `total_ns += end - start` and bumps the status counter.

**Window closes charge against the per-thread drained-event watermark** (max event timestamp seen for that thread), falling back to consumer-now only for threads idle beyond a threshold. (Review fix: charging consumer wall-clock at window close would permanently misattribute in-flight awaits into immutable delta blocks and make every routine drain latency fire the clamp. A separate `reorder_clamped` counter is kept distinct from `clock_anomalies`.) **Stated accuracy bound:** `self_ns`/`await_ns` are bucket-accurate to within one consumer drain latency at window edges; `enters`/`ends`/`total_ns` land in the window where the event drains.

The raw stream today cannot distinguish "running bytecode" from "parked on await": a sysop window is visible but a future-await inside a bytecode frame emits nothing, and a ready-inline sysop never parks. Two new cold-path raw records instrument the actual engine park points (per-park, not per-call):

```
TAG 0x06 SuspendThread (22 B): reason (SysOp|Await|AwaitAny|EarlyYield), thread_id, suspend_seq, ts
TAG 0x07 ResumeThread (30 B): thread_id, suspend_seq, suspend_ts (carried), ts
```

`Resume` carries the suspend timestamp copied from a local the engine held across its own `.await`, making resume **self-contained**: awaiting duration is computable from the resume alone, immune to cross-ring reordering. Ready-inline sysops emit neither, so their window counts as running -- correctly. Scheduler delay after wake folds into awaiting (declared; revisit with data). Emission sites: adjacent to the existing park/`prof_refresh_vm_ring` seams in `bex_engine`. `DiskEventV1` gains matching variants; `reconstruct_bamlprof` and every reader learn to skip them **in the same PR that adds them** (the `prof_gate.rs` suite asserts empty diagnostics and would otherwise break).

Open EndFunction-less windows (a 60 s LLM sysop) therefore accrue awaiting time into every flush window and appear live in the UI.

### 5.4 LLM enrichment (new)

`sys_llm` completion emits one cold raw record per LLM call:

```
TAG 0x08 LlmCallMeta (~40 B): thread_id, call_id, model_id u32, tokens_in u32, tokens_out u32,
                              flags (provider_error | parse_error | retry), ts
```

`model_id` is interned in a small per-engine side table registered like function metadata and emitted as `model_birth` rows in the session stream (Section 6.3). The consumer resolves `(thread_id, call_id)` -> CCT node and bumps the node's LLM counters; the same record is joined to value captures by call key. Dollar cost is a query-time computation (`lookup("prices.csv", on=model)` -- Section 8), never stored.

### 5.5 Spawn edges: hybrid aggregation

At `StartThread`, resolve the spawning call's node -> `spawn_ctx_node` (defer if pending). At the child's first root call, intern the spawn edge keyed `(spawn_ctx_node, child_entry_function)`; all equivalent spawns share **one** child subtree (10k identical workers cost one subtree). Edge columns: spawn/live/completed/errored/ cancelled counts + total running/awaiting ns.

Instance preservation (the hybrid half): a bounded per-partition table keeps the first 64 instances plus up to 256 exceptional ones (errored/cancelled/latency-triggered/pinned) with name, times, status, and dump reference. Overflow increments `instances_dropped` -- aggregates stay lossless; only per-instance identity rows are bounded, explicitly.

### 5.6 Recursion and depth

Active stacks are never truncated. CCT depth has no fixed cap; unbounded recursion is bounded by an explicit **recursion fold**: past depth 512, before creating a new node for `(p, f)`, scan <=8 nearest ancestors for `f` and reuse on hit (a back-edge, flagged `RECURSION_FOLD`, `folded_frames` counted). Counts and time stay exact; path uniqueness beyond depth 512 coarsens, visibly. Corpus max depth is 14; the scan is cold.

### 5.7 Partition lifecycle (review fix -- the always-on server case)

Partitions are freed at **boundary completion**, not engine close: final charge, final delta, checkpoint contribution, spawn-edge settle, boundary snapshot fold (Section 6.5), then the partition's node rows are marked sealed and its recent-call ring, instance table, and defer buffers are dropped. A server engine's steady-state memory is O(live boundaries + background lane), not O(boundaries ever served). The background lane (partition with no boundary) is bounded by session epoch rotation (Section 6.1). A consumer-RSS gate on a many-boundaries workload enforces this (Section 10, C11).

Unbound partitions past a byte budget spill to `.baml/history/_unbound/` and are surfaced by `baml doctor` -- but with Phase H host wiring in place, unbound is the exception (orphan work), not the CLI norm.

### 5.8 The recent-call ring

Per partition: the last `R = 4096` completed calls plus all open calls, in 56 B slots -- `{thread_idx u32, call_id u64, node u32, parent_call_id u64, start_ns, end_ns, status, dump_ref}` plus a partition-local thread table. (Review fix: `call_id` is per-thread; the slot carries the thread half of the key.) Eviction increments `evicted_calls`; the UI must render "showing last 4096 of N -- older calls: aggregates + flight recorder". This ring is the exact-recency source for the default-mode timeline (Section 9.4).

### 5.9 Flight recorder

A bounded ring of **raw drained bytes** -- one memcpy on the drain path, zero transcode until a trigger fires:

```
chunks: VecDeque<{engine_id, first_ticks, last_ticks, bytes}>; cap 16 MiB native / 4 MiB wasm
```

At ~80 B/call-pair raw, 16 MiB ~ 200k call pairs ~ 11 s of the working-agent trace (and 21 ms of the pathological hot loop -- which is exactly what the CCT is for). Eviction is whole-chunk FIFO with `evicted_upto` queryable -- the retained-window contract is explicit.

**Dump:** transcode retained chunks through the existing `to_disk_event` path into `sessions/<sess>/flight/<ts>-<trigger>.bamlprof` -- exact reuse of `.bamlprof` framing so every existing reader works -- plus a sibling `.bamlcids` pin manifest (values referenced by the dump are GC roots, Section 6.7). The header gains optional `boundary_id`, `trigger_reason`, `trigger_node_id`, `cct_segment_seq/block_seq` so the UI jumps CCT node -> exact-event evidence. `boundary.bamlmeta` records dump references.

### 5.10 Consumer capacity and the shedding ladder (review fix)

Stated capacity model: at the 50 ns/call gate one consumer core sustains ~20 M call-pairs/s; one measured hot loop produces 9.5 M pairs/s, so **two concurrent hot producer threads can exceed capacity**, and today's only outcome is ring growth to 1 GiB then `process::abort()`. For a system that is on by default in production, abort cannot be the only answer:

- `BAML_RING_OVERFLOW_POLICY = abort | shed`. **Dev default: `abort`** (loud, current behavior). **Server/SDK default: `shed`.**
- The shedding ladder under sustained overload, in order: (1) stop flight-recorder memcpys; (2) stop full-trace transcode; (3) defer value canonicalization (bounded queue, then CaptureLoss); (4) **shed structural ranges as a last resort**: drop whole drained ranges, mark every affected thread degraded, close their open frames to the unattributable node via the Section 5.2 resync machinery, and count `shed_ranges` + `shed_events`. CCT aggregation itself is never disabled; the process never aborts in `shed` mode.
- Every shed step emits a marker block and surfaces in `health()`. A saturation benchmark (N hot producers vs ring growth and shed behavior) is a gate, not a watched canary (Section 10, C12).

### 5.11 The per-call budget

Target <=45 ns/call-pair on the integrated bench; CI regression gate <=50; absolute never-exceed 60 (one number set, used by both the engine and the benchmark suite). The per-operation decomposition (decode ~6, thread lookup ~4, intern+bumps ~22 measured, charge ~3, recent-ring ~3, recorder memcpy ~3-8, dirty mark ~2, flush amortized <1) is a **target table, not a measured budget** -- the only measured row is the 22 ns intern prototype. Landing `crates/bex_events/benches/cct_engine.rs` (two-ring migration fixture, 3,537-node shape, depth-14 stacks) and recording measured per-op breakdowns is a **hard precondition for deleting the legacy run-store path** (Section 11).

What disappears from today's ~123 ns/call: protobuf encode + 285 MB/s buffered writes, the per-event heap `String` in `ProfileEventSource::Live`, per-event run-store lock + envelope clone, per-event history envelope clone.

### 5.12 wasm

`CooperativeProfileDrain` embeds the same `CctEngine`; drains stop materializing per-event envelopes. Window close is driven by event timestamps at cooperative drain points; the segment sink is an in-memory byte buffer surfaced as downloadable segments, or pure in-memory snapshots when persistence is off. The playground/VSCode UI reads CCT state through the same `bex_query` engine compiled to wasm (Section 9) -- there is no second wasm data plane. Recorder default 4 MiB.

## 6. On-disk: layout, formats, durability, retention

### 6.1 The `.baml/` layout

```
<project>/.baml/
  .gitignore                       # "*" (existing)
  cache/                           # bytecode cache (existing, untouched)
  dict/                            # per-revision dictionaries (Section 4.2)
    baml_rev_1_<b64url>.bamldict
  sessions/                        # the continuous-session unit (first-class)
    <started_secs>-<proc_euid_hex32>-e<engine_id>/
      session.bamlmeta             # begin / heartbeat / epoch / end records (append-only)
      cct/
        seg-000001.bamlseg         # session CCT delta stream (active tail + sealed)
      flight/
        <ts_ms>-<trigger>.bamlprof # flight-recorder dumps (exact events, bounded)
        <ts_ms>-<trigger>.bamlcids # CID pin manifest for values the dump references
      raw/                         # ONLY when BAML_PROFILE_RAW=1 (absorbs N5: "tcpdump")
        raw-000001.bamlprof        # rotated 64 MiB
  history/                         # per-boundary atomic unit (name and role kept)
    _unbound/                      # orphan-partition spill (Section 5.7), surfaced by baml doctor
    <created_ms>-<target_slug>-baml_id_1_<b64>/
      boundary.bamlmeta            # begin + bound + complete records (absorbs N2)
      cct.bamlcct                  # sealed folded CCT snapshot, written at completion
      manifest.bamlcids            # append-only list of value CIDs this boundary references
      thread-<tid>/
        stack-<K>.bamlprof         # FULL-TRACE MODE ONLY (unchanged framing)
        value-<K>.bamlvalue        # capture-root records (bodies now CID refs, Section 7)
      blobs/                       # LEGACY read-only; new writes never land here
      export/                      # only in exported bundles
  store/                           # project-level content-addressed value store (Section 7.3)
    packs/
      pack-<proc_euid_hex32>-<seq6>.bamlpack
      pack-<proc_euid_hex32>-<seq6>.bamlpack.idx
      pack-<proc_euid_hex32>-<seq6>.lease
    writers.lock                   # shared: open pack writers; exclusive: GC
    gc.lock
  retention.log                    # append-only jsonl tombstones of everything deleted
  uploads.pin                      # (future cloud) ids pinned until acked
  profiles/                        # LEGACY: no longer written; drained by retention
```

**Ruling -- where CCT data lives (the central placement conflict):** CCT delta rows are written **once, into the per-session stream**. A session is one engine's lifetime in one process -- key `(process_euid, engine_id)`, exactly the file-header half of the quad. A long-running server serving ten thousand boundaries has one session whose CCT stream is the continuous truth (this is what makes fleet-level `ctx()` queries and always-on production telemetry possible); a CLI run has a short-lived session containing one boundary. Boundary attribution is structural, not per-row: `node_birth` rows carry `partition_id`, and a `partition_bind` block (written when the host binds a boundary) maps `partition_id -> boundary_local_id -> BoundaryId`. Delta rows stay 4-byte-node-keyed with no boundary column.

At **boundary completion** the consumer folds that boundary's rows and writes a sealed `cct.bamlcct` snapshot into the boundary dir (same container, `node_total` blocks, node ids re-densified; KBs -- corpus p99 ~ 226 KB, p50 ~ trivial). The boundary dir is therefore self-contained for share/export/delete _after completion_; live and crashed boundaries are served by filter+fold over session segments. **Retention rule (review fix):** before deleting session segments, retention materializes the folded snapshot for every begin-without-complete boundary that lacks one (recording `cct_loss` if the fold fails) -- the "crashed boundary is a readable partial run" contract cannot silently expire.

**Session epochs:** sessions rotate (new session dir, fresh node table, carry-over checkpoint written at epoch close) at 256 MiB of CCT bytes or 24 h, bounding the node table and `node_birth` growth of months-long processes. **The session directory is the epoch unit and the node-id scope:** node ids are unique within one session dir (`(process_euid, engine_id, started_secs)`), restart with each epoch, and are meaningless outside it -- every consumer of a node id resolves it against that directory's `node_birth` chain. Queries fold across epochs exactly as across sessions (logical CCT identity is the function-id path, Section 4.6).

### 6.2 The CCT segment container (`.bamlseg` / `.bamlcct`)

**Ruling -- one container, custom framing; Arrow/Parquet are export codecs.** The active role needs per-block checksums, commit markers, torn-tail recovery, and append-only growth -- a WAL's requirements, which Arrow IPC does not provide. The sealed role is **the same file** after a seal-by-append (footer index + trailer; no rewrite, no crash window). Columns are contiguous fixed-width little-endian arrays, 8-byte aligned, so a zero-copy Arrow `ArrayRef` view over the mmap gives Arrow interop (and Parquet export) without the Arrow container.

Header (112 B, fsynced at create): magic `BCCT`, format_version u16, header_len u16, process_euid [16], engine_id u64, session_seg_seq u32, block_align u32 (=64), started_epoch_ns u64, clock kind/quality + tick ratio, **revision_id [32]** (names the dictionary; 32 bytes per the Section 4.2 ruling), reserved, header_crc32c.

Block framing: 32 B header (`DBLK` magic, kind u8, flags u8 [bit0 zstd], row_count u32, payload_len u32, first_ts_ns u64, last_ts_ns u64) + column-major payload (each column 8-byte aligned) + 16 B trailer (crc32c over header+payload, block_seq u32 monotonic, commit_marker u64). A block is committed iff magic, CRC, seq, and marker all validate. Recovery scans from the header, accepts committed blocks, stops at the first failure. **Reads never mutate; `baml doctor --truncate-torn` may explicitly truncate (tombstoned).** (There is no writer-reopen path: a crashed session's segments are only ever read.)

Seal: flush + D1 -> append `footer_index` block (per-block {kind, offset, row_count, first/last ts, node_id min/max} -- time and node pruning without touching payloads) -> 48 B trailer (`BCCTFOOT`, index offset/len, total_rows, CRC, `TSEG` end magic) -> D2. Reader protocol: valid trailer => sealed, mmap + index; else active/torn => block scan. Rotation: 4 MiB or 15 min or engine close.

### 6.3 Block kinds (v1)

Adding a column set = new block kind; readers skip unknown kinds (forward compat).

| kind | name                      | row layout                                                                                                                                                                                 | when                                                                                                                                    |
| ---- | ------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------- |
| 1    | `cct_delta`               | node_id u32, enters u32, ends_ok u32, ends_err u32, ends_cancel u32, ends_exit u32, total_ns u64, self_ns u64, await_ns u64 -- **48 B**                                                    | per 250 ms window, dirty nodes only                                                                                                     |
| 2    | `node_birth`              | node_id u32, parent_node_id u32, function_id u32, logical_thread_id u64, partition_id u32 -- 24 B                                                                                          | once per new node, before first referencing delta                                                                                       |
| 3    | `spawn_edge`              | edge_id u32, parent_node u32, entry_fn u32, child_root_node u32, spawn_delta u32, completed_delta u32, errored_delta u32, cancelled_delta u32, running_ns delta u64, awaiting_ns delta u64 | per window, dirty edges                                                                                                                 |
| 4    | `watermark`               | wall_epoch_ns u64, drained_through_ts_ns u64, events_drained u64, durable_kind u8, reason u8                                                                                               | at each D1 sync **that follows new blocks**; when idle, a heartbeat watermark at >=10 s cadence only (keeps idle cost single-digit B/s) |
| 5    | `partition_bind`          | partition_id u32, boundary_local_id u32, boundary_id [16], created_ms u64                                                                                                                  | at host bind                                                                                                                            |
| 6    | `footer_index`            | --                                                                                                                                                                                         | seal only (kind 7 reserved; the 48 B seal trailer of Section 6.2 is an out-of-band fixed structure, not a framed block)                 |
| 8    | `node_total` (checkpoint) | same 48 B as kind 1, ABSOLUTE values                                                                                                                                                       | see cadence below                                                                                                                       |
| 9    | `cct_hist`                | node_id u32, 16 x u32 duration buckets on a x4 stride (1 us, 4 us, ..., >= ~17.9 min) -- 68 B                                                                                              | per window, nodes with >=1 close                                                                                                        |
| 10   | `llm_delta`               | node_id u32, llm_calls_delta u32, tokens_in_delta u64, tokens_out_delta u64, provider_errs_delta u32, parse_errs_delta u32, model_id u32                                                   | per window, dirty LLM nodes                                                                                                             |
| 11   | `model_birth`             | model_id u32, name (len-prefixed utf8)                                                                                                                                                     | once per interned model                                                                                                                 |
| 12   | `marker`                  | loss / degraded / shed / budget-exhausted / epoch-close diagnostics                                                                                                                        | as needed                                                                                                                               |
| 13   | `instance`                | thread_id u64, edge_id u32, status u8, name_len u16, start/end_ns u64, dump_seq u32, name                                                                                                  | bounded instance rows                                                                                                                   |

Kind 9 exists because tail-latency queries (p95/p99 per context) are unanswerable retroactively from sums -- this was a launch requirement from the query-surface judging, and it is cheap: 68 B x nodes-with-closes per window, and the hot loop touches 3 nodes.

**Checkpoint cadence (review fix):** kind-8 checkpoints are emitted when **delta bytes since the last checkpoint >= checkpoint size** (i.e., amortized <=2x write volume), not on a fixed block count -- a lone open call must not trigger a 166 KB full-table checkpoint every 16 s. Reader fold cost stays bounded: <= one checkpoint + the deltas since it.

**Steady-state byte examples (from the formats above, framing included):** hot loop = 3 dirty, closing nodes => per 250 ms window one delta block (32+3x48+16 = 192 B) + one hist block (32+3x68+16 = 252 B) ~ 1.8 KB/s, ~3.5 KB/s with the <=2x checkpoint amortization -- under C3's 6 KB/s gate, and ~10^5x below the measured 446 MB/s. Lone in-flight LLM call => one delta row + framing per window ~ 400 B/s (no hist row -- an open call has no closes). Idle => heartbeat watermarks only ~ 7 B/s. Working agent (p99 3,537 nodes, all dirty every window -- the worst case) => ~1.9 MB/s ceiling, which is why the flush cadence and real dirty-set behavior are gated by benchmark C3 on the agent workload, not asserted (Section 12 risk 6 holds the mitigation options).

### 6.4 `session.bamlmeta` and `boundary.bamlmeta`

**Ruling -- append-only record streams** (`BMET` framing: len-prefixed, CRC per record), not rewrite-in-place. Crash detection uses the **session** heartbeat (one writer, coarse 10 s cadence, D0) plus pid liveness -- not per-boundary heartbeat rewrites.

- `session.bamlmeta`: `begin {process_euid, engine_id, pid, started, revision_id}` (D2), `heartbeat` (D0), `epoch_close`, `end {reason}` (D2).
- `boundary.bamlmeta`: `begin {boundary_id, target, source: cli|playground|sdk, created_ms, project_id, revision_id, capture_defaults}` (D2, written by the **host**; `project_id` kept from N2 so exported boundary dirs carry project identity with zero project state); `bound {session_dir, first_seg_seq, partition_id, boundary_local_id}` (written by the **consumer** -- the host does not know segment sequences; this record is the host<->consumer handshake the review demanded, transported via `ControlMsg::BindBoundary{boundary_id, root_thread, ack}`); `complete {status, completed_ms, last_seg_seq, counts, diagnostics, dump_refs}` (D2, consumer); optional `trigger`/`loss` records.
- Listing = one meta-file scan per boundary dir: O(#runs), ~200 B/run, no segment reads. `begin` without `complete` + dead session heartbeat => **crashed (partial, readable)**.

### 6.5 The boundary snapshot `cct.bamlcct`

Same BCCT container, always sealed: `node_total` blocks (final folded counters, node ids re-densified 1..N, birth columns embedded), final `cct_hist` totals, `spawn_edge` totals, `llm` totals, one `partition_bind` row, footer + trailer. Written at completion via tmp+rename (D2). Readers prefer it; absence + begin-without-complete => fold from the session segments named by the `bound` record.

### 6.6 Durability ladder

- **D0 buffered** -- `write_all`; survives process crash (page cache), not power loss.
- **D1 synced** -- `sync_data` on the file; survives power loss for content.
- **D2 anchored** -- D1 + parent-dir fsync (create/rename visibility); survives power loss.

| Class                                       | Steady state         | Milestones                                                                                           | Declared loss window                                                                                                           |
| ------------------------------------------- | -------------------- | ---------------------------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------ |
| session/boundary meta                       | heartbeats D0        | begin/bound/complete/end D2                                                                          | none for milestones                                                                                                            |
| CCT active segment                          | D0 per 250 ms window | **D1 group commit every 1 s or 1 MiB** + watermark block; D1 at boundary completion and engine close | **<=1.25 s** of aggregate deltas (power loss: one group-commit interval + one flush window); <= current window (process crash) |
| sealed segments, snapshots, dicts, pack idx | --                   | D2 at seal                                                                                           | none                                                                                                                           |
| value packs                                 | D0 on append         | D1 before any root referencing the chunks commits; D2 at seal                                        | orphan chunks only                                                                                                             |
| `.bamlvalue`, full-trace `.bamlprof`        | D0 cadence           | D1 on flush/close                                                                                    | tail records, reader-tolerated                                                                                                 |
| `manifest.bamlcids`                         | D0                   | grouped with pack D1 (Section 6.7); sealed at completion                                             | none for committed roots                                                                                                       |

**fsync never runs on the drain path** (review fix): a helper thread performs syncs; the durable watermark advances on completion. Consumer stall p99 under the durability matrix is benchmark-gated (C12).

Crash recovery on open (readers and `baml doctor`): meta records -> per-segment trailer probe or block scan -> completeness = last watermark + torn-tail position, reported as "aggregates complete through T; tail lost <= delta". Recovery gates are **per durability class**: process-crash recovery measures to the last committed block (D0 survives SIGKILL); the power-loss model measures to the last watermark block. "Killed before begin => no boundary dir" is a pass category in crashfuzz.

### 6.7 Value store: packs, index, GC

Pack format (`.bamlpack`): 48 B header (`BPK1`, origin_euid, pack_seq, created) + repeated chunk records `{rec_magic u16, kind u8, storage u8 (raw|zstd), cid [32], logical_len u32, stored_len u32, payload, crc32c}`. Append-only; one active pack per writing process (no cross-process file contention); sealed at 64 MiB or process exit. Physical packing is independent of logical CIDs -- repack/compress/compact never changes identity.

Index (`.bamlpack.idx`, at seal, tmp+rename): `BPKI` + 256-way fanout + sorted `{cid, offset, logical_len, stored_len}` entries + CRC binding to its pack. Readers mmap all idx files (fanout + binary search, newest-first); the active pack is indexed in its owner's memory; the index is always rebuildable by scanning packs.

**Root commit ordering (review fix -- roots must never dangle at any age):**

1. Append chunks to the pack.
2. Pack D1 (group-committed on the value drain thread).
3. Append the CID(s) to `manifest.bamlcids` **and** the capture-root record to `.bamlvalue`, in that order, with the manifest append inside the same group-commit barrier as the pack sync.
4. GC additionally derives roots from `.bamlvalue` capture records for any boundary whose manifest is unsealed -- a persisted root is a root even if the manifest tail was lost.

**GC protocol (review fix -- the delete->dedupe->sweep race):** v1 uses coarse exclusive locking, which is simple and correct:

- Every open `PackWriter` holds a **shared** flock on `store/writers.lock`.
- `baml gc` / `baml clean` takes it **exclusive** -- GC never runs concurrently with writers. If writers are live, GC skips the store with a notice (retention tiers that don't touch the CAS still run). Active packs are additionally protected by their `.lease` heartbeat. `store/gc.lock` serializes concurrent GC invocations against each other (held exclusive for the duration of a pass).
- Mark = union of `manifest.bamlcids` in live boundary dirs + `flight/*.bamlcids` pins + `uploads.pin` closure + derived roots from unsealed boundaries. Sweep = unmarked idx entries older than the 24 h grace; packs with live chunks are compacted (rewrite live records to a fresh pack, delete old pack whole), fully-dead packs unlinked. Every deletion tombstoned in `retention.log`.
- The adversarial interleaving (delete boundary -> new writer dedupes against a now-unreferenced old chunk -> GC) is a **required test**; under the exclusive lock it reduces to "GC waits", and the crashfuzz suite asserts _no readable root ever references a sweepable CID_.
- An epoch/lease protocol that lets GC run concurrently with writers is future work, explicitly out of v1 (Section 12).

Delete = `rm -rf` the boundary dir (chunks become unreferenced; next GC reclaims). Export = copy dir + write `export/pack-*.bamlpack(.idx)` containing the closure of `manifest.bamlcids` + the referenced dictionaries -- the exported dir opens anywhere with zero project state (reader search path: `[boundary/export/, project store/]`).

### 6.8 Retention

Defaults (overridable in `baml.toml [observability]` and env):

| Root                 | Age               | Size                                    | Floor                                                                                    |
| -------------------- | ----------------- | --------------------------------------- | ---------------------------------------------------------------------------------------- |
| `history/`           | 30 d              | 2 GiB                                   | newest 20 boundaries                                                                     |
| `sessions/`          | 7 d               | 1 GiB (raw/ <=512 MiB per session)      | sessions referenced by kept boundaries; snapshot-materialize before delete (Section 6.1) |
| `store/`             | reachability (GC) | soft 4 GiB -> boundary eviction then GC | closure of kept boundaries                                                               |
| `dict/`              | while referenced  | --                                      | --                                                                                       |
| `profiles/` (legacy) | 7 d               | --                                      | --                                                                                       |

Degradation order when a budget binds: raw full-trace segments -> flight dumps -> per-boundary full-trace segments (boundary stays readable via snapshot + values) -> whole oldest boundaries (releasing their CAS closure) -> sealed session CCT segments last (aggregates are smallest and are the always-on contract). Value packs are never deleted directly -- only via reachability. Every deletion is tombstoned; readers surface "removed by retention on <date>" instead of silent absence. `baml clean [--dry-run|--all]` is the CLI entry; a cheap budget check runs at session start (consumer thread).

### 6.9 Versioning and fixtures

| File                   | Magic                           | Ext                           |
| ---------------------- | ------------------------------- | ----------------------------- |
| CCT segment / snapshot | `BCCT` / `BCCTFOOT`             | `.bamlseg` / `.bamlcct`       |
| Revision dictionary    | protobuf `RevisionDictionaryV1` | `.bamldict`                   |
| Value pack / index     | `BPK1` / `BPKI`                 | `.bamlpack` / `.bamlpack.idx` |
| Meta streams           | `BMET`                          | `.bamlmeta`                   |
| CID manifests          | `BCID`                          | `.bamlcids`                   |
| Exact-event index      | `BIX1`                          | `.bamlidx` (Section 9.5)      |
| Event/value streams    | protobuf headers (existing)     | `.bamlprof` / `.bamlvalue`    |

Additive change = new block kind / proto field / record variant, skipped by old readers. Breaking change = version bump; readers support N and N-1; a reader hitting N+1 fails explicitly. Value identity versions are separate knobs (`node_codec_version` inside the CID domain; physical pack version never affects CIDs). Golden fixtures (`crates/bex_events/testdata/golden/v1/`): byte-exact examples of every file class + canonical-value corpus with asserted CIDs + torn-tail fixtures truncated at every interesting offset. `v1/` is frozen forever; codec changes mint `v2/`.

### 6.10 Migration

Readers kept forever: `.bamlprof`, `.bamlvalue`, per-boundary `blobs/` CAS, v1 boundary-dir opener. Nothing is bulk-converted; old dirs age out via retention; optional `baml history migrate <boundary>` converts blobs->packs for users who want old runs in the new export/GC world. Value-id semantics untouched.

Rollout flag: `BAML_OBS_LAYOUT = v1 | dual | v2` -- Release A defaults `dual` (v1 boundary segments keep writing; v2 sessions/CCT/meta/CAS written alongside; readers prefer v2; kill switch = `v1`), Release B defaults `v2` (per-boundary stack segments become opt-in full trace), Release C deletes v1 writers (readers remain, fixtures stay in CI). This flag composes with the pipeline flag in Section 11.

---

## 7. Values: policy, staging, and the content-addressed store

### 7.1 Capture policy at compile time

`FunctionCaptureProps` gains a fourth option and the compiled-in defaults become:

| Function class                       | inputs   | output   | error    | promote_on_error |
| ------------------------------------ | -------- | -------- | -------- | ---------------- |
| LLM                                  | Auto     | Auto     | Auto     | Auto             |
| UserDefined / Companion / AutoDerive | Disabled | Disabled | Auto     | Auto             |
| Builtin / Internal                   | Disabled | Disabled | Disabled | Disabled         |

Root capture stays host policy (`BoundaryContext.capture_defaults`), not per-function data. The dictionary exposes `capture_flags` per function (inputs/output/error/promote 2 bits each + `is_llm` + `captures_any`), so tooling knows "this function captures values" without scanning artifacts, upload planners build manifests per revision, and the CCT UI badges captured nodes. `capture_policy_version` in the dictionary header names the default table that produced the flags.

### 7.2 Trigger-promoted staging (the "short-lived value buffer")

The retroactive story -- "when a helper errors, I want the args it actually received" -- requires values that were captured speculatively and made durable only on a trigger:

- Functions whose resolved mask includes `promote_on_error` capture into the TraceHeap as today, but the drafts land in a **staging ring** (bounded bytes, default 32 MiB native / 8 MiB wasm) tagged speculative, instead of the durable drain queue.
- Staged drafts are released at frame close (normal completion) under LRU/byte pressure -- cheap, no serialization, no hashing, no I/O (the reserve-before-copy invariant holds; a failed reservation still does zero work).
- When a trigger fires (Section 3.1), staged drafts belonging to the failing subtree (matched by `TraceCallKey` prefix) are **promoted**: moved to the durable drain queue, canonicalized, and written with `role: promoted` + the trigger id. Everything else about them is a normal capture.
- The cost model is honest: staging pays the deep copy on every staged call. The default therefore stages only `error: Auto` origin captures (the error value, already captured at throw) plus `promote_on_error` functions' inputs; staging inputs of _every_ user function is a per-function or per-boundary opt-in, not the default. The flight recorder remains the universal retroactive evidence (exact events need no copies).
- `CaptureLoss` records cover staging-ring evictions that a later trigger would have wanted: the trigger's promotion report includes `staged_evicted: N` so "we would have had it but the buffer was too small" is visible and tunable.

### 7.3 Continuous drain (absorbs N7)

Values drain to disk **while the boundary is open**: a high-water-mark wake (producer notifies at ~1/2 budget) plus a coarse interval, uniform across hosts; the final drain at resolution is the same call. The pending budget becomes a flow-control window, not a per-run total cap -- the 17th capture no longer dies at `TraceCaptureConfig::enabled(16)`. Value-id allocation order is preserved (same shared per-boundary allocator, invoked earlier); `RunStarted` is written at the first drain. Mid-run crash loss shrinks from "all values" to "the un-drained tail".

**Threading (review fix):** the value plane does **not** run on the prof consumer. A per-process value drain service (owned by `bex_events`, invoked from the host-side drain path that exists today) performs canonicalization, hashing, pack appends, and the group commit. Its CPU is measured separately (C10) and a combined CCT+value consumer-stall row (C12) guards interference. wasm keeps inline/BlobRef bodies with the existing 64 KiB threshold in v1 -- the CAS is native-only until a storage adapter exists (Section 12).

### 7.4 The content-addressed value DAG

- CID = BLAKE3-256 over `(node_codec_version || canonical_node_bytes)`, domain-prefixed.
- Canonical encoding: versioned, deterministic (map-key ordering, float normalization, absent-vs-null rules) -- golden-fixture-pinned (C9).
- **Schema identity in the encoding (ruling -- must be right before the C9 fixture freeze):** canonical node bytes are schema-erased structural encodings; class and enum type identity is carried as the `definition_key` string (and enum variants by name), so **renames change CIDs** -- consistent with Section 4.4's "renames break the join by design", and a rename shows up as content change in drift review, which is true. Fields encode by declared presence: absent, null, and default-filled are three distinct encodings; unknown fields (captured under a newer schema) encode positionally by name so cross-revision equality means structural equality, not schema equality.
- Inline threshold 2 KiB per DAG node; strings/bytes chunk at fixed 128 KiB; lists/maps leaf at fixed 128 entries; `.bamlvalue` whole-body inline threshold drops 64 KiB -> 4 KiB so transcript dedupe actually engages.
- `.bamlvalue` capture bodies become `DagRef { root_cid, node_codec_version, logical_len }` (new record variant, additive; old readers skip). Capture-root metadata (call key, role, timestamps, status) is unchanged; **value ids never renumber**.
- `readValue(boundaryId, valueRef)` remains the only hydration path; hydration is byte-budgeted with child-CID handles for selective descent (the query surface's `get()` contract, Section 8.4).
- Fixed chunking first; Prolly trees / content-defined chunking only if measurements justify them (the same 80/20 ruling as the research doc). Cycles are prohibited in captured values (semantic trees); `CyclicReference` omissions already exist.

Expected effect on the measured corpus: transcript-append workloads go from N(N+1)/2 to ~N bytes + bounded structural overhead (C5 gate: growth exponent <=1.2, >=20x reduction at N=64 with a 64 KiB prompt); repeated system prompts and media store once per project.

### 7.5 Privacy, consent, audit

- Timing/profile upload and value upload are separate consent decisions (cloud phase), and locally: `BAML_HISTORY=0` disables durable capture wholesale; per-function attributes and boundary defaults narrow it.
- Redaction is represented in the capture pipeline (redaction descriptors on capture props), applied before hashing -- redacted values get CIDs of the redacted content.
- New audit records in `.bamlvalue` (absorbing the deferred `CapturePolicyChanged` work): `CapturePolicyChanged`, `PromotionOccurred {trigger, scope, records, staged_evicted}`, feeding the `audit()` query source (Section 8). The eng-lead privacy-audit P0 story reads these.
- Content-addressed storage is project-scoped locally and tenant-scoped in the cloud; digest existence must never leak across tenants.

---

## 8. The query surface: BQL

Three candidate surfaces were designed competitively (SQL over DataFusion virtual tables; a purpose-built pipeline DSL; a BAML-native stdlib API) and scored by three judges under agent-user, human-dev, and implementer lenses. The pipeline DSL won the human (8.5) and implementer (8) lenses; the agent lens preferred SQL (8.5) but conceded that the DSL's trust machinery, made mandatory, resolves SQL's worst agent hazards (silent-empty results, opt-in completeness). The decisive implementer argument is structural: `bridge_wasm` ships under an enforced gzip size gate (absolute ceiling 4.5 MiB in `.cargo/size-gate.toml`, plus a 3% delta guard over the committed 4.4 MiB baseline), and embedding DataFusion (~2-4 MB compressed alone) is disqualifying, while the pipeline engine is small hand-rolled Rust over deps already in the lockfile.

**Decision: one query engine (`bex_query`), one primary surface (BQL -- "BAML Query Language"), everything else a wrapper or an export.** The web app, CLI, LSP, playground, MCP tool, and future cloud API all call the same engine; sealed segments export to Parquet so DuckDB/DataFusion work _outside_ the product as the unbounded-expressiveness escape hatch.

### 8.1 Shape

A BQL query is a pipeline of typed stages: `source | transform | ... | sink`.

```
runs(last=24h, status=errored) | calls(fn="extract_*") | values(role=input) | top(10, by=total_bytes)
```

CLI: `baml q '<query>' [--format table|json|ndjson] [--explain] [--schema] [--cursor C] [--snapshot W] [-f file.bql --param k=v]`. The same engine is a library used by the web app, LSP, playground, and MCP server.

### 8.2 Typed set kinds

Every stage has a declared signature checked at plan time. Nine kinds:

| Kind      | Keyed by                                                                                                                                                         | Backing store                                                     |
| --------- | ---------------------------------------------------------------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------- |
| RunSet    | run_id (ULID BoundaryId)                                                                                                                                         | run index (bamlmeta scan)                                         |
| CtxSet    | logically (revision_id, canonical function-id path); session-epoch-local node ids are the physical row keys, resolved at fold time via `node_birth` parent-chase | CCT delta segments                                                |
| CallSet   | (run_id, thread_id, call_id) -- exact instances                                                                                                                  | recent-call ring, value join keys, flight dumps, full traces ONLY |
| ValueSet  | capture_id -> {call key, role, cid, bytes, status}; lazy                                                                                                         | capture roots + value DAG                                         |
| EventSet  | ordered exact events with declared window bounds                                                                                                                 | flight dumps / full traces                                        |
| SpawnSet  | (parent_cct_node, child_entry_fn)                                                                                                                                | spawn-edge aggregates                                             |
| SeriesSet | CtxSet x time bucket, each bucket carrying `complete`                                                                                                            | delta blocks + watermarks                                         |
| DiffSet   | aligned pairs (cross-revision via `align=fqn`)                                                                                                                   | dictionary alignment                                              |
| Table     | terminal: ordered, bounded rows + mandatory meta footer                                                                                                          | --                                                                |

Two implicit coercions only: RunSet->CtxSet (implicit run-scoped `ctx()`) and X->Table at pipeline end. The CtxSet/CallSet split encodes the capture contracts in the type system: aggregates are always available; exact instances exist only where an exact source covers the scope, and `instances(source=...)` is the honest gate -- it raises `E_NO_EXACT_SOURCE` naming remedies (arm flight recorder / `@capture` / bounded trace) instead of returning zero rows.

### 8.3 Stage catalog (abridged)

- Sources: `runs() run(id) ctx() dumps() trace(session) health() storage() audit() revs() triggers()`
- Filter/select: `calls(fn=, path=, kind=llm) errors() failure() where(expr) limit() sort() select()`
- Tree: `rollup(by=fn|path|file|package) callers(fn) callees(depth) spawns() tree()`
- Time: `series(bucket=, metrics=[...]) delta(vs=prev|range|rev)`
- Compare: `diff(P1,P2,align=fqn) compare(metrics, match_io) vdiff(role, max_nodes)` (Merkle short-circuit)
- Values: `values(role=[...]) get(max_bytes=64kb, depth, as=Type) instances(source=)`
- Events: `events(around=call|trigger, before=N, after=N, threads=)`
- Sinks: `top(k,by) stats(aggs, by) hist(metric) lookup(file, on=) table() tree() flame() export(format=jsonl|parquet) live(interval) explain() completeness()`

Path patterns compile inside string literals by type-directed coercion (`path="main>>handle_request>extract_*"`; `>` child, `>>` descendant, `*` glob). Human literals throughout: `24h`, `64kb`, `20%`, `yesterday`, `03:00..03:10`. Percentile functions (`p50/p95/p99`) fold the `cct_hist` blocks -- which is why kind 9 is a v1 storage commitment: without it these degrade to mean-with-warning for all captured history.

### 8.4 The trust contract (mandatory, not opt-in)

1. **Completeness footer on every result**: `meta = {complete, watermarks, capture_loss[], sources_consulted, truncated, next_cursor, warnings[], snapshot}` computed from the blocks the query actually touched. An agent can never assert "no errors" from a blind window without noticing.
2. **Bounded by construction**: implicit default limits (1000 rows); byte-budgeted value hydration (`get()` defaults 64 KiB) returning child CIDs for selective Merkle descent; keyset cursors over ULID run ids (compared as decoded 16-byte payloads, Section 2.5.7 -- the base64url string form does not sort). No query can OOM a laptop, server, or wasm host. `E_BUDGET` returns a resume cursor plus the child CIDs to descend into -- a budget failure is a navigation hint.
3. **Fail closed**: typed machine-readable errors with caret spans and ready-to-paste corrected queries: `E_NO_EXACT_SOURCE`, `E_REVISION_MISMATCH` (ctx ids across revisions without `align=fqn`), `E_MISSING_BLOCK` (never decode a partial value as whole), `E_STAGE_INPUT`, `E_UNKNOWN_FIELD` (lists valid fields for the kind).
4. **Empty is explained**: an empty result is success, with meta distinguishing idle vs capture loss vs watermark lag.
5. **Snapshot pinning**: every envelope echoes a snapshot watermark; `--snapshot W` re-runs any query bit-identically against the pinned segment set. Every pasted query in a bug report is a reproduction; every agent citation is re-verifiable.
6. **Self-description**: `baml q --schema` returns one JSON document -- stages, fields, units, enum values, example literals, and a drill-down query template for every ID-typed column -- so an agent bootstraps in one call. The same static stage registry generates the BQL language service (completion, hover with units) for the playground query box and VSCode.

### 8.5 Representative queries (P0 stories)

```
# Exact prompt + raw model output for a wrong answer
runs(latest) | calls(fn="ExtractResume", kind=llm) | values(role=[input, raw_output, output]) | get(max_bytes=256kb)

# Runaway hot loop, live
runs(current) | calls() | top(5, by=calls) | tree() | live(interval=2s)

# Failed run: one bounded evidence bundle
run("run_0147") | failure()

# Exact events before the failure
run("run_0147") | dumps(trigger=error) | events(around=trigger, before=200, after=20, threads=all)

# Incident window: hot error contexts, busy vs stuck
ctx(range=2026-07-30T03:00..03:10) | top(20, by=errors) | select(path, calls, errors, self_ns, awaiting_ns, wait_share)

# Deploy correlation across a revision boundary
ctx(rev=["v418","v419"], last=48h, align=fqn) | calls(path="main>>handle_request>extract_invoice")
  | series(bucket=15m, metrics=[calls, errors, p95(total_ns), mean_awaiting_ns])

# Agent: verify my fix
diff(runs(rev="ab12", last=7d), runs(rev="cd34", last=7d), align=fqn)
  | calls(fn="user.hello.retry") | compare(metrics=[calls, errors, self_ns, awaiting_ns], match_io=true)

# Duplicate prompts -- CID as a query primitive
runs(latest) | calls(kind=llm) | values(role=input) | stats(n=count(), by=cid) | where(n > 1)

# LLM spend by root function and model (usd computed via lookup, never stored)
ctx(last=30d) | calls(kind=llm) | lookup("prices.csv", on=model)
  | stats(usd=sum(tokens_in*in_price + tokens_out*out_price), by=[root_fn, model])

# Is the 03:00-04:00 data trustworthy?
health(range=03:00..04:00, process=all) | select(process, complete_through, capture_loss, shedding, backlog_age, termination)

# Privacy audit
audit(last=90d) | stats(records=count(), bytes=sum(bytes), by=[fn, role, trigger, consent_scope, redaction])
```

### 8.6 Grafts adopted from the losing designs

Launch requirements ([launch]) and fast-follows:

- [launch] `cct_hist` histogram blocks in storage (from SQL's `dur_hist` column) -- done in Section 6.3.
- [launch] BQL->ClickHouse compilation of the aggregate subset, prototyped **before** the cloud query API freezes, with a golden corpus in CI diffing local vs ClickHouse results.
- Typed hydration `get(as=MyType)` / `baml obs hydrate <cid> --type T` -- decode against the capturing revision's schema, schema-erased JSON fallback (from baml-native).
- `failure()` evidence-bundle source: error payload + promoted args + sibling ok/error counts + flight-recorder tail, one bounded result.
- Parameterized query files (`-f triage.bql --param run=...`) and user lookup tables.
- `critical_path()` -- typed EventSet-in, so `E_NO_EXACT_SOURCE` gates it honestly onto flight-dump/full-trace scope (the research is explicit that CCT + spawn edges alone cannot reconstruct the exact concurrent critical path).
- The layering endgame: once `bex_query` is proven, wrap it as a thin `baml.obs` stdlib module (one engine, two surfaces) -- deferred, not v1.
- The playground's 60 fps viewport path is a recognized-query fast path inside the same engine (`live()`/`flame()` sinks streaming transferable typed arrays) -- an acceptance-gated implementation detail, not a second API.

### 8.7 Agent access

Agents consume BQL through an MCP tool with identical semantics to the CLI (`query`, `schema`, `hydrate`, multi-statement scripts with named result sets in one round trip). The mandatory meta footer + fail-closed errors are what make autonomous use safe: the agent always knows whether its evidence is complete, gets a corrected query on a mistake, and cites `(query, snapshot)` pairs as re-verifiable evidence.

---

## 9. The local web app

Still thinking through if it is two UIs or not. Probably not but wanted thoughts.

### 9.1 Product shape: one server, two UIs

- **`baml studio [PATH]`** -- new CLI subcommand; resolves the project like the playground does and starts the same `baml_lsp_server` axum server with the runs list as the landing page. Studio opens any directory containing `.baml/` even with no compilable sources -- it is a trace viewer first.
- **`baml playground`** -- unchanged UX; gains a Runs tab; its `ExecutionProfileView` is replaced by the same components studio uses.
- **VSCode / browser** -- no native server; `bex_query` runs as wasm (`ObserveEngine`) inside the existing webview worker.

Routes: `/studio`, `/playground`, `/api/ws` (existing JSON control WS, slimmed), `/api/obs` (new binary observability WS), `/api/obs/blob/{boundary}/{cid}` (Range-capable body streaming). Loopback-only binding and origin checks inherited. The server is stateless for observability -- disk is the database -- so multiple studio instances on one project are safe.

### 9.2 `bex_query`: sans-io engine, three hosts

New crate `crates/bex_query` (deps: `bex_events`, `memmap2` behind `native`; no tokio in core; `wasm` feature for no-threads/no-mmap). **No async trait -- the engine is sans-io**: queries either complete from resident bytes or return the exact byte ranges they need (`Poll::NeedData { ranges }`); the host fetches and retries. This is the only model that works identically for native mmap, wasm linear memory (extension-host `readRange` bridge), and HTTP Range requests, without an async runtime in wasm.

```rust
pub trait SegmentSource: Send + Sync {
    fn committed_len(&self, file: FileId) -> u64;   // watermark for tails
    fn view(&self, range: &ByteRange) -> Option<&[u8]>;
    fn generation(&self, file: FileId) -> u64;      // bumped on tail growth / meta transitions
}
```

Sources: `MmapSource` (native; never maps beyond committed length), `CacheSource` (wasm + HTTP Range; byte-range LRU), `LiveMirrorSource` (same-process: reads the consumer's in-RAM active delta blocks -- identical block format in RAM and on disk, so the query code cannot tell; this is what makes same-process live views ~0-latency instead of waiting on group commit).

API surface: `list_runs`, `open_run`, `run_dictionary`, `timeline`, `spawn_edges`, `left_heavy`, `sandwich`, `top_functions`, `search`, `value_refs`, `read_value`, `diff`, plus the BQL entry point `query(bql: &str) -> Poll<Frame>` -- the named methods are the recognized-query fast paths the UI uses; both go through the same planner. Every request carries the bounded-size contract (`pixel_width <= 8192`, `lanes <= 256`, `max_bytes` default 4 MiB hard-capped 16 MiB).

**Caches are byte-budgeted, not entry-capped** (review fix): decoded caches (pyramid slices, CCT folds, dictionaries) share one byte budget -- native 256 MiB, wasm 32 MiB -- with per-entry size accounting, asserted by a peak-RSS gate on the 10 GiB corpus (C7).

### 9.3 Wire: BQF1 columnar frames

Custom fixed little-endian frames, not Arrow IPC (schema machinery buys nothing for ~10 known frame kinds) and not JSON (which produced the measured 2.21 GB). 40 B header (magic, kind, flags [lod_degraded | partial_tail | more_lanes], request_id, data epoch, ncols, nrows) + column directory + 8-aligned column payloads (strings as offsets+bytes pairs) + CRC trailer. Decodes in ~150 lines of TS into zero-copy `TypedArray` views; wasm returns the buffer via transfer.

`/api/obs` protocol: client sends small JSON (`query` / `sub` / `setViewport` / `unsub`); server sends BQF1 binary. Per subscription: at most one frame in flight (latest-state snapshot on ack/timer), <=30 Hz, every frame <= `max_bytes` enforced by LOD climb with `lod_degraded` set. Steady-state bandwidth <= `max_bytes x 30/s` **independent of event rate** -- replaying the 4,096-event artifact costs one ~50 KB frame instead of 2.21 GB, and a hot-loop run costs the same viewport frame as an idle run. This wire is benchmark-gated (C13) so the bound is proven, not asserted.

**Ruling -- one live plane:** the run store stops projecting profile events entirely (`PROFILE_EVENTS_CAP`, `recompute_record_profile`, and profile-event ingestion are deleted at the end of the migration window; `run_to_wire` stops serializing `calls[]`). The CctEngine's snapshots feed `bex_query` (via `LiveMirrorSource`); there is no CctPatch-through-run-store path. The run store keeps only low-rate run state (status, result/error refs, diagnostics, fetch payloads) on the existing `/api/ws`.

### 9.4 The default-mode timeline (review fix -- specified, not assumed)

The flagship timeline must be honest about what the default capture mode contains. Three data tiers, all labeled in the UI:

1. **Exact-recency tier** -- the recent-call ring (last 4096 completed + all open calls per partition) renders as real rects for the trailing window; open calls render live with growing extents. "Showing last 4096 -- older calls: aggregates" appears at the tier boundary (`evicted_calls` drives the label).
2. **Aggregate tier** -- beyond the ring, per-thread swimlanes render **activity bands** folded from CCT window deltas (per (thread x 250 ms bucket): busy/awaiting fractions, dominant function color, error ticks) -- not fake rects. Zooming below window resolution in this tier shows the explicit "aggregate resolution limit" notice.
3. **Exact-evidence overlays** -- flight-recorder dumps and full-trace segments (where they exist) render as fully zoomable exact regions, visually bracketed, with the trigger marker linking back to the CCT node that fired it.

Spawn connectors draw from spawn-edge aggregates (band tier) and exact `StartThread` events (exact tiers); never grafted depth. Left Heavy / Sandwich / top-functions views are pure CCT folds and work identically in every mode. Sub-rect interactions (wheel zoom at cursor, drag pan, click-to-zoom, minimap, breadcrumbs, per-lane binary-search hit testing, 1-device-pixel minimum width) are as specified in the webapp design -- the 1.5% data-space floor is structurally impossible because percentages of run duration never appear.

### 9.5 The exact-event index (`.bamlidx`), right-sized

Sidecar indexes exist **only for exact-event artifacts** (flight dumps and full-trace segments -- both bounded), never for CCT segments (delta blocks are already time-bucketed and footer-indexed). Review fixes applied to the BIX1 spec:

- Per-lane base bucket count scales with events: `buckets = min(64 Ki, 4 x events_in_lane)` -- a 70-event lane costs ~280 buckets, not 64 Ki. Dense span-driven sizing is gone.
- Buckets carry **byte offsets** into the segment (per-slab offset column), so sub-bucket raw extraction seeks directly -- O(pixels + bucket slab), never O(segment events).
- A per-segment index byte cap (default: index <= 25% of segment bytes) with level shedding is part of the format, and index bytes are benchmark-gated (C11).
- Index building runs on a background low-priority task, never the drain thread; rebuild-on-open is the normal fallback under load, not an error path.
- In-segment checkpoints (kind 8) are the **single authority** for folded counters; the sidecar carries only pyramid/lane/spawn data. (Removes the duplicate-authority conflict after doctor truncation.)

### 9.6 Views

- **Runs list** -- bamlmeta scan (O(#runs), ~200 B each); crashed = begin-without-complete + dead session heartbeat; filters engine-side; revision picker fed from meta `revision_id`.
- **Run detail** -- timeline (Section 9.4) + tabbed CCT flame / Left Heavy / Sandwich + value inspector; selecting a timeline region scopes the CCT tabs.
- **Left Heavy** -- preorder SoA emission of nodes with extent >= 1/(2\*pixel_width), one synthetic "smaller" node per truncated parent (visible aggregation).
- **Sandwich** -- callers above / callees below a selected FunctionKey; launched from the top-functions table.
- **Value inspector** -- capture list with availability (`pending|available|missing|omitted| lost|promoted`); skeleton hydration with per-level budgets and child-CID drill; blob bodies stream over `/api/obs/blob` with Range (image/PDF preview).
- **Diff** -- two runs or two revisions over aligned dictionaries (`definition_key` join, `def_content_hash` change badges); differential flame (red/green by delta) + functions table by |delta|.
- **Query box** -- a BQL input with the language service (completion/hover from the stage registry); every view's underlying query is copyable ("view as BQL"), which is the UI-to-bug-report bridge.
- **Live tail** -- subscription with `follow: true`; joining mid-run, reconnecting, or attaching a second tab costs exactly one frame.

### 9.7 Delivery stages

1. **UI first, no engine:** `pkg-observe-ui` canvas components (FlameCanvas, LeftHeavy) over a `LegacySnapshotClient` adapting today's in-memory run snapshot. Fixes the 1.5% bug, DOM-per-block, and grafted threads immediately. Old view removable one release later.
2. **Engine native:** `bex_query` + `boundary.bamlmeta` + `/api/obs` + `WsObserveClient`; runs list, run detail, Left Heavy, live tail in the playground's Runs tab. The JSON profile wire dies here.
3. **Studio:** `baml studio` entry, Sandwich, value inspector, search, BQL query box.
4. **wasm + diff:** `ObserveEngine` for VSCode, HTTP-Range `CacheSource` for static hosting (open a shared boundary dir from a URL), diff view.

---

## 10. Benchmarks and acceptance criteria

The suite proves the new pipeline better and prevents regression forever. Every number is a **gate** (pass/fail, committed threshold) or a **tracked trend**; rows are tagged `measured | extrapolated | inspected`, and only `measured` rows can gate. No cross-machine absolute comparisons: gates are paired same-job ratios or absolute-vs-committed per-platform baselines (size-gate discipline, which the repo already runs).

### 10.1 Claims

| ID  | Claim                                             | Gate (summary)                                                                                                                                                                                                                                                                                                                                                                                                                                                     | Cadence                          |
| --- | ------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | -------------------------------- |
| C1  | Producer hot path unchanged (~10 ns/call)         | ring-push <=15 ns/pair median; end-to-end slope delta <=12 ns/call trivial callee, <=3% wall realistic callee                                                                                                                                                                                                                                                                                                                                                      | PR ratio, nightly abs            |
| C2  | Consumer CPU >=2x better                          | paired ratio <=0.50x legacy; nightly absolute <=50 ns/call (target 45, never-exceed 60 -- one number set, Section 5.11); self-reported consumer CPU cross-checked against differential method within 25%                                                                                                                                                                                                                                                           | PR 2M-call ratio, nightly 15M    |
| C3  | Bytes by workload shape                           | hot loop <=6 KB/s (vs 446 MB/s; Section 6.3 computes ~3.5 KB/s); agent paired ratio >=10x; idle <=44 B/s (achievable via idle watermark suppression, Section 6.3 kind 4 -- computed ~7 B/s); **formulas include block framing and checkpoint volume**                                                                                                                                                                                                              | nightly; PR hot-loop ratio >=50x |
| C4  | Sublinearity                                      | bytes/s flat +/-25% across a 100x call-rate sweep; bytes/path stable for P in {64..4096}; flat over duration                                                                                                                                                                                                                                                                                                                                                       | PR smoke, nightly full           |
| C5  | Transcript values quadratic -> linear             | growth exponent <=1.2; >=20x reduction at N=64/64 KiB; incremental bytes <= new-message + 8 KiB                                                                                                                                                                                                                                                                                                                                                                    | PR exponent smoke, nightly       |
| C6  | Ingest/open path replaces the quadratic run store | **candidate = `bex_query` open + fold + live frame** (the run-store path is deleted): open 4,096-event artifact <=250 ms; wire <=10 MB; linear scaling to 100k events <=2 s, RSS <=256 MB                                                                                                                                                                                                                                                                          | PR + nightly                     |
| C7  | Query latency O(pixels) on multi-GB history       | cold open <=500 ms; warm top-functions <=100 ms; viewport <=200 KB always, p95 <=100 ms native / 250 ms wasm; viewport bytes invariant +/-10% between 1M-call and 36M-call runs; **peak RSS <= decoded-cache budget + fixed slack** (native and wasm budgets separately, Section 9.2); **corpus includes a CCT-only artifact and a deep-zoom-on-hot-loop row**                                                                                                     | nightly, release                 |
| C8  | Crash recovery                                    | kill -9 fuzz x1000: every boundary opens or is a legitimate "killed-before-begin" pass; 0 corrupted sealed segments; torn tails truncate to last commit; missing CIDs reported, never silently decoded; **recovered-gap gates per durability class**: process-crash -> last committed block; power-loss model -> last watermark, gate <=2 s = the declared 1.25 s window (Section 6.6) + 0.75 s measurement slack; **no readable root references a sweepable CID** | PR subset, nightly 1000-iter     |
| C9  | CID/canonical stability                           | golden fixtures byte-exact across macOS arm64 + Linux x86_64; round-trip proptests; v1 fixtures frozen forever                                                                                                                                                                                                                                                                                                                                                     | every PR                         |
| C10 | Value-plane CPU                                   | throughput >=300 MB/s/core; **latency gate is a per-size curve: <= max(2 ms, size/250 MB/s)** (the flat 5 ms gate was arithmetically impossible at 8 MiB); transcript hash-CPU curve is a permanent tracked row (the quadratic-rescan trigger for incremental hashing)                                                                                                                                                                                             | PR throughput, nightly curve     |
| C11 | Index plane + partition lifecycle                 | `.bamlidx` bytes <=25% of segment; seal/build CPU per shape; **consumer RSS flat on a 10k-boundary server workload** (partition seal-and-drop proof)                                                                                                                                                                                                                                                                                                               | nightly                          |
| C12 | Consumer stall + saturation                       | consumer stall p99 under the durability matrix (fsync off-thread proof); N-hot-producer saturation: shed ladder engages, CCT never disabled, no abort in `shed` mode, ring high-water bounded                                                                                                                                                                                                                                                                      | nightly                          |
| C13 | Live wire bound                                   | per-subscription bytes/s <= max_bytes x rate cap, independent of event rate; measured on hot-loop live tail                                                                                                                                                                                                                                                                                                                                                        | nightly                          |

### 10.2 Workloads

Committed under `crates/tools_obs_bench/workloads/` as plain `.baml` files: `hotloop/` (the measured reproduction at 500k/5M/15M/36M -- the 36M variant reproduces the original 1.69 GB artifact's call count for the Section 10.5 acceptance row; anchor: 226,088,292 bytes / 45.2 B/call at 5M on legacy), `hotloop/bench_rate` (fixed-wall, 100x rate sweep via a work knob), `paths/gen_paths` (deterministic generator: P distinct contexts at constant total calls, P in {4..4096} bracketing the corpus p99 (= observed max) of 3,537), `agent/agent_like` (86 logical threads, depth 14, sysops + awaits + caught errors -- the multithreaded-ring workload every prior benchmark lacked, and the crashfuzz target), `transcript/transcript_append` (64 KiB and 1 MiB prompts, N in {16..128}), `idle/idle_agent`, `deep/recursion_depth` (two variants: depth 200 asserts **no** fold below the 512 threshold; depth 1024 asserts `RECURSION_FOLD` flags and `folded_frames` counts -- the fold engages only past 512, Section 5.6), plus a synthetic 10 GiB sealed-segment corpus generator (seeded, CI-cached) **in both full-trace and CCT-only variants**.

The research corpus stays private; a numbers-only manifest (`corpus/sheep-council-2026-07.toml`) is committed so analyzer regressions are checkable against recorded distributions. Every run embeds a `MachineManifest` (OS/arch/CPU/disk kind/governor/clock/rustc/git/env/runner class). macOS arm64 + Linux x86_64 are both first-class CI legs.

### 10.3 Harness

`crates/tools_obs_bench` (binary `obs-bench`), cloning the `tools_size_gate` architecture: per-platform TOML baselines, JSON row output, `check`/`refresh-baseline` subcommands, a reusable workflow with per-platform jobs + one enforce aggregate, and a baseline-refresh workflow. Subcommands: `run, check, refresh-baseline, calibrate, prof-stats, value-stats, replay, crashfuzz, validate, corpus {scan|synth}, gen-paths, report`.

The four uncommitted investigation examples are **recreated as supported tooling** (prof_stats, value_stats, replay as subcommands; the CCT update microbench as a committed `bex_events` bench with 1/16/1024/4096-function and depth-14 variants). Criterion is rejected: the workspace convention is harness-less benches, and the suite needs child rusage, consumer-thread CPU attribution, and bytes-on-disk metrics criterion doesn't provide. All benches emit one NDJSON row schema consumed by `obs-bench report`.

Key instrumentation (the single most important addition): `BAML_OBS_STATS=<path>` makes the prof consumer and the value drain service self-report CPU (`RUSAGE_THREAD` / `thread_info`), events, bytes, blocks, flushes, fsyncs, ring growth, and shed counters at exit -- converting C2/C10/C12 from inference to measurement.

**The pipeline flag** (review fix -- load-bearing for every paired baseline): `BAML_PROFILE_PIPELINE = legacy | dual | cct`, forked at **`ConsumerState::transcode`** (the consumer fan-out -- not engine init). `legacy` runs today's three sinks; `dual` runs both (paired A/B in one job, and the correctness oracle below); `cct` is the new pipeline. The legacy sinks stay compilable behind this flag until the paired-baseline release closes (Section 11); deletions land last.

**Correctness oracle:** `prof_gate.rs` (the ~2,900-line G3 lossless contract suite) keeps running against the raw sink with `BAML_PROFILE_RAW=1` as the raw-path oracle, and gains a **CCT-equivalence gate**: in `dual` mode, replay the same programs and assert CCT counters == raw-derived counters per function (counts, status splits) exactly. Suspend/Resume/LlmMeta record variants are added to `reconstruct_bamlprof`'s skip set in the same PR that introduces them.

### 10.4 Honest-measurement rules (the +109% postmortem, codified)

Retained from the original benchmark: paired on/off, same binary/machine/session, user+sys not just wall, best-of-N, release builds, slope fitting. Required additions, built in: multithreaded rings (agent workload + 8-producer microbench); dual reporting of trivial-callee slope (upper bound) and realistic-callee overhead % (the quotable number); sustained >=60 s runs so writeback engages; a `--durability=power-loss` matrix leg; a no-spare-core leg (pinned cpuset) so consumer CPU can't hide in wall time; direct consumer-thread CPU with differential cross-check (>25% disagreement flags the row); durability-class labels on every bytes/latency row. `obs-bench report` emits the claim ledger; release claims must cite ledger rows by bench id.

### 10.5 Acceptance criteria (concretized Section 17)

| Criterion                                                                        | Number                                                                                                                                                                                     | Basis                         |
| -------------------------------------------------------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ----------------------------- |
| Producer-thread ns/call                                                          | <=15 median (measured ~10 today)                                                                                                                                                           | C1                            |
| Consumer CPU per M calls                                                         | <=50 ms integrated; target 45; never-exceed 60                                                                                                                                             | C2                            |
| Aggregate bytes per continuously-dirty node per minute                           | <=32 KB (48 B delta + 68 B hist per 250 ms window + framing share + <=2x checkpoint; typical nodes are dirty in few windows and land far below)                                            | C3/C4                         |
| Hot-loop run (36.2 M calls, 3.8 s; `bench_36m`)                                  | boundary snapshot `cct.bamlcct` <=10 KB; attributable session CCT bytes <= C3 rate x duration ~ 23 KB (vs 1.69 GB measured)                                                                | C3                            |
| Time-to-query top functions                                                      | <=100 ms warm, <=500 ms cold on 10 GiB                                                                                                                                                     | C7                            |
| Viewport response                                                                | <=200 KB; p95 <=100 ms native / 250 ms wasm                                                                                                                                                | C7                            |
| Completeness-detection latency                                                   | <= flush cadence + 5 s                                                                                                                                                                     | C8                            |
| Loss window by mode                                                              | aggregate <=1.25 s power-loss (Section 6.6) / <= window process-crash; values: explicit CaptureLoss only; full trace: explicit bound                                                       | C8                            |
| Max normal local disk; disk-exhaustion behavior; upload backlog; value retention | **TBD product** (suite reports the inputs; qualitative gates already binding)                                                                                                              | --                            |
| Qualitative (binding now)                                                        | no silent truncation; every file versioned; crash tests per claimed durability class; full trace never default; UI never materializes all calls; awaiting != running; spawn != stack depth | inspected rows -> named tests |

---

## 11. Implementation plan -- the merged phase ledger

One ledger across all five subsystems (review fix: the designs' individual plans had circular dependencies; deletions now land last). Phases are independently valuable and sequential except where marked parallel.

| Phase                                                  | Contents                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                                          | Depends on                        |
| ------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- | --------------------------------- |
| **P0 -- Truth & tooling**                              | Commit `obs-bench` skeleton + prof-stats/value-stats/replay; committed CCT microbench; `ConsumerStats` self-reporting; golden-fixture scaffolding; `BAML_PROFILE_PIPELINE` flag forked at the consumer fan-out (all modes = legacy behavior initially); fix the 1.5% width floor (UI Stage 1 can start in parallel)                                                                                                                                                                                                               | --                                |
| **P1 -- Compile-time identity**                        | `identity.rs` finalizer + emit/link/pack-load call sites (pack-envelope version-prefix prerequisite PR first); revision/source hashing (salsa); `RevisionDictionaryV1` + `.bamldict` writer; `DefinitionMeta`/lambda identity (borsh bump + cache FORMAT_VERSION bump); capture_flags; delete ProgramId; populate header fields 10/11; engine walk becomes verify-only                                                                                                                                                            | P0                                |
| **P2 -- CCT engine (RAM)**                             | `prof/cct/` modules; causal defer + thread-lifecycle deferral + resync; charge-to-current with per-thread watermarks; Suspend/Resume (0x06/0x07) + LlmCallMeta (0x08) records + reader skip-tolerance (same PR); spawn edges + instances; recursion fold; recent-call ring (56 B slots); integrated bench lands here -- **measured <=50 ns/call is the exit gate**, with the recorder-ring memcpy included as an equal-cost stub (the real recorder lands in P6; the ledger re-affirms the gate under C2 when it does, before P9) | P1 (dense ids)                    |
| **P3 -- Session storage**                              | BCCT segment writer/reader (all 13 block kinds incl. `cct_hist`/`llm_delta`); sessions/ layout + session.bamlmeta; checkpoint-by-bytes cadence; watermark + off-thread fsync; seal + footer; `boundary.bamlmeta` (host begin + consumer bound/complete via `ControlMsg::BindBoundary`); boundary snapshot fold; `dual` mode writes v2 alongside v1 (`BAML_OBS_LAYOUT=dual`)                                                                                                                                                       | P2                                |
| **P4 -- Query engine + web app core**                  | `bex_query` (sans-io, MmapSource + LiveMirrorSource); BQF1; `/api/obs`; runs list; Left Heavy; default-mode timeline (three tiers); live tail; playground Runs tab; C6/C7 gates                                                                                                                                                                                                                                                                                                                                                   | P3                                |
| **P5 -- Values: CAS + staging + continuous drain**     | canonical encoder + CID + golden fixtures (C9); packs/index/writers.lock; drain service threading; continuous drain (N7); staging ring + trigger promotion; audit records; `manifest.bamlcids` + root-commit ordering; GC under exclusive lock; retention (`baml clean`)                                                                                                                                                                                                                                                          | P3 (layout), parallel with P4     |
| **P6 -- Flight recorder + triggers + full trace**      | recorder ring + dump path + `.bamlcids` pins; OnError/OnLatency/Manual triggers wired to promotion (P5) and dumps; full-trace mode with `TraceBudgetExhausted`; exact-event `.bamlidx` (right-sized) + timeline exact tiers                                                                                                                                                                                                                                                                                                       | P3; overlays land after P4        |
| **P H -- Host wiring (the "on by default" milestone)** | CLI `run`/`test`, cffi SDK, pack_host: mint ULID BoundaryId, `BindBoundary`, begin/complete meta, CaptureDefaults, completion barrier before exit (`drain -> complete -> flush_and_join -> exit`); `BAML_HISTORY` + baml.toml knobs; shed-mode default for SDK hosts; privacy change documented                                                                                                                                                                                                                                   | P3 minimum; full value with P5/P6 |
| **P7 -- BQL surface**                                  | parser/planner/stage catalog over `bex_query`; CLI `baml q`; `--schema`; completeness footers everywhere; snapshot pinning; MCP tool; studio query box + language service; `failure()`, diff/compare, vdiff                                                                                                                                                                                                                                                                                                                       | P4, P5                            |
| **P8 -- Studio + wasm + diff**                         | `baml studio`; value inspector; Sandwich; search; `ObserveEngine` wasm feature; HTTP-Range source; diff view; ClickHouse compilation prototype + golden corpus ([launch] before any cloud API freeze)                                                                                                                                                                                                                                                                                                                             | P4-P7                             |
| **P9 -- Deletions**                                    | Legacy run-store profile projection, `PROFILE_EVENTS_CAP`, `recompute_record_profile`, router per-event duty, JSON profile wire, v1 layout writers -- **only after**: paired baselines recorded for one release cycle, CCT-equivalence oracle green, C2/C3/C6/C7 gates green                                                                                                                                                                                                                                                      | everything                        |

Prior-plan absorption: N1 (boundary dir atomic unit) [x] kept; N2 (bamlmeta) -> P3; N3 (boundary*id in headers) -> P3; N4 (SetFunctionId routed) -> P2 (consumed by CCT; carried in full-trace/dumps); N5 (raw demotion) -> P3 (`BAML_PROFILE_RAW`); N6 (self-attaching router) -> P3 `BindBoundary` + partition binding (the router's job is subsumed); N7 (continuous drain) -> P5. The TASK/2 id rulings ride P1/P3 as mechanical PRs: quad collapse to one struct/proto/string; SpanId + Collector shim deletion; `bamlv_1*...`public ValueRef encoding; **exact-call identity serializes as`baml*call_1*...`on every public surface** (BQL CallSet/EventSet rows, MCP results, blob routes, playground wire -- fulfilling the TASK/2 requirement that the playground wire and the query surface agree on one public call identity); and PayloadId's instability is **mooted by the Section 9.3 wire slimming** -- the JSON profile-patch protocol that exposed it is deleted, and the surviving fetch-log payloads on`/api/ws`derive their ids stably from`(call quad, kind, seq)`.

---

## 12. Risks and open questions

**Risks accepted with mitigations:**

1. **The integrated per-call budget is a target, not a measurement.** Only the 22 ns intern is measured; the P2 exit gate (integrated bench <=50 ns/call) is the mitigation, and deleting the legacy path is fenced behind it.
2. **Two hot producers can exceed one consumer core.** Mitigated by the shed ladder + `shed` default for servers (Section 5.10) and gated by C12 -- but the multi-consumer scaling question (sharding drains by engine) is deliberately deferred until a real workload demands it.
3. **Value staging pays deep-copy costs speculatively.** Defaults stage only error-origin captures + `promote_on_error` inputs; the flight recorder carries the general retroactive load. Cost visibility via C10 rows.
4. **GC requires writer quiescence in v1.** Correct but coarse; long-running servers defer CAS reclamation until a maintenance window. The epoch/lease concurrent-GC protocol is scheduled work, not a v1 blocker.
5. **wasm value CAS is out of v1** -- wasm keeps inline/BlobRef with the 64 KiB threshold until a browser storage adapter exists.
6. **Agent-shape dirty-set volume.** A p99 CCT (3,537 nodes) with every node dirty every window produces a ~1.9 MB/s ceiling (delta + hist rows + framing) -- fine locally, meaningful for future cloud upload. Mitigations if C3 shows a problem: adaptive window widening under high dirty counts, or per-node flush hysteresis. Decide on measurement, not now.
7. **Sampled profiling is rejected for v1** -- a deliberate ruling, not a silent drop of the research's fallback recommendation: the shed ladder plus the CCT-never-disabled invariant covers overload, and the P2 exit gate (<=50 ns/call integrated) covers cost. If that gate fails, sampling is the named fallback to revisit.

**Open questions (owner: initiative lead unless noted):**

1. Error-class counters in CCT (parse vs provider vs user-thrown) -- v1 carries `parse_errs`/`provider_errs` only for LLM nodes (0x08 flags); a general error-class column family is deferred until the values join proves insufficient.
2. Scheduler-delay attribution (wake->resume folded into awaiting) -- declared; revisit with data from the agent workload.
3. Retention defaults (30 d / 2 GiB / 4 GiB) -- product sign-off needed; the mechanism is settled.
4. Disk-exhaustion terminal behavior -- explicit error state is binding; the exact degrade-vs-stop policy is TBD product.
5. Cloud upload protocol (manifest negotiation, watermarks, ClickHouse schema) -- shaped by Sections 6/7 formats but specified in a follow-up document; the [launch] ClickHouse compilation prototype (P8) is its forcing function.
6. `baml.obs` stdlib wrapper over `bex_query` -- endgame, post-P8.
7. Whether `SetFunctionId`-style `$id` overrides should surface in CCT node identity (currently: recent-call annotation + full-trace/dump fidelity only). Note the deliberate contract change relative to TASK/2 N4: N4 made `SetFunctionId` durable because it was the only carrier of `$id` semantics; in this design, default aggregate capture keeps per-call `$id` overrides durable **only** in flight dumps and full-trace segments (stated in the Section 3 contract table). If durable `$id` history matters beyond that scope, it must become a value-plane record -- decide before P6.

---

## 13. Appendix A: Reconciliation register

Rulings on every cross-design conflict the adversarial review surfaced:

| #   | Conflict                                                                                               | Ruling                                                                                                                                                                                                                             |
| --- | ------------------------------------------------------------------------------------------------------ | ---------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| R1  | CCT placement: per-boundary vs per-session vs both                                                     | **Per-session stream** + folded per-boundary snapshot at completion + retention-time snapshot materialization for crashed boundaries (Section 6.1)                                                                                 |
| R2  | Two segment byte formats (BAMLCCT1/72 B/48 B rows vs BCCT/96 B/56 B rows)                              | **One BCCT container** (Section 6.2) with the 48 B delta row (no `running_ns` column -- running is derivable; self/await are the stored split) + new hist/llm kinds                                                                |
| R3  | Revision id 16 vs 32 bytes; three dictionary specs                                                     | **32-byte BLAKE3-256**; one dictionary: `.baml/dict/baml_rev_1_....bamldict` protobuf (Section 4.2); per-boundary function tables rejected                                                                                         |
| R4  | Node-id scoping: per-boundary vs session-wide                                                          | **Session-epoch-scoped ids** (one session dir), partition-disjoint sets; `partition_bind` maps to boundaries; snapshots re-densify; logical cross-session identity is the function-id path (Section 6.1, Section 5.1, Section 8.2) |
| R5  | Three durability ladders / cadences                                                                    | **D0/D1/D2 naming; 250 ms windows; 1 s / 1 MiB group commit; off-thread fsync**; per-class crash gates (Section 6.6, C8)                                                                                                           |
| R6  | bamlmeta append-only vs rewrite-per-heartbeat                                                          | **Append-only BMET records**; crash detection via session heartbeat + pid; consumer writes the seg-range fields via `BindBoundary` handshake (Section 6.4)                                                                         |
| R7  | Two UI live planes (CctPatch->run-store vs bex_query/BQF1)                                             | **bex_query/BQF1 only**; run store exits the profile business; LiveMirrorSource serves same-process live (Section 9.3)                                                                                                             |
| R8  | Flight-recorder placement + missing GC pins                                                            | **Session `flight/` dir + `.bamlcids` pins** (mechanism from the CCT design, placement + pins from storage) (Section 5.9, Section 6.7)                                                                                             |
| R9  | Full-trace default on (playground) vs off                                                              | **Off everywhere**; playground gets one-click arm; dual-mode transition only (Section 3.2)                                                                                                                                         |
| R10 | Timeline designed against full-trace data the default won't produce                                    | **Three-tier default timeline** (recent ring / aggregate bands / exact overlays); `.bamlidx` only for exact-event artifacts, right-sized with offsets, off-thread, byte-capped (Section 9.4, Section 9.5)                          |
| R11 | Recent-call slots missing thread half of the call key                                                  | 56 B slots with partition-local thread index (Section 5.8)                                                                                                                                                                         |
| R12 | Window-close wall-clock charging; missing EndThread/StartThread defer; defer-timeout wedge             | Per-thread drained watermarks; lifecycle deferral; synthesized-parent resync + degraded markers (Section 5.2, Section 5.3)                                                                                                         |
| R13 | `def_content_hash` churns on pool-index constants                                                      | HashProjection canonicalizes object refs to definition_keys; "unrelated edit => unchanged hashes" golden test (Section 4.4)                                                                                                        |
| R14 | CAS GC dedupe-vs-sweep race; two-file root ordering                                                    | Exclusive `writers.lock` GC; manifest-before-root inside the pack group commit; `.bamlvalue`-derived roots for unsealed manifests (Section 6.7)                                                                                    |
| R15 | Benchmarks need the legacy pipeline the CCT design deletes; C6 target vanishes under the webapp design | `BAML_PROFILE_PIPELINE` forked at the consumer fan-out; deletions in P9 only; C6 re-targeted to `bex_query` (Section 10.3, Section 11)                                                                                             |
| R16 | prof_gate.rs orphaned by raw demotion; Suspend/Resume breaks reconstruction asserts                    | Raw-oracle mode + CCT-equivalence gate; reader skip-tolerance in the same PR (Section 10.3, P2)                                                                                                                                    |
| R17 | "On by default" has no host owner (CLI mints no boundary)                                              | **Phase H** host wiring; unbound spill demoted to orphan-only (Section 3.2, Section 11)                                                                                                                                            |
| R18 | Pack-load finalizer targets a seam that doesn't exist; envelope has no version field                   | Finalize at borsh-load site; version-prefix prerequisite PR; no legacy decoder (libsui pairing) (Section 4.1)                                                                                                                      |
| R19 | C10 gates arithmetically incompatible; value threading unassigned                                      | Per-size latency curve; dedicated value drain service, never the prof consumer (Section 7.3, C10)                                                                                                                                  |
| R20 | Checkpoint write amplification; consumer saturation; byte-claims omitting framing                      | Checkpoint-by-bytes cadence; shed ladder + C12; framing-inclusive formulas in C3 (Section 6.3, Section 5.10)                                                                                                                       |

## 14. Appendix B: Relationship to prior documents

- `1-impl/tracing/*`, `1-impl/data-ingestion/*` -- the landed implementation this design builds on. Its invariants ledger (never renumber value ids; quad scoping; torn-tail tolerance; producer-never-blocks; `StartThread`-first wire order; `function_id: 0` sentinel) remains binding except where explicitly amended here (function-id assignment moves to compile time; sentinel rows move to the reserved low range).
- `2-not-impl/history-canonicalization.md`, `reference-history-profiling-and-value-artifact.md` -- absorbed: N1-N7 are mapped into phases in Section 11; the id rulings ride P1/P3; the "six do-nots" all hold in this design (the one deliberate change: `function_id` becomes compile-time-assigned and per-revision-stable -- still never cross-revision without the dictionary join).
- `3-not-impl/research/tracing.md` -- the measurements and the CCT/columnar/flight-recorder direction; this document turns its recommendations into decided formats and adds what it left open (session unit, exact block schemas, host wiring, query surface).
- `3-not-impl/research/value-compression.md` -- the value-DAG direction; Section 7 adopts its 80/20 architecture with v1 policies and the review-fixed GC/root-commit protocols.
