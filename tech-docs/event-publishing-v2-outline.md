# Event Publishing System — Design Document v2 (Outline)

> Outline for the v2 design doc. Written as a structured plan with
> progressive milestones that each build on the last.
>
> Reference material:
> - v1 design doc: [`event-publishing-baml-language.md`](./event-publishing-baml-language.md)
> - Implementation plan: [`event-publishing-implementation-plan.md`](./event-publishing-implementation-plan.md)

---

## Part I: Foundations

### 1. Introduction & Motivation

- What we're building: a runtime event system for `baml_language`
- What it enables: tracing, streaming, observability, IDE preview
- How it differs from `engine/`: channels not callbacks, builtins not opcodes,
  `BexExternalValue` not `serde_json::Value`
- Scope: this doc. What's out of scope (partial parser internals, Boundary
  API protocol).

### 2. Design Principles

1. Unified event type (`RuntimeEvent`) — no intermediate enums
2. Channel-based delivery — no callbacks
3. `BexExternalValue` for all payloads — no `serde_json::Value`
4. `baml.events.send()` builtin — no special opcodes
5. `VmExecState::Event` yield for synchronous events (watch)
6. Zero-cost when no channel provided

### 3. Core Design Decisions

Three decisions with Context / Decision / Rationale / Consequences.

- 3.1 Channel-based delivery (not callbacks)
- 3.2 `baml.events.send()` builtin (not special opcodes)
- 3.3 `VmExecState::Event` for synchronous watch yields

### 4. Architecture Overview

Single diagram: event sources → channel → consumers/sinks.
Show the two event paths (VM yield, SysOp::EventSend).

### 5. `RuntimeEvent` Type System

Flat reference of all types. Define once, reference everywhere.

- 5.1 `RuntimeEvent` enum (all variants)
- 5.2 `EventMeta` (span_id, parent_span_id, root_span_id, timestamp)
- 5.3 `SpanContext` (the nested call ID tracking structure)
- 5.4 Function events (`FunctionStart`, `FunctionEnd`)
- 5.5 LLM events (`LlmRequest`, `LlmResponse`, `LlmRawRequest`, `LlmRawResponse`)
- 5.6 Header events (`Enter`, `Exit`)
- 5.7 Stream events (`Start`, `Update`, `End`)
- 5.8 Watch events (`Registered`, `VariableChanged`, etc.)
- 5.9 Block / Viz events

---

## Part II: Milestones

Each milestone is a self-contained unit of work that can be tested and
shipped independently. Later milestones build on earlier ones.

### Milestone 1: Function Start/End for Top-Level Calls

**Goal**: When you call `engine.call_function("Foo", args)`, emit
`Function(Start)` and `Function(End)` events with the correct args/result.

**What this requires**:
- `baml_events` crate with `RuntimeEvent`, `FunctionEvent` types
- `SysOp::EventSend` + `baml.events.send()` builtin
- Engine accepts `Option<mpsc::UnboundedSender<RuntimeEvent>>`
- Engine emits `Function(Start)` before VM execution, `Function(End)` after
  (or: compiler wraps the entry point with `baml.events.send()` calls)

**Key design point**: These events are emitted by the **engine**, not the
compiler. The engine is the component that receives the `call_function("Foo", args)`
call from the host language — it knows which function is the top-level entry
point. We do NOT want to instrument every expression function with start/end
events; only the function the user explicitly called from Python/TS gets
these. (Nested LLM functions get their own start/end in Milestone 2, but
intermediate expression functions in the call chain do not.)

This means `BexEngine::call_function()` does:
```
emit Function(Start { name, args })     ← engine emits before VM starts
vm.execute(bytecode_for_name, args)
emit Function(End { name, result })     ← engine emits after VM finishes
```

**Test**: Call a simple expression function through `BexEngine` with a
channel. Assert `Function(Start { name: "Foo", args })` and
`Function(End { result: Ok(value) })` appear on the channel.
Verify that if `Foo` calls helper expression function `Bar`, no
`Function(Start/End)` events appear for `Bar`.

### Milestone 2: Function Start/End for Nested LLM Calls

**Goal**: When `Foo()` calls `Bar()` which is an LLM function, we see
events for both: `Foo.Start` → `Bar.Start` → ... → `Bar.End` → `Foo.End`.

**What this requires**:
- The compiler inserts `baml.events.send("function_start", ...)` and
  `baml.events.send("function_end", ...)` around LLM function bodies
- Show exactly where in the LLM call sequence these are inserted:

  ```
  baml.events.send("function_start", { name, args })   ← NEW
  template = baml.llm.get_jinja_template(name)
  client_fn = baml.llm.get_client_function(name)
  client = client_fn()
  prompt = client.render_prompt(template, args)
  prompt = client.specialize_prompt(prompt)
  request = client.build_request(prompt)
  response = baml.http.send(request)                    ← async
  result = client.parse(response, name)
  baml.events.send("function_end", { name, result })    ← NEW
  return result
  ```

- For expression functions that call LLM functions, the nesting is natural:
  `Foo` is bytecoded, it calls `Bar` which hits the LLM path. Events nest
  by construction.

**Test**: Expression function `Outer(x)` calls LLM function `Inner(x)`.
Execute through engine with channel. Assert event order:
`Outer.Start` → `Inner.Start` → `Inner.End` → `Outer.End`.

### Milestone 3: Span Context and Nested Call IDs

**Goal**: Every event carries a `SpanContext` that enables reconstructing
the call tree. Each function call gets a unique span ID. Child spans
reference their parent.

**What this requires**:
- Define `SpanContext`:
  ```rust
  pub struct SpanContext {
      /// Unique ID for this span (function call invocation)
      pub span_id: SpanId,
      /// Parent span (None for the root call)
      pub parent_span_id: Option<SpanId>,
      /// Root span (the top-level call_function invocation)
      pub root_span_id: SpanId,
  }
  ```
- **How span IDs are assigned**: The engine (or the `baml.events.send` handler)
  maintains a span stack. When `function_start` is sent, push a new span ID.
  When `function_end` is sent, pop. The current top of stack is the parent.
  - Option A: Thread-local / VM-local span stack in the engine
  - Option B: The compiler passes span info as args to `baml.events.send()`
  - Option C: The engine intercepts `function_start`/`function_end` events
    and enriches them with span context before writing to the channel
  - Discuss tradeoffs of each

- **How `engine/` does it today**: `call_id_stack: Vec<FunctionCallId>` is
  maintained in `RuntimeContextManager`. Each `start_call()` pushes a new ID.
  Each `finish_call()` pops. All trace events carry the full stack.

- **Proposed approach for `baml_language`**: Engine maintains a `SpanStack`
  alongside the VM. When `SysOp::EventSend` fires with `"function_start"`,
  the engine pushes a new span. When `"function_end"` fires, it pops.
  Before writing to the channel, the engine stamps every event with the
  current `SpanContext`.

**Test**: `A()` calls `B()` calls `C()`. Assert:
- `A.Start` has `span_id=s1, parent=None, root=s1`
- `B.Start` has `span_id=s2, parent=s1, root=s1`
- `C.Start` has `span_id=s3, parent=s2, root=s1`
- `C.End` has `span_id=s3`
- `B.End` has `span_id=s2`
- `A.End` has `span_id=s1`

### Milestone 4: Intermediate LLM Events

**Goal**: Between `Function(Start)` and `Function(End)` for an LLM call,
emit fine-grained events: prompt rendered, HTTP request sent, HTTP response
received, response parsed.

**What this requires**:
- Additional `baml.events.send()` calls inserted into the LLM call sequence:

  ```
  baml.events.send("function_start", { name, args })
  template = baml.llm.get_jinja_template(name)
  client_fn = baml.llm.get_client_function(name)
  client = client_fn()
  prompt = client.render_prompt(template, args)
  prompt = client.specialize_prompt(prompt)
  baml.events.send("llm_request", { prompt, client })    ← NEW
  request = client.build_request(prompt)
  response = baml.http.send(request)
  baml.events.send("llm_raw_response", { status, body }) ← NEW
  result = client.parse(response, name)
  baml.events.send("llm_response", { result, usage })    ← NEW
  baml.events.send("function_end", { name, result })
  ```

- All payloads are `BexExternalValue`
- These events carry the span context from Milestone 3 (they inherit the
  current span — the LLM function's span)

**Test**: Execute LLM function with mock HTTP. Assert event sequence:
`Function(Start)` → `Llm(Request)` → `Llm(RawResponse)` → `Llm(Response)` → `Function(End)`.
Assert all events share the same `span_id`.

### Milestone 5: Publishing Events to Host Languages (CFFI)

**Goal**: Python/TS can receive `Function(Start/End)` and LLM events via
the existing CFFI callback mechanism.

**What this requires**:
- `bridge_cffi` implements `call_function_stream_from_c()`:
  - Creates `(event_tx, event_rx)` channel internally
  - Spawns execution task: `engine.call_function(name, args, Some(event_tx))`
  - Spawns reader task: reads `event_rx`, encodes events to protobuf,
    calls C callback
- Encoding: `RuntimeEvent` → protobuf `CffiValueHolder`
  - For now, Function and LLM events can encode their `BexExternalValue`
    payloads using existing `external_to_cffi_value()`
  - Streaming encoding (with `CffiValueStreamingState`) comes in Milestone 7

**Test**: Python integration test: call a BAML function, register an event
listener, assert `Function(Start/End)` events are received with correct
typed values.

### Milestone 6: Header Events

**Goal**: Emit `Header(Enter)` and `Header(Exit)` events for hierarchical
execution visualization (IDE, Boundary dashboard).

**What this requires**:
- Map existing `VizEnter`/`VizExit` opcodes (already in bytecode for
  `//# header` annotations) to `RuntimeEvent::Header(Enter/Exit)`
- The VM already executes these opcodes. Change them to yield
  `VmExecState::Event(RuntimeEvent::Header(...))` instead of
  `VmExecState::Notify(Viz { ... })`
- Implicit headers for LLM functions: the compiler can insert
  `baml.events.send("header_enter", ...)` before the LLM sequence and
  `baml.events.send("header_exit", ...)` after

**Test**: BAML program with `//# header` annotations. Assert proper
`Header(Enter/Exit)` nesting. Assert LLM functions get implicit headers.

### Milestone 7: Streaming — Partial Values to Host Languages

**Goal**: The full streaming path: SSE chunks → partial parse → typed
partial values → Python `async for`.

**What this requires (sub-milestones)**:

#### 7a. Streaming HTTP in `sys_native`

- New `http_stream` + `http_stream_next` SysOps
- `reqwest` streaming via `bytes_stream()`
- SSE line parsing

#### 7b. `StreamState` + `BexStreamValue`

- VM-local `StreamState` (accumulate, 50ms throttle, dedup)
- `BexStreamValue` = `BexExternalValue` + `StreamingCompletion` per field
- `StreamEventData::Update` carries `BexStreamValue`

#### 7c. Partial JSON Parsing

- `parse_partial(text, return_type) → (BexExternalValue, CompletionMap)`
- JSON fixup heuristics
- Completion state per field

#### 7d. VM Streaming Execution

- `handle_stream_chunk` / `handle_stream_end` in VM
- VM yields `Stream(Update)` with `BexStreamValue`
- Engine writes to channel

#### 7e. CFFI Streaming Encoding

- `stream_value_to_cffi()` wraps incomplete fields in `CffiValueStreamingState`
- Reader task encodes `Stream(Update)` events to protobuf
- C callback with `is_done=0` for partials, `is_done=1` for final

#### 7f. Host Language Consumer

- Python `BamlStream`: queue + coalesce + `async for`
- Verify existing `BamlStream` works with new protobuf (same wire format)

**Test**: Python `async for partial in b.stream.MyFunc(...)`. Assert
partials arrive with progressively more complete fields. Assert final
value is correct.

### Milestone 8: Watch Variable Events

**Goal**: `watch let` variables emit events when they change.

**What this requires**:
- Replace `VmExecState::Notify(WatchNotification)` with
  `VmExecState::Event(RuntimeEvent::Watch(...))`
- Engine writes watch events to the same channel
- Watch + stream integration: when a watched var backs a stream output,
  emit both `Watch(VariableChanged)` and `Stream(Update)`

**Test**: Program with `watch let x = 0; x = 1; x = 2`. Assert
`Watch(Registered)`, `Watch(VariableChanged)` x2.

### Milestone 9: Boundary Publisher

**Goal**: Events are batched and uploaded to the Boundary API.

**What this requires**:
- `BoundaryPublisher` as an `EventSink` that reads from a channel
- Batching, compression, env var config
- Fan-out task that reads from the main channel and dispatches to sinks

### Milestone 10: LSP / IDE Integration

**Goal**: IDE shows real-time execution visualization.

---

## Part III: Cross-Cutting Concerns

### 11. WASM Considerations

`web_time`, async alternatives, channel compat.

### 12. Configuration

Env vars table (matching `engine/` behavior).

### 13. Open Questions

Curated from v1, with answers where we've decided.

### 14. References

- engine/ files (one clean list)
- baml_language/ files (one clean list)

---

## Structural Notes

### What changed from the v1 outline

| v1 Outline (previous) | v2 Outline (this) |
|---|---|
| Organized by **component** (types, production, delivery, streaming, CFFI) | Organized by **milestones** (what you can ship and test, in order) |
| Streaming is one monolithic section | Streaming split into 7a–7f sub-milestones |
| Span context mentioned in passing | Milestone 3 is dedicated to span context design |
| LLM tracing assumes opcodes in some places | Consistently uses `baml.events.send()` everywhere |
| No clear build order | Each milestone builds on the previous |
| CFFI integration is one section | Split: M5 (basic event publishing), M7e (streaming encoding) |
| Watch is a standalone section | Milestone 8, after streaming |

### Why milestone-based structure

The v1 outline was organized like a reference manual (types → production →
delivery → streaming). That's good for looking things up but bad for
understanding what to build first and how things connect.

A milestone-based structure mirrors how we'll actually implement:
1. Get basic events flowing (M1–M2)
2. Make them useful with context tracking (M3)
3. Add detail (M4)
4. Ship to users (M5)
5. Add rich features (M6–M7)
6. Infrastructure (M8–M10)

Each milestone has a clear "test" that proves it works. You can demo M1
after a few days, M5 after a couple weeks.

### Section length targets

| Section | Target |
|---|---|
| Part I (1–5): Foundations | ~400 lines |
| Part II (M1–M10): Milestones | ~800 lines |
| Part III (11–14): Cross-cutting | ~150 lines |
| **Total** | **~1350 lines** |
