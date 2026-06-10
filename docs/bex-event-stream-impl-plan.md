> **Status (2026-06-10):** Implemented in full — PR1–PR5 all landed on
> branch `antonio/bex-prof-ring` (see PR #3733, which also merges and
> reconciles the M0/M1 identity work from PR #3730 into one id universe).
> The §2.6 interim function-id provider is already replaced by the M0
> `ProgramMetadata` table; `$id` (M1) is live, VM-sourced, per-call.
> Measured numbers + post-review addenda are in the commit messages of
> that branch and in `bex-event-stream-design-v2.md` §0.1 (clock
> 8.5 ns/read; consumer drain 7.5M ev/s/core; call-pair overhead ~63 ns
> on the pure-call microbench, ~0–4.4% on realistic workloads;
> profiling ships default-off). Known deltas from this plan, all
> documented in commits: the loom cfg is `baml_loom` (a global
> `--cfg loom` breaks third-party crates), `StartThread` is 36 B incl.
> tag (the field list wins over the size column), miri builds stub the
> clock (minstant's `#[ctor]` runs `_rdtsc`), and `RingHandle::push` is
> `unsafe` (the not-after-TLS-teardown half of its contract is not
> type-enforceable).

# BEX Tracing — Ring / Producer / Consumer Implementation Plan (M2–M4)

**Status:** Implementation-ready. Grounded in `baml_language/` at commit `78f913109` (branch `canary`). All file:line references below are as of that commit — anchor by **symbol name** first, line number second (lines drift; symbols don't).

**Authoritative design:** `/media/tony/WesternDigitalNvmeSsd/Code/bex-event-stream-design-v2.md` (the v2 spec). This plan implements its **M2 (ring) + M3 (producer) + M4 (consumer/artifact)** milestones, with the seven concurrency decisions locked in review (§1 below) baked into the spec text. The gap review (`bex-event-stream-design-GAPS.md`) is historical context for v1; v2 + this plan supersede it.

**Out of scope here:** M0 `ids.rs`/`CallRef` encode-decode (Paulo), M1 `$id` language surface + `SetFunctionId`, M5 renderer/speedscope/diff, cloud wire, payload capture, markers. This plan defines the *seams* to those so nothing blocks on them.

---

## 0. TL;DR for the implementing session

You are building three things inside a new `prof` module of `crates/bex_events/`:

1. **A segmented SPSC ring** (per `(engine, os-thread)`): lock-free push (`memcpy` + one Release store per event), lossless-by-growth (linked segments, never drops, never blocks), free-list recycling, append-only global registry with orphan/pool lifecycle. Verified by **loom + miri before anything else is built on it**.
2. **A producer hot path** in `BexVm`/`bex_engine`: mint `call_id` per call, write `CallFunction`/`EndFunction`/`StartThread`/`EndThread` raw records into the VM-held ring pointer. Target cost ~25 ns per call pair, dominated by two clock reads.
3. **A background consumer** (`std::thread`, never touches GC/heap permits): drains rings, transcodes raw records → protobuf `DiskEventV1` (prost), appends per-engine `.bamlprof` files. Parked-flag + `park_timeout` wake.

Work in PR-sized phases (§5). PR2 (ring + loom) is the keystone — do not start PR4 (VM integration) until PR2's loom suite is green.

Build/test like CI: `rustup run 1.93.0 cargo test --all-features` (toolchain is pinned to 1.93.0 in `rust-toolchain.toml`; nightly-default builds hide workspace asserts).

---

## 1. Locked decision ledger (from design review, 2026-06-10)

These resolve ambiguities in v2 §6 and are **not open for re-litigation** during implementation:

| # | Decision |
|---|---|
| D1 | **Drain hand-off:** when the consumer observes `next != null`, it **re-loads `commit_len` and drains the remainder before recycling**. Sound because the producer's link store is sequenced after its last commit store to that segment, so the Acquire on `next` makes the final `commit_len` visible; the producer never writes a segment after linking past it. |
| D2 | **Recycle reset:** the **producer owns the reset**. After popping from the free-list (or `alloc`), it stores `commit_len = 0` and `next = null` (Relaxed), *then* links with `next.store(seg, Release)` — the Release publishes the resets. Safety chain: consumer's free-list push (Release) → producer's pop (Acquire) guarantees the consumer's last read of the segment happens-before the reset. One init path for recycled and fresh segments. |
| D3 | **Cache layout:** `Ring` has three cache-line-aligned groups — producer `{head, head_pos}`, consumer `{tail, tail_pos}`, shared `{free-list head, flags}`. In `Segment`, `{commit_len, next}` live on their own line, separate from the buffer pointer. Use `crossbeam_utils::CachePadded` (new dep) or a local `#[repr(align(128))]` wrapper. |
| D4 | **Wake protocol:** consumer sets an atomic **parked flag**, then `park_timeout(≈50ms)`. Producer checks the flag **only on segment-fill** (~1/9,300 events) and calls `Thread::unpark` if set. Never a mutex, never blocks while holding the heap permit. The Dekker-style lost-wakeup race is *documented as benign*: the timer bounds it to one interval of extra ring growth — that is **why the timer exists**. |
| D5a | **Ring handle:** the **VM holds the ring pointer** (the VM is engine-bound by construction). The engine refreshes it at the existing per-step write site in `run_thread_event_loop` (see §3.2) — a TLS `(engine → ring)` lookup happens once per resume, **never per push**. Lazy init lives at the resume site. |
| D5b | **Thread death:** OS-thread death (TLS-map destructor) sets an **orphaned flag** on each of that thread's rings (Release). The consumer drains an orphaned ring to empty, then marks it **pooled**. New threads **claim a pooled ring by CAS on its state** before allocating fresh. The registry is **append-only forever** — no lock-free removal anywhere; memory bounded by peak concurrent threads. |
| D6 | **Capacity framing:** 100M events/s is the **burst** producer-write budget. The doc/knobs must carry: sustainable rate ≈ `N_consumers × per-core transcode rate` (measure in PR3; ballpark 10–20M ev/s/core), burst tolerance ≈ `cap / (produce − drain)` seconds, **hitting `BAML_RING_MAX_OVERFLOW_BYTES` is a hard process error, stated plainly**. Consumer sharding is a tuning knob with a concrete trigger (a bench showing one consumer saturated), not MVP scope. |
| D7 | **Free-list shrink:** consumer-side cap — when retiring a drained segment, if the ring's free-list already holds `BAML_RING_FREELIST_CAP` (default 2–4) segments, `free()` it instead of pushing. Approximate count via Relaxed `AtomicUsize`; off-by-one harmless. |

Minor consequences also locked: the master-switch read rides the same VM-snapshot mechanism as the ring pointer (refreshed per resume, no per-event global load); the registry is an **append-only lock-free linked list** (push-only Treiber — no pop ⇒ no ABA); shutdown **joins/stops VMs before the final drain** so the last commits are visible (thread join is a full sync).

---

## 2. Codebase map — what exists today (verified at `78f913109`)

### 2.1 The VM hot path (`crates/bex_vm/src/vm.rs`)

| Thing | Where | Notes |
|---|---|---|
| `BexVm` struct | `vm.rs:622-720` | Has `traced_frames: Vec<usize>` (:689) and `current_span_context: Option<bex_events::SpanContext>` (:694) — the legacy `@trace` machinery. **`bex_vm` already depends on `bex_events`** — no new crate edge needed. |
| Constructors | `BexVm::new` :1078-1140 (field inits ~:1134-1135); `test_vm` :277-298 | New fields must be initialized in **both**. |
| `OpCode::Call` arm | :4852-4918 | Resolves callee, delegates to `execute_call_from_locals_offset`. |
| `OpCode::CallIndirect` arm | :4921-5027 | Host closures exit via sysop at ~:4960; bytecode callees delegate to the same shared fn. |
| `execute_call_from_locals_offset` | :2892-3259 | The shared call path. `let is_traced = callee.trace;` at **:3045** (legacy, leave intact). Bytecode frame push at **:3204-3210**. Legacy `SpanNotify(FunctionEnter)` yield at :3216-3227. **The new `CallFunction` push goes right after the frame push.** |
| `OpCode::Return` arm | :5028-5070 | Frame pop at **:5046**. Legacy `SpanNotify(FunctionExit)` at :5059-5065. **The new `EndFunction{Ok}` push goes here.** |
| Unwind frame pops | **:2637** (Native frame), **:2730** (Bytecode frame) | Both clean up `traced_frames`/`interrupt_frame` right after (:2638-2650, :2738-2751). **Bytecode pop at :2730 must emit `EndFunction{Error}`; Native pop at :2637 emits nothing** (natives don't emit `CallFunction` in PR4a). |
| CPS continuation pop | :3959 | Pops a `Frame::Native` — no event in PR4a. |
| `Frame` / `BytecodeFrame` | fields: `function, instruction_ptr, locals_offset, type_args, faulting_pc` | **Add `call_id: u64` here** (PR4). Frames live in a `Vec`, so +8 bytes is a Vec-element cost, not an `Object` cost. |
| `EarlyYieldCheck` | `bex_vm_types/src/lib.rs:66-140` | Counter decrement; atomic touched only every `1<<25` instr. Confirms there is **no per-call atomic to piggyback on** — our per-call cost is genuinely additive. |
| `Object` size assert | `bex_vm_types/src/types.rs:1727-1730` | **`size_of::<Object>() <= 64`** (the design docs said 80 — stale). We add nothing to `Object`; `function_id` goes on `Function` (boxed, :377-475, `trace: bool` at :474) so the assert is untouched. |
| `FunctionKind` | `types.rs:274-302` | `Bytecode \| SysOp(SysOp) \| NativeUnresolved \| Native(*const ())`. |

### 2.2 The engine (`crates/bex_engine/src/lib.rs`)

| Thing | Where | Notes |
|---|---|---|
| `run_thread_event_loop` | :2499-3316 | The VM driver. Signature takes `mut thread: ActiveHeapPermit<BexThread>`, `call_id: CallId` (the **engine root-call id** — see naming note §2.5), `span_state`, `cancel`. |
| **Per-step write site** | **:2510-2512** | `thread.vm.current_span_context = ...` runs at the top of the loop, before each `exec()` (:2514). **This is the D5a refresh site**: ring pointer + master-switch snapshot + `thread_id` get written here. Sound because `exec()` never crosses an `.await`, and tokio migrates tasks only at `.await` — so one refresh per exec covers OS-thread migration. |
| `VmExecState` dispatch | :2578-3314 | Arms: `Complete` :2579-2684, `SysOp` :2686-2868 (permit release/reacquire :2758-2765), `Spawn` :2870-2961, `Await` :2963-3066 (release :3013, reacquire :3027), `AwaitAny` :3076-3141, `Event` :3143-3220, `Notify` :3222-3224 (no-op), `SpanNotify` :3226-3310 (legacy), `EarlyYield` :3311-3313. |
| GC machinery | `collect_garbage` :1262-1347 (`request_park` :1265), `gc_safepoint` :1971-1992 (`checking_gc` CAS :1975-1978), `maybe_collect_garbage` :1999-2013 | The producer **must never block** while holding `ActiveHeapPermit` — this is why D4 forbids a mutex on the wake path and why the consumer is heap-permit-free. |
| Root thread creation | :1493-1495 (`BexThread::new_root`, permit, acquire) | **`StartThread` (root) emission site.** Root `RuntimeEvent` currently emitted at :1581-1591. |
| Child spawn | `Spawn` arm :2870-2961; `spawn_thread_inner` :2319-2456; child VM built :2362-2375; `tokio::spawn` :2451 / `wasm_bindgen_futures::spawn_local` :2453; child runs with `local_span_state = None` (:2427) | **`StartThread` (child) emission site** — this is where `parent_thread_id` + `parent_call_id` (the spawn edge) are in hand. Note: spawned bodies emit no legacy spans today; the *new* stream emits for them unconditionally (this was GAPS #5 — v2 resolves it as "always emit"). |
| Thread completion | `Complete` arm :2579-2684 (child settles future :2585-2586) | **`EndThread` emission site.** |
| Legacy emit | `emit()` :1069-1074 → `bex_events::event_store::emit` + optional `event_sink` (:499) | Leave intact. The new stream is **parallel plumbing**; ripping out Collector/`@trace` is explicitly *not* part of M2–M4 (avoids GAPS #13/#14 sequencing). |
| Engine identity | **none** — no `engine_id` field; multiple `Arc<BexEngine>` per process are legal; constructor `BexEngine::new(bytecode_program, sys_ops, event_sink, argv)` at :801 | **Add `engine_id: u64`** minted from a process-global `AtomicU64` in `BexEngine::new` (PR4). |
| wasm32 | `park_requested` omitted (:512-513); spawns via `spawn_local` (:2350, :2453) | No threads on wasm. **MVP: the master switch is forced off on `wasm32`** (compile to no-op); the cooperative-drain path is designed (v2 §6.5) but lands in a later PR. Rationale: no consumer thread + no TSC clock there; needs a js-backed clock decision first. |
| Shutdown | No engine shutdown hook. CLI flushes the legacy sink at `run_command.rs:670-672` after `rt.block_on(...)` returns | **Consumer flush hooks go at the same places** (§5 PR3). |

### 2.3 Events crates & periphery

- `crates/bex_events/`: `event_store.rs` (`static COLLECTOR_STORE: OnceLock<Mutex<CollectorStore>>` at :77 — the lock the new system exists to bypass), `span_id.rs` (`SpanId(uuid::Uuid)`, v4), `types.rs` (`RuntimeEvent`), `collector.rs`, `serialize.rs`. **The new code lives here as `src/prof/`** (see §2.4).
- `crates/bex_events_native/`: the existing channel+thread sink — `mpsc::sync_channel(4096)` at :87 (bounded, **drops on full** — the exact failure mode we're replacing), publisher thread spawned :89-92, `flush()` with ack + 30s timeout. Good prior art for thread naming and flush-ack shape; do not reuse the channel.
- Tokio runtime construction sites (for awareness — **no `on_thread_start` hook is needed** thanks to lazy init at the resume site): `baml_cli/src/run_command.rs:656` and `:939`, `baml_pack_host/src/main.rs:250`, `baml_lsp_server/src/lib.rs:101`, `bridge_cffi/src/lib.rs:64-69` (`OnceCell<Arc<Runtime>>`), `bex_project/.../wasm_helpers.rs:73`, `baml_tests/benches/runtime_benchmark.rs:81`.
- `stow.toml:145-157`: bridge namespace may depend on `{bex_project, bex_events, bex_events_native, bex_heap, bex_resource_types}`. Siting the ring in `bex_events` **requires no stow change** and bridges can reach init/flush APIs directly.
- Benches: `crates/baml_tests/benches/runtime_benchmark.rs` (generated from `tools/speedtest/workloads/*.md`), `codspeed-divan-compat` shim in `baml_tests/Cargo.toml:50`. **Verify a call-heavy workload exists; if not, add one** (M3 acceptance needs it — call-light benches can't see per-call cost).

### 2.4 Crate siting & dependencies

**New module: `crates/bex_events/src/prof/`** with submodules `clock.rs`, `record.rs`, `ring.rs`, `registry.rs`, `consumer.rs`, `file.rs` (+ `proto/` for the `.proto`). Re-export the producer API minimally (`bex_events::prof::{Ring, RingHandle, init, flush_and_join}`).

Workspace dependency changes (`baml_language/Cargo.toml`):

| Dep | Status | Action |
|---|---|---|
| `prost` 0.14 / `prost-build` 0.14.1 | **already present** (:174-175) | use for `DiskEventV1` |
| `borsh` 1.5, `uuid` 1 (v4), `parking_lot` 0.12, `crossbeam-channel` 0.5, `tokio` 1.36 | present | `uuid` for `process_id` |
| `minstant` | **add** | the calibrated-TSC clock. Its `Anchor` type does exactly the `started_at_epoch_ns` rebase the header needs. (`quanta` is the fallback if minstant misbehaves on some target — bench both in PR1.) |
| `crossbeam-utils` | **add** | `CachePadded` (D3). Tiny; alternatively hand-roll `#[repr(align(128))]`. |
| `loom` | **add as dev-dep** of `bex_events` | gate with `#[cfg(loom)]` + `--cfg loom` test profile |

Not needed (despite v1-doc mentions): `bitflags`, `arc-swap`, `core_affinity`, `num_cpus`, `zstd`.

### 2.5 Naming hazard — `CallId` already exists

`sys_types/src/lib.rs:135` defines `pub struct CallId(pub u64)` — it identifies one **engine root invocation** (used by `RuntimeEvent`, threaded through `run_thread_event_loop`). The design's per-function-call id is a different thing. **In `prof` code, name the new types `ProfCallId(u64)`, `ProfThreadId(u64)`, `EngineId(u64)`, `FunctionId(u32)`** (or nest under `prof::ids::`), and leave the final public naming to Paulo's M0 `ids.rs`. Do not reuse `sys_types::CallId` for per-call identity.

### 2.6 Function identity seam (until M0 lands)

Nothing exists yet (no `FunctionId`/`CallRef`/`FunctionMetadataTable` anywhere in the workspace). Don't block on Paulo:

- Add `function_id: u32` to `Function` (`types.rs:377-475`, boxed — no `Object` size impact), default `0` = unassigned.
- **Interim provider:** at `BexEngine::new` (:801), walk the `Program`'s functions, assign sequential ids, and build a `FunctionMetadataTable { fqn, source_file, span, kind }` snapshot for the file header. This matches v2's contract ("per-run runtime metadata, not stable across recompiles; FQN is the cross-run key") — M0 merely moves assignment to compile time. Keep it behind one function (`build_function_table(&Program) -> (assignments, table)`) so M0 replaces a single seam.

---

## 3. The ring — final specification (implements v2 §6 + D1–D7)

### 3.1 Types

```rust
// bex_events/src/prof/ring.rs
pub const DEFAULT_SEG_BYTES: usize = 256 * 1024;   // BAML_RING_SEG_BYTES, clamp [64 KiB, 16 MiB]

struct SegSync {
    commit_len: AtomicU32,        // producer Release per push → consumer Acquire
    next: AtomicPtr<Segment>,     // producer Release on link → consumer Acquire
}
struct Segment {
    buf: Box<[u8]>,               // read-only ptr after init; bytes via UnsafeCell-style raw access
    sync: CachePadded<SegSync>,   // D3: its own line, away from buf ptr
}

#[repr(u8)]
enum RingState { Active = 0, Orphaned = 1, Pooled = 2 }   // D5b lifecycle

struct RingProducer { head: *mut Segment, head_pos: usize }
struct RingConsumer { tail: *mut Segment, tail_pos: usize }
struct RingShared {
    state: AtomicU8,              // Active/Orphaned (Release) /Pooled; claim = CAS Pooled→Active
    free_head: AtomicPtr<Segment>,// Treiber; consumer pushes, producer pops (SPSC ⇒ no ABA, see §3.5)
    free_len: AtomicUsize,        // approximate (Relaxed), for D7 cap
    engine_id: UnsafeCell<u64>,   // written by claimant before first push; published by first commit
}
pub struct Ring {
    p: CachePadded<RingProducer>, // D3 group 1 — producer-only
    c: CachePadded<RingConsumer>, // D3 group 2 — consumer-only
    s: CachePadded<RingShared>,   // D3 group 3 — shared, touched ~per segment
}
```

Rings are allocated once and **never freed** (registry is append-only; pooled rings are reused). `Box::leak` at creation; `&'static Ring` everywhere — this is what makes orphan/drain race-free without epochs.

### 3.2 Producer protocol (hot path)

```rust
#[inline]
pub fn push(&self /* &'static, single OS thread */, rec: &[u8]) {
    let p = self.p_mut();                          // producer-only fields
    if p.head_pos + rec.len() > seg_capacity() {   // slow path: ~1 / 9,300 events
        let seg = self.free_pop()                  //   Treiber pop (Acquire)
                      .unwrap_or_else(|| alloc_segment());   // counts toward overflow cap
        // D2: producer owns the reset; published by the link Release below.
        seg.sync.commit_len.store(0, Relaxed);
        seg.sync.next.store(null_mut(), Relaxed);
        unsafe { (*p.head).sync.next.store(seg, Release) };  // link = publish resets + final bytes
        p.head = seg; p.head_pos = 0;
        // D4: wake only here, only if parked.
        if CONSUMER_PARKED.load(Relaxed) { consumer_handle().unpark(); }
    }
    unsafe { copy_nonoverlapping(rec.as_ptr(), buf_ptr(p.head).add(p.head_pos), rec.len()) };
    p.head_pos += rec.len();
    unsafe { (*p.head).sync.commit_len.store(p.head_pos as u32, Release) };  // 1 atomic/event
}
```

Hot-path budget per event: bounds check + `memcpy` (~28–40 B) + one Release store. No TLS access (the caller holds the ring pointer — D5a), no global loads (master switch is a VM-snapshot bool), no CAS, no lock, no allocation in steady state.

**Overflow cap:** `alloc_segment` does `OVERFLOW_BYTES.fetch_add(seg)` against `BAML_RING_MAX_OVERFLOW_BYTES`; exceeding it is a **hard process error** (clear message + abort) per D6 — never a silent drop.

### 3.3 Consumer protocol (drain — D1 baked in)

```rust
fn drain_ring(&self, r: &'static Ring) -> bool /* made progress */ {
    let c = r.c_mut();                                  // consumer-only fields
    let mut progress = false;
    loop {
        let seg = c.tail;
        let mut committed = seg.sync.commit_len.load(Acquire) as usize;
        progress |= self.consume(seg, &mut c.tail_pos, committed);
        let next = seg.sync.next.load(Acquire);
        if next.is_null() { return progress; }          // open segment; caught up for now
        // D1: next != null ⇒ producer is done with seg, and the Acquire above
        // (synchronizing with the link Release) makes its FINAL commit_len visible.
        committed = seg.sync.commit_len.load(Acquire) as usize;
        progress |= self.consume(seg, &mut c.tail_pos, committed);   // drain the remainder
        self.retire(r, seg);                            // D7: free-list cap or free()
        c.tail = next; c.tail_pos = 0;
    }
}
```

`consume` parses records `[tail_pos, committed)` and transcodes (§4). It never reads past `committed`, so producer byte-writes and consumer reads are range-disjoint; publication is the `commit_len`/`next` Release→Acquire pairs only.

`retire` (D7): `if r.s.free_len.load(Relaxed) >= FREELIST_CAP { free_segment(seg) } else { treiber_push(seg); free_len += 1 }`.

### 3.4 Registry, orphaning, pooling (D5b)

```rust
// bex_events/src/prof/registry.rs — append-only, push-only Treiber list (no pop ⇒ no ABA)
struct RegNode { ring: &'static Ring, next: AtomicPtr<RegNode> }
static REGISTRY_HEAD: AtomicPtr<RegNode>;   // CAS-push (Release); consumer walks with Acquire loads
```

- **Acquire a ring** (resume-site lazy init, D5a): scan the registry for a `Pooled` ring and `state.compare_exchange(Pooled, Active, Acquire, Relaxed)`; on success write `engine_id` (published by the first push's Release); on no pooled ring, allocate + CAS-append a new node. Happens once per `(engine, os-thread)` lifetime — scan cost is irrelevant.
- **TLS map:** `thread_local! { RefCell<SmallVec<(EngineId, &'static Ring)>> }`, consulted **once per exec resume**, not per push. Its `Drop` impl is the orphan trigger: `ring.s.state.store(Orphaned, Release)` for each entry. (tokio blocking-pool threads die after idle timeout — this path is routine, not exotic.)
- **Consumer sweep:** for each registry node — `Active`: drain. `Orphaned` (Acquire): drain to empty (the Release store on orphan + Acquire here orders after the producer's last push), then `state.store(Pooled, Release)`. `Pooled`: skip.

### 3.5 Why the free-list has no ABA

Per ring: exactly one pusher (the consumer thread) and one popper (the ring's OS thread). ABA on pop requires the observed head node to be removed and re-pushed by *another* popper concurrently; with a single popper that interleaving cannot exist. This invariant must be stated in code comments and exercised in loom. (The *ring pool* avoids pointer-CAS entirely — claiming is a state-CAS on a never-freed `&'static Ring` — so it has no ABA either.)

### 3.6 Wake/park (D4)

```rust
static CONSUMER_PARKED: AtomicBool;
// consumer main loop:
loop {
    let progress = sweep_all_rings();
    do_periodic(also: heartbeat, flush);
    if !progress {
        CONSUMER_PARKED.store(true, SeqCst);     // SeqCst: cheap here, runs at most ~20×/sec
        if !quick_recheck_any_work() { park_timeout(WAKE_INTERVAL) }   // default 50ms
        CONSUMER_PARKED.store(false, Relaxed);
    }
}
```

Producer side is the two lines in §3.2's slow path. **Document in code:** the flag/recheck shrinks but does not close the lost-wakeup window; `park_timeout` makes the residual race cost "≤50ms of extra ring growth", never loss.

### 3.7 Memory-ordering summary (the loom checklist)

| Edge | Release | Acquire | Publishes |
|---|---|---|---|
| record publish | `commit_len.store` (producer) | `commit_len.load` (consumer) | record bytes ≤ commit_len |
| segment link | `next.store` (producer) | `next.load` (consumer) | final `commit_len` of old seg; **D2 resets + all bytes of new seg's first records** |
| segment recycle | free-list push CAS (consumer) | free-list pop CAS (producer) | consumer is done reading the segment |
| orphan | `state.store(Orphaned)` (producer thread dtor) | `state.load` (consumer) | all records pushed before death |
| pool / claim | `state.store(Pooled)` (consumer) | claim CAS (new producer) | consumer is done draining |

loom tests (PR2, **before anything else**): (1) D1 hand-off — producer commits, links, keeps writing; assert zero record loss across all interleavings; (2) D2 recycle — full cycle push→drain→recycle→reuse; assert consumer never observes stale `commit_len`/`next`; (3) free-list SPSC integrity; (4) orphan→drain→pool→claim with no loss and no double-claim; (5) registry concurrent append + claim. miri runs the same tests un-loomed (raw-pointer/UnsafeCell discipline, unaligned reads).

---

## 4. Records, events, artifact

### 4.1 Raw ring records (little-endian, fixed layout per tag, `read_unaligned`)

```
tag u8 …fields                                            size
0x01 CallFunction : flags u8, thread_id u64, call_id u64,
                    parent_call_id u64 (0 = None),
                    function_id u32, ts_ns u64              38 B
0x02 EndFunction  : status u8 (Ok=0|Error=1), thread_id u64,
                    call_id u64, ts_ns u64                  26 B
0x03 StartThread  : flags u8, thread_id u64,
                    parent_thread_id u64 (0 = root),
                    parent_call_id u64 (0 = None), ts_ns u64,
                    name_len u16, name bytes            35 B + name (name ≤ 256 B)
0x04 EndThread    : status u8, thread_id u64, ts_ns u64     18 B
0x05 SetFunctionId: thread_id u64, call_id u64,
                    id [u8;16], ts_ns u64               (reserved — M1 emits it)
```

`call_id` counters start at 1 so `0` means None. `engine_id`/`process_id` are **never** in records (per-ring `engine_id` + file header, v2 §4). Tag→size table is compiled in (same-binary producer/consumer); only `StartThread` is variable-length and is guaranteed ≤ one segment by the name cap.

### 4.2 Clock

`minstant::Instant::now()` for `ts_ns` (nanos since process start via a process-global zero point), `minstant::Anchor` for the header's `started_at_epoch_ns: u128`. PR1 ships a 10-line bench (`ns per now()` on this hardware) — **the design's ~25 ns/call-pair budget assumes ≤10 ns/read; verify before PR4.** wasm32: clock module compiles to a stub; master switch is off (§2.2).

### 4.3 `DiskEventV1` (prost) and `.bamlprof`

The on-disk/wire contract (v2 §4). This is the Rust shape; `prof/proto/bamlprof.proto` mirrors it as a `oneof` message (statuses as proto enums; build with the existing workspace `prost-build` 0.14.1 — see `bridge_ctypes` for prior art on prost codegen wiring).

```rust
enum DiskEventV1 {
    StartThread {
        thread_id: u64,
        parent_thread_id: Option<u64>,   // spawning thread (None = engine-root thread)
        parent_call_id: Option<u64>,     // spawning call in the parent thread
        name: Option<String>,            // user-defined, runtime → inline, length-capped ≤256 B
        timestamp_ns: u64,
    },
    EndThread {
        thread_id: u64,
        status: ThreadEndStatus,
        timestamp_ns: u64,
    },
    CallFunction {
        thread_id: u64,
        call_id: u64,
        parent_call_id: Option<u64>,     // intra-thread caller (None = thread-root call); emitted by the VM
        function_id: u32,
        timestamp_ns: u64,
    },
    SetFunctionId {
        thread_id: u64,
        call_id: u64,
        id: [u8; 16],                    // $id override UUID (baml.id.new()); absent ⇒ $id = CallRef.
        timestamp_ns: u64,               // reserved at M2–M4; M1 emits it.
    },                                   // NB: sets the call's $id — unrelated to function_id (u32 metadata)
    EndFunction {
        thread_id: u64,
        call_id: u64,
        status: FunctionEndStatus,
        timestamp_ns: u64,
    },
    Heartbeat {
        timestamp_ns: u64,               // process-level liveness; stamped by the consumer (MVP)
    },
}

enum FunctionEndStatus { Ok, Error }                     // minimal; extensible (protobuf-compatible)
enum ThreadEndStatus   { Completed, Cancelled, Errored } // minimal; extensible

struct EventFileHeaderV1 {
    process_id: [u8; 16],              // UUID, minted once per process (OnceLock, uuid v4)
    engine_id: u64,
    program_id: ProgramId,               // NOTE: no ProgramId type exists in the workspace yet —
                                         // confirm with Paulo/M0 what identifies a Program
    started_at_epoch_ns: u128,           // wall anchor; wall(event) = started_at_epoch_ns + timestamp_ns
    function_table: FunctionMetadataTable,   // §2.6 interim provider until M0
}
```

Content is identical to the raw ring records (§4.1) — transcoding is a pure encoding change, no enrichment (the VM already emits `parent_call_id`). The `Option<u64>` fields correspond to the ring's `0 = None` convention. **`StartThread.name` is an inline `String`, not an interned `name_id: u32`** — locked in v2 (Appendix A): thread names are runtime values, an interning side-table would duplicate the ring as a second runtime producer→consumer channel and break partial-parse self-containment; `StartThread` is rare (once per thread), so the bytes are irrelevant.

File framing: header message, then length-delimited `DiskEventV1`s (`prost` length-delimiter), buffered writer, flush on cadence + shutdown. One file per `engine_id`, demuxed from the ring's `engine_id`: `.baml/profiles/<process_id>-<started_at>-<engine>.bamlprof`. `Heartbeat` is stamped by the consumer on a timer (MVP simplification of "uploader emits it"). `baml clean` integration is a one-liner in the CLI (existing clean category mechanism, or a follow-up if none exists).

---

## 5. Phased implementation (each phase = one PR, with acceptance gates)

### PR1 — Scaffolding, clock, records (no concurrency)
- Workspace deps (`minstant`, `crossbeam-utils`, dev `loom`); `bex_events/src/prof/` module skeleton; knob parsing (`BAML_PROFILE`, `BAML_RING_SEG_BYTES`, `BAML_RING_MAX_OVERFLOW_BYTES`, `BAML_RING_FREELIST_CAP`, wake interval).
- `record.rs` encode/decode for §4.1 + round-trip unit tests; `clock.rs` + the clock micro-bench.
- **Gate:** clock bench result recorded in the PR description (feeds the §5.4 cost model).

### PR2 — Ring core + lifecycle + loom/miri (the keystone)
- `ring.rs` (§3.1–3.3), `registry.rs` (§3.4), wake protocol (§3.6) as a library with a fake consumer.
- Full loom suite (§3.7) + miri + a 2-thread stress test (producer floods, consumer counts; assert `consumed == produced` across burst patterns including forced growth and forced recycling).
- **Gate:** loom + miri green in CI. **No PR4 work starts before this lands.**

### PR3 — Consumer thread + protobuf + `.bamlprof`
- `consumer.rs`: `std::thread` named `bex-prof-consumer` (prior art: `bex_events_native` :89-92), lazily spawned by a `OnceLock` on first ring registration; sweep loop = §3.4 + §3.6; transcode (§4.3); per-engine file writer; heartbeat.
- Lifecycle: `prof::flush_and_join(timeout)` (drain-to-empty + file flush, ack pattern like `NativeEventSink::flush`), called from: `run_command.rs` next to the existing `sink.flush()` (:670-672), `baml_pack_host/src/main.rs` main exit, `bridge_cffi` (alongside its existing shutdown/flush surface), `baml_lsp_server` (on server shutdown/reload). Shutdown ordering note per §1 (joined VMs ⇒ final commits visible).
- Throughput harness: synthetic producer → measure **events/s/core** for drain+transcode+write; write the D6 capacity formula with the measured number into the module docs and the v2 doc's §11.
- **Gate:** end-to-end fake-producer test produces a `.bamlprof` that a test reader parses back to the exact event sequence; measured drain rate recorded.

### PR4 — VM + engine integration (producer)
- **a (structural core):**
  - `BexVm`: `prof_ring: Option<&'static Ring>`, `prof_enabled: bool`, `prof_thread_id: u64`, `call_id_counter: u64`, `current_call_id: u64` (init in `BexVm::new` :1078 and `test_vm` :277).
  - `BytecodeFrame`: add `call_id: u64`.
  - Engine: `engine_id` on `BexEngine` (minted in `new` :801); `ProfThreadId` minted per logical thread (engine `AtomicU64`, at `new_root`/`new_child` construction sites); **resume-site refresh** at :2510-2512 (ring lookup via TLS map / claim / append + switch snapshot).
  - Emission: `CallFunction` after frame push in `execute_call_from_locals_offset` (:3204-3210); `EndFunction{Ok}` in `Return` (:5028); `EndFunction{Error}` at the bytecode unwind pop (:2730); nothing at Native pops (:2637, :3959). `call_id` mint always (it's `$id` semantics); ring write gated on `prof_enabled`.
  - `StartThread` at root (:1493-1495) and child spawn (:2870-2961, where the spawn edge is in hand); `EndThread` in `Complete` (:2579-2684).
  - Interim function-table provider (§2.6).
- **b (call coverage beyond bytecode):** `SysOp` function calls get an engine-side `CallFunction`/`EndFunction` pair around the `SysOp` arm (:2686-2868) — this is what makes LLM calls visible on the timeline; `Native(*const ())` inline calls likewise VM-side. Both reuse the same record shapes; frame-pop balance table updated.
- **Gates (the design's acceptance bars):**
  - **G3 lossless:** a spawn-heavy, call-heavy stress program; assert on-disk event count/balance == expected (every `CallFunction` has exactly one `EndFunction`; every `StartThread` an `EndThread`); run with tiny `SEG_BYTES` to force constant growth/recycle.
  - **G2 overhead:** call-heavy bench (add a workload under `tools/speedtest/workloads/` if none is call-heavy) comparing `BAML_PROFILE=0` vs `=1` vs pre-PR baseline; target ≤~2%, with the clock-dominance breakdown.
  - A reconstruction smoke test: read the `.bamlprof`, rebuild the tree per v2 §7.2, assert parent/child and inclusive/exclusive sanity on a known program.

### PR5 — Hardening & flip
- Orphan-path soak (spawn_blocking churn test), freelist-cap tuning, overflow-cap abort path test (assert the message, not a silent OOM).
- Default-on decision: ship **default-off** (`BAML_PROFILE=1` opt-in) until PR4 gates have CI history, then flip to default-on (v2's stated end state) in its own one-line PR. *(Deliberate, temporary deviation from v2 §5.2 for rollout safety.)*
- Doc sync: fold measured numbers + this ledger back into `bex-event-stream-design-v2.md` (§6 pseudocode replaced by §3 here, §11 capacity formula).

---

## 6. Invariants to enforce in review (the things that bite)

1. **Producer never blocks, ever**: no mutex, no condvar-notify, no `park`, no unbounded spin anywhere reachable from `push()` — it runs holding `ActiveHeapPermit` (GC `request_park` at lib.rs:1265 waits on ALL permits; a blocked producer = engine-wide GC stall).
2. **Consumer never touches the GC heap or permits**: it reads rings, the immutable function table `Arc`, and its own scratch. Nothing else. (This is what makes lossless-by-growth deadlock-free — v2 §6.4.)
3. **Balance:** every frame that emitted `CallFunction` emits exactly one `EndFunction` — Return (:5046) and bytecode-unwind (:2730) are the only two bytecode pop sites today; if a new pop site appears, it owes an event. Native/CPS pops (:2637/:3959) emit nothing **because they emitted nothing on entry** (PR4a) — keep entry/exit symmetric per `FunctionKind`.
4. **`exec()` never crosses an `.await`** — the per-exec refresh (D5a) is sound only while that holds. Add a debug assertion or a comment-contract at the refresh site; if anyone ever makes `step_compact` async-yield mid-exec, the ring pointer model must be revisited.
5. **`call_id` is minted unconditionally** (master switch gates emission only) — it's `$id` language semantics (v2 §3), and M1 will read it.
6. **No event from the consumer thread into rings** (it has no ring; heartbeat goes straight to the file writer).
7. **Registry nodes and rings are never freed** — `&'static` is the lifetime model; reuse, don't reclaim.

## 7. Open coordination points

- **Paulo / M0:** final `ids.rs` naming + `CallRef::encode/decode`; replaces §2.6's interim function-id provider and the `Prof*` placeholder names. The wire (u64/u32 quad) does not change.
- **M1 owner:** `$id` read/override → `SetFunctionId` (tag 0x05 reserved; record shape already specced).
- **`baml clean` / profiles dir:** confirm `.baml/profiles/` as the artifact home and whether a clean category exists to hook.
- **wasm32 cooperative drain:** designed (single ring, drain at yield points) but deferred; tracked as a follow-up issue when wasm needs profiling.
