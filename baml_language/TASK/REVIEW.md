# BEX Event Identity & Program Metadata — Implementation Review

**Branch:** `paulo/bex-event-identity-metadata` (`d318a6ceb`, one commit ahead of `canary`)
**Date:** 2026-06-10
**Reviewed against:** `TASK/TICKET.md` (Paulo's identity design), Antonio's `bex-event-stream-design-v2.md` (`BoundaryML/thoughts`), `TASK/POST-IMPLEMENTATION-NOTES.md`.

**Method.** Eight parallel review dimensions (engine lifecycle, VM notification path, compiler lowering, event encoding, concurrency/perf, contract alignment, entropy/minimality, test quality), every finding independently and adversarially verified by separate agents instructed to refute it from the code (72 findings raised, 71 confirmed, 1 refuted), a completeness critic over files no dimension owned, plus direct reading of the core integration points and **one empirical reproduction** (the `$id` override-loss bug was confirmed by writing and running the missing test, then reverting). The author's validation matrix re-ran green (75 tests) before review started.

---

## 0. Verdict

The identity **model** is right and faithfully transcribed: the quad scoping, reversible `CallRef`/`ThreadRef`, the six `DiskEventV1` variants field-for-field, the header shape, sink-independent `call_id` minting, and the metadata join path all match the agreed contract. The happy path is solidly built and solidly tested.

The branch is **not yet in good standing to merge**, for five fixable reasons:

| # | Finding | Severity | Why it matters to Antonio |
|---|---------|----------|---------------------------|
| 1 | Caught exceptions (`try/catch`) permanently desync the call-identity stack | **Critical** | Violates "every entered frame emits exactly one EndFunction" — his renderer's *only structural assumption* — on an ordinary language construct |
| 2 | `$id` override lost after any nested bytecode call (empirically confirmed) | Major | Language primitive returns wrong value; `SetId` stream stays right, `$id` reads go stale |
| 3 | Watch filters that call any function abort with `VmInternalError::ExpectedCompletion` | Major | Regression vs canary, introduced by the new notification yield |
| 4 | Orphan `EndFunction` without `CallFunction` — guaranteed for every `call_callable` | Major | His reference reconstruction `expect("CallFunction precedes EndFunction")` panics deterministically |
| 5 | `timestamp_ns` is wall-clock epoch nanos, not monotonic-since-process-start | Major | His rebase formula `wall = started_at_epoch_ns + ts` and sort-by-ts ordering are both broken |

Beyond these: a cluster of compiler-layer `$id` holes (compound assignment silently no-ops; non-string assignment skips type checking and explodes inside a `throws never` builtin; `$id` isn't reserved), a hot-path mechanism that inverts the agreed performance contract and must be treated as disposable scaffolding, several interim-sink hazards, and a meaningful amount of removable engine-side entropy.

**The single most important takeaway for the cross-team split:** Antonio's branch will be based on this one. Every invariant he depends on must be pinned by a test *here*, so that if either of you changes something, CI catches it before his consumer does. Section 9 specifies that contract suite in full. Right now, of the four paths in this branch's own Review Guidance question 1 (normal return, error, cancellation, early yield), **only normal return has any balanced-event coverage** — a repo-wide grep shows the only statuses ever asserted in any test are `FunctionEndStatus::Ok` and `ThreadEndStatus::Completed`. Every non-Ok status branch added by this diff (~15 emission sites) is dead-untested code.

---

## 1. What is in good standing (verified, not assumed)

These were checked explicitly, not taken on faith:

- **Wire shapes match the contract exactly.** All six `DiskEventV1` variants carry the agreed fields with the agreed widths (u64 thread/call ids, u32 `function_id`, `Option` parent edges). `SetId` carries the raw `[u8; 16]` and round-trips the `baml_id_1_` string encoding. `EventFileHeaderV1` matches TICKET §5.1 (Antonio's header plus the agreed optional snapshot/revision fields). `Heartbeat` is correctly type-only — defined, never emitted by the VM, per his spec (it's the uploader's job).
- **`ids.rs` is clean.** Panic-free on malformed external input (lengths validated before slicing), canonical base64url, versioned prefixes, per-component distinctness tested. `RuntimeId::decode` of a `baml_thread_1_…` string fails cleanly with `InvalidPrefix`.
- **Serialization is serde_json-based, not hand-rolled JSON** — quoting/control-char/non-ASCII escaping is correct; `Option` fields consistently serialize as `null`; the u128 header field is sensibly stringified for JS consumers.
- **No data races.** `SpanState` (including the non-atomic `next_call_id`), the VM's `current_bex_identity`, and `pending_disk_events` are exclusively owned by one event-loop future per BEX thread. Shared counters (`NEXT_ENGINE_ID`, `next_thread_id`) are atomics.
- **`call_id` minting is sink-independent.** `SpanState` is created unconditionally in `call_function`, so `$id` works with tracing off — satisfying TICKET 11.4 and Antonio §5.2. (Covered only implicitly by tests; see §9.3 T11.)
- **Spawn edges match TICKET §9.2 exactly**: `StartThread{parent_thread_id, parent_call_id}` on the child, child root `CallFunction{call_id: 1, parent_call_id: None}`.
- **Per-invocation `SpanState`** means no leaked counters or stacks in long-lived engines; `ActiveCallGuard` is RAII-correct; exactly one header per engine, emitted in `new()` before any events.
- **The default no-op `EventSink` methods** keep all five workspace implementors compiling (including `bridge_wasm`'s `WasmEventSink` and the LSP `PlaygroundEventSink`).
- **The "over-modeling" concern mostly evaporates.** `ThreadRef`, `SemanticLanes`, `Hash256`, `RevisionId`, `SourceSnapshotId` are all pinned by the ticket's contract shape and correctly left `Option`/`None`. Keep them.
- **Untouched-file sweep was clean**: Cargo.lock is one correct hunk; `collector.rs`/`event_store.rs`/`bridge_cffi/host_spans.rs` are pure `identity: None` plumbing; the Python `baml_events_pb2.py/.pyi` were regenerated correctly (descriptor verified field-by-field against the .proto); `tools_onionskin`'s new arm is correct; wasm parity holds (`web_time`, existing getrandom pattern, uuid `js` feature); no TODOs or production-path unwraps added.

---

## 2. Critical & major correctness findings

### 2.1 (CRITICAL) Caught exceptions permanently desync the call-identity stack

**Where:** `crates/bex_vm/src/vm.rs:2786-2799` (and the Native-frame twin at ~2679-2692); engine side `crates/bex_engine/src/lib.rs:4119, 4123, 4144-4156, 4235, 3461`.

**Mechanism.** When a throw is caught by a handler in an *outer* frame (`try { f() } catch { … }` where `f` makes bytecode calls), `try_unwind_exception` pops the unwound frames and silently drains `runtime_call_frames` (and `traced_frames`):

```rust
// vm.rs:2793 — no notification is yielded here
while self.runtime_call_frames.last().is_some_and(|d| *d >= self.frames.len()) {
    self.runtime_call_frames.pop();
}
```

Exit notifications are produced *only* on `OpCode::Return` (vm.rs:5100-5110, 5137-5143). `OpCode::Throw` resolves catches entirely inside the run loop without yielding, so the engine never learns a catch happened. The engine ignores the `frame_depth` it is already given (`frame_depth: _` at lib.rs:4119) and has no resync mechanism, so the unwound frames' `EngineSpan`s stay on `SpanState.stack` forever.

**Consequences on that thread, after one caught throw that crosses a bytecode frame:**

1. The next call's `parent_call_id = state.stack.last()` (lib.rs:4123) points at the dead, unwound call — corrupt parent edges from then on.
2. Every subsequent `RuntimeCallNotification::FunctionExit` pops positionally (lib.rs:4144-4156), so `EndFunction` events are emitted with call_ids **shifted by one per unwound frame**; `SpanNotification::FunctionExit` pops unconditionally (lib.rs:4235) and misattributes the same way for traced calls.
3. At `VmExecState::Complete` the engine blind-pops the top of the stack assuming it's the root span (lib.rs:3461) — it pops a stale inner span, emits `EndFunction(Ok)` for the wrong call_id, emits the legacy `FunctionEnd` event labeled with the wrong function name but carrying the root's result, and the true root call (and every other stale span) **never gets an `EndFunction` before `EndThread`**.
4. `$id` read after the catch reflects the stale top span — wrong identity.

This violates Antonio §5.1 ("Every entered frame emits exactly one EndFunction, **including frames popped during exception unwinding** — this balance is the renderer's only structural assumption") and TICKET §5.3 parent rules, on a completely ordinary construct. All five verification agents traced it independently end-to-end.

**Note:** `traced_frames` had the same silent-pop weakness on canary, but it was latent (traced frames are rare, exceptions across them rarer). This branch extends the pattern to *every bytecode call* and to the new disk stream, making it load-bearing.

**Fix direction.** Two viable shapes:
- *Notify on unwind:* yield an unwind notification (or batch: "truncate to depth N, status=Error/Unwound") from `try_unwind_exception` so the engine pops and emits `EndFunction` per truncated span.
- *Watermark resync:* the engine already receives `frame_depth` on every `FunctionEnter`; store it on `EngineSpan` and, on every notification, truncate `SpanState.stack` to entries whose depth is consistent, emitting `EndFunction` for each truncated span. This also self-heals any future silent-pop path.

Independently, make the `Complete` arm pop the span whose `runtime_call_id` is the known root (call_id 1) rather than blind `pop()` — cheap defense in depth.

**Tests to add (the contract guard — this is the test that would have caught the branch's main defect):**

- **T1 — `bex_disk_events_balance_across_caught_exception` (`crates/bex_engine/tests/tracing.rs`).**
  ```baml
  function boom() -> int { throw MyErr() }
  function safe() -> int { try { boom() } catch { 0 } }
  function after() -> int { 1 }
  function main() -> int { let a = safe(); after() }
  ```
  With `CapturingSink`, assert on the **full ordered** disk-event vector:
  (a) exactly one `CallFunction` and exactly one `EndFunction` per `call_id` — build a `HashMap<call_id, (calls, ends)>` and assert every entry is `(1, 1)`;
  (b) `boom`'s `EndFunction` exists and carries a non-Ok status once unwind emission lands (initially: exists at all);
  (c) `after()`'s `CallFunction.parent_call_id` is **`main`'s call_id**, not `boom`'s or `safe`'s;
  (d) the root `EndFunction` is the last function event and has `call_id == 1`;
  (e) total event count is exact (no duplicates).
  Variants worth a second test each: catch **two frames up** (`try { a() }` where `a` calls `b`, `b` throws — exercises multi-frame truncation, where positional popping shifts by more than one), and a **traced (LLM-style) frame** between thrower and catcher (exercises the `SpanNotify` unconditional pop at lib.rs:4235).
- **T2 — `id_is_correct_after_caught_exception`.** Same program, `main` returns `$id` after the catch; decode it and assert `call_id == 1` (the root), not a stale nested id.
- **T3 — VM-level unit test (`crates/bex_vm`)**: drive a program with a caught cross-frame throw through a minimal runner and assert `runtime_call_frames` is empty at `Complete` *and* that the count of `FunctionEnter` notifications equals the count of `FunctionExit`+unwind notifications. This pins the VM-side invariant independently of the engine, so if Antonio relocates emission into the VM (his ring), the balance invariant survives the move.

### 2.2 (MAJOR — empirically confirmed) `$id` override is lost after any nested bytecode call

**Where:** `crates/bex_engine/src/lib.rs:1369-1393` (`current_bex_identity_for_state`), assignment site lib.rs:3277-3280; `crates/bex_vm/src/package_baml/id.rs:42-49`; `EngineSpan` definition lib.rs:272-282.

**Reproduced live.** A test modeled on `baml_id_assignment_overrides_current_id` with one nested call:

```baml
function helper() -> int { 1 }
function main() -> string {
    let next = baml.id.new();
    $id = next;
    let mid = $id;       // == next  ✓ (override visible)
    let x = helper();
    let after = $id;     // == baml_call_1_… default CallRef  ✗ (override LOST)
    next + "|" + mid + "|" + after
}
```

Observed: `mid == next == baml_id_1_bD-4GM…`, but `after == baml_call_1_AWpl…` (default). Same failure inside `spawn` bodies.

**Mechanism.** The override lives *only* in `vm.current_bex_identity.runtime_id` — a transient String. Before every `vm.exec()` step the engine overwrites it via `current_bex_identity_for_state`, which reuses the previous string only when `previous.call_id == current_span.runtime_call_id`. After `helper()` returns, `previous` holds *helper's* identity, the filter mismatches, and line 1386 re-encodes the parent's **default** CallRef. `EngineSpan` and `SpanState` have no override field, so the override is unrecoverable. The existing tests pass only because they read `$id` back immediately — and `baml.id.new()` is native, so it doesn't trip the bytecode-call path.

Note the disk stream stays *correct* (the `SetId` event was emitted at set time); it's the language-level read that goes stale. Consumers and the language would now disagree about the same call's `$id`.

**Fix.** Persist the override at span level: add `id_override: Option<String>` (or the 16-byte uuid) to `EngineSpan`. The cleanest write-back point is where the engine drains `pending_disk_events` — the `SetId` event already carries exactly `{thread_id, call_id, uuid}`; match it to `state.stack` and store. Then `current_bex_identity_for_state` prefers `current_span.id_override` over re-encoding. This automatically covers spawned threads (each child loop has its own `SpanState`).

**Tests to add:**

- **T4 — `id_override_survives_nested_call`.** Exactly the repro above (it is already written and validated — lift it as-is). Assert `next == mid == after`.
- **T5 — `id_override_survives_nested_call_in_spawn`.** Same pattern inside `spawn { … }` (also already validated as failing today).
- **T6 — `id_override_not_inherited_by_nested_call`.** The inverse direction, pinning scoping semantics for Antonio: after `$id = next`, call `helper()` which itself reads `$id` — assert helper's `$id` is its **own default CallRef** (`call_id == 2`), not the parent's override. Nothing pins this today; if someone later "fixes" the override by making it sticky-global on the VM, this catches the overcorrection.
- **T7 — `set_id_emitted_once_per_override`.** Assert exactly one `SetId` disk event per override (count, not `any()`), with `call_id` equal to the overridden call, and ordered after that call's `CallFunction` and before its `EndFunction`. Guards SetId/CallFunction adjacency semantics Antonio's resolver relies on (§4.3: "absence of SetId for a call ⇒ `$id` is the CallRef").

### 2.3 (MAJOR) Watch filters that call any function now abort the program

**Where:** `crates/bex_vm/src/vm.rs:3382-3394` (`process_notifications` → `interrupt()`), trigger at vm.rs:3283-3289.

**Mechanism.** User-supplied watch filter functions run via the recursive `interrupt() → self.exec()` mini-runner, whose result match accepts only `Complete`:

```rust
match self.interrupt(filter_func, &[state.value])? {
    VmExecState::Complete(v) => …,
    _ => return Err(VmInternalError::ExpectedCompletion.into()),
}
```

Since every non-traced bytecode call now yields `RuntimeCallNotify::FunctionEnter` unconditionally, a watch filter whose body calls **any** helper — previously fine on canary, where non-traced calls stayed inside the dispatch loop — now deterministically kills the whole program with an internal error. `WatchFilter::Function` is user-reachable via `OpCode::Watch`. This is the concrete instance of the post-impl notes' own risk: "any direct VM runner that assumes only `EarlyYield` is ignorable may now also need to ignore or consume `RuntimeCallNotify`." The repo's external runners were updated; the VM's *internal* runner was not.

**Fix.** In `interrupt()`, loop on `exec()`, swallowing (resuming through) `RuntimeCallNotify` — and decide deliberately about `SpanNotify`/`EarlyYield` while there. Note: events generated inside the filter would bypass the engine's `SpanState`; the simplest correct v1 is to *not* mint identity for filter-internal calls (swallow without pushing), and document it.

**Tests to add:**

- **T8 — `watch_filter_calling_helper_function_works`.** A program installing a watch with a filter function whose body calls a helper; assert the program completes and the watch fires. This is a *regression* test — it passes on canary and fails on this branch today.
- **T9 — `watch_filter_calls_do_not_corrupt_identity`.** Same program, then read `$id` and assert the main thread's disk stream is balanced (per-call_id (1,1) map from T1) — pins whatever scoping decision you make for filter-internal calls.

### 2.4 (MAJOR) Orphan `EndFunction` without `CallFunction` — guaranteed for every `call_callable`

**Where:** `crates/bex_engine/src/lib.rs:1416-1424` (`emit_disk_call_function` gate) vs lib.rs:1427-1439 (`emit_disk_end_function`, unconditional; called from 1470, 1515, 3465, 4151, 4238); trigger lib.rs:2303 (`"<callable>"` label) + lib.rs:2061.

**Mechanism.** `emit_disk_call_function` silently skips emission when the name→`function_id` lookup returns `None`, but every `EndFunction` site is unconditional. `call_callable` passes the literal label `"<callable>"`, which never resolves — so **every** invocation through `call_callable` (HTTP server handlers, `VmSpawner::spawn_with_callable`) emits `StartThread`, *no* `CallFunction`, then an orphan `EndFunction{call_id: 1}` and `EndThread`. The same applies to any nested call whose notification name misses both the FQN and display-name lookups. Antonio's reconstruction (§7.2) does `map.remove(&e.call_id).expect("CallFunction precedes EndFunction")` — an orphan is a deterministic panic in his renderer.

**Fix.** Never skip the `CallFunction`. Add a reserved "unknown function" sentinel row to the metadata table (exactly like the synthetic spawn-closure row) and emit with it; separately, give `call_callable` a real identity by resolving the callee's function object to its pool index instead of a placeholder label. (Suppressing the `EndFunction` symmetrically is strictly worse — it hides calls.)

**Tests to add:**

- **T10 — `call_callable_emits_balanced_disk_lifecycle`.** Drive `call_callable` with a sink; assert the exact sequence `StartThread, CallFunction, EndFunction, EndThread` with matching call_ids — i.e., that the root call **does** get a `CallFunction` (sentinel or real id). Fails today.
- **T11 — `every_end_function_has_matching_call_function` (suite-level invariant helper).** Factor the per-call_id `(calls, ends) == (1, 1)` assertion from T1 into a helper (`assert_balanced(&events)`) and call it in **every** disk-event test. This single helper is the cheapest contract net you can give Antonio: any future change that breaks balance fails many tests at once, with a precise per-call_id diff.
- **T12 — `unresolved_function_name_still_emits_call_function`.** Synthesize a notification name not in the table (or call through a path with a display-name miss) and assert a `CallFunction` with the sentinel id is emitted. Also assert the **display-name collision** case: two functions in different packages with the same display name must not be attributed to the same `function_id` (see §4.3 — today `function_id_for_name`'s fallback can misattribute).

### 2.5 (MAJOR) `timestamp_ns` is wall-clock epoch nanos, not monotonic-since-process-start

**Where:** `crates/bex_engine/src/lib.rs:1402-1407` (`timestamp_epoch_ns`), duplicated in `crates/bex_vm/src/package_baml/id.rs:68-73`.

**Mechanism.** `SystemTime::now().duration_since(UNIX_EPOCH)` for every `timestamp_ns`, *and* `started_at_epoch_ns` is also absolute epoch. Antonio §4 is explicit and emphatic: `timestamp_ns` is monotonic nanos **since process start**, globally comparable, "**never wall-clock** — wall-clock can step backward and corrupt durations/ordering," rebased once via `wall(event) = started_at_epoch_ns + timestamp_ns`. As written: his rebase formula yields ~2× epoch; his group-by-thread-sort-by-ts reconstruction can be corrupted by NTP steps; and epoch-nanos (~1.78e18) exceed JS's 2^53 safe-integer range, so his JSONL consumers silently lose precision — three independent breakages from one field. This is the cheapest fix on the list and the most certain to bite him first.

**Fix.** One process-global anchor (`OnceLock<Instant>` next to `ProcessEuid::current()` in `bex_events`), `timestamp_ns = anchor.elapsed().as_nanos() as u64`; keep `started_at_epoch_ns` as the wall anchor captured once alongside it. Delete both per-crate `timestamp_epoch_ns` helpers in favor of one `bex_events::now_ns()`. (The calibrated-TSC clock is Antonio's M2 dependency choice; `Instant` is the correct interim.)

**Tests to add:**

- **T13 — `timestamps_are_relative_to_process_start`.** Run a program; assert every `timestamp_ns << started_at_epoch_ns` (e.g. `< 10^15`, ~11 days of process uptime — generous but catches absolute-epoch values at ~1.78e18 forever). This is the test whose absence let the violation ship invisibly.
- **T14 — `timestamps_compose_with_wall_anchor`.** Capture `SystemTime::now()` before and after the run; assert `started_at_epoch_ns + event.timestamp_ns` lands within `[before, after]` for the first and last event. This pins the *composition formula itself* — exactly what Antonio's renderer computes.
- **T15 — `timestamps_are_monotonic_per_thread`.** Assert each thread's event timestamps are non-decreasing in emission order. (With `Instant` this is guaranteed; the test exists to catch a future regression back to `SystemTime` or a per-thread clock mixup when the TSC clock lands.)

### 2.6 (MAJOR) Root cancellation reported as `Error`; children report `Cancelled`

**Where:** `crates/bex_engine/src/lib.rs:2119-2127` (root status mapping); contrast child path lib.rs:3566-3573.

**Mechanism.** All root-path cancellations surface as `Err(cancelled_unhandled_throw())` (cancel-at-sysop 3575, cancel-during-async-op 3609, cancel-at-await 3847/3896, cancel-wins race 3503). `run_entry_point` maps **every** `Err` to `ThreadEndStatus::Error`, and the error drain emits `FunctionEndStatus::Error` for all open spans. `is_cancelled_engine_error` exists (lib.rs:423) but is never consulted here. So a host-cancelled root run reads as an error, while spawned children in the *same trace* correctly read `Cancelled` — a renderer will classify the same cancel two different ways. Bonus: a clean `baml.sys.exit(code)` also lands as `EndFunction(Error)/EndThread(Error)`.

**Fix.** Branch on `is_cancelled_engine_error(err)` in the root epilogue to emit `Cancelled`/`Cancelled`; decide an explicit mapping for `EngineError::Exit` (arguably `Completed` for code 0).

**Tests to add (these also close the "every non-Ok status branch is dead-untested" gap):**

- **T16 — `root_cancellation_emits_cancelled_statuses`.** Start a root call that parks on an await/sleep, fire its `CancellationToken`, assert `EndFunction{call_id: 1, status: Cancelled}` then `EndThread{status: Cancelled}` — balanced per T11's helper. Fails today (gets `Error`).
- **T17 — `spawned_child_cancellation_emits_cancelled`.** Cancel mid-child; assert the **child thread's** `EndFunction(Cancelled)`/`EndThread(Cancelled)` and that the parent thread's stream is unaffected. (Passes today — pins the already-correct child behavior so it can't regress while fixing the root.)
- **T18 — `root_error_emits_error_statuses` / `spawned_child_error_emits_error_statuses`.** Unhandled throw at root and in a child; assert `EndFunction(Error)` (with the error string on the legacy event) and `EndThread(Error)`, balanced, and for the nested case assert *every* open ancestor span got an `EndFunction(Error)` (pins the `emit_function_end_events_with_status` drain).
- **T19 — `sys_exit_status_mapping`.** Pin whatever you decide for `baml.sys.exit(0)` / `exit(1)` explicitly, so the choice is documented in test form.
- **T20 — `early_yield_resume_produces_identical_disk_stream`.** Run the same program with and without early-yield enabled (see `bex_vm/tests/early_yield.rs` for the harness); assert the two disk-event streams are identical modulo timestamps. This is the fourth quadrant of Review Guidance Q1, currently untested — and it guards Antonio against suspension-related duplication/loss when he moves emission into the VM.

### 2.7 (MINOR) Child-thread engine-error path emits `EndThread(Error)` without closing in-flight calls

**Where:** `crates/bex_engine/src/lib.rs:3200-3211` (spawn task `Err` arm; also the `RootValue`-invariant arm at 3194).

When `run_thread_event_loop` returns `Err(EngineError)` for a spawned child (conversion failure pushing a sysop result, `future_ready` errors, internal settle failures), the spawn task logs and emits `EndThread(Error)` but never drains `local_span_state` — the child's open `CallFunction`s get no `EndFunction`. The root path handles the same case via `emit_error_function_end_events` (lib.rs:2119-2122); the child path is asymmetric. Antonio's §7.3 safety net ("EndThread closes any still-open frame as incomplete") bounds the damage — hence minor — but it still violates the balance rule on a reachable path.

**Fix.** Mirror the root path: `emit_error_function_end_events(call_id, &mut local_span_state, …)` before the `EndThread(Error)` in both arms.

**Test:** **T21 — `child_engine_error_closes_open_calls`.** Force an engine-level error in a child (the conversion-failure path is the most reachable); assert the child's stream is balanced with `EndFunction(Error)` preceding `EndThread(Error)`.

### 2.8 (MINOR) Dropping the `call_function` future mid-execution truncates the event stream

**Where:** `crates/bex_engine/src/lib.rs:2106-2132`; no `Drop` impl for `SpanState`/`EngineSpan`.

All terminal emission happens inline after `run_thread_event_loop` returns. If the host drops the `call_function` future at an await point (`tokio::time::timeout`, `select!`) — a realistic embedding pattern — `StartThread`/`CallFunction` were emitted but no `End*` ever will be, while the engine and sink stay alive. The trace is indistinguishable from a process that died mid-run. `ActiveCallGuard` cleans the registry on drop; event balance has no equivalent RAII.

**Fix.** Either a drop-guard around the per-invocation span state that emits `Cancelled` `EndFunction`s + `EndThread` for still-open spans, or an explicit documented contract that hosts must cancel via `cancel_function_call` and await completion. Decide one; don't leave it implicit.

**Test:** **T22 — `dropped_call_future_closes_thread` (if guard chosen).** `tokio::time::timeout(small, engine.call_function(…))` around a parked call; after the timeout, assert the sink received `EndThread` (status `Cancelled`) and balanced `EndFunction`s. If you choose the documentation route instead, write the test asserting current truncation behavior with a comment marking it intentional — so a change in either direction is visible.

---

## 3. Compiler-layer `$id` findings

The author's notes claim the silent-temp-assignment miscompile was closed. It was closed **only for plain `=`**. The failure mode is narrowed, not eliminated, and there is no diagnostic backstop for future syntax forms.

### 3.1 (MAJOR) `$id += x` silently compiles to a no-op

**Where:** `crates/baml_compiler2_mir/src/lower.rs:11149-11171` (`AstStmt::AssignOp` — no `is_runtime_id_path` check), silent temp fallback at lower.rs:11232-11237; TIR accepts it because `$id` infers as String and `String + String` is valid concatenation.

`$id += "-suffix"` compiles with zero diagnostics, reads an uninitialized temp, discards the result: no `SetId`, no error, no effect. This is *exactly* the bug class the special case was added to prevent.

**Fix.** Since `baml.id.set` only accepts override IDs, compound assignment can never be meaningful — make `$id` as an `AssignOp` target a **compile error**. Separately and more durably: make `lower_lvalue`'s unresolved-single-segment fallback a diagnostic instead of a silent temp, so *any* future special form fails loudly rather than no-oping. That fallback is the root enabler of this entire bug class.

### 3.2 (MAJOR) `$id = <non-string>` skips type checking and fails at runtime inside a `throws never` builtin

**Where:** `crates/baml_compiler2_tir/src/builder.rs:5887-5890` (no-declared-type else-branch), `get_declared_type` at builder.rs:6085-6095 (returns `None` for `$id`).

TIR types a `$id` *read* as String but never enforces String on *assignment*: `Stmt::Assign` checks the RHS only against `get_declared_type(target)`, which is `None` for `$id`, so the mismatch branch is skipped. MIR then lowers unconditionally to `baml.id.set(value)`, and the generated glue's `vm.as_string(...)?` fails at runtime — from a builtin declared `throws never`. `$id = 42` should be a trivial compile-time `TypeMismatch`.

**Fix.** In `Stmt::Assign`, when the target is the `$id` path, treat the declared type as `Ty::String` (the same type the read special case returns).

### 3.3 (MAJOR, from the completeness critic) `baml.id.set` declares `throws never` but throws catchable `root.errors.InvalidArgument`

**Where:** `crates/baml_builtins2/baml_std/baml/ns_id/id.baml:15-16`; `crates/bex_vm/src/package_baml/id.rs:34-48`.

Three realistic paths throw: (1) the string fails `RuntimeId` decode; (2) the string is a valid **default CallRef** rather than an override — so even `baml.id.set(baml.id.current())` throws; (3) no identity active (`$init`, non-engine runners). Per vm.rs:3324, `VmBamlError::InvalidArgument` becomes a catchable `root.errors.InvalidArgument` exception — a checked-throws soundness lie on a brand-new public API. The repo's own convention handles this correctly elsewhere (`bigint.isqrt` declares `throws root.errors.InvalidArgument`).

**Fix.** Declare `function set(id: string) -> string throws root.errors.InvalidArgument` (and update the doc comment to say what inputs are valid). Related nit: `current()` is annotated `//baml:mut_vm` but only reads VM state — `//baml:vm` suffices; only `set()` needs the mutable borrow.

### 3.4 (MINOR) `$id` is not a reserved name

**Where:** lexer `tokens.rs:122` accepts `$`-prefixed identifiers; no layer rejects `$id` as a binding.

`let $id = 42` (or a parameter named `$id`) compiles and creates a real local — which is then **silently dead**: reads hit the TIR/MIR special cases first, and `$id = x` calls `baml.id.set(x)` even though the local exists. Worse, the layers disagree: with `let $id: int = 1; $id = 2`, TIR type-checks `2` against `int` (accepting), while MIR routes to `baml.id.set(2)` → runtime failure.

**Fix.** Reject `$id` (or all `$`-prefixed names) as a binding name in let/param/pattern lowering with a "reserved identifier" diagnostic.

### 3.5 (MINOR) `$id.foo` / `$id.len()` fail with a misleading "unresolved name: $id"

**Where:** `builder.rs:7781-7845` — the multi-segment path branch has no `$id` knowledge; only `segments.len() == 1` does (builder.rs:7741-7747).

Member access on the documented primitive is a compile error claiming `$id` is unresolved, while `let x = $id; x.len()` works. Fails loudly (good), wrong message (bad).

**Fix.** Treat a `$id` root in the multi-segment branch as a String-typed value root, or at minimum special-case the diagnostic ("`$id` is a value; bind it to a local before member access" — or just support it).

### 3.6 (MINOR) The design doc's call-site form `foo($id = baml.id.new())` doesn't compile — and the semantics changed

**Where:** parser treats `$id =` in argument position as a labeled argument; TIR reports `UnknownNamedArgument { "$id" }` (builder.rs:1984-1991).

TICKET §9.4's *only* override example is the call-site form, and Antonio's doc (§3, §4.3) specifies it too: **the caller names the callee's call**, with `SetId` emitted immediately after `CallFunction`. The implemented surface is callee-side in-body assignment — semantically different (a function can only rename *itself*; a caller orchestrating N calls can't tag them) and `SetId` is no longer adjacent to `CallFunction`. The post-impl notes' deviation #6 acknowledges the surface shape but not that the contract's example fails to compile.

**This is a product/contract decision to make explicitly with Antonio, not a code fix.** Either implement the call-site form (special-case a `$id` label in call-arg lowering → `SetId` on the new call), or amend both design docs and emit a targeted diagnostic ("`$id` cannot be set at the call site; assign `$id` inside the function") instead of the generic unknown-argument error.

### 3.7 (MINOR, entropy) The AST-level `$id` rewrite is effectively dead code; the special form lives in three layers with two strategies

**Where:** `crates/baml_compiler2_ast/src/lower_expr_body.rs:2409-2426`.

The AST rewrite only fires for single-segment `PATH_EXPR` nodes, but the parser emits a lone `$id` as a bare token — every realistic read flows through the bare-token paths handled by the TIR type stub + MIR rewrite. No test can currently reach the AST arm. The two live-vs-dead strategies also differ (AST: synthesized `Call` through full TIR checking; MIR: direct `builder.call` bypassing TIR) and will drift. Bonus bug in the dead arm: for `$id<T>` its early return skips `wrap_generic_apply`, silently dropping type args.

**Fix.** Pick one canonical owner for the desugar. Simplest: delete the AST rewrite and keep TIR (type) + MIR (lowering), with a comment in each naming the other. Better long-term: desugar once at AST so TIR/MIR special cases disappear entirely.

### Tests to add for the whole compiler cluster

Compiler `$id` handling is string-matched across three crates with **zero compiler-layer tests** — all coverage is end-to-end through `bex_engine`, exercising only the two happy shapes. Add, in the compiler crates themselves:

- **T23 — MIR snapshot tests (`baml_compiler2_mir`):** (a) bare `$id` read lowers to a call of `baml.id.current` (and *not* to a name lookup); (b) `$id = e` lowers to `baml.id.set(e)`; (c) once fixed: `$id += e` produces the targeted diagnostic; (d) once fixed: a binding named `$id` produces the reserved-identifier diagnostic.
- **T24 — TIR tests (`baml_compiler2_tir`):** (a) `$id` infers as String; (b) once fixed: `$id = 42` is a `TypeMismatch` at compile time; (c) `foo($id = x)` produces whichever diagnostic you standardize on (pin the *message*, not just "errors" — the current `UnknownNamedArgument` wording is part of what's wrong).
- **T25 — runtime tests (`bex_engine/tests/tracing.rs`):** (a) `baml.id.set(baml.id.current())` throws a *catchable* `root.errors.InvalidArgument` once the throws clause is fixed (try/catch it in BAML and assert the catch runs); (b) `baml.id.set("garbage")` likewise; (c) `$id` read during `$init` — pin whatever you decide in §5.4 (today: empty string).
- **T26 — `id_member_access_diagnostic`:** pin the improved diagnostic (or the working behavior) for `$id.len()`.

These matter doubly for the cross-team setup: Antonio's doc treats `$id` as a stable language primitive his events serialize. The compiler tests are what keep the primitive's *surface* stable underneath him while the lowering machinery is refactored (e.g. when the dead AST layer is removed).

---

## 4. The hot path: `RuntimeCallNotify` vs the performance contract

### 4.1 (CRITICAL as perf / coordination) Every non-traced bytecode call exits the VM dispatch loop twice and round-trips the engine

**Where:** `crates/bex_vm/src/vm.rs:3283-3289` (enter), 5100-5110 / 5137-5143 (exit); engine handler `crates/bex_engine/src/lib.rs:4114-4160`; loop head lib.rs:3273-3283.

**Mechanism and cost, per non-traced call.** Pre-branch, a non-traced bytecode call stayed inside the interpreter loop (`*function = unsafe { self.load_function(*frame_idx)? }` — the line this diff removed). Now the VM suspends and returns to the engine's async loop on **enter and exit** of every call. The dispatch loop's own annotation reads "Measured: 20-40% speedup from inlining the dispatch loop" — and this mechanism leaves that loop entirely, twice per call. Each round-trip pays:

- a `String` clone of the callee name on enter (vm.rs:3226 — now cloned for *all* bytecode calls, not just traced), and **a second name clone on exit that the only consumer ignores** (`function_name: _` at lib.rs:4144);
- `SpanId::new()` — a UUID v4 via getrandom (workspace `uuid` has no fast-rng feature);
- an O(F) **linear string scan** of the function table (~536 stdlib entries; double scan on a miss) — see §4.3;
- two `SystemTime::now()` reads;
- `DiskEventV1` construction + dyn sink dispatch (when a sink exists);
- on resume: SpanContext rebuild, an identity-`String` clone **every loop iteration**, and a CallRef base64 encode on call transitions (§4.2).

Estimated 1-10µs per call against Antonio's ~10ns/call budget (100M events/sec, ≤2% slowdown) — two to four orders of magnitude over, **with no off switch**: most of this cost is paid even with `event_sink = None`.

**This is the central coordination item, not a "bug" to patch in place.** Antonio's design puts emission *inside* the VM (raw ~30-byte memcpy into a thread-local ring; dominant cost = two TSC reads) precisely to avoid what this branch built. His M3 replaces this path. The risk is that the scaffolding hardens: his branch builds consumers atop a mechanism that must be deleted, or the two mechanisms end up coexisting (double emission). What's worth doing **now**:

1. **Gate the yield.** Skip the `runtime_call_frames.push` + `RuntimeCallNotify` return when identity consumers are absent (no sink and no `$id` usage — or behind the future master switch), restoring the inline `load_function` path. Note the constraint: `call_id` minting must survive the gate ($id is a language feature) — which today is engine-side, hence:
2. **Agree on the seam with Antonio.** The clean target (his §5.1 + your M2): the **VM owns the per-thread `call_id` counter and current-call state**; the engine *reads* identity instead of minting it on yield. That makes the notification mechanism deletable without moving `$id` semantics again. Write the agreed seam into both TASK docs.
3. **Make the notification carry the function's pool index, not its name** (§4.3) and make `FunctionExit` a unit variant — removes both per-call clones and the table scan regardless of when the ring lands.

### 4.2 (MAJOR) Eager `CallRef` string minting violates TICKET 11.4

**Where:** `crates/bex_engine/src/lib.rs:3277-3280` + `current_bex_identity_for_state` (lib.rs:1369-1393).

TICKET §11.4: "CallRef is not minted as a string unless `$id` is read or artifact/rendering needs it." The engine eagerly computes the encoded string before **every** `vm.exec()` step — `encode()` (Vec alloc + base64 + format!) whenever the active call changed, a `String` clone when it didn't. With the per-call yields, that's ≥2 encodes per non-traced call even if the program never touches `$id`.

**Fix.** Hand the VM the raw `(thread_id, call_id, override)` tuple and let `baml.id.current()` encode lazily on read. This also deletes a chunk of §4.1's cost and composes with the §2.2 fix (the override becomes part of the tuple, not a pre-rendered string).

### 4.3 (MAJOR) `function_id` resolution is a per-call linear name scan with a misattribution hazard

**Where:** `crates/bex_engine/src/lib.rs` `function_id_for_name` (FQN scan, then display-name scan on miss); `FunctionMetadataTable::get`/`function_id_for_fqn` are `Vec::iter().find()` (`crates/bex_events/src/metadata.rs:92-105`).

Two independent problems:

- **Perf:** O(F) string comparisons per call (double on miss), on the §4.1 hot path.
- **Correctness:** the display-name fallback means two functions with the same display name in different packages can be attributed to the **same** `function_id`. It also diverges from the engine's existing canonical name-resolution seam (`lookup_function`), so the two can disagree.

**Fix.** The deep fix is free: `FunctionId` **is** the object-pool index (that's how `build_program_metadata` assigns it). The VM holds the frame's function object/pool index at both notification sites — pass the index in `RuntimeCallNotification` and delete the reverse lookup entirely. The name then doesn't need to ride the notification at all. Keep `function_id_for_fqn` for the *root* label lookup only (or resolve the root via `lookup_function`'s index too), and drop the display-name fallback (a miss should hit the §2.4 sentinel, never a guess).

### 4.4 (Context — verified non-issue) No `BAML_PROFILE` master switch

The one **refuted** finding, recorded so nobody re-raises it: the master switch is explicitly Antonio's M3 deliverable (his milestone table), and TICKET never mentions it — its absence here is in-scope-correct. The half of the contract this branch *does* own is honored: `call_id` minting is sink-independent. But note the interaction with §4.1: until his switch exists, nothing can turn the per-call yield off. The PR description should state the dependency explicitly.

### Tests/benchmarks to add

- **T27 — call-heavy criterion bench (`crates/bex_engine/benches/` or `bex_vm`).** Antonio §12 is explicit: "validated with a *call-heavy* benchmark (the existing call-light VM benches won't catch per-call cost)." A tight loop of ~1M trivial nested bytecode calls, three configs: (a) canary baseline, (b) this branch sink-less, (c) this branch with a null sink. Record results in the PR. Wire it so a future "gate the yield" change has a number to validate against. This is the instrument whose absence let a 100-1000× per-call regression land unflagged through a green functional suite.
- **T28 — `sinkless_engine_still_mints_correct_ids`.** `event_sink = None`; run nested + spawn program returning `$id`s from several depths; decode and assert exact `(thread_id, call_id)` values. Currently only *implicitly* covered. This is the test that protects `$id`-with-tracing-off when the gating work in §4.1 happens — the highest-risk regression of that refactor.
- **T29 — `function_id_matches_pool_index` (after the §4.3 fix).** For every `CallFunction` captured in a nested program, assert `function_id` equals the table row whose FQN is the actual callee — i.e., resolution by identity, not by name. Plus the display-name-collision case from T12.
- **T30 — laziness guard (after §4.2).** Unit-test `current_bex_identity_for_state`'s replacement: a run that never reads `$id` performs zero `CallRef::encode` calls (countable via a test-only counter or by asserting the identity tuple type carries no String). Coarse but pins the 11.4 rule which today has no possible test.

---

## 5. Interim sink / transport findings (`bex_events_native`)

These are all bounded by "the JSONL sink is interim transport that Antonio's ring+consumer replaces" — but his consumer work will *read these files in the meantime*, so the contract still matters.

### 5.1 (MAJOR) Multi-engine traces are unattributable in one JSONL file

**Where:** `crates/bex_events_native/src/lib.rs:247-272`; `bex_engine/src/lib.rs` per-engine `next_thread_id: AtomicU64::new(1)`; LSP and `bridge_cffi` both share one sink/file for all engines.

The header-only scoping rule (engine_id never repeated per event) is justified in Antonio's design by per-engine files ("a file is already engine-scoped"). The interim writer breaks the premise: one `NativeEventSink` (one `BAML_TRACE_FILE`) is shared by every engine in the process; each engine appends its own `bex_header_v1` and restarts `thread_id` at 1. After the second header, `bex_call_function {thread_id:1, call_id:1}` is ambiguous between engines — **colliding identities in the artifact**, which is materially worse than "interim encoding." Additionally `writeln!` on a raw `File` issues separate write(2) calls for line and newline, so concurrent writers can interleave mid-line and corrupt the JSONL.

**Fix (pick one):** put `engine_id` on each interim JSONL line (cheap; delete when the ring lands), or scope the file path per engine (`<file>.<engine_id>`), or make `thread_id` allocation process-global. And buffer through a single writer (`BufWriter` + one `write_all` per line) or a file lock.

### 5.2 (MINOR) The header rides the same lossy `try_send` as events

**Where:** `crates/bex_events_native/src/lib.rs:83-91` vs channel `sync_channel(4096)` at lib.rs:118.

If the channel is saturated when `BexEngine::new` emits the header (realistic under the LSP's shared sink, given every call now produces 2+ events), the header — carrying the **function table needed to interpret every later line** — is silently dropped while later events persist. Only an aggregate "dropped" counter hints at it.

**Fix.** Headers are once-per-engine and load-bearing: use a blocking `send` (or bounded retry) for `PublisherMessage::Event(BufferedEvent::Header(…))`, or reserve capacity for control messages, or at minimum log a loud, header-specific error.

### 5.3 (MINOR) The sink silently drops disk events at 4096 backlog — interim G3 violation

**Where:** lib.rs:73-80.

`try_send` + dropped-counter for ordinary disk events too. Antonio G3 is *lossless-by-growth*; the never-blocks half is honored, the never-drops half is not. Dropped `CallFunction`/`EndFunction`s also produce unbalanced traces (§2.4's panic again, from a different direction). Acceptable interim **only if stated**: document in the sink that the JSONL artifact may be lossy until the ring lands — or buffer unboundedly for structural events (Vec + mutex swap), which matches his lossless-by-growth shape in spirit.

### Tests to add

- **T31 — `end_to_end_jsonl_file_is_consumer_parseable`** *(the author's own follow-up #5, made concrete)*. Run a program with nested calls + one spawn through a real `NativeEventSink` to a temp file. Then act as the consumer: parse every line with serde_json; assert (a) line 1 is `bex_header_v1`; (b) every `bex_call_function.function_id` resolves in the parsed header's `function_table` and the resolved FQN equals the expected callee; (c) per-thread `Call/End` balance and ordering; (d) the spawn edge fields on `bex_start_thread`; (e) every line is individually valid JSON (catches interleaving corruption). **This single test is the closest thing to running Antonio's consumer in your CI** — it exercises serializer key names, file ordering, header completeness, and the join, all at once.
- **T32 — `two_engines_one_file_is_attributable`** (after the §5.1 fix). Two `BexEngine`s sharing one sink, one call each; assert each event line can be attributed to the right engine (via per-line engine_id or per-engine files — whichever you pick).
- **T33 — JSONL shape tests for the missing 5 of 6 variants.** Today only `SetId` and the header have serializer tests; `StartThread`, `CallFunction`, `EndFunction`, `EndThread`, `Heartbeat` have none — **a key typo ships silently to Antonio's interim consumer.** One test per variant asserting the exact key set and `type` string (`bex_call_function`, `parent_call_id`, …). These are the cheapest contract pins in the whole plan: pure unit tests, no engine needed. Also pin the status strings (`"ok"`, `"error"`, `"cancelled"`, `"completed"`) — they are wire contract now.
- **T34 — `header_is_never_dropped`** (after §5.2): saturate the channel with junk events from a test thread, construct an engine, assert the file contains the header.

---

## 6. Smaller contract items (each one sentence + test pointer)

- **Status enum naming:** `ThreadEndStatus::Error` vs the agreed `Errored`; `FunctionEndStatus` grows `Cancelled` ahead of contract (protobuf-compatible, but tell Antonio so his .proto matches). *Test:* T33 pins the wire strings; the rename, if made, happens before those tests freeze it.
- **`RuntimeEventIdentity` redundancy:** it carries `thread_id`/`call_id` **and** a full `call_ref` (which embeds them plus process/engine), and the JSONL/proto emit both with no enforced invariant — three parallel encodings of one identity, with a per-event base64 encode. Consider dropping the embedded `call_ref` from the wire (consumers can derive it from header + ids, which is Antonio's whole header-only design) or asserting the invariant at construction. *Test:* if kept, an `event_encode` test asserting `call_ref.thread_id == thread_id && call_ref.call_id == call_id` for every encoded event.
- **`baml.id.current()` returns `""` with no active identity** (`$init`, onionskin) while `set()` errors — inconsistent, and an empty string flows silently into logs/concatenations. Make `current()` mirror `set()`'s error, or document the sentinel. *Test:* T25(c).
- **`emit_runtime_events` asymmetry:** `emit_function_end_events_with_status` honors the flag; `emit_completed_top_function_end` and the root-completion block don't — after a §2.1 desync, the latter two leak legacy events for spans never announced to the legacy stream. Fixed for free by the §7 consolidation. *Test:* T1's traced-frame variant catches the visible symptom.
- **Spawn-test determinism:** the spawn assertions (`start_threads[0]/[1]` indexing) are deterministic only because `#[tokio::test]` defaults to the current-thread runtime; under `flavor = "multi_thread"` they'd race. Pin the flavor explicitly or partition assertions per-thread.

---

## 7. Entropy & maintainability cleanups

None of these block correctness; together they're roughly the "shrink the diff ~20% without losing contract value" set. Ordered by leverage:

1. **Consolidate the three FunctionEnd emission blocks** (`emit_function_end_events_with_status` loop body, `emit_completed_top_function_end`, the inline root-completion block at lib.rs:3460-3500) into one `emit_span_end(&self, call_id, state, span, status, result, error)` — they are ~30-line near-copies that have *already drifted* (the `emit_runtime_events` asymmetry above). The drain becomes a while-pop over the same helper, and the policy gets a single decision point.
2. **Consolidate the spawn path:** `EndThread` is emitted from four copy-pasted sites (cancelled-before-start, `SettledChild`, `RootValue`-invariant, `Err`) — compute one `ThreadEndStatus` and emit once at a single exit point, as `call_function` already does. Extract `start_root_span(thread_id, label, function_id, parent_edges) -> SpanState` shared by `run_entry_point` and the spawn body (today's child-root setup is a parallel re-implementation, the kind that diverges when Antonio's Marker events arrive).
3. **Slim `RuntimeCallNotification`:** `frame_depth` is never read (until §2.1's watermark fix gives it a job — decide which); `FunctionExit::function_name` is looked up, cloned, and discarded on every return. Unit variant + pool index per §4.3.
4. **Derive the serializers:** ~175 hand-rolled lines in `serialize.rs` (`disk_event_to_jsonl`, `event_file_header_to_jsonl`, four enum mappers) mirror every field by hand; serde + serde_json are already dependencies and `BamlMeta` in the same file already derives. `#[derive(Serialize)]` with `#[serde(tag = "type", rename_all = "snake_case")]`-style attributes plus `serialize_with` helpers for base64 byte arrays and the u128-string collapses it to attributes — and T33's shape tests pin the output across the migration. Every future field otherwise gets added twice with no compile-time sync check.
5. **Deduplicate header construction:** `EventFileHeaderV1` is built field-by-field in `BexEngine::new` *and* in `event_file_header_v1()`; construct `Self` first and reuse the method. Consider `EventFileHeaderV1` holding a `ProgramMetadata` internally and flattening at serialization (the flat wire shape is contract; the in-process duplication isn't).
6. **Naming/placement nits:** rename `ids::CallId` → `BexCallId` at the definition (deletes the `CallId as BexCallId` aliases in five+ files and prevents confusion with `sys_types::CallId`, which `RuntimeEvent` *also* carries); move the duplicated `timestamp_epoch_ns` into `bex_events` (§2.5 does this anyway); drop the unused `ThreadRef` re-export from `bex_engine`.
7. **Heuristics to retire on schedule:** `derive_owner_type_definition_key` (capital-letter sniffing on FQN segments) and `derive_lambda_metadata` (`find("<lambda")`) are acknowledged-interim — add `// TODO(bep-053): replace with compiler-owned metadata` markers so they don't quietly become load-bearing. The display-name fallback in `function_id_for_name` should be deleted, not marked (§4.3).
8. **Document the contract types.** `ids.rs` and `metadata.rs` ship ~22 public types with **zero doc comments**, while the semantics live in `TASK/` — which is *untracked* (`?? TASK/`) and won't land with the commit. These are exactly the types Antonio joins against (which `Hash256`? what are the lanes? what distinguishes `ProgramId` from `SourceSnapshotId`?). The same diff documents its `EventSink` methods, so this is inconsistent within itself. Port the load-bearing semantics (the quad scoping rules, the reversibility property, "function_id is not stable across recompiles," the `None`-until-enriched fields) into `///` docs before merge — and either commit the TASK docs or link the thoughts-repo paths from the module docs.

---

## 8. Cross-team contract summary for Antonio

What he can rely on from this branch today, and what to flag in the PR/sync:

**Holds:** event/field shapes (§1); SetId `[u8;16]` round-trip; call_id always minted; spawn edges; header-before-events per engine (when the header isn't dropped, §5.2); `Heartbeat` left to the uploader; `parent_call_id` same-thread rule on the happy path.

**Breaks he'd hit, in the order he'd hit them:** timestamps (§2.5) → orphan EndFunction panics his reconstruction (§2.4) → try/catch corruption (§2.1) → multi-engine file ambiguity (§5.1) → statuses Error-vs-Cancelled/Errored (§2.6, §6).

**Decisions needing his agreement, not just fixes:** the `$id` override surface (call-site vs in-body — §3.6, semantics differ, both docs currently disagree with the code); the VM-owns-call_id seam and the fate of `RuntimeCallNotify` when his ring lands (§4.1); the `FunctionEndStatus::Cancelled` / `Errored` naming (§6); interim JSONL lossiness (§5.3).

---

## 9. The contract test suite — consolidated plan

**Philosophy.** His branch builds on ours. Tests here are the *contract*: every invariant his consumer assumes must fail loudly in this repo's CI if either side changes it. Three tiers:

**Tier A — invariant helpers used by every disk-event test** (cheapest, highest leverage):
- `assert_balanced(&events)` — per-call_id `(CallFunction, EndFunction)` counts are exactly `(1,1)`, Call precedes End (T11).
- `assert_thread_closed(&events, thread_id)` — exactly one StartThread/EndThread pair, End last.
- Retrofit these into the **existing** loose tests: today, outside the one exact-sequence root-lifecycle test, assertions are `any()`/`find_map` — a duplicated EndFunction, a missing EndFunction for call 1, or End-before-Call ordering all pass silently. Upgrade the nested-call, spawn, and both SetId tests to full ordered-vector or count+order assertions.

**Tier B — the missing behavioral tests**, priority order:
1. T1/T2/T3 — caught-exception balance + `$id` (catches the critical bug; not even on the original follow-up list)
2. T4/T5/T6/T7 — override persistence, scoping, SetId adjacency (T4/T5 already written and validated by the repro agent)
3. T16-T20 — every non-Ok status path: root cancel, child cancel, root/child error, sys.exit, early-yield equivalence
4. T8/T9 — watch-filter regression
5. T10/T12 — call_callable + unresolved-name balance and sentinel
6. T13/T14/T15 — timestamp semantics & composition
7. T28 — sink-less `$id` correctness (explicit, pre-refactor)
8. T31 — end-to-end JSONL consumer test
9. T21/T22 — child engine-error drain; dropped-future policy
10. **Two-engine scoping (TICKET §11.2's own row, currently uncovered):** two `BexEngine`s in one process; assert distinct `engine_id()`s and that thread 1/call 1 in each produce **distinct encoded CallRefs** — the actual G3 collision-avoidance mechanism (`NEXT_ENGINE_ID`) has no test.
11. Decode-edge completion in `ids.rs` unit tests: ThreadRef malformed/version-mismatch (only CallRef is covered), truncated-payload `InvalidLength`, override wrong-length payload, and `RuntimeId::decode(thread_ref.encode()) == Err(InvalidPrefix)` (cross-type prefix).

**Tier C — compiler, serializer, and perf pins:** T23-T26 (compiler layers), T33 (per-variant JSONL shapes + status strings), T29/T30 (function-id identity + CallRef laziness, post-refactor), T27 (call-heavy bench), T32/T34 (sink fixes).

**Coverage map vs TICKET §11, after this plan:** 11.1 encoding — complete (today: CallRef-only on malformed/version). 11.2 uniqueness — complete (today: missing the two-engine integration row). 11.3 parent edges — already complete; T1 adds the adversarial case. 11.4 `$id` — complete including the two currently-violated rows ("minted when tracing disabled" → T28 explicit; "not minted as string unless read" → T30). 11.5 metadata — T29 + T31(b) close the emitted-id→table join from a *written file*; lambda-path rows remain follow-up with the compiler-owned metadata. 11.6 hash join — covered at unit level today; unchanged.

---

## 10. Prioritized actions before merge

1. **Fix §2.1** (unwind resync) with T1-T3 — the merge blocker.
2. **Fix §2.2** (override on `EngineSpan`) with T4-T7 — T4/T5 already exist from the repro.
3. **Fix §2.3** (interrupt runner) with T8 — straight regression vs canary.
4. **Fix §2.4** (sentinel function_id; `call_callable` identity) with T10-T12.
5. **Fix §2.5** (monotonic clock) with T13-T15 — smallest diff, biggest Antonio impact.
6. **Fix §2.6** (cancel statuses) + §3.3 (throws clause) with T16-T19, T25.
7. **Compiler holes §3.1/§3.2** (+ ideally §3.4's reserved-name diagnostic and the `lower_lvalue` loud-fallback) with T23/T24.
8. **§4.3** (pool index in notifications) — small, deletes per-call scans and clones now, independent of the ring timeline.
9. **Sync with Antonio:** override surface (§3.6), the VM-owns-call_id seam and `RuntimeCallNotify`'s expiration (§4.1), status naming, interim JSONL caveats (§5). Record outcomes in both TASK docs *and* in code docs (§7.8).
10. **Tier A retrofit + T31 + T33** — the standing contract net.
11. Entropy passes (§7) as a follow-up commit — consolidations first (they reduce the surface the fixes above touch), serde derives last (behind T33's pins).

Items 1-6 are independent of each other and of Antonio's timeline; none changes the wire contract except the timestamp fix (which changes values, not shapes — and is the change he needs).

---

*Full machine-readable findings (all 72, with evidence quotes, line numbers, and per-finding adversarial-verifier reasoning) were produced during this review; the condensed form above is authoritative where they differ. Verification stats: 71/72 confirmed; the single refutation (§4.4) is documented above with its reasoning.*
