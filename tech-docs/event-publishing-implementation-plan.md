# Event Publishing Implementation Plan

> Implementation plan for the design in
> [`event-publishing-baml-language.md`](./event-publishing-baml-language.md).

## Key Design Decisions (Updated)

These decisions were refined after reviewing the actual `bex_engine` architecture:

### 1. All event values use `BexExternalValue` (no `serde_json::Value`)

Every event payload — function start args, function end results, LLM request
params, stream update values — uses `BexExternalValue`. This is the canonical
external value type in `baml_language/`. No `serde_json::Value` anywhere in
the event types.

### 2. Channel-based delivery (not callbacks)

Events flow through `tokio::sync::mpsc::UnboundedSender<RuntimeEvent>`. The
caller creates a `(tx, rx)` pair and passes `tx` into `engine.call_function()`.
The engine writes events to the channel; the caller reads from the receiver.

**Why**: Callbacks create lifetime hell, require `block_in_place` for CFFI,
and couple producer to consumer. Channels are composable, async-friendly,
and naturally decouple production from consumption.

### 3. `baml.events.send()` builtin (not special opcodes)

Instead of `LlmFunctionEnter`/`LlmFunctionExit` opcodes, the compiler inserts
calls to a `baml.events.send(event_type, payload)` builtin. This is implemented
as `SysOp::EventSend` in the engine — it writes to the channel and returns
`Null` immediately (fire-and-forget). The existing LLM call sequence
(`get_jinja_template` → `build_client` → `render_prompt` → `http.send` →
`parse`) is already orchestrated through builtins/SysOps, so event emission
naturally slots in as additional `baml.events.send()` calls around those steps.

**Why**: No new opcodes, uses existing infra, the compiler controls placement,
and it's trivially testable with the existing VM test helpers.

### 4. `VmExecState::Event` still needed for watch yields

Watch variable events (`Watch(VariableChanged)`) still use
`VmExecState::Event(RuntimeEvent)` because they need **synchronous yield**
semantics — the VM pauses so the host language can process the event before
the next instruction. The engine writes these to the same channel.

---

## Current State (What Exists Today)

| Component | Status | Notes |
|---|---|---|
| `baml_events` crate | **Does not exist** | Needs to be created from scratch |
| `bex_vm` `VmExecState` | Has `Notify(WatchNotification)` | Needs `Event(RuntimeEvent)` variant |
| `bex_engine` Notify handling | **Ignored** (`// Ignore watch notifications for now`) | Needs event channel dispatch |
| `bex_vm` Viz opcodes | `VizEnter`/`VizExit` exist with `HeaderContextEnter` | Map to `Header(Enter/Exit)` events |
| `llm_ops` crate | Exists — builds clients, renders prompts, parses responses | No streaming support yet |
| `sys_native` crate | Exists — HTTP via `reqwest`, file I/O, shell | `http_fetch` is non-streaming |
| `bridge_cffi` streaming | **Placeholder** ("Streaming not implemented") | Needs full implementation |
| `bridge_cffi` encoding | `external_to_cffi_value()` works for final values | No streaming/partial encoding |
| `BexStreamValue` | **Does not exist** | Needs to be created |
| `StreamState` (VM) | **Does not exist** | Needs to be created |
| Partial JSON parsing | **Does not exist** in `baml_language/` | Exists in `engine/baml-lib/jsonish/` |
| `baml_builtins` infra | Rich — `define_builtins!` macro, `NativeFunctions` trait | Easy to add `baml.events.send` |
| `baml_tests` infra | Rich — snapshot tests, VM helpers, engine helpers | Good foundation for new tests |
| LLM call sequence | Orchestrated via SysOps: `LlmGetJinjaTemplate` → ... → `HttpSend` → `LlmParseResponse` | Event sends slot in between these |

---

## Guiding Principles

1. **Each phase must be independently testable.** No phase should require a
   subsequent phase to verify correctness.
2. **Test at the lowest level first.** Rust unit tests in the crate being
   changed, before integration tests.
3. **Avoid big-bang integration.** Wire up one event type end-to-end before
   adding the rest.
4. **Keep `engine/` working.** Nothing in these phases should break the
   existing `engine/` pipeline. The two systems are independent.
5. **Use feature flags for incomplete work.** If a phase spans multiple PRs,
   gate new behavior behind `#[cfg(feature = "events")]` or similar until
   the full phase is done.

---

## Phase 0: Foundation — `baml_events` Crate

**Goal**: Create the core event types that every subsequent phase depends on.
This phase has zero runtime behavior — it's pure types. No dispatcher, no
sinks — just the `RuntimeEvent` enum and its children.

**Crate**: `baml_language/crates/baml_events/` (new)

### Tasks

1. **Create `baml_events` crate** with `Cargo.toml`.
   - Dependencies: `bex_external_types` (for `BexExternalValue`, `Ty`),
     `indexmap`, `web_time` (for WASM-compat timestamps).
   - No dependency on `bex_vm`, `bex_engine`, or any compiler crate. This crate
     is a leaf dependency (alongside `bex_external_types`).

2. **Define core types** (`src/types.rs`):
   - `RuntimeEvent` enum — all variants from the design doc:
     `Function`, `Header`, `Stream`, `Llm`, `Watch`, `Block`, `Viz`, `Tags`
   - `EventMeta` — call stack, timestamp, event ID.
   - Individual event structs: `FunctionEvent`, `HeaderEvent`, `StreamEvent`,
     `LlmEvent`, `WatchEvent`, `StreamEventData`, etc.
   - **All payloads use `BexExternalValue`** — no `serde_json::Value`.
   - Start with `Clone + Debug` derives. Add `Serialize` later.

3. **Define `RuntimeEvent::from_raw(event_type: &str, payload: BexExternalValue)`**:
   - A constructor that the `SysOp::EventSend` handler will call.
   - Maps string event types to enum variants.
   - This is how `baml.events.send("function_start", { ... })` becomes
     `RuntimeEvent::Function(FunctionEvent::Start { ... })`.

4. **Implement `EventCollector`** for testing:
   - Simple `Arc<Mutex<Vec<RuntimeEvent>>>` that can be used in tests.
   - Wrapped behind a helper function: `collect_events() -> (Sender, EventCollector)`.

### Tests

```
baml_events/src/
├── types.rs          # RuntimeEvent, EventMeta, BexExternalValue payloads
├── lib.rs            # Re-exports
└── tests/
    └── types_test.rs # Construction, from_raw, Clone
```

- **`types_test.rs`**: Construct each event variant, assert Debug output.
  Test `from_raw()` round-trip for each event type string.
- **`EventCollector` test**: Create collector, push events, assert retrieval.
- `cargo test -p baml_events`

### Exit Criteria

- `baml_events` compiles on all targets (native, wasm32).
- All unit tests pass.
- `RuntimeEvent` enum has all variants with `BexExternalValue` payloads.
- `from_raw()` handles all event type strings.

### Estimated Effort: Small (1–2 days)

---

## Phase 1: VM Event Yield + Engine Channel + `SysOp::EventSend`

**Goal**: Three things in one phase (they're tightly coupled):
1. Replace `VmExecState::Notify(WatchNotification)` with `VmExecState::Event(RuntimeEvent)`.
2. Add `event_tx: Option<mpsc::UnboundedSender<RuntimeEvent>>` to the engine,
   update `call_function()` to accept a channel sender.
3. Add `SysOp::EventSend` and the `baml.events.send()` builtin.

**Crates touched**: `bex_vm_types`, `bex_vm`, `bex_engine`, `baml_events`,
`baml_builtins`, `baml_compiler_emit`

### Tasks

1. **Add `baml_events` dependency** to `bex_vm_types` and `bex_vm`.

2. **Change `VmExecState`** in `bex_vm/src/vm.rs`:
   ```rust
   pub enum VmExecState {
       Await(HeapPtr),
       ScheduleFuture(HeapPtr),
       Complete(Value),
       Event(RuntimeEvent),      // ← replaces Notify(WatchNotification)
   }
   ```

3. **Migrate existing `Notify` call sites** in `bex_vm`:
   - Convert `WatchNotification::Variables(ids)` →
     `VmExecState::Event(RuntimeEvent::Watch(WatchEvent::VariableChanged { ... }))`.
   - Convert `WatchNotification::Block(...)` →
     `VmExecState::Event(RuntimeEvent::Block(...))`.
   - Convert `WatchNotification::Viz { ... }` →
     `VmExecState::Event(RuntimeEvent::Header(...))` for header viz events.

4. **Add channel to `BexEngine`**:
   - `call_function()` gets a new parameter:
     `event_tx: Option<mpsc::UnboundedSender<RuntimeEvent>>`.
   - Store it in the engine for the duration of the call.
   - In the event loop, when `VmExecState::Event(event)`:
     ```rust
     VmExecState::Event(event) => {
         if let Some(tx) = &event_tx {
             let _ = tx.send(event);
         }
         // resume VM on next iteration
     }
     ```

5. **Add `SysOp::EventSend`**:
   - In `bex_vm_types/src/types.rs`: add `EventSend` variant to `SysOp`.
   - In `bex_engine/src/lib.rs`, handle it:
     ```rust
     SysOp::EventSend => {
         // args[0] = event type string, args[1] = payload
         let event_type = args[0].as_str()?;
         let payload = args[1].clone();
         let event = RuntimeEvent::from_raw(event_type, payload);
         if let Some(tx) = &event_tx {
             let _ = tx.send(event);
         }
         SysOpResult::Ready(Ok(BexExternalValue::Null))
     }
     ```

6. **Add `baml.events.send()` builtin**:
   - In `baml_builtins/src/lib.rs` inside `with_builtins!`:
     ```
     mod events {
         fn send(event_type: String, payload: any) -> null;
     }
     ```
   - In `baml_compiler_emit/src/lib.rs`, map the path:
     `"baml.events.send" => Some(SysOp::EventSend)`
   - In `bex_vm/src/native.rs`, the native impl is a no-op (SysOp handles it).

7. **Remove `WatchNotification` enum**.

### Tests

- **VM unit tests** (`bex_vm/tests/watch.rs`):
  - Existing watch tests should still pass. Update `assert_vm_emits()` helper
    to check for `RuntimeEvent::Watch(...)` instead of `WatchNotification`.

- **New VM test** (`bex_vm/tests/events.rs`):
  - Compile a program with `watch let x = 0; x = 1; x = 2`.
  - Step the VM, collect all `VmExecState::Event(...)` yields.
  - Assert exactly 1 `Watch(Registered)` + 2 `Watch(VariableChanged)`.

- **Engine channel test** (`bex_engine/tests/events.rs`):
  - Create `(tx, rx)` channel, pass `tx` to `call_function()`.
  - Execute a program that emits watch events.
  - Collect from `rx`, assert expected events.

- **`baml.events.send()` test** (`bex_engine/tests/event_send.rs`):
  - Write a BAML program that calls `baml.events.send("function_start", { name: "test" })`.
  - Execute through engine with channel.
  - Assert `RuntimeEvent::Function(Start { name: "test" })` received on `rx`.

- `cargo test -p bex_vm -p bex_engine -p baml_events -p baml_builtins`

### Risk: Breaking existing watch tests

The `assert_vm_emits()` helper in `baml_tests/src/vm.rs` currently checks
for `WatchNotification`. This must be updated to check for `RuntimeEvent`.
Since `baml_tests` is only used by tests, this is safe.

### Exit Criteria

- `WatchNotification` is removed.
- All existing `bex_vm` watch tests pass with the new `Event` variant.
- Engine writes events to channel correctly.
- `baml.events.send()` works end-to-end: BAML source → compile → VM → engine → channel.
- `cargo test` passes for all crates in `baml_language/`.

### Estimated Effort: Medium (3–4 days)

---

## Phase 2: Header Events

**Goal**: Emit `RuntimeEvent::Header(Enter/Exit)` from the VM when it
executes `VizEnter`/`VizExit` opcodes for header contexts.

**Crates touched**: `bex_vm`, `baml_events`

### Tasks

1. **Map existing `VizEnter`/`VizExit` with `HeaderContextEnter`** to emit
   `RuntimeEvent::Header(HeaderEvent { data: Enter { ... } })`.

2. **Ensure proper nesting**: The VM already tracks viz node state. Map the
   viz `header_level` to the `HeaderEvent`'s `level` field.

3. **Emit `Header(Exit)` on scope exit** (when `VizExit` opcode fires).

### Tests

- **VM test** (`bex_vm/tests/headers.rs`):
  - Write a BAML program with `//# header` annotations.
  - Step VM, collect `Header(Enter)` and `Header(Exit)` events.
  - Assert correct nesting: every `Enter` has a matching `Exit`.
  - Assert `level` values are correct for nested headers.

- **Engine test** (`bex_engine/tests/headers.rs`):
  - Execute a program with headers through the engine.
  - Collect events via `CollectorSink`.
  - Assert the event sequence matches expectations.

- **Snapshot test**: Add a BAML test project in `baml_tests/projects/` that
  exercises headers, snapshot the emitted events.

### Exit Criteria

- Header enter/exit events fire for every `//# header` annotation.
- Nesting is correct (including when headers are inside function calls).
- Events appear in `CollectorSink` with correct metadata.

### Estimated Effort: Small (1–2 days)

---

## Phase 3: Streaming HTTP in `sys_native`

**Goal**: Add streaming HTTP support to `sys_native` so the VM can receive
SSE chunks incrementally instead of waiting for the full response.

**Crates touched**: `sys_native`, `sys_types`, `bex_vm_types`

### Tasks

1. **Add `http_stream` sys op** to `sys_types`:
   - New op that initiates an HTTP request and returns a stream handle.
   - Subsequent `http_stream_next` calls return the next SSE chunk or `None`
     when the stream ends.

2. **Implement in `sys_native`**:
   - Use `reqwest::Response::chunk()` or `bytes_stream()` for streaming.
   - Store the ongoing `reqwest::Response` in `ResourceRegistry`.
   - Each `http_stream_next` call polls for the next chunk.

3. **Add SSE parsing**: Parse `data:` lines from the raw byte stream into
   text chunks (or use an SSE parsing library).

### Tests

- **Unit test** (`sys_native/tests/http_stream.rs`):
  - Start a local HTTP server (using `axum` or `wiremock` in dev-deps) that
    serves SSE events with known content.
  - Call `http_stream` + `http_stream_next` in a loop.
  - Assert all chunks are received in order.
  - Assert stream termination (returns `None`).

- **Timeout test**: Server sends first chunk after 100ms, then idles.
  Assert timeout behavior if configured.

- **Error test**: Server returns 500. Assert clean error propagation.

### Exit Criteria

- `sys_native` can stream HTTP responses chunk-by-chunk.
- SSE parsing extracts `data:` content correctly.
- Resource cleanup on stream completion/error.

### Estimated Effort: Medium (2–3 days)

---

## Phase 4: `StreamState` and `BexStreamValue`

**Goal**: Add the in-VM streaming state machine and the typed partial value
type that carries completion metadata.

**Crates touched**: `bex_vm` (new `stream_state.rs`), `bex_external_types`
(new `stream_value.rs`), `baml_events`

### Tasks

1. **Create `BexStreamValue`** in `bex_external_types`:
   - `value: BexExternalValue` — the partial value tree.
   - `completions: IndexMap<String, StreamingCompletion>` — per-field state.
   - `return_type: Ty` — declared return type (for encoding).
   - `StreamingCompletion` enum: `Pending`, `Started`, `Done`.

2. **Create `StreamState`** in `bex_vm/src/stream_state.rs`:
   - `accumulated_text: String`
   - `last_emitted_serialized: Option<String>` (dedup)
   - `last_parse_time: Instant` (throttle)
   - `parse_interval: Duration` (default 50ms)
   - Methods: `push_chunk()`, `should_parse()`, `should_emit()`.

3. **Update `StreamEventData::Update`** to carry `BexStreamValue` instead
   of raw `serde_json::Value`.

### Tests

- **`BexStreamValue` unit tests** (`bex_external_types/tests/stream_value.rs`):
  - Create a `BexStreamValue` with some fields `Done`, some `Pending`.
  - Assert `is_complete()` returns `false`.
  - Set all to `Done`, assert `is_complete()` returns `true`.
  - Test `from_final()` constructor.

- **`StreamState` unit tests** (`bex_vm/tests/stream_state.rs`):
  - Push chunks, verify `accumulated_text` grows.
  - Test `should_parse()` respects the 50ms interval.
  - Test `should_emit()` dedup: same string → false, different → true.
  - Test rapid pushes: only first parse within interval returns true.

### Exit Criteria

- `BexStreamValue` and `StreamingCompletion` compile and have tests.
- `StreamState` manages accumulation/throttle/dedup correctly.
- `StreamEventData::Update` carries `BexStreamValue`.

### Estimated Effort: Small (1–2 days)

---

## Phase 5: Partial JSON Parsing

**Goal**: Port or re-implement partial JSON parsing for the new compiler.
This is the component that takes incomplete LLM output like
`{"name": "Joh` and produces a typed `BexExternalValue` with completion
states for each field.

**Crates touched**: New crate or module (e.g., `baml_language/crates/jsonish_lite/`
or module inside `llm_ops`)

### Tasks

1. **Decide: port `engine/baml-lib/jsonish/` or write new?**
   - **Recommendation**: Write a lighter version. The existing `jsonish` is
     tightly coupled to `ResponseBamlValue`, `TypeIR`, and the old type system.
   - The new parser needs to produce `BexExternalValue` + `StreamingCompletion`
     map, not `BamlValueWithMeta<ResponseValueMeta>`.

2. **Implement `parse_partial(text: &str, return_type: &Ty) → Result<(BexExternalValue, CompletionMap)>`**:
   - Use `serde_json` to attempt parsing. On failure, try heuristic fixups
     (close open braces/brackets/strings).
   - Walk the parsed value tree against `return_type` to determine which
     fields are `Done` vs `Started` vs `Pending`.

3. **Implement `parse_final(text: &str, return_type: &Ty) → Result<BexExternalValue>`**:
   - Strict parsing, no fixups. Returns error if JSON is invalid.

### Tests

- **Complete JSON**: Parse `{"name": "John", "age": 30}` against
  `class Person { name string, age int }`. Assert all fields `Done`.

- **Truncated string field**: Parse `{"name": "Joh`. Assert `name` is
  `Started` with value `"Joh"`, `age` is `Pending`.

- **Truncated object**: Parse `{"name": "John"`. Assert `name` is `Done`,
  `age` is `Pending`.

- **Truncated array**: Parse `{"items": [1, 2`. Assert `items` is `Started`.

- **Nested objects**: Parse partial nested class. Assert inner fields have
  correct completion states.

- **Empty input**: Parse `""`. Assert all fields `Pending`.

- **Final parse**: Assert `parse_final` rejects truncated input.

- **Fuzz test (optional)**: Generate random truncation points in valid JSON.
  Assert `parse_partial` never panics.

### Exit Criteria

- Partial parser produces correct `BexExternalValue` + completion map for
  common LLM output patterns.
- Final parser rejects incomplete input.
- All tests pass.

### Estimated Effort: Medium-Large (3–5 days)

This is the highest-risk phase. Partial parsing is subtle. Consider
starting with a simple version (JSON fixup heuristics) and iterating.

---

## Phase 6: VM Streaming Execution

**Goal**: Wire up the full streaming path inside the VM: receive SSE chunks
from `sys_native`, accumulate in `StreamState`, throttle-parse into
`BexStreamValue`, yield `VmExecState::Event(Stream(Update))`.

**Crates touched**: `bex_vm`, `llm_ops`, `bex_engine`

### Tasks

1. **Add streaming LLM call path in `llm_ops`**:
   - `execute_llm_stream()` that initiates an HTTP stream and returns a
     stream handle to the VM.
   - The VM then enters a loop: `http_stream_next` → `push_chunk` →
     maybe parse → maybe yield `Event(Stream(Update))`.

2. **Implement `handle_stream_chunk` in VM** (from design doc):
   - On each chunk: accumulate → throttle check → partial parse → dedup →
     yield `Event(Stream(Update { value: BexStreamValue }))`.

3. **Implement `handle_stream_end` in VM**:
   - Final parse → yield `Event(Stream(End { final_value }))`.

4. **Emit `Stream(Start)` on first successful parse**.

5. **Wire up in `bex_engine`**: The engine already handles
   `VmExecState::Event(...)` (from Phase 1). Stream events flow through
   the same path.

### Tests

- **Mock streaming test** (`bex_engine/tests/streaming.rs`):
  - Set up a local HTTP server that sends 5 SSE chunks for a simple class:
    `{"na` → `me": "Jo` → `hn", "a` → `ge": 3` → `0}`
  - Execute an LLM function through the engine.
  - Collect events via `CollectorSink`.
  - Assert: 1 `Stream(Start)`, N `Stream(Update)` with progressive
    completions, 1 `Stream(End)`.
  - Assert: final value is `{ name: "John", age: 30 }`.

- **Dedup test**: Send identical chunks. Assert no duplicate `Update` events.

- **Throttle test**: Send 100 chunks in < 50ms. Assert fewer than 100
  `Update` events (throttle kicked in).

- **Error recovery test**: Stream fails mid-way. Assert `Stream(End)` is
  emitted with error.

### Exit Criteria

- A BAML LLM function can be streamed through `bex_engine`.
- `Stream(Start/Update/End)` events fire correctly.
- Dedup and throttle work.
- Final value is correct.

### Estimated Effort: Large (4–6 days)

---

## Phase 7: `bridge_cffi` Streaming

**Goal**: Implement `call_function_stream_from_c()` in `bridge_cffi` so
host languages (Python, TS, Ruby, Go) can consume typed stream values via
the C FFI.

**Crates touched**: `bridge_cffi`

### Tasks

1. **Add `stream_value_to_cffi()`** in `bridge_cffi/src/ctypes/value_encode.rs`:
   - Walks `BexStreamValue` tree.
   - Wraps incomplete fields in `CffiValueStreamingState` protobuf.
   - Complete fields encode via existing `external_to_cffi_value()`.

2. **Implement `call_function_stream_from_c()`**:
   - Parse args (same as `call_function_from_c`).
   - Create `(event_tx, event_rx)` channel.
   - Spawn two tasks:
     - **Execution task**: `engine.call_function(name, args, Some(event_tx))`.
     - **Event reader task**: reads from `event_rx`, encodes `Stream(Update)`
       events to protobuf via `stream_value_to_cffi()`, calls C callback with
       `is_done=0`. On `Stream(End)`, sends final via callback with `is_done=1`.
   - Trigger `on_tick` callback periodically.

3. **Implement cancellation** (`cancel_function_call`):
   - Add a cancellation mechanism (e.g., `CancellationToken` or tripwire)
     to `BexEngine`.

### Tests

Testing CFFI is harder because it crosses language boundaries. Strategy:

- **Rust-level round-trip test** (`bridge_cffi/tests/stream_encode.rs`):
  - Create a `BexStreamValue` with known fields.
  - Call `stream_value_to_cffi()`.
  - Decode the protobuf bytes back.
  - Assert the `CffiValueStreamingState` wrapper is present for incomplete
    fields and absent for complete fields.

- **Rust-level callback mock test**:
  - Set up a mock callback that captures all calls.
  - Call `call_function_stream_from_c` with a real engine and mock HTTP server.
  - Assert callback was called N times with `is_done=0` and once with `is_done=1`.
  - Assert the protobuf bytes decode correctly.

- **Python integration test** (deferred to Phase 8, but can start here):
  - Write a simple BAML file with an LLM function.
  - Call via Python using the generated client.
  - `async for partial in stream`: assert partials have expected shape.

### Exit Criteria

- `call_function_stream_from_c()` works end-to-end from Rust.
- Protobuf encoding of streaming state is correct.
- Callback invocation pattern matches `engine/` behavior (partials with
  `is_done=0`, final with `is_done=1`).

### Estimated Effort: Medium (3–4 days)

---

## Phase 8: End-to-End Python Integration

**Goal**: Verify the full path from Python user code through `bridge_cffi`
to the VM and back. This is the "it works for real" phase.

**Crates/files touched**: `integ-tests/`, possibly `baml_codegen_python/`

### Tasks

1. **Write a BAML test file** (`integ-tests/baml_src/test-files/vm/streaming.baml`):
   ```
   client<llm> TestClient { ... }

   class StreamTestOutput {
     name string
     age int
     bio string
   }

   function StreamTest(input: string) -> StreamTestOutput {
     client TestClient
     prompt #"Extract info: {{ input }}"#
   }
   ```

2. **Generate Python client**: `uv run baml-cli generate --from ../baml_src`

3. **Write Python test** (`integ-tests/python/tests/test_vm_streaming.py`):
   ```python
   import pytest
   from baml_client import b
   from baml_client.types import StreamTestOutput

   @pytest.mark.asyncio
   async def test_stream_basic():
       """Stream a function and verify partial types."""
       partials = []
       stream = b.stream.StreamTest("John is 30 and loves coding")
       async for partial in stream:
           partials.append(partial)
           # Partial should have Optional fields
           assert hasattr(partial, 'name')
           assert hasattr(partial, 'age')
           assert hasattr(partial, 'bio')

       # At least one partial before final
       assert len(partials) >= 1

       final = await stream.get_final_response()
       assert isinstance(final, StreamTestOutput)
       assert final.name is not None
       assert final.age is not None

   @pytest.mark.asyncio
   async def test_stream_progressive():
       """Verify partials get more complete over time."""
       field_counts = []
       stream = b.stream.StreamTest("Alice is 25")
       async for partial in stream:
           count = sum(1 for f in [partial.name, partial.age, partial.bio] if f is not None)
           field_counts.append(count)

       # Later partials should have >= fields as earlier ones
       for i in range(1, len(field_counts)):
           assert field_counts[i] >= field_counts[i-1], \
               f"Partial {i} had fewer fields than partial {i-1}"
   ```

4. **Run tests**:
   ```bash
   cd integ-tests/python
   uv run maturin develop --manifest-path ../../baml_language/crates/bridge_cffi/Cargo.toml
   uv run baml-cli generate --from ../baml_src
   uv run pytest tests/test_vm_streaming.py -v
   ```

### Tests

- **Basic stream**: Function returns, partials are yielded, final is correct.
- **Progressive completeness**: Later partials have more fields filled.
- **Error handling**: Invalid LLM response → stream yields error.
- **Cancellation**: Cancel mid-stream, assert clean teardown.
- **Multiple concurrent streams**: Run 3 streams in parallel, all complete.

### Exit Criteria

- Python `async for partial in stream` works end-to-end.
- `get_final_response()` returns correctly typed final value.
- At least 3 test scenarios pass in CI.

### Estimated Effort: Medium (2–3 days)

---

## Phase 9: LLM Function Tracing via `baml.events.send()`

**Goal**: Automatically emit trace events (`Function(Start/End)`,
`Llm(Request/Response)`) for every LLM function call by inserting
`baml.events.send()` calls into the compiled bytecode.

**Crates touched**: `baml_compiler_emit`

### How it works

Today, LLM functions are compiled with `FunctionMeta::Llm { prompt_template, client }`
and the LLM call sequence is orchestrated through a chain of SysOp calls:
`LlmGetJinjaTemplate` → `LlmGetClientFunction` → `RenderPrompt` →
`SpecializePrompt` → `LlmBuildRequest` → `HttpSend` → `LlmParseResponse`.

We insert `baml.events.send()` calls **around** this existing sequence:

```
// Before the LLM call sequence:
baml.events.send("function_start", { name: "MyFunc", args: kwargs })

// Before http.send():
baml.events.send("llm_request", { client: client.name, prompt: ... })

// After http.send(), before parse:
baml.events.send("llm_response", { raw_text: response.text(), ... })

// After parse (or on error):
baml.events.send("function_end", { name: "MyFunc", result: parsed_value })
```

### Tasks

1. **Update `baml_compiler_emit`** to emit `baml.events.send()` bytecode
   around LLM function bodies:
   - When emitting an LLM function body, generate bytecode that calls
     `baml.events.send("function_start", ...)` before the LLM call chain.
   - Generate `baml.events.send("function_end", ...)` after the chain.
   - Generate `baml.events.send("llm_request", ...)` before `http.send()`.
   - Generate `baml.events.send("llm_response", ...)` after `http.send()`.

2. **No new opcodes, no VM changes, no engine changes**.
   Everything works through the existing `SysOp::EventSend` (from Phase 1).

### Tests

- **Trace event test** (`bex_engine/tests/tracing.rs`):
  - Execute an LLM function (with mock HTTP server).
  - Pass channel to `call_function()`.
  - Collect events from channel.
  - Assert event sequence: `Function(Start)` → `Llm(Request)` →
    `Llm(Response)` → `Function(End)`.

- **Nested call test**: Expression function calls LLM function.
  Assert `Function(Start)` for outer, then inner LLM events, then
  `Function(End)` for outer.

- **Snapshot test**: Add to `baml_tests/projects/` — verify emitted
  bytecode includes `baml.events.send()` calls around LLM sequence.

- **No-channel test**: Execute LLM function without passing a channel.
  Assert no errors (events are silently dropped).

### Exit Criteria

- Every LLM function call emits `Function(Start/End)` and `Llm(Request/Response)`.
- Events contain `BexExternalValue` payloads (args, result, prompt, etc.).
- No new opcodes were added.
- Existing tests still pass.

### Estimated Effort: Medium (2–3 days)

This is much simpler than the original plan because we're not adding
opcodes or VM logic — just compiler emission changes.

---

## Phase 10: Watch → Stream Integration

**Goal**: Connect watch variable events with the streaming pipeline. When a
`watch let` variable backs a stream output, changes to that variable should
also produce `Stream(Update)` events.

**Crates touched**: `bex_vm`, `baml_events`

### Tasks

1. **Detect streaming context**: When the VM is executing an LLM function
   with an active `StreamState`, and a `watch` variable maps to the output,
   link them.

2. **Dual emission**: When a watched variable changes:
   - Emit `Watch(VariableChanged)` (always).
   - If the variable backs a stream output, also emit `Stream(Update)` with
     the latest parsed value.

3. **Implement `WatchProjection` sink** (from design doc):
   - A sink that filters `Watch(VariableChanged)` events and routes them
     to per-variable channels.

### Tests

- **Watch + Stream test** (`bex_vm/tests/watch_stream.rs`):
  - Program with `watch let output = ...` that changes multiple times.
  - Assert both `Watch(VariableChanged)` and `Stream(Update)` events fire.

- **WatchProjection test**: Register projection for a specific variable.
  Assert only that variable's events are routed to the channel.

### Exit Criteria

- Watch variable changes in a streaming context produce both event types.
- `WatchProjection` correctly filters and routes events.

### Estimated Effort: Medium (2–3 days)

---

## Phase 11: Boundary Publisher Sink

**Goal**: Implement the `BoundaryPublisher` event sink that batches events
and uploads them to the Boundary API, matching `engine/`'s behavior.

**Crates touched**: `baml_events` (new sink), `bex_engine`

### Tasks

1. **Implement `BoundaryPublisher`** as an `EventSink`:
   - Batches events (configurable batch size).
   - Flushes periodically or when batch is full.
   - Uploads via HTTP to Boundary API.
   - Handles compression for large batches.

2. **Configuration via env vars**: `BOUNDARY_API_URL`, `BOUNDARY_API_KEY`,
   batch size, compression threshold.

3. **Wire up in `bex_engine`**: Auto-register when env vars are set.

### Tests

- **Unit test**: Mock HTTP server, publish 100 events, assert correct
  batch sizes and payloads.
- **Compression test**: Publish a large event, assert compression kicks in.
- **Flush on shutdown**: Publish events, call `shutdown()`, assert all
  pending events are flushed.
- **Error handling**: Mock server returns 500, assert graceful degradation
  (log + drop, don't crash).

### Exit Criteria

- Events are batched and uploaded to Boundary API.
- Configuration matches `engine/` env vars.
- Graceful degradation on API errors.

### Estimated Effort: Medium (3–4 days)

---

## Phase 12: LSP Integration

**Goal**: Implement an `LspNotifier` event sink that sends events to the
LSP server for real-time execution visualization in the IDE.

**Crates touched**: `baml_events`, `baml_lsp_server`

### Tasks

1. **Implement `LspNotifier` sink**: Converts `RuntimeEvent` to LSP
   notifications.
2. **Wire up in playground/preview**: When running a function from the IDE,
   attach the `LspNotifier` sink to see live updates.

### Tests

- Unit tests with mock LSP channel.
- Manual testing in VS Code extension.

### Exit Criteria

- IDE shows real-time header enter/exit, stream updates, and watch changes.

### Estimated Effort: Medium (3–4 days)

---

## Dependency Graph

```
Phase 0: baml_events crate (types only)
    │
    ▼
Phase 1: VmExecState::Event + Engine channel + SysOp::EventSend + baml.events.send()
    │
    ├──► Phase 2: Header Events (from existing VizEnter/VizExit)
    │
    ├──► Phase 9: LLM Function Tracing (compiler inserts baml.events.send() calls)
    │
    ├──► Phase 11: Boundary Publisher (reads from channel, batches, uploads)
    │
    ├──► Phase 12: LSP Integration (reads from channel, sends to IDE)
    │
    ├──► Phase 3: Streaming HTTP (sys_native)
    │       │
    │       ▼
    ├──► Phase 4: StreamState + BexStreamValue
    │       │
    │       ▼
    ├──► Phase 5: Partial JSON Parsing
    │       │
    │       ▼
    ├──► Phase 6: VM Streaming Execution
    │       │
    │       ▼
    ├──► Phase 7: bridge_cffi Streaming (reads channel, encodes to protobuf)
    │       │
    │       ▼
    ├──► Phase 8: E2E Python Integration
    │
    └──► Phase 10: Watch → Stream Integration (after 6)
```

**Critical path**: 0 → 1 → 3 → 4 → 5 → 6 → 7 → 8

**Parallelizable after Phase 1**:
- Phase 2 (Headers) — independent of streaming.
- Phase 9 (LLM Tracing) — just compiler changes, independent of streaming.
- Phase 11 (Publisher) — reads from channel, independent of streaming.
- Phase 12 (LSP) — reads from channel, independent of streaming.
- Phases 3, 4, 5 can partly overlap (different crates, different people).

---

## Total Estimated Effort

| Phase | Effort | Dependencies |
|---|---|---|
| 0: `baml_events` crate (types) | 1–2 days | None |
| 1: VM Event + Channel + EventSend | 3–4 days | Phase 0 |
| 2: Header Events | 1–2 days | Phase 1 |
| 3: Streaming HTTP | 2–3 days | Phase 1 |
| 4: StreamState + BexStreamValue | 1–2 days | Phase 0 |
| 5: Partial JSON Parsing | 3–5 days | Phase 4 |
| 6: VM Streaming Execution | 4–6 days | Phases 3, 4, 5 |
| 7: bridge_cffi Streaming | 3–4 days | Phase 6 |
| 8: E2E Python Integration | 2–3 days | Phase 7 |
| 9: LLM Function Tracing | 2–3 days | Phase 1 (compiler-only) |
| 10: Watch → Stream Integration | 2–3 days | Phase 6 |
| 11: Boundary Publisher | 3–4 days | Phase 1 |
| 12: LSP Integration | 3–4 days | Phase 1 |
| **Total (sequential)** | **~30–45 days** | |
| **Total (with parallelism)** | **~18–25 days** | |

---

## Testing Summary

| Test Type | Where | What |
|---|---|---|
| **Rust unit tests** | `baml_events/tests/` | Event types, dispatcher, sinks |
| **VM unit tests** | `bex_vm/tests/` | Event emission, watch→event, stream state, headers |
| **Engine integration** | `bex_engine/tests/` | Full execution with CollectorSink |
| **CFFI round-trip** | `bridge_cffi/tests/` | Protobuf encode/decode, callback invocation |
| **Snapshot tests** | `baml_tests/projects/` | Bytecode opcodes for trace/header |
| **Python E2E** | `integ-tests/python/tests/` | Real streaming, partial types, final values |
| **Mock HTTP tests** | `bex_engine/tests/`, `sys_native/tests/` | Local HTTP server for deterministic streaming |

**Test commands at each phase**:
```bash
# Per-crate
cargo test -p baml_events
cargo test -p bex_vm
cargo test -p bex_engine
cargo test -p bridge_cffi

# All baml_language crates
cargo test --workspace --manifest-path baml_language/Cargo.toml

# Python E2E
cd integ-tests/python && uv run pytest tests/test_vm_streaming.py -v
```

---

## Risk Register

| Risk | Impact | Mitigation |
|---|---|---|
| Partial JSON parsing is harder than expected | Phase 5 blocks everything after | Start with simple heuristics (close braces), iterate. Can ship a "good enough" version. |
| `sys_native` streaming HTTP is complex | Phase 3 blocks streaming | `reqwest` has good streaming support. Can prototype with `bytes_stream()` quickly. |
| Breaking `bex_vm` watch tests during Phase 1 | Blocks all development | Update `baml_tests` helpers first, then change the enum. Small PR. |
| CFFI protobuf compatibility | Phase 7 may need proto changes | Shared proto already has `CffiValueStreamingState`. Test encoding early. |
| Performance regression from event dispatch | Events add overhead to every VM step | `EventDispatcher` is behind `Option<Arc<...>>` — zero cost when disabled. Benchmark. |
| WASM compatibility | Some phases may break WASM | Use `web_time` instead of `std::time`, avoid `tokio::spawn` in wasm. Test early. |
