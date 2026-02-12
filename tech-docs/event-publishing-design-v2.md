# Event Publishing System — Design Document v2

> Milestones 1–8: From first event to S3 publishing, host-language observability, and cross-language span tracking.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Design Principles](#2-design-principles)
3. [Core Design Decisions](#3-core-design-decisions)
4. [Architecture Overview](#4-architecture-overview)
5. [Event Type System](#5-event-type-system)
6. [The Collector](#6-the-collector)
7. [Milestone 1: FunctionStart / FunctionEnd for Top-Level Calls](#7-milestone-1-functionstart--functionend-for-top-level-calls)
8. [Milestone 2: Global Publisher for S3 / Boundary API](#8-milestone-2-global-publisher-for-s3--boundary-api)
9. [Milestone 3: FunctionStart / FunctionEnd for Nested LLM Calls](#9-milestone-3-functionstart--functionend-for-nested-llm-calls)
10. [Milestone 4: Span Context and Nested Call IDs](#10-milestone-4-span-context-and-nested-call-ids)
11. [Milestone 5: Intermediate LLM Events](#11-milestone-5-intermediate-llm-events)
12. [Milestone 6: Publishing Events to Host Languages (CFFI)](#12-milestone-6-publishing-events-to-host-languages-cffi)
13. [Milestone 7: Header Events](#13-milestone-7-header-events)
14. [Milestone 8: Host-Language Span Tracking (`@trace` in Python/TS)](#14-milestone-8-host-language-span-tracking-trace-in-pythonts)
15. [Deferred Work](#15-deferred-work)
16. [Open Questions](#16-open-questions)
17. [Implementation Checklists](#17-implementation-checklists)
18. [References](#18-references)

---

## 1. Introduction

**What**: A runtime event system for the `baml_language` compiler that
publishes function-level, LLM-level, and control-flow events through a
channel to consumers (Collector, host languages, IDE, Boundary API).

**Why**: The new compiler needs the same observability as `engine/`, but
designed from scratch — no translation layers, no callback gymnastics.

**Scope**: This document covers Milestones 1–8 (basic function events through
S3 publishing, header events, and host-language span tracking via `@trace`).
Streaming (M9), watch variables (M10), and custom sinks (M11–M12) are
deferred to a future addendum.

**Out of scope**: Partial JSON parser internals, Boundary API protocol,
SSE streaming implementation.

---

## 2. Design Principles

1. **Unified event type** — `RuntimeEvent` everywhere, no intermediate enums.
2. **Global EventBus delivery** — `event_bus::emit(event)` from anywhere, no callbacks, no plumbed channels.
3. **`BexExternalValue` for all payloads** — no `serde_json::Value`.
4. **Engine emits top-level events; compiler instruments LLM functions** — no special VM opcodes for tracing.
5. **Zero-cost when no span** — if no `root_span_id` is provided, no events are emitted or stored.
6. **Collector as first-class consumer** — events are stored in a `Collector` that host languages query, mirroring `engine/`'s design.

---

## 3. Core Design Decisions

### 3.1 Global EventBus (Not Callbacks, Not Plumbed Channels)

**Context**: `engine/` uses `on_event: Fn(FunctionResult)` callbacks that
require `'static` bounds, `Arc` gymnastics, and `block_in_place` for CFFI.
An alternative — threading an `mpsc::UnboundedSender<RuntimeEvent>` through
the engine — would require changing function signatures at every layer,
modifying the `SysOpFn` function pointer boundary
(`fn(heap, args) -> SysOpResult`), and still cannot reach code inside SDK
interceptors (e.g., AWS Bedrock's `CollectorInterceptor`) or independently
spawned async tasks.

**Decision**: A global `EventBus` singleton (`Lazy<Mutex<EventStore>>`)
that any code can call: `event_bus::emit(event)`. The bus stores events
for active collectors (indexed by `root_span_id`) and forwards all events
to the global publisher.

**Rationale**: This mirrors `engine/`'s proven `BAML_TRACER` pattern,
which has 15 call sites across the codebase — none of which receive a
channel parameter. Events are emitted from function lifecycle code, LLM
orchestrators, HTTP request/response handlers, AWS SDK interceptors,
streaming handlers, and tag updates. A channel-based approach would
require plumbing `event_tx` through 4-5 layers including the `SysOpFn`
function pointer boundary and spawned async tasks, which is impractical.
The global singleton has minimal overhead (one mutex lock per `emit()`,
fast `HashMap` lookup + `Vec::push` inside).

**Consequences**: Any code — engine, sys_ops, SDK interceptors, spawned
tasks — can emit events without receiving a channel parameter.
`call_function()` receives a lightweight `root_span_id: Option<SpanId>`
(just a UUID) instead of a mutable channel. When `None`, no events are
stored or published (zero-cost). Collectors register to track specific
`root_span_id`s and query the global store on demand (pull model, like
`engine/`).

### 3.2 Engine Emits Top-Level Events; Compiler Instruments LLM Functions

**Context**: We only want `FunctionStart`/`FunctionEnd` for the function
the user explicitly called from Python/TS (the "top-level call"), not for
every intermediate expression function. But LLM functions — which can be
nested inside expression functions — need their own events.

**Decision**: Two event sources, no overlap:
- **Engine** emits `FunctionStart`/`FunctionEnd` for top-level
  **expression function** calls. The engine knows the function name, args,
  and result — it wraps the VM execution.
- **Compiler** instruments LLM function bytecode with `baml.events.send()`
  calls that emit `FunctionStart`/`FunctionEnd` and LLM-level events for
  each LLM function invocation — whether it's the top-level call or nested.

If the top-level call is itself an LLM function, the engine does **not**
emit `FunctionStart`/`FunctionEnd` — the compiler-inserted bytecode
handles it. This avoids duplicate events.

**Rationale**: The engine is the natural place for expression function
events (it receives the call from the host language, and expression
functions have no built-in instrumentation). The compiler is the natural
place for LLM function events (it controls the calling sequence and
already wraps the LLM steps). This avoids instrumenting every expression
function while ensuring every LLM call is traced, with no duplication.

**Consequences**: Two paths into `event_bus::emit()` — engine-direct (expression
functions) and SysOp-mediated (LLM functions). Both converge on the same
`RuntimeEvent` type. The engine checks `FunctionMeta::Llm` to decide
whether to emit top-level events.

### 3.3 Collector for In-Process Event Storage

**Context**: `engine/` has a `Collector` that stores function call logs
in a global `TraceStorage` with reference counting. Host languages
(`Python`, `Ruby`) pass collectors to function calls and later query them.

**Decision**: Implement a similar `Collector` for `baml_language`,
backed by a global `EventStore` that mirrors `engine/`'s `TraceStorage`.
Collectors track `root_span_id`s with reference counting and query the
store on demand (pull model).

**Rationale**: The global store pattern is proven in `engine/`
(`BAML_TRACER`). It allows any code to emit events via `event_bus::emit()`
without receiving a channel. The store only retains events for spans that
have at least one tracking collector (ref count > 0), so memory is bounded.

**Consequences**: The `Collector` is created by the caller, registered to
track a `root_span_id` before the call starts, and queries the global
store when the host language asks for logs.

### 3.4 Crate Separation: Types vs. Capabilities

**Context**: The event system needs types (pure data structs), a
collector (in-process storage + query), and a publisher (global
singleton, HTTP client, gzip, S3 uploads). These have vastly different
dependency profiles.

**Decision**: Three crates, not one:

| Crate | Purpose | Key dependencies |
|-------|---------|-----------------|
| **`baml_events`** | Event type definitions (`RuntimeEvent`, `SpanContext`, `FunctionLog`, etc.) + global `EventStore` and `emit()` | `uuid`, `web_time`, `bex_external_types`, `once_cell` |
| **`baml_collector`** | Collector handle: tracks `root_span_id`s, queries `EventStore` (`Collector`, `build_function_log()`) | `baml_events`, `indexmap` |
| **`baml_publisher`** | Global singleton publisher for S3 / Boundary API | `baml_events`, `reqwest`, `flate2`, `once_cell`, `tokio`, `anyhow`, `baml_rpc` |

**Rationale**: `bex_engine` only needs to construct `RuntimeEvent`s and
call `event_bus::emit()` — it should not pull in `reqwest`, `flate2`, or
any HTTP/compression machinery. The `EventStore` in `baml_events` is
lightweight (just a `HashMap` behind a `Mutex` plus `once_cell` for the
global static — no heavy dependencies). Keeping `baml_events` as a near-leaf
crate means any crate in the workspace can depend on it cheaply. The heavier
capabilities (Collector queries, Publisher HTTP uploads) are isolated in
their own crates and only depended on by `bridge_cffi`, the integration
point.

**Consequences**:
- `bex_engine` depends on `baml_events` only (types + `emit()`).
- `bridge_cffi` depends on all three (`baml_events`, `baml_collector`,
  `baml_publisher`).
- Adding a new sink (e.g., console logger, OpenTelemetry exporter) means
  adding a new crate, not bloating the types crate.

---

## 4. Architecture Overview

```
Host Language (Python/TS)
    │
    │  call_function("Foo", args, collectors)
    ▼
┌──────────────────────────────────────────────────┐
│  bridge_cffi                                     │
│  ┌────────────────────────────────────────────┐  │
│  │ Creates root_span_id = SpanId::new()       │  │
│  │ Registers collectors to track root_span_id │  │
│  │ Calls engine.call_function(name, args,     │  │
│  │     Some(root_span_id), parent_span_id)   │  │
│  │ Unregisters collectors after completion     │  │
│  └────────────────────────────────────────────┘  │
└──────────────────────────────────────────────────┘
    │
    ▼
┌──────────────────────────────────────────────────┐
│  BexEngine                                       │
│                                                  │
│  call_function():                                │
│    1. event_bus::emit(FunctionStart{...})         │ ← direct global call
│    2. vm = BexVm::new(...)                       │
│    3. result = run_event_loop(&vm)               │
│    4. event_bus::emit(FunctionEnd{...})           │ ← direct global call
│    5. return result                              │
│                                                  │
│  run_event_loop():                               │
│    match vm.exec() {                             │
│      ScheduleFuture(id) => {                     │
│        SysOp::EventSend => {                     │ ← compiler-inserted
│          event_bus::emit(event)                   │ ← direct global call
│          vm.set_future_ready(id, Null)           │
│        }                                         │
│        other_sys_op => execute_sys_op(...)        │
│      }                                           │
│      Await(id) => drain_futures(...)              │
│      Complete(v) => return v                      │
│    }                                             │
└──────────────────────────────────────────────────┘
    │
    │  event_bus::emit() internally routes to:
    ▼
┌─────────────────────────────────────────────────────────┐
│  Global EventStore (baml_events)                        │
│                                                         │
│  emit(event):                                           │
│    1. If root_span_id is tracked → store event          │
│    2. Forward to publisher sink → batches → S3 (M2)     │
│                                                         │
│  Collectors query on demand:                            │
│    collector.logs() → reads from EventStore             │
│  Publisher receives all events:                         │
│    publisher_sink(event) → batch → compress → S3        │
└─────────────────────────────────────────────────────────┘
```

**Two event sources (both call `event_bus::emit()` directly)**:

| Source | Events | Mechanism |
|--------|--------|-----------|
| `BexEngine::call_function()` | Top-level `FunctionStart`/`FunctionEnd` | Direct `event_bus::emit()` call |
| Compiler-inserted `baml.events.send()` | LLM `FunctionStart`/`FunctionEnd`, `LlmRequest`, `LlmResponse`, etc. | `SysOp::EventSend` → engine calls `event_bus::emit()` → returns `Null` |

---

## 5. Event Type System

All types live in the **`baml_events`** crate (types only — see
[Section 3.4](#34-crate-separation-types-vs-capabilities)).

### 5.1 `RuntimeEvent`

```rust
// baml_language/crates/baml_events/src/lib.rs

/// A runtime event emitted during BAML execution.
#[derive(Clone, Debug)]
pub struct RuntimeEvent {
    /// Span context for correlating events in a call tree.
    pub ctx: SpanContext,
    /// When this event was created (absolute UTC for serialization).
    pub timestamp: web_time::SystemTime,
    /// The event payload.
    pub event: EventKind,
}

/// The kind of runtime event.
#[derive(Clone, Debug)]
pub enum EventKind {
    /// Function lifecycle events.
    Function(FunctionEvent),
    /// LLM-specific events (request/response details).
    Llm(LlmEvent),
    /// Hierarchical execution context markers.
    Header(HeaderEvent),
    /// Metadata/tag updates (set arbitrary key-value pairs on the current span).
    SetTags(TraceTags),
}

/// Arbitrary metadata tags attached to a function call.
pub type TraceTags = Vec<(String, BexExternalValue)>;
```

### 5.2 `SpanContext`

```rust
/// Identifies where an event sits in the call tree.
#[derive(Clone, Debug)]
pub struct SpanContext {
    /// Unique ID for this span (one per function invocation).
    pub span_id: SpanId,
    /// Parent span (None for the root / top-level call).
    pub parent_span_id: Option<SpanId>,
    /// Root span (the top-level call_function invocation).
    pub root_span_id: SpanId,
}

/// Opaque span identifier.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpanId(uuid::Uuid);

impl SpanId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4())
    }
}
```

### 5.3 Function Events

```rust
#[derive(Clone, Debug)]
pub enum FunctionEvent {
    Start(FunctionStart),
    End(FunctionEnd),
}

#[derive(Clone, Debug)]
pub struct FunctionStart {
    /// Function name (e.g., "ExtractResume", "ClassifyIntent").
    pub name: String,
    /// Arguments as (param_name, value) pairs.
    pub args: Vec<(String, BexExternalValue)>,
    /// Whether this is a streaming call.
    pub is_stream: bool,
}

#[derive(Clone, Debug)]
pub struct FunctionEnd {
    /// Function name.
    pub name: String,
    /// Result: Ok(value) or Err(error message).
    pub result: Result<BexExternalValue, String>,
    /// Wall-clock duration of the function call.
    pub duration: std::time::Duration,
}
```

### 5.4 LLM Events

```rust
#[derive(Clone, Debug)]
pub enum LlmEvent {
    /// Prompt has been rendered and is ready to send.
    Request(LlmRequest),
    /// Raw HTTP request about to be sent.
    RawRequest(LlmRawRequest),
    /// Raw HTTP response received.
    RawResponse(LlmRawResponse),
    /// Parsed LLM response.
    Response(LlmResponse),
}

/// Unique identifier for a single HTTP request/response pair.
/// Used to correlate LlmRequest → RawRequest → RawResponse → LlmResponse
/// and to distinguish retry attempts (each retry gets a new RequestId).
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct RequestId(uuid::Uuid);

impl RequestId {
    pub fn new() -> Self { Self(uuid::Uuid::new_v4()) }
}

#[derive(Clone, Debug)]
pub struct LlmRequest {
    /// Correlates all events for this LLM call attempt.
    pub request_id: RequestId,
    /// The rendered prompt.
    pub prompt: BexExternalValue,
    /// Client name (e.g., "GPT4").
    pub client_name: String,
    /// Provider name (e.g., "openai", "anthropic").
    pub provider: String,
    /// Provider-specific parameters (temperature, max_tokens, etc.).
    pub params: BexExternalValue,
}

#[derive(Clone, Debug)]
pub struct LlmRawRequest {
    /// Correlates with the LlmRequest.
    pub request_id: RequestId,
    /// HTTP method.
    pub method: String,
    /// Request URL.
    pub url: String,
    /// Request headers.
    pub headers: Vec<(String, String)>,
    /// Request body.
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct LlmRawResponse {
    /// Correlates with the LlmRequest.
    pub request_id: RequestId,
    /// HTTP status code.
    pub status: u16,
    /// Response headers.
    pub headers: Vec<(String, String)>,
    /// Response body.
    pub body: String,
    /// Duration of the HTTP call (time between LlmRawRequest and LlmRawResponse).
    /// Computed by `build_function_log()` from event timestamps, not by the emitter.
    pub duration: std::time::Duration,
}

#[derive(Clone, Debug)]
pub struct LlmResponse {
    /// Correlates with the LlmRequest.
    pub request_id: RequestId,
    /// Parsed output value.
    pub value: BexExternalValue,
    /// Raw text output from the LLM (before parsing).
    pub raw_text_output: Option<String>,
    /// Token usage (if reported by provider).
    pub usage: Option<TokenUsage>,
    /// Model identifier returned by the provider (e.g., "gpt-4o-2024-08-06").
    pub model: Option<String>,
    /// Finish reason from the provider ("stop", "length", etc.).
    pub finish_reason: Option<String>,
    /// Client stack: chain of clients used (e.g., ["MyRoundRobin", "MyFallback", "GPT4"]).
    /// For simple clients this is a single entry. For strategy clients (fallback/round-robin)
    /// this shows the full resolution chain, matching engine/'s LoggedLLMResponse.client_stack.
    pub client_stack: Vec<String>,
    /// Whether the LLM call was successful (no parse errors).
    pub is_success: bool,
    /// Error message if the call failed.
    pub error_message: Option<String>,
}

#[derive(Clone, Debug, Default)]
pub struct TokenUsage {
    pub input_tokens: Option<i64>,
    pub output_tokens: Option<i64>,
    pub cached_input_tokens: Option<i64>,
}
```

### 5.5 Header Events

Header events map from the VM's `VizExecEvent` which carries rich
metadata from the `VizNodeMeta` compiled by the bytecode emitter.

```rust
#[derive(Clone, Debug)]
pub enum HeaderEvent {
    Enter(HeaderEnter),
    Exit(HeaderExit),
}

#[derive(Clone, Debug)]
pub struct HeaderEnter {
    /// Human-readable label (e.g., "ExtractResume", "if branch", "retry attempt 2").
    pub name: String,
    /// Unique node ID within the function (from VizExecEvent.node_id).
    pub node_id: u32,
    /// Type of visualization node (FunctionRoot, HeaderContextEnter, BranchGroup, etc.).
    pub node_type: VizNodeType,
    /// Header level (only for HeaderContextEnter nodes, from `//# header` annotations).
    pub header_level: Option<u8>,
}

#[derive(Clone, Debug)]
pub struct HeaderExit {
    /// Matching label.
    pub name: String,
    /// Matching node ID.
    pub node_id: u32,
}
```

The `VizNodeType` enum is re-exported from `bex_vm_types::bytecode`
(variants: `FunctionRoot`, `HeaderContextEnter`, `BranchGroup`,
`BranchArm`, `Loop`, `OtherScope`). These are useful for IDE rendering
and the Boundary dashboard to distinguish different kinds of execution
blocks.

---

## 6. The Collector

### 6.1 How `engine/` Does It Today

In `engine/`, the `Collector` works as follows:

1. **Global singleton**: `BAML_TRACER` is a `Lazy<Mutex<TraceStorage>>` that
   holds all trace events keyed by `FunctionCallId`.

2. **Reference counting**: When a `Collector` tracks a function call, it
   increments a ref count in `TraceStorage`. When the collector is dropped,
   it decrements the ref count. At zero, events are purged.

3. **Event flow**: `TraceEvent`s are `put()` into `BAML_TRACER` during
   execution. The `Collector` holds a set of `FunctionCallId`s it cares
   about. When queried (e.g., `collector.logs()`), it builds a `FunctionLog`
   by reading events from `TraceStorage` for each tracked ID.

4. **Host language usage** (Python):
   ```python
   collector = baml.Collector()
   result = await b.ExtractResume(resume_text, baml_options={"collectors": [collector]})
   for log in collector.logs:
       print(log.function_name, log.usage)
   ```

**Key files**:
- `engine/baml-runtime/src/tracingv2/storage/storage.rs` — `TraceStorage`, `Collector`, `FunctionLog`
- `engine/baml-runtime/src/tracing/mod.rs` — `start_call()` tracks collectors
- `engine/language_client_python/src/types/log_collector.rs` — Python wrapper

### 6.2 Design for `baml_language`

We keep the same user-facing API but change the internals to use the
global `EventStore` (mirroring `engine/`'s `BAML_TRACER` / `TraceStorage`
pattern). The `Collector` tracks `root_span_id`s with reference counting
and queries the store on demand.

#### Global `EventStore`

```rust
// baml_language/crates/baml_events/src/event_store.rs

use std::sync::Mutex;
use std::collections::HashMap;
use once_cell::sync::Lazy;

use crate::*;

/// Global event store. Mirrors engine/'s BAML_TRACER / TraceStorage.
///
/// Events are stored only when at least one collector is tracking the
/// root_span_id (ref count > 0). All events are forwarded to the
/// publisher sink regardless of tracking.
pub static EVENT_STORE: Lazy<Mutex<EventStore>> = Lazy::new(|| {
    Mutex::new(EventStore::default())
});

/// Callback for forwarding events to the publisher (registered once at startup).
type PublisherSink = Box<dyn Fn(RuntimeEvent) + Send>;

#[derive(Default)]
pub struct EventStore {
    /// Events indexed by root_span_id. Only stored when ref_count > 0.
    events_by_root_span: HashMap<SpanId, Vec<RuntimeEvent>>,
    /// Reference counts: how many collectors are tracking each root_span_id.
    ref_counts: HashMap<SpanId, usize>,
    /// Publisher sink (set once at startup by baml_publisher).
    publisher_sink: Option<PublisherSink>,
}

/// Emit an event. Called from anywhere — engine, sys_ops, interceptors.
///
/// This is the primary API. It:
/// 1. Stores the event if any collector is tracking its root_span_id.
/// 2. Forwards to the publisher sink (for S3 upload) unconditionally.
pub fn emit(event: RuntimeEvent) {
    let store = EVENT_STORE.lock().unwrap();

    // Store if tracked
    if store.ref_counts.contains_key(&event.ctx.root_span_id) {
        // Need mutable access — drop and re-lock
        drop(store);
        let mut store = EVENT_STORE.lock().unwrap();
        store.events_by_root_span
            .entry(event.ctx.root_span_id.clone())
            .or_default()
            .push(event.clone());

        // Forward to publisher
        if let Some(sink) = &store.publisher_sink {
            sink(event);
        }
    } else {
        // Not tracked — just forward to publisher
        if let Some(sink) = &store.publisher_sink {
            sink(event);
        }
    }
}

/// Register a publisher sink. Called once at startup by baml_publisher.
pub fn set_publisher_sink(sink: impl Fn(RuntimeEvent) + Send + 'static) {
    let mut store = EVENT_STORE.lock().unwrap();
    store.publisher_sink = Some(Box::new(sink));
}

/// Start tracking a root_span_id (increment ref count).
/// Called by Collector when it begins tracking a call.
pub fn track(root_span_id: &SpanId) {
    let mut store = EVENT_STORE.lock().unwrap();
    *store.ref_counts.entry(root_span_id.clone()).or_insert(0) += 1;
}

/// Stop tracking a root_span_id (decrement ref count).
/// When ref count reaches zero, events for that span are purged.
pub fn untrack(root_span_id: &SpanId) {
    let mut store = EVENT_STORE.lock().unwrap();
    if let Some(count) = store.ref_counts.get_mut(root_span_id) {
        *count -= 1;
        if *count == 0 {
            store.ref_counts.remove(root_span_id);
            store.events_by_root_span.remove(root_span_id);
        }
    }
}

/// Read events for a root_span_id (used by Collector to build FunctionLogs).
pub fn events_for_span(root_span_id: &SpanId) -> Option<Vec<RuntimeEvent>> {
    let store = EVENT_STORE.lock().unwrap();
    store.events_by_root_span.get(root_span_id).cloned()
}
```

#### `Collector` struct

The `Collector` is now a lightweight handle that tracks `root_span_id`s
and queries the global `EventStore` on demand (pull model). It does not
own events — it reads them from the store.

```rust
// baml_language/crates/baml_collector/src/lib.rs

use std::sync::{Arc, Mutex};
use std::collections::HashMap;
use baml_events::*;

/// In-memory event collector.
///
/// Created by the host language, registered to track root_span_ids
/// before function calls. Queries the global EventStore on demand.
///
/// Mirrors engine/'s Collector which tracks FunctionCallIds in
/// TraceStorage with reference counting.
#[derive(Debug, Clone)]
pub struct Collector {
    inner: Arc<Mutex<CollectorInner>>,
}

#[derive(Debug)]
struct CollectorInner {
    /// Name for debugging.
    name: String,
    /// Ordered list of root span IDs this collector is tracking.
    tracked_spans: Vec<SpanId>,
    /// Cached FunctionLog builds (invalidated on re-query).
    cached_logs: HashMap<SpanId, Arc<FunctionLog>>,
}

impl Collector {
    pub fn new(name: Option<String>) -> Self {
        Self {
            inner: Arc::new(Mutex::new(CollectorInner {
                name: name.unwrap_or_else(|| "collector".into()),
                tracked_spans: Vec::new(),
                cached_logs: HashMap::new(),
            })),
        }
    }

    pub fn name(&self) -> String {
        self.inner.lock().unwrap().name.clone()
    }

    /// Start tracking a root_span_id. Called before engine.call_function().
    /// Increments the ref count in the global EventStore.
    pub fn track_call(&self, root_span_id: SpanId) {
        event_store::track(&root_span_id);
        self.inner.lock().unwrap().tracked_spans.push(root_span_id);
    }

    /// Return all function logs (one per tracked root span), in insertion order.
    /// Reads from the global EventStore on each call (pull model).
    pub fn function_logs(&self) -> Vec<Arc<FunctionLog>> {
        let inner = self.inner.lock().unwrap();
        inner
            .tracked_spans
            .iter()
            .filter_map(|span_id| {
                // Read events from global EventStore (pull model)
                let events = event_store::events_for_span(span_id)?;
                build_function_log(&events, span_id)
            })
            .collect()
    }

    /// Return the last function log.
    pub fn last_function_log(&self) -> Option<Arc<FunctionLog>> {
        let inner = self.inner.lock().unwrap();
        let span_id = inner.tracked_spans.last()?;
        let events = event_store::events_for_span(span_id)?;
        build_function_log(&events, span_id)
    }

    /// Aggregate token usage across all tracked functions.
    pub fn usage(&self) -> TokenUsage {
        let logs = self.function_logs();
        let mut total = TokenUsage::default();
        for log in &logs {
            total.input_tokens = merge_opt(total.input_tokens, log.usage.input_tokens);
            total.output_tokens = merge_opt(total.output_tokens, log.usage.output_tokens);
            total.cached_input_tokens = merge_opt(
                total.cached_input_tokens,
                log.usage.cached_input_tokens,
            );
        }
        total
    }

    /// Clear cached logs. Events are managed by the global EventStore.
    pub fn clear(&self) {
        let mut inner = self.inner.lock().unwrap();
        inner.cached_logs.clear();
    }
}

impl Drop for Collector {
    fn drop(&mut self) {
        // Decrement ref counts in the global EventStore.
        // When ref count hits zero, events for that span are purged.
        let inner = self.inner.lock().unwrap();
        for span_id in &inner.tracked_spans {
            event_store::untrack(span_id);
        }
    }
}

fn merge_opt(a: Option<i64>, b: Option<i64>) -> Option<i64> {
    match (a, b) {
        (Some(x), Some(y)) => Some(x + y),
        (x, y) => x.or(y),
    }
}
```

#### `build_function_log` — Mirrors `engine/`'s `build_function_log()`

This is the core logic that converts raw events into a structured
`FunctionLog`. It mirrors `engine/`'s `build_function_log()` which:
1. Walks all events for the root span's `FunctionStart`/`FunctionEnd`
2. Walks ALL spans sharing the same `root_span_id` to find nested LLM events
3. Groups LLM events by `RequestId` into `LLMCall` objects (one per retry attempt)
4. Determines the "selected" call (earliest successful by request ID order)
5. Aggregates token usage across all LLM calls
6. Merges metadata/tags
7. Caches the result after `FunctionEnd` arrives

```rust
fn build_function_log(
    inner: &mut CollectorInner,
    root_span_id: &SpanId,
) -> Option<Arc<FunctionLog>> {
    // Check cache first
    if let Some(cached) = inner.cached_logs.get(root_span_id) {
        return Some(cached.clone());
    }

    let root_events = inner.events_by_span.get(root_span_id)?;

    // Extract FunctionStart/FunctionEnd from root span
    let mut function_name = None;
    let mut args = None;
    let mut is_stream = false;
    let mut result = None;
    let mut start_time: Option<web_time::SystemTime> = None;
    let mut end_time: Option<web_time::SystemTime> = None;
    let mut metadata: HashMap<String, BexExternalValue> = HashMap::new();

    for event in root_events {
        match &event.event {
            EventKind::Function(FunctionEvent::Start(s)) => {
                function_name = Some(s.name.clone());
                args = Some(s.args.clone());
                is_stream = s.is_stream;
                start_time = Some(event.timestamp);
            }
            EventKind::Function(FunctionEvent::End(e)) => {
                result = Some(e.result.clone());
                end_time = Some(event.timestamp);
            }
            EventKind::SetTags(tags) => {
                for (k, v) in tags {
                    metadata.insert(k.clone(), v.clone());
                }
            }
            _ => {}
        }
    }

    let fname = function_name?;

    // Compute timing
    let start_ms = start_time.map(system_time_to_utc_ms).unwrap_or(0);
    let duration_ms = end_time.map(|end| {
        system_time_to_utc_ms(&end).saturating_sub(start_ms)
    });

    // Walk ALL spans that share this root to find LLM events.
    // Group by RequestId to build individual LLMCall objects.
    struct CallAccumulator {
        llm_request: Option<LlmRequest>,
        llm_raw_request: Option<LlmRawRequest>,
        llm_raw_response: Option<LlmRawResponse>,
        llm_response: Option<LlmResponse>,
        first_seen_ms: Option<i64>,
        last_seen_ms: Option<i64>,
    }

    let mut calls_map: HashMap<RequestId, CallAccumulator> = HashMap::new();
    let mut raw_llm_response: Option<String> = None;

    for (_, span_events) in &inner.events_by_span {
        let belongs_to_root = span_events
            .first()
            .map(|e| e.ctx.root_span_id == *root_span_id)
            .unwrap_or(false);
        if !belongs_to_root {
            continue;
        }

        for event in span_events {
            let time_ms = system_time_to_utc_ms(&event.timestamp);
            match &event.event {
                EventKind::Llm(LlmEvent::Request(req)) => {
                    let acc = calls_map.entry(req.request_id.clone()).or_default();
                    acc.llm_request = Some(req.clone());
                    acc.first_seen_ms.get_or_insert(time_ms);
                }
                EventKind::Llm(LlmEvent::RawRequest(req)) => {
                    let acc = calls_map.entry(req.request_id.clone()).or_default();
                    acc.llm_raw_request = Some(req.clone());
                    acc.first_seen_ms.get_or_insert(time_ms);
                }
                EventKind::Llm(LlmEvent::RawResponse(resp)) => {
                    let acc = calls_map.entry(resp.request_id.clone()).or_default();
                    acc.llm_raw_response = Some(resp.clone());
                    acc.last_seen_ms = Some(time_ms);
                }
                EventKind::Llm(LlmEvent::Response(resp)) => {
                    let acc = calls_map.entry(resp.request_id.clone()).or_default();
                    acc.llm_response = Some(resp.clone());
                    acc.last_seen_ms = Some(time_ms);
                    if resp.raw_text_output.is_some() {
                        raw_llm_response = resp.raw_text_output.clone();
                    }
                }
                EventKind::SetTags(tags) => {
                    for (k, v) in tags {
                        metadata.insert(k.clone(), v.clone());
                    }
                }
                _ => {}
            }
        }
    }

    // Build LLMCall objects from accumulators
    let mut calls: Vec<LLMCall> = Vec::new();
    let mut total_usage = TokenUsage::default();

    for (request_id, acc) in &calls_map {
        let (client_name, provider) = acc.llm_request.as_ref()
            .map(|r| (r.client_name.clone(), r.provider.clone()))
            .unwrap_or_default();

        let call_start_ms = acc.first_seen_ms.unwrap_or(start_ms);
        let call_end_ms = acc.last_seen_ms.unwrap_or(call_start_ms);
        let call_duration_ms = call_end_ms.saturating_sub(call_start_ms);

        let call_usage = acc.llm_response.as_ref()
            .and_then(|r| r.usage.clone())
            .unwrap_or_default();

        // Accumulate total usage
        total_usage.input_tokens = merge_opt(total_usage.input_tokens, call_usage.input_tokens);
        total_usage.output_tokens = merge_opt(total_usage.output_tokens, call_usage.output_tokens);
        total_usage.cached_input_tokens = merge_opt(
            total_usage.cached_input_tokens,
            call_usage.cached_input_tokens,
        );

        let is_success = acc.llm_response.as_ref()
            .map(|r| r.is_success)
            .unwrap_or(false);

        calls.push(LLMCall {
            request_id: request_id.clone(),
            client_name,
            provider,
            timing: Timing {
                start_time_utc_ms: call_start_ms,
                duration_ms: Some(call_duration_ms),
            },
            raw_request: acc.llm_raw_request.clone(),
            raw_response: acc.llm_raw_response.clone(),
            usage: call_usage,
            selected: false, // determined below
            is_success,
        });
    }

    // Determine "selected" call: earliest successful (by request_id order).
    // Mirrors engine/'s logic which sorts by ULID/UUID lexicographic order.
    let selected_idx = calls.iter().enumerate()
        .filter(|(_, c)| c.is_success)
        .min_by_key(|(_, c)| c.request_id.clone())
        .map(|(i, _)| i);
    if let Some(idx) = selected_idx {
        calls[idx].selected = true;
    }

    let log = Arc::new(FunctionLog {
        span_id: root_span_id.clone(),
        function_name: fname,
        log_type: if is_stream { "stream".into() } else { "call".into() },
        timing: Timing {
            start_time_utc_ms: start_ms,
            duration_ms,
        },
        usage: total_usage,
        calls,
        raw_llm_response,
        metadata,
    });

    // Cache only if FunctionEnd has arrived (matches engine/ behavior)
    if end_time.is_some() {
        inner.cached_logs.insert(root_span_id.clone(), log.clone());
    }

    Some(log)
}

fn system_time_to_utc_ms(st: &web_time::SystemTime) -> i64 {
    st.duration_since(web_time::SystemTime::UNIX_EPOCH)
        .unwrap_or_else(|_| std::time::Duration::from_secs(0))
        .as_millis() as i64
}
```

#### `FunctionLog` — Full API Surface (Matches `engine/`)

The `engine/` `FunctionLog` exposes: `function_name`, `log_type` ("call"
vs "stream"), `timing`, `usage`, `calls` (list of `LLMCall`), `raw_llm_response`,
`metadata`/`tags`, and `selected_call`. We match all of these:

```rust
/// A single function call's log, built from collected events.
///
/// Mirrors engine/'s FunctionLogInner but uses BexExternalValue
/// instead of serde_json::Value for metadata.
#[derive(Clone, Debug)]
pub struct FunctionLog {
    /// Span ID of this function call.
    pub span_id: SpanId,
    /// Function name.
    pub function_name: String,
    /// "call" or "stream".
    pub log_type: String,
    /// Start time (UTC ms) and duration.
    pub timing: Timing,
    /// Aggregated token usage across all LLM calls.
    pub usage: TokenUsage,
    /// Individual LLM call attempts (one per retry).
    /// Each has its own request/response, timing, usage, and selected flag.
    pub calls: Vec<LLMCall>,
    /// Raw text output from the selected LLM response.
    pub raw_llm_response: Option<String>,
    /// Arbitrary metadata/tags set during execution.
    pub metadata: HashMap<String, BexExternalValue>,
}

impl FunctionLog {
    /// Return the "selected" LLM call (the one whose result was used).
    /// When there are retries, this is the earliest successful call.
    pub fn selected_call(&self) -> Option<&LLMCall> {
        self.calls.iter().find(|c| c.selected)
    }
}

/// A single LLM call attempt (one HTTP request/response pair).
///
/// When retries are involved, there will be multiple LLMCall objects
/// in a FunctionLog. One is marked `selected = true`.
#[derive(Clone, Debug)]
pub struct LLMCall {
    /// Unique request identifier (correlates all events for this attempt).
    pub request_id: RequestId,
    /// Client name (e.g., "GPT4").
    pub client_name: String,
    /// Provider name (e.g., "openai").
    pub provider: String,
    /// Timing for this specific call.
    pub timing: Timing,
    /// The raw HTTP request sent.
    pub raw_request: Option<LlmRawRequest>,
    /// The raw HTTP response received.
    pub raw_response: Option<LlmRawResponse>,
    /// Token usage for this call.
    pub usage: TokenUsage,
    /// Whether this call was selected as the "winning" result.
    pub selected: bool,
    /// Whether this call succeeded (parsed without errors).
    pub is_success: bool,
}

/// Absolute timing information.
///
/// Uses UTC milliseconds (not relative Instant) so it can be
/// serialized and displayed. Matches engine/'s Timing struct.
#[derive(Clone, Debug, Default)]
pub struct Timing {
    pub start_time_utc_ms: i64,
    pub duration_ms: Option<i64>,
}
```

### 6.3 How Events Flow Into the Collector

```
call_function(name, args, collectors)
    │
    │  bridge_cffi:
    │    root_span_id = SpanId::new()
    │    parent_span_id = host_ctx.span_id   // from @trace (M8), or None
    │    for c in collectors:
    │        c.track_call(root_span_id)       // registers with global EventStore
    │
    │  engine.call_function(name, args, Some(root_span_id), parent_span_id)
    │    │
    │    │  Engine + compiler-inserted code call event_bus::emit() directly:
    │    │    emit(FunctionStart{...})          // stored in EventStore
    │    │    emit(LlmRequest{...})             // stored in EventStore
    │    │    emit(LlmResponse{...})            // stored in EventStore
    │    │    emit(FunctionEnd{...})            // stored in EventStore
    │    │
    │    │  Each emit() also forwards to publisher sink → S3
    │
    ▼ (after execution completes)
    host language queries:
      collector.logs()    → reads from EventStore by tracked root_span_ids
      collector.usage()   → aggregates from logs
    on collector drop:
      untrack(root_span_id) → events purged when ref count hits zero
```

### 6.4 Comparison with `engine/` Collector

| Aspect | `engine/` Collector | `baml_language` Collector |
|--------|-------------------|--------------------------|
| **Storage** | Global `BAML_TRACER` singleton (`Lazy<Mutex<TraceStorage>>`) | Global `EVENT_STORE` singleton (`Lazy<Mutex<EventStore>>`) |
| **Event delivery** | Events `put()` into global store, collector tracks IDs | Events `emit()`ted into global store, collector tracks `root_span_id`s |
| **Reference counting** | Manual `inc_ref`/`dec_ref` on `FunctionCallId` | Same pattern: `track()`/`untrack()` on `SpanId` |
| **Memory cleanup** | On `dec_ref` to zero, events purged from global map | On `untrack` to zero, events purged from global map |
| **Thread safety** | Global mutex on every `put()` and `get()` | Global mutex on every `emit()` and `events_for_span()` |
| **Query** | `FunctionLog::new(id)` rebuilds from global events | `collector.function_logs()` reads from global store |
| **Concurrency** | All concurrent calls share one global store | Same — all calls share one global store |

### 6.5 Host Language API (Python)

The Python API stays identical to `engine/`. Every property that
`engine/`'s `FunctionLog` exposes is matched:

```python
collector = baml.Collector("my_trace")
result = await b.ExtractResume(resume_text, baml_options={"collectors": [collector]})

# ── Collector-level queries ──
for log in collector.logs:             # List[FunctionLog]
    print(f"{log.function_name}: {log.usage}")
last = collector.last                  # Optional[FunctionLog]
usage = collector.usage                # Usage (aggregated)

# ── FunctionLog properties (mirrors engine/) ──
log = collector.last
log.id                  # str (span ID)
log.function_name       # str
log.log_type            # "call" | "stream"
log.timing              # Timing(start_time_utc_ms, duration_ms)
log.usage               # Usage(input_tokens, output_tokens, cached_input_tokens)
log.calls               # List[LLMCall] — one per retry attempt
log.raw_llm_response    # Optional[str]
log.metadata            # Dict[str, Any]
log.tags                # Dict[str, Any] (alias for metadata)
log.selected_call       # Optional[LLMCall] — the "winning" retry

# ── LLMCall properties (mirrors engine/) ──
call = log.selected_call
call.client_name        # str (e.g., "GPT4")
call.provider           # str (e.g., "openai")
call.selected           # bool
call.timing             # Timing
call.usage              # Optional[Usage]
call.http_request       # Optional[HTTPRequest] (url, method, headers, body)
call.http_response      # Optional[HTTPResponse] (status, headers, body)
```

### 6.6 Code Changes for the Collector

| File | Change |
|------|--------|
| `baml_events/src/event_store.rs` | **New** — `EventStore`, `emit()`, `track()`/`untrack()`, `set_publisher_sink()` |
| `baml_events/src/lib.rs` | **New** — `RuntimeEvent`, `EventKind`, `SpanContext`, all event types |
| `baml_collector/src/lib.rs` | **New** — `Collector`, `track_call()`, `function_logs()`, `Drop` impl |
| `bridge_cffi/src/ffi/functions.rs` | Extract collectors, create root_span_id, register tracking, call engine |
| `bridge_cffi/src/ffi/objects.rs` | Expose `Collector` to host language (new/logs/last/clear/usage) |
| `bridge_cffi/Cargo.toml` | Add `baml_events`, `baml_collector` dependencies |

---

## 7. Milestone 1: FunctionStart / FunctionEnd for Top-Level Calls

### Goal

When you call `engine.call_function("Foo", args)` where `Foo` is an
**expression function**, emit `FunctionStart` and `FunctionEnd` events
with the correct args and result. (If `Foo` is an LLM function, these
events come from the compiler-inserted bytecode in M3 instead — the
engine does not double-emit.)

### What This Requires

#### 1. Create the `baml_events` and `baml_collector` crates

```
baml_language/crates/baml_events/          ← types + global EventStore
├── Cargo.toml
└── src/
    ├── lib.rs          ← RuntimeEvent, EventKind, SpanContext, SpanId
    ├── function.rs     ← FunctionEvent, FunctionStart, FunctionEnd
    ├── llm.rs          ← LlmEvent (empty for now)
    ├── header.rs       ← HeaderEvent (empty for now)
    └── event_store.rs  ← EventStore, emit(), track()/untrack(), set_publisher_sink()

baml_language/crates/baml_collector/       ← collector queries
├── Cargo.toml
└── src/
    └── lib.rs          ← Collector, track_call(), function_logs(), Drop
```

`baml_events` dependencies: `uuid`, `web_time`, `bex_external_types`, `once_cell`.
`baml_collector` dependencies: `baml_events`, `indexmap`, `web_time`.

#### 2. Add `root_span_id` to `BexEngine::call_function()`

**File**: `baml_language/crates/bex_engine/src/lib.rs`

Current signature:
```rust
pub async fn call_function(
    &self,
    function_name: &str,
    args: &[BexValue],
) -> Result<BexExternalValue, EngineError>
```

New signature:
```rust
pub async fn call_function(
    &self,
    function_name: &str,
    args: &[BexValue],
    root_span_id: Option<SpanId>,       // lightweight: just a UUID, not a channel
    parent_span_id: Option<SpanId>,     // from host-language @trace span (M8)
) -> Result<BexExternalValue, EngineError>
```

When `root_span_id` is `None`, no events are emitted (zero-cost).
When `Some`, the engine uses it as the root of its `SpanStack` and
emits events via `event_bus::emit()`. No channel plumbing required.

When `parent_span_id` is `Some`, the engine's top-level span becomes
a child of the host-language `@trace` span (see [Milestone 8](#14-milestone-8-host-language-span-tracking-trace-in-pythonts)).
When `None`, the top-level span has no parent (it is the root).

#### 3. Emit events in `call_function()`

The engine wraps the VM execution with event emission **only if the
function is NOT an LLM function**. If it IS an LLM function, the
compiler-inserted `baml.events.send("function_start/end")` in the LLM
bytecode (Milestone 3) already emits these events — the engine must not
duplicate them.

```rust
// In BexEngine::call_function()

// Use the root span provided by the caller (bridge_cffi), or return early
// if no tracing is requested.
let Some(root_span) = root_span_id else {
    // No tracing — execute without emitting events (zero-cost path)
    return self.execute_function(function_name, args).await;
};

// parent_span_id comes from the host language's @trace context (M8).
// When set, this BAML call is a child of a host-language span.
let ctx = SpanContext {
    span_id: root_span.clone(),
    parent_span_id: parent_span_id.clone(),  // None if no host @trace, Some if nested
    root_span_id: root_span.clone(),
};

// Check if this is an LLM function (which self-instruments via compiler bytecode).
let is_llm_function = self.function_has_llm_meta(function_name);

// Emit FunctionStart only for non-LLM functions (expression functions).
// LLM functions emit their own FunctionStart from compiler-inserted bytecode.
if !is_llm_function {
    let named_args: Vec<(String, BexExternalValue)> = self
        .function_params(function_name)
        .unwrap_or_default()
        .iter()
        .zip(args.iter())
        .map(|((name, _ty), val)| {
            (name.to_string(), val.to_external_value())
        })
        .collect();

    event_bus::emit(RuntimeEvent {
        ctx: ctx.clone(),
        timestamp: web_time::SystemTime::now(),
        event: EventKind::Function(FunctionEvent::Start(FunctionStart {
            name: function_name.to_string(),
            args: named_args,
            is_stream: false, // set by caller for streaming path
        })),
    });
}

let start = web_time::Instant::now();

// --- existing: create VM, set entry point, run event loop ---
// Note: no event_tx parameter needed. The event loop calls event_bus::emit()
// directly when it intercepts SysOp::EventSend from compiler-inserted code.
let result = self.run_event_loop_with_epoch(&mut vm, my_epoch).await;

// Emit FunctionEnd only for non-LLM functions
if !is_llm_function {
    event_bus::emit(RuntimeEvent {
        ctx: ctx.clone(),
        timestamp: web_time::SystemTime::now(),
        event: EventKind::Function(FunctionEvent::End(FunctionEnd {
            name: function_name.to_string(),
            result: match &result {
                Ok(val) => Ok(val.clone()),
                Err(e) => Err(e.to_string()),
            },
            duration: start.elapsed(),
        })),
    });
}

result.and_then(|value| self.to_bex_external(value, &return_type))
```

Where `function_has_llm_meta()` checks the `body_meta` field on the
heap-allocated `Function` object:

```rust
/// Check if a function has LLM metadata (i.e., was declared with a prompt/client body).
fn function_has_llm_meta(&self, name: &str) -> bool {
    let Some((ptr, _kind)) = self.resolved_function_names.get(name) else {
        return false;
    };
    // SAFETY: ptr is from resolved_function_names, a compile-time object
    let obj = unsafe { ptr.get() };
    match obj {
        Object::Function(func) => matches!(func.body_meta, Some(FunctionMeta::Llm { .. })),
        _ => false,
    }
}
```

#### 4. No channel threading needed

Unlike the channel-based approach, `run_event_loop_with_epoch()` does
**not** need an `event_tx` parameter. When it intercepts
`SysOp::EventSend` (Milestone 3+), it calls `event_bus::emit()` directly.
The engine stores the `SpanStack` as a local variable in `call_function()`
and passes it to the event loop — but the event *delivery* is handled by
the global `EventStore`, not by a plumbed channel.

```rust
async fn run_event_loop_with_epoch(
    &self,
    vm: &mut BexVm,
    my_epoch: u64,
    // No event_tx parameter! Events go through event_bus::emit().
) -> Result<BexValue, EngineError> {
    // ... existing code unchanged for M1 ...
}
```

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_events/Cargo.toml` | **New** types + EventStore crate manifest | ~15 |
| `baml_events/src/lib.rs` | `RuntimeEvent`, `EventKind`, `SpanContext`, `SpanId` | ~60 |
| `baml_events/src/function.rs` | `FunctionEvent`, `FunctionStart`, `FunctionEnd` | ~30 |
| `baml_events/src/event_store.rs` | `EventStore`, `emit()`, `track()`/`untrack()` | ~80 |
| `baml_collector/Cargo.toml` | **New** collector crate manifest | ~10 |
| `baml_collector/src/lib.rs` | `Collector`, `track_call()`, `function_logs()`, `Drop` | ~100 |
| `bex_engine/Cargo.toml` | Add `baml_events` dependency | ~1 |
| `bex_engine/src/lib.rs` | New `call_function` signature with `root_span_id`, emit via `event_bus::emit()` | ~40 |

### Test

```rust
#[tokio::test]
async fn test_top_level_function_events() {
    let engine = /* create test engine with a simple expression function "Add" */;

    // Create root span and register a collector to track it
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    let result = engine.call_function("Add", &[BexValue::Int(2), BexValue::Int(3)], Some(root_span)).await.unwrap();
    assert_eq!(result, BexExternalValue::Int(5));

    // Query events from the global EventStore via the collector
    let events = event_store::events_for_span(&root_span).unwrap();

    assert_eq!(events.len(), 2);

    // First event: FunctionStart
    assert!(matches!(&events[0].event, EventKind::Function(FunctionEvent::Start(s)) if s.name == "Add"));

    // Second event: FunctionEnd
    assert!(matches!(&events[1].event, EventKind::Function(FunctionEvent::End(e)) if e.name == "Add"));
    assert!(matches!(&events[1].event, EventKind::Function(FunctionEvent::End(e)) if e.result == Ok(BexExternalValue::Int(5))));

    // Both share same span
    assert_eq!(events[0].ctx.span_id, events[1].ctx.span_id);
    assert!(events[0].ctx.parent_span_id.is_none()); // root call
}
```

---

## 8. Milestone 2: Global Publisher for S3 / Boundary API

### Goal

After M1 establishes the global `EventStore` with `FunctionStart`/`FunctionEnd`,
wire up a **global singleton publisher** that forwards events to the
Boundary API / S3 — the same destination as `engine/`'s `publisher.rs`.
The publisher registers itself as the `publisher_sink` on the `EventStore`,
so all emitted events are automatically forwarded. This gives us production
observability from the very first milestone that emits events.

### Why This Comes Early

Publishing is the primary consumer of events in production. Without it,
events only live in the in-process Collector (useful for host-language
querying but invisible to the Boundary dashboard). Moving this to M2
means:

1. **Immediate production value** — the moment M1 emits events, M2
   ships them externally.
2. **End-to-end validation** — we can verify the full pipeline (emit →
   batch → compress → S3 → Boundary ingest) before adding complexity
   (nested calls, LLM events, headers).
3. **Flush/shutdown correctness** — getting lifecycle management right
   early prevents surprises when concurrency increases in later milestones.

### Why a Global Singleton (Not Per-Call)

The v2 design uses a global `EventStore` for event storage (mirroring
`engine/`'s `BAML_TRACER`), with collectors tracking specific `root_span_id`s.
Publishing is a separate concern that receives **all** events:

| Concern | Scope | Right pattern |
|---------|-------|---------------|
| **EventStore** (storage) | Global, ref-counted | Global `Lazy<Mutex<EventStore>>` |
| **Collector** (local query) | Per-call | Tracks `root_span_id`s, queries `EventStore` |
| **Publisher** (S3/Boundary) | Global, cross-call | Global singleton + background task (registered as `publisher_sink`) |

Reasons a global singleton is necessary for publishing:

- **Cross-call batching** — the publisher batches events from all
  concurrent `call_function()` invocations. Per-call channels would
  fragment batches and reduce upload efficiency.
- **Shared HTTP client** — one connection pool, one TLS session cache.
- **Single backpressure point** — one bounded channel to apply
  back-pressure when the Boundary API is slow.
- **Unified flush** — `baml.flush()` flushes one publisher, not N
  active calls.

### How `engine/` Does It Today

`engine/`'s `publisher.rs` uses `OnceCell` statics for the channel and
task handles:

```rust
static PUBLISHING_CHANNEL: OnceCell<mpsc::Sender<PublisherMessage>> = OnceCell::new();
static PUBLISHING_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();
static BLOB_UPLOADER_CHANNEL: OnceCell<mpsc::Sender<BlobUploaderMessage>> = OnceCell::new();
static BLOB_UPLOADER_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();
```

`start_publisher()` is called once during `BamlRuntime::new_runtime()`.
It checks for `BOUNDARY_API_KEY`, creates bounded channels, and spawns
two background tasks:

- **`TracePublisher`** — batches trace events (default 500), serializes
  to JSON, gzip-compresses if > 2 MB, uploads to S3 via presigned URL.
- **`BlobUploader`** — batches large media blobs (default 10), uploads
  separately.

Events enter via `publish_trace_event()` which does a non-blocking
`try_send` — events are dropped (with a warning) if the queue is full.

### Design for `baml_language`

We replicate this pattern in a new **`baml_publisher`** crate, separate
from the types crate (see [Section 3.4](#34-crate-separation-types-vs-capabilities)).

#### Global Publisher

```rust
// baml_language/crates/baml_publisher/src/lib.rs

use std::sync::Arc;
use core::time::Duration;
use once_cell::sync::OnceCell;
use tokio::sync::mpsc;

use baml_events::RuntimeEvent;

/// Global publisher channel.
/// Bounded to `4 * batch_size` to prevent unbounded memory growth.
static EVENT_PUBLISHER: OnceCell<mpsc::Sender<PublisherMessage>> = OnceCell::new();

#[cfg(not(target_arch = "wasm32"))]
static PUBLISHER_TASK: OnceCell<Arc<tokio::task::JoinHandle<()>>> = OnceCell::new();

enum PublisherMessage {
    Event(RuntimeEvent),
    Flush(tokio::sync::oneshot::Sender<()>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

/// Configuration for the publisher, read from environment variables.
pub struct PublisherConfig {
    pub api_key: Option<String>,
    pub base_url: String,
    pub batch_size: usize,
    pub compression_threshold_mb: f64,
    pub max_upload_mb: usize,
}

impl Default for PublisherConfig {
    fn default() -> Self {
        Self {
            api_key: None,
            base_url: "https://api.boundaryml.com".into(),
            batch_size: 500,
            compression_threshold_mb: 2.0,
            max_upload_mb: 10,
        }
    }
}

/// Initialize the global publisher. Called once at engine/CFFI startup.
/// No-op if `BOUNDARY_API_KEY` is not set or publisher is already running.
pub fn start_publisher(
    config: PublisherConfig,
    #[cfg(not(target_arch = "wasm32"))] rt: Arc<tokio::runtime::Runtime>,
) {
    if config.api_key.is_none() {
        log::debug!("Skipping publisher: BOUNDARY_API_KEY not set");
        return;
    }

    EVENT_PUBLISHER.get_or_init(|| {
        let capacity = config.batch_size * 4;
        let (tx, rx) = mpsc::channel(capacity);
        let publisher = TracePublisher::new(rx, config);

        #[cfg(not(target_arch = "wasm32"))]
        {
            let handle = rt.spawn(async move { publisher.run().await });
            PUBLISHER_TASK.get_or_init(|| Arc::new(handle));
        }

        #[cfg(target_arch = "wasm32")]
        {
            wasm_bindgen_futures::spawn_local(async move {
                publisher.run().await;
            });
        }

        tx
    });
}

/// Fire-and-forget: forward an event to the global publisher.
/// Returns silently if the publisher is not initialized or the queue is full.
pub fn publish_event(event: RuntimeEvent) {
    if let Some(tx) = EVENT_PUBLISHER.get() {
        match tx.try_send(PublisherMessage::Event(event)) {
            Ok(()) => {}
            Err(mpsc::error::TrySendError::Full(_)) => {
                log::warn!(
                    "Trace event queue full. Dropping event. \
                     Consider increasing BAML_TRACE_BATCH_SIZE."
                );
            }
            Err(mpsc::error::TrySendError::Closed(_)) => {
                log::warn!("Trace publisher channel closed.");
            }
        }
    }
}

/// Flush all pending events to S3. Blocks until the current batch is uploaded.
pub async fn flush() -> anyhow::Result<()> {
    let Some(tx) = EVENT_PUBLISHER.get() else {
        return Ok(());
    };
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
    tx.send(PublisherMessage::Flush(ack_tx))
        .await
        .map_err(|e| anyhow::anyhow!("Publisher channel closed: {e}"))?;
    tokio::time::timeout(Duration::from_secs(30), ack_rx)
        .await
        .map_err(|_| anyhow::anyhow!("Flush timed out"))??;
    Ok(())
}
```

#### TracePublisher (Background Task)

The `TracePublisher` is structurally identical to `engine/`'s. It:

1. Collects `RuntimeEvent`s from the bounded channel.
2. Batches them (default 500, configurable via `BAML_TRACE_BATCH_SIZE`).
3. Flushes on batch-full or every 2 seconds (whichever comes first).
4. Converts events to RPC format (reusing `baml_rpc` types).
5. Serializes to JSON, gzip-compresses if above threshold.
6. Requests a presigned S3 URL from the Boundary API
   (`CreateTraceEventUploadUrl`).
7. Uploads via HTTP PUT with S3 metadata headers.

```rust
struct TracePublisher {
    rx: mpsc::Receiver<PublisherMessage>,
    config: PublisherConfig,
    client: reqwest::Client,
}

impl TracePublisher {
    fn new(rx: mpsc::Receiver<PublisherMessage>, config: PublisherConfig) -> Self {
        Self { rx, config, client: reqwest::Client::new() }
    }

    async fn run(mut self) {
        let mut buffer: Vec<RuntimeEvent> = Vec::new();
        let mut tick = tokio::time::interval(Duration::from_secs(2));

        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => match msg {
                    PublisherMessage::Event(event) => {
                        buffer.push(event);
                        if buffer.len() >= self.config.batch_size {
                            self.upload_batch(std::mem::take(&mut buffer)).await;
                        }
                    }
                    PublisherMessage::Flush(ack) => {
                        if !buffer.is_empty() {
                            self.upload_batch(std::mem::take(&mut buffer)).await;
                        }
                        let _ = ack.send(());
                    }
                    PublisherMessage::Shutdown(ack) => {
                        if !buffer.is_empty() {
                            self.upload_batch(std::mem::take(&mut buffer)).await;
                        }
                        let _ = ack.send(());
                        break;
                    }
                },
                _ = tick.tick() => {
                    if !buffer.is_empty() {
                        self.upload_batch(std::mem::take(&mut buffer)).await;
                    }
                }
            }
        }
    }

    async fn upload_batch(&self, batch: Vec<RuntimeEvent>) {
        // 1. Convert RuntimeEvents → RPC TraceEventBatch
        // 2. Serialize to JSON bytes
        // 3. Gzip compress if > compression_threshold_mb
        // 4. Request presigned S3 URL from Boundary API
        // 5. PUT to S3 with metadata headers
        // (mirrors engine/'s process_batch_impl)
    }
}
```

#### Integration with the Global EventStore

Unlike the earlier channel-based design, the publisher does not receive
events through a per-call reader task. Instead, it registers itself as
the `publisher_sink` on the global `EventStore`:

```rust
// baml_language/crates/baml_publisher/src/lib.rs — initialization

pub fn start_publisher(config: PublisherConfig) {
    // ... (OnceCell channel + task setup as above) ...

    // Register with the global EventStore.
    // Every event_bus::emit() call will now also forward to the publisher.
    let tx = EVENT_PUBLISHER.get().unwrap().clone();
    baml_events::event_store::set_publisher_sink(move |event| {
        let _ = tx.try_send(PublisherMessage::Event(event));
    });
}
```

This means:

- **No reader task needed** — the `EventStore::emit()` function handles
  fan-out internally (store for collectors + forward to publisher sink).
- The Collector queries events from the `EventStore` (pull model, per-call scope).
- The Publisher receives events from the `EventStore` (push model, global scope,
  cross-call batching).
- Neither blocks the other — `try_send()` inside the sink is fire-and-forget.

#### Initialization

`start_publisher()` is called once during CFFI initialization (or the
first `call_function()` invocation). It reads configuration from
environment variables, matching `engine/`'s behavior:

| Env var | Default | Purpose |
|---------|---------|---------|
| `BOUNDARY_API_KEY` | (none) | Required. Publisher is disabled without it. |
| `BOUNDARY_API_URL` | `https://api.boundaryml.com` | Boundary API base URL. |
| `BAML_TRACE_BATCH_SIZE` | `500` | Events per upload batch. |
| `BAML_TRACE_COMPRESSION_THRESHOLD_MB` | `2.0` | Gzip threshold. |
| `BAML_MAX_TRACE_UPLOAD_MB` | `10` | Max upload size (drop if exceeded). |

#### Blob Uploads (Deferred)

The `BlobUploader` (for large media like images/audio in function args)
is deferred to a later milestone. In M2, function args containing blobs
are serialized inline. The blob extraction + separate upload channel
(`BLOB_UPLOADER_CHANNEL`) will be added when media-heavy workloads
require it.

### Comparison with `engine/` Publisher

| Aspect | `engine/` Publisher | `baml_language` Publisher |
|--------|-----------------------|--------------------------|
| **Singleton** | `OnceCell<mpsc::Sender<PublisherMessage>>` | Same pattern |
| **Background task** | `TracePublisher::run()` on Tokio runtime | Same |
| **Batching** | 500 events or 2s timer | Same |
| **Compression** | Gzip if > 2 MB | Same |
| **S3 upload** | Presigned URL from Boundary API | Same |
| **Backpressure** | `try_send`, drop on full | Same |
| **Blob handling** | Separate `BlobUploader` task | Deferred |
| **Event format** | `TraceEventWithMeta` → RPC conversion | `RuntimeEvent` → RPC conversion |
| **Runtime update** | `PublisherMessage::UpdateRuntime` | Not needed (no global runtime swap) |

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_publisher/Cargo.toml` | **New** publisher crate manifest | ~15 |
| `baml_publisher/src/lib.rs` | **New** — `start_publisher()`, `publish_event()`, `flush()`, `PublisherConfig` | ~80 |
| `baml_publisher/src/publisher.rs` | `TracePublisher` struct, `run()` loop, `upload_batch()` | ~120 |
| `bridge_cffi/src/ffi/init.rs` | Call `start_publisher()` during initialization (registers publisher_sink with EventStore) | ~10 |
| `bridge_cffi/Cargo.toml` | Add `baml_publisher` dependency | ~1 |

### Test

```rust
#[tokio::test]
async fn test_publisher_receives_events() {
    // Use a mock HTTP server (e.g., wiremock) to verify S3 uploads
    let mock_server = MockServer::start().await;

    // Mock the presigned URL endpoint
    Mock::given(method("POST"))
        .and(path("/api/v1/trace/upload-url"))
        .respond_with(ResponseTemplate::new(200).set_body_json(json!({
            "upload_url": format!("{}/s3/upload", mock_server.uri()),
            "upload_metadata": {}
        })))
        .mount(&mock_server)
        .await;

    // Mock the S3 PUT
    Mock::given(method("PUT"))
        .and(path("/s3/upload"))
        .respond_with(ResponseTemplate::new(200))
        .expect(1)
        .mount(&mock_server)
        .await;

    // Start publisher pointing at mock
    start_publisher(PublisherConfig {
        api_key: Some("test-key".into()),
        base_url: mock_server.uri(),
        batch_size: 2, // small batch for testing
        ..Default::default()
    }, rt.clone());

    // Publish events
    publish_event(make_function_start_event("Foo"));
    publish_event(make_function_end_event("Foo"));

    // Flush and verify
    flush().await.unwrap();
    // The S3 PUT mock expectation verifies the upload happened
}

#[tokio::test]
async fn test_publisher_disabled_without_api_key() {
    start_publisher(PublisherConfig::default(), rt.clone());
    // publish_event should silently no-op
    publish_event(make_function_start_event("Foo"));
    // No panic, no error
}

#[tokio::test]
async fn test_publisher_drops_on_backpressure() {
    start_publisher(PublisherConfig {
        api_key: Some("test-key".into()),
        batch_size: 1, // tiny batch
        ..Default::default()
    }, rt.clone());

    // Fill the queue (capacity = 4 * batch_size = 4)
    for i in 0..10 {
        publish_event(make_function_start_event(&format!("Func{i}")));
    }
    // Should not panic — excess events are dropped with a warning
}
```

---

## 9. Milestone 3: FunctionStart / FunctionEnd for Nested LLM Calls

### Goal

When `Foo()` (expression function) calls `Bar()` (LLM function), we see
events for both: `Foo.Start` -> `Bar.Start` -> `Bar.End` -> `Foo.End`.

### Background: How LLM Functions Work Today

The LLM calling sequence is already written as **real BAML code** in
`baml_builtins/baml/llm.baml`. The key function is `call_llm_function()`:

```baml
// baml_builtins/baml/llm.baml

function call_llm_function(function_name: string, args: map<string, unknown>) -> string {
    let jinja_string = baml.llm.get_jinja_template(function_name);
    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();
    let prompt = primitive_client.render_prompt(jinja_string, args);
    let specialized_prompt = primitive_client.specialize_prompt(prompt);
    let http_request = primitive_client.build_request(specialized_prompt);
    let http_response = baml.http.send(http_request);
    return primitive_client.parse(http_response, function_name);
}
```

This file is compiled as part of the builtin library and produces real
bytecode. The individual steps (`get_jinja_template`, `render_prompt`, etc.)
are `#[sys_op]` builtins handled by the engine.

**However**, user-defined LLM functions (with `FunctionBody::Llm`) are
currently compiled as metadata-only stubs:

```rust
// baml_compiler_emit/src/lib.rs — current LLM function compilation
Function {
    kind: FunctionKind::SysOp(SysOp::RenderPrompt),  // stub!
    bytecode: Bytecode::new(),                        // empty!
    body_meta: Some(FunctionMeta::Llm { prompt_template, client }),
}
```

There's an existing TODO to change this:
```
// TODO: Eventually these should compile to bytecode that calls
// `baml.llm.render_prompt` orchestrator.
```

### Approach: Add `baml.events.send()` to `llm.baml` + Compile LLM Functions to Call It

Since the LLM calling sequence already exists as BAML code, we:

1. **Add `baml.events.send()` calls directly into `llm.baml`** — this is
   just editing a `.baml` file, not generating bytecode by hand.
2. **Change `FunctionBody::Llm` compilation** to generate a function body
   that calls `call_llm_function(function_name, args)` instead of being a
   stub. This fulfills the existing TODO.
3. **Add `SysOp::EventSend`**, `SysOp::NewRequestId`, and `baml.events.*`
   as new builtins.

### What This Requires

#### 1. Add `SysOp::EventSend` and `SysOp::NewRequestId`

**File**: `baml_language/crates/bex_vm_types/src/types.rs`

```rust
pub enum SysOp {
    // ... existing variants ...

    /// Send a runtime event (fire-and-forget).
    /// Arguments: [event_type: String, payload: Map<String, Unknown>]
    /// Returns: Null
    EventSend,

    /// Generate a new unique RequestId (UUID v4 string).
    /// Arguments: none
    /// Returns: String
    NewRequestId,
}
```

#### 2. Add `baml.events` builtins

**File**: `baml_language/crates/baml_builtins/src/lib.rs`

Add to the `with_builtins!` macro:

```rust
// =====================================================================
// Event operations (fire-and-forget tracing events)
// =====================================================================
mod events {
    /// Send a runtime event.
    /// event_type: "function_start" | "function_end" | "llm_request" | etc.
    /// payload: Map with event-specific fields.
    #[sys_op]
    fn send(event_type: String, payload: Map<String, Unknown>) -> Null;

    /// Generate a new unique RequestId (UUID v4 as a string).
    /// Used to correlate LLM events within a single call attempt.
    #[sys_op]
    fn new_request_id() -> String;
}
```

#### 3. Register `SysOp::EventSend` and `SysOp::NewRequestId` in the compiler

**File**: `baml_language/crates/baml_compiler_emit/src/lib.rs`

```rust
fn sys_op_for_builtin_path(path: &str) -> Option<SysOp> {
    match path {
        // ... existing mappings ...
        "baml.events.send" => Some(SysOp::EventSend),
        "baml.events.new_request_id" => Some(SysOp::NewRequestId),
        _ => None,
    }
}
```

#### 4. Handle `SysOp::EventSend` in the engine

**File**: `baml_language/crates/bex_engine/src/lib.rs`

Handle `EventSend` inline in the `ScheduleFuture` arm, before the generic
`execute_sys_op()` dispatch. No channel parameter needed — calls
`event_bus::emit()` directly:

```rust
VmExecState::ScheduleFuture(id) => {
    let pending = vm.pending_future(id)?;
    let args = self.vm_args_to_bex_values(vm, &pending.args);

    if pending.operation == SysOp::EventSend {
        // Fire-and-forget event emission via global EventBus
        if let Some(event) = self.build_event_from_args(&args, &span_stack) {
            event_bus::emit(event);
        }
        // Always succeeds, returns Null
        vm.set_future_ready(id, Value::Null)?;
        continue;
    }

    // ... existing SysOp dispatch ...
}
```

Note: no `event_tx` guard needed. If no collector is tracking the
`root_span_id`, the event is still forwarded to the publisher sink
(if configured) but not stored — this is handled inside `EventStore::emit()`.

Where `build_event_from_args()` converts the `(event_type, payload)` args
into a `RuntimeEvent` using the current span context from the span stack
(Milestone 4).

#### 5. Add event sends to `llm.baml`

**File**: `baml_language/crates/baml_builtins/baml/llm.baml`

The LLM calling sequence already exists as real BAML code. We just add
`baml.events.send()` calls to `call_llm_function()`. In M3 we only add
`function_start`/`function_end`; intermediate LLM events are added in M5.

```baml
function call_llm_function(function_name: string, args: map<string, unknown>) -> string {
    // Emit function_start
    baml.events.send("function_start", { "name": function_name, "args": args })

    let jinja_string = baml.llm.get_jinja_template(function_name);
    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    let prompt = primitive_client.render_prompt(jinja_string, args);
    let specialized_prompt = primitive_client.specialize_prompt(prompt);

    let http_request = primitive_client.build_request(specialized_prompt);
    let http_response = baml.http.send(http_request);
    let parse_result = primitive_client.parse(http_response, function_name);

    // Emit function_end
    baml.events.send("function_end", { "name": function_name, "result": parse_result.content })

    return parse_result.content;
}
```

This is the key simplification: **we're just editing a `.baml` file**, not
generating bytecode by hand. The `call_llm_function` is compiled to real
bytecode like any other BAML function, and the `baml.events.send()` calls
become `SysOp::EventSend` dispatches naturally.

> **Note**: Error handling (guaranteeing `function_end` on failure) and
> retry orchestration are deferred — see [Section 14](#14-deferred-work).

#### 6. LLM Function Delegation Strategy

**File**: `baml_language/crates/baml_compiler_emit/src/lib.rs`

Change the `FunctionBody::Llm` compilation from the current metadata-only
stub to generating a body that delegates to `call_llm_function()`.
This fulfills the existing TODO:

```
// TODO: Eventually these should compile to bytecode that calls
// `baml.llm.render_prompt` orchestrator.
```

The user writes:
```baml
function ExtractResume(text: string) -> Resume {
    client "openai/gpt-4o"
    prompt #"Extract resume info from: {{ text }}"#
}
```

The compiler generates bytecode equivalent to:
```baml
function ExtractResume(text: string) -> Resume {
    return call_llm_function("ExtractResume", { "text": text });
}
```

The `call_llm_function` in `llm.baml` handles the entire LLM sequence
including event emission — the compiler just needs to generate the
delegation call.

**Why delegation instead of inlining**: By delegating to a single
`call_llm_function()` implementation in `llm.baml`, all LLM functions
share the same instrumented code path. Adding new events (M5), retries,
or error handling only requires changing one `.baml` file. The compiler
doesn't need to re-emit per-function bytecode — it just generates a
single call instruction.

#### 7. Return Type Casting (`string` → actual return type)

`call_llm_function()` returns `string` (the raw parsed content from
`parse_result.content`). But the user's LLM function may declare a
return type of `Resume`, `int`, or any other BAML type. The compiler
must insert a cast/parse step.

**Approach**: The compiler emits the delegation call followed by a
type coercion instruction. The generated bytecode is equivalent to:

```baml
function ExtractResume(text: string) -> Resume {
    let raw: string = call_llm_function("ExtractResume", { "text": text });
    return baml.parse(raw, Resume);  // parse string into target type
}
```

Where `baml.parse(value, TargetType)` is a builtin that uses the
existing JSON/text parser infrastructure to coerce the string into the
declared return type. This mirrors how `engine/`'s output parsing works
— the LLM returns text, and it's parsed into the declared output type.

Alternatively, `primitive_client.parse()` could be made type-aware
(receiving the target type as a parameter), so it returns the correctly
typed value directly. This decision depends on whether parsing happens
inside or outside the `call_llm_function()` boundary. Either way, the
return value flowing out of the user's LLM function will be correctly
typed — the compiler ensures the cast.

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_vm_types/src/types.rs` | Add `SysOp::EventSend`, `SysOp::NewRequestId` variants | ~5 |
| `baml_builtins/src/lib.rs` | Add `baml.events.send()` and `baml.events.new_request_id()` builtins | ~12 |
| `baml_builtins/baml/llm.baml` | Add `baml.events.send()` calls to `call_llm_function()` | ~4 |
| `baml_compiler_emit/src/lib.rs` | Map `baml.events.*` → `SysOp::EventSend` / `SysOp::NewRequestId` | ~2 |
| `baml_compiler_emit/src/lib.rs` | Change `FunctionBody::Llm` to delegate to `call_llm_function()` + return type cast | ~25 |
| `bex_engine/src/lib.rs` | Handle `SysOp::EventSend` and `SysOp::NewRequestId` in event loop | ~25 |
| `bex_engine/src/lib.rs` | `build_event_from_args()` helper | ~40 |

**Note**: The previous design assumed we'd need to manually generate ~15-20
bytecode instructions per LLM function. Since `llm.baml` already contains
the full calling sequence as BAML code, we just edit that file and change
the compiler to delegate to it. Much simpler.

### Test

```rust
#[tokio::test]
async fn test_nested_llm_function_events() {
    // Setup: expression function "Outer" calls LLM function "Inner"
    let engine = /* create test engine */;
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    let result = engine.call_function("Outer", &args, Some(root_span.clone())).await.unwrap();

    let events = event_store::events_for_span(&root_span).unwrap();

    // Event order: Outer.Start → Inner.Start → Inner.End → Outer.End
    assert_eq!(events.len(), 4);
    assert!(matches!(&events[0].event, EventKind::Function(FunctionEvent::Start(s)) if s.name == "Outer"));
    assert!(matches!(&events[1].event, EventKind::Function(FunctionEvent::Start(s)) if s.name == "Inner"));
    assert!(matches!(&events[2].event, EventKind::Function(FunctionEvent::End(e)) if e.name == "Inner"));
    assert!(matches!(&events[3].event, EventKind::Function(FunctionEvent::End(e)) if e.name == "Outer"));
}
```

---

## 10. Milestone 4: Span Context and Nested Call IDs

### Goal

Every event carries a `SpanContext` that enables reconstructing the call
tree. Each function call gets a unique span ID. Child spans reference
their parent.

### How `engine/` Does It

`engine/` uses `call_id_stack: Vec<FunctionCallId>` maintained in
`RuntimeContextManager`. Each `start_call()` pushes a new ID. Each
`finish_call()` pops. All `TraceEvent`s carry the full stack as
`call_stack: Vec<FunctionCallId>`.

### Design for `baml_language`

The engine maintains a **`SpanStack`** alongside the VM. This stack tracks
the current span hierarchy.

```rust
// In bex_engine/src/lib.rs (or a new span.rs)

/// A single entry in the span stack, tracking both identity and timing.
struct SpanEntry {
    span_id: SpanId,
    /// When this span was pushed (for computing FunctionEnd.duration).
    started_at: web_time::Instant,
}

/// Tracks the current span hierarchy during execution.
///
/// Thread-safety note: The SpanStack is owned by a single async task
/// (the event loop) and passed as `&mut`. No cross-thread sharing.
struct SpanStack {
    root: SpanId,
    /// Parent of the root span, if this BAML call is nested under a
    /// host-language @trace span (M8). None if this is a top-level call.
    parent_of_root: Option<SpanId>,
    stack: Vec<SpanEntry>,
}

impl SpanStack {
    fn new(root_span_id: SpanId) -> Self {
        Self::new_with_parent(root_span_id, None)
    }

    /// Create a new span stack where the root span optionally has a parent
    /// from the host language's @trace context (M8).
    fn new_with_parent(root_span_id: SpanId, parent_span_id: Option<SpanId>) -> Self {
        Self {
            root: root_span_id.clone(),
            parent_of_root: parent_span_id,
            stack: vec![SpanEntry {
                span_id: root_span_id,
                started_at: web_time::Instant::now(),
            }],
        }
    }

    /// Push a new child span. Returns the new SpanContext.
    fn push(&mut self) -> SpanContext {
        let parent = self.stack.last().map(|e| e.span_id.clone());
        let span_id = SpanId::new();
        self.stack.push(SpanEntry {
            span_id: span_id.clone(),
            started_at: web_time::Instant::now(),
        });
        SpanContext {
            span_id,
            parent_span_id: parent,
            root_span_id: self.root.clone(),
        }
    }

    /// Pop the current span. Returns (context, duration) of the popped span.
    fn pop(&mut self) -> Option<(SpanContext, std::time::Duration)> {
        if self.stack.len() <= 1 {
            return None; // Don't pop the root
        }
        let entry = self.stack.pop()?;
        let duration = entry.started_at.elapsed();
        let parent = self.stack.last().map(|e| e.span_id.clone());
        Some((
            SpanContext {
                span_id: entry.span_id,
                parent_span_id: parent,
                root_span_id: self.root.clone(),
            },
            duration,
        ))
    }

    /// Get the current span context (top of stack).
    fn current(&self) -> SpanContext {
        let entry = self.stack.last().unwrap();
        let parent = if self.stack.len() > 1 {
            self.stack.get(self.stack.len() - 2).map(|e| e.span_id.clone())
        } else {
            // Root span's parent is the host-language @trace span (if any)
            self.parent_of_root.clone()
        };
        SpanContext {
            span_id: entry.span_id.clone(),
            parent_span_id: parent,
            root_span_id: self.root.clone(),
        }
    }

    /// Get the elapsed duration of the current span (without popping).
    fn current_elapsed(&self) -> std::time::Duration {
        self.stack.last().unwrap().started_at.elapsed()
    }
}
```

### How Span IDs Are Assigned

| Event | Who assigns | Mechanism |
|-------|------------|-----------|
| Top-level `FunctionStart` | Engine | `SpanStack::new()` creates root span |
| Top-level `FunctionEnd` | Engine | Uses root span from `SpanStack` |
| Nested LLM `function_start` | Engine (intercepting `EventSend`) | Engine sees `event_type == "function_start"` → calls `span_stack.push()` |
| Nested LLM `function_end` | Engine (intercepting `EventSend`) | Engine sees `event_type == "function_end"` → calls `span_stack.pop()` |
| LLM events (request, response) | Engine (intercepting `EventSend`) | Uses `span_stack.current()` |
| Header events | Engine | Uses `span_stack.current()` |

**Key insight**: The compiler doesn't need to know about span IDs. It just
emits `baml.events.send("function_start", ...)`. The **engine** intercepts
these and stamps them with the correct span context by maintaining the
`SpanStack`.

### Integration in `run_event_loop_with_epoch()`

```rust
async fn run_event_loop_with_epoch(
    &self,
    vm: &mut BexVm,
    my_epoch: u64,
    span_stack: &mut SpanStack,  // no event_tx — uses event_bus::emit()
) -> Result<BexValue, EngineError> {
    // ...
    VmExecState::ScheduleFuture(id) => {
        let pending = vm.pending_future(id)?;
        let args = self.vm_args_to_bex_values(vm, &pending.args);

        if pending.operation == SysOp::EventSend {
            let event_type = extract_string(&args[0]);
            let payload = extract_map(&args[1]);

            // Span management and duration computation based on event type
            let (ctx, duration) = match event_type.as_str() {
                "function_start" => {
                    (span_stack.push(), None)  // new child span
                }
                "function_end" => {
                    let ctx = span_stack.current();
                    let (_, dur) = span_stack.pop()
                        .expect("function_end without matching function_start");
                    (ctx, Some(dur))
                }
                _ => (span_stack.current(), None),  // LLM events use current span
            };

            // Direct global call — no channel plumbing needed
            event_bus::emit(RuntimeEvent {
                ctx,
                timestamp: web_time::SystemTime::now(),
                event: build_event_kind(&event_type, payload, duration),
            });

            vm.set_future_ready(id, Value::Null)?;
            continue;
        }
        // ... existing SysOp dispatch ...
    }
}
```

**Duration flow**: The `FunctionEnd.duration` field is computed by the
engine, not by `llm.baml`. When the engine sees `"function_end"` in an
`EventSend`, it pops the span stack (which returns the elapsed time since
the matching `push()`) and passes that duration to `build_event_kind()`.
The `llm.baml` code does not need to compute or pass duration -- it's
purely engine-side bookkeeping.

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_events/src/lib.rs` | `SpanContext`, `SpanId` (already defined in M1) | 0 |
| `bex_engine/src/lib.rs` | `SpanStack` struct with `SpanEntry`, `parent_of_root` (tracks timing per span) | ~70 |
| `bex_engine/src/lib.rs` | Span management + duration computation in `EventSend` handler | ~25 |
| `bex_engine/src/lib.rs` | Pass `SpanStack` through call_function → event loop, `new_with_parent()` | ~10 |

### Test

```rust
#[tokio::test]
async fn test_span_context_nesting() {
    // A() calls B() calls C()
    let engine = /* test engine */;
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    engine.call_function("A", &[], Some(root_span.clone())).await.unwrap();
    let events = event_store::events_for_span(&root_span).unwrap();

    // A.Start
    let a_start = &events[0];
    assert!(a_start.ctx.parent_span_id.is_none());
    let a_span = a_start.ctx.span_id.clone();
    let root = a_start.ctx.root_span_id.clone();

    // B.Start (child of A)
    let b_start = &events[1];
    assert_eq!(b_start.ctx.parent_span_id, Some(a_span.clone()));
    assert_eq!(b_start.ctx.root_span_id, root);
    let b_span = b_start.ctx.span_id.clone();

    // C.Start (child of B)
    let c_start = &events[2];
    assert_eq!(c_start.ctx.parent_span_id, Some(b_span.clone()));
    assert_eq!(c_start.ctx.root_span_id, root);

    // C.End (same span as C.Start)
    let c_end = &events[3];
    assert_eq!(c_end.ctx.span_id, c_start.ctx.span_id);

    // B.End
    assert_eq!(events[4].ctx.span_id, b_span);

    // A.End
    assert_eq!(events[5].ctx.span_id, a_span);
}
```

---

## 11. Milestone 5: Intermediate LLM Events

### Goal

Between `FunctionStart` and `FunctionEnd` for an LLM call, emit
fine-grained events: prompt rendered, raw HTTP request, raw HTTP response,
parsed response.

### RequestId Creation and Threading

Each LLM call attempt (one HTTP round-trip) needs a unique `RequestId` to
correlate its events (`LlmRequest` → `LlmRawRequest` → `LlmRawResponse`
→ `LlmResponse`). When retries are added (deferred), each retry gets a
new `RequestId`.

**Decision**: The `RequestId` is created in `llm.baml` via a new builtin
`baml.events.new_request_id()` and threaded through all event sends for
that attempt. This keeps the engine stateless with respect to request
correlation — the BAML code explicitly passes the ID.

#### New builtin: `baml.events.new_request_id()`

```rust
// In baml_builtins/src/lib.rs — add to the events module
mod events {
    #[sys_op]
    fn send(event_type: String, payload: Map<String, Unknown>) -> Null;

    /// Generate a new unique RequestId (UUID v4 as a string).
    #[sys_op]
    fn new_request_id() -> String;
}
```

The engine handles `SysOp::NewRequestId` by returning a new UUID string:

```rust
SysOp::NewRequestId => {
    let id = uuid::Uuid::new_v4().to_string();
    vm.set_future_ready(id, Value::String(id));
}
```

### How `model`, `finish_reason`, and `client_stack` Flow

The `primitive_client.parse()` SysOp (`SysOp::LlmParseResponse`) currently
panics with a TODO. When implemented, it will call `llm_ops::parse_response()`
which returns an `LlmProviderResponse` containing `content`, `model`,
`finish_reason`, and `usage` (see `llm_ops/src/parse_response/types.rs`).

**Decision**: `primitive_client.parse()` returns a **map** (not just a
string) containing the parsed content plus metadata:

```
{
    "content": "the extracted text",
    "model": "gpt-4o-2024-08-06",
    "finish_reason": "stop",
    "usage": { "input_tokens": 150, "output_tokens": 42 },
    "client_stack": ["GPT4"]  // populated by the engine from client resolution
}
```

The `call_llm_function()` code destructures this to pass metadata into
the event sends. The `client_stack` comes from the `primitive_client`
object which knows its own name and any parent strategy clients.

### What This Requires

Add more `baml.events.send()` calls to `call_llm_function()` in
`baml_builtins/baml/llm.baml`. Since this is plain BAML code, no
bytecode generation is involved:

**File**: `baml_language/crates/baml_builtins/baml/llm.baml`

```baml
function call_llm_function(function_name: string, args: map<string, unknown>) -> string {
    baml.events.send("function_start", { "name": function_name, "args": args })

    // Generate a unique RequestId for this LLM call attempt.
    // When retries are added, this will be inside the retry loop so each
    // attempt gets its own ID.
    let request_id = baml.events.new_request_id()

    let jinja_string = baml.llm.get_jinja_template(function_name);
    let resolve_client_fn = baml.llm.get_client_function(function_name);
    let primitive_client = resolve_client_fn();

    let prompt = primitive_client.render_prompt(jinja_string, args);
    let specialized_prompt = primitive_client.specialize_prompt(prompt);

    // Emit llm_request after prompt is ready
    baml.events.send("llm_request", {
        "request_id": request_id,
        "prompt": specialized_prompt,
        "client_name": primitive_client.client_name,
        "provider": primitive_client.provider,
        "params": primitive_client.params
    })

    let http_request = primitive_client.build_request(specialized_prompt);

    // Emit llm_raw_request before sending
    baml.events.send("llm_raw_request", {
        "request_id": request_id,
        "method": http_request.method,
        "url": http_request.url,
        "headers": http_request.headers,
        "body": http_request.body
    })

    let http_response = baml.http.send(http_request);

    // Emit llm_raw_response after receiving
    baml.events.send("llm_raw_response", {
        "request_id": request_id,
        "status_code": http_response.status_code,
        "headers": http_response.headers,
        "body": http_response.body
    })

    // parse() returns a map with content + metadata (model, finish_reason, usage, client_stack)
    let parse_result = primitive_client.parse(http_response, function_name);

    // Emit llm_response after parsing
    baml.events.send("llm_response", {
        "request_id": request_id,
        "value": parse_result.content,
        "raw_text_output": parse_result.content,
        "model": parse_result.model,
        "finish_reason": parse_result.finish_reason,
        "usage": parse_result.usage,
        "client_stack": parse_result.client_stack,
        "is_success": true
    })

    baml.events.send("function_end", { "name": function_name, "result": parse_result.content })
    return parse_result.content;
}
```

This is just editing a `.baml` file — the compiler handles everything else.

### Engine: `build_event_kind()` Handles New Event Types

The `duration` parameter is passed from the event loop (computed by the
`SpanStack` on pop). `RequestId` and metadata fields are extracted from
the payload map.

```rust
fn build_event_kind(
    event_type: &str,
    payload: &[(String, BexExternalValue)],
    duration: Option<std::time::Duration>,
) -> EventKind {
    match event_type {
        "function_start" => EventKind::Function(FunctionEvent::Start(FunctionStart {
            name: get_string(payload, "name"),
            args: get_named_values(payload, "args"),
            is_stream: false, // streaming path sets this separately
        })),
        "function_end" => EventKind::Function(FunctionEvent::End(FunctionEnd {
            name: get_string(payload, "name"),
            result: get_result(payload, "result"),
            duration: duration.unwrap_or_default(), // computed by SpanStack
        })),
        "llm_request" => EventKind::Llm(LlmEvent::Request(LlmRequest {
            request_id: RequestId::from_string(get_string(payload, "request_id")),
            prompt: get_value(payload, "prompt"),
            client_name: get_string(payload, "client_name"),
            provider: get_string(payload, "provider"),
            params: get_value(payload, "params"),
        })),
        "llm_raw_request" => EventKind::Llm(LlmEvent::RawRequest(LlmRawRequest {
            request_id: RequestId::from_string(get_string(payload, "request_id")),
            method: get_string(payload, "method"),
            url: get_string(payload, "url"),
            headers: get_string_pairs(payload, "headers"),
            body: get_string(payload, "body"),
        })),
        "llm_raw_response" => EventKind::Llm(LlmEvent::RawResponse(LlmRawResponse {
            request_id: RequestId::from_string(get_string(payload, "request_id")),
            status: get_int(payload, "status_code") as u16,
            headers: get_string_pairs(payload, "headers"),
            body: get_string(payload, "body"),
            duration: std::time::Duration::ZERO, // HTTP duration tracked by engine
        })),
        "llm_response" => EventKind::Llm(LlmEvent::Response(LlmResponse {
            request_id: RequestId::from_string(get_string(payload, "request_id")),
            value: get_value(payload, "value"),
            raw_text_output: get_opt_string(payload, "raw_text_output"),
            usage: get_opt_usage(payload, "usage"),
            model: get_opt_string(payload, "model"),
            finish_reason: get_opt_string(payload, "finish_reason"),
            client_stack: get_string_list(payload, "client_stack"),
            is_success: get_bool(payload, "is_success"),
            error_message: get_opt_string(payload, "error_message"),
        })),
        "header_enter" => EventKind::Header(HeaderEvent::Enter(HeaderEnter {
            name: get_string(payload, "name"),
        })),
        "header_exit" => EventKind::Header(HeaderEvent::Exit(HeaderExit {
            name: get_string(payload, "name"),
        })),
        other => {
            log::warn!("Unknown event type: {other}");
            // Skip unknown event types gracefully
            return EventKind::SetTags(vec![]);
        }
    }
}
```

Where `RequestId::from_string()` parses a UUID string back into a `RequestId`:

```rust
impl RequestId {
    pub fn new() -> Self { Self(uuid::Uuid::new_v4()) }

    pub fn from_string(s: String) -> Self {
        Self(uuid::Uuid::parse_str(&s).unwrap_or_else(|_| uuid::Uuid::new_v4()))
    }
}
```

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_builtins/baml/llm.baml` | Add `baml.events.send()` calls for intermediate events with `request_id` | ~30 |
| `bex_engine/src/lib.rs` | `build_event_kind()` handles `llm_*` event types incl. metadata extraction | ~80 |
| `baml_events/src/llm.rs` | LLM event types (already defined in Section 5.4, updated with `model`/`finish_reason`/`client_stack`) | 0 |
| `llm_ops/src/lib.rs` | Implement `execute_parse_response()` to return map with `content`/`model`/`finish_reason`/`usage`/`client_stack` | ~40 |

**Note**: All the LLM event instrumentation is done by editing `llm.baml`.
No compiler or bytecode changes needed beyond what M3 already provides.
The `model`, `finish_reason`, and `client_stack` fields come from the
`primitive_client.parse()` SysOp which returns them as part of its
result map.

### Test

```rust
#[tokio::test]
async fn test_llm_intermediate_events() {
    let engine = /* test engine with mock HTTP */;
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    engine.call_function("SomeLlmFunc", &args, Some(root_span.clone())).await.unwrap();
    let events = event_store::events_for_span(&root_span).unwrap();

    // Verify event sequence
    let kinds: Vec<&str> = events.iter().map(|e| match &e.event {
        EventKind::Function(FunctionEvent::Start(_)) => "fn_start",
        EventKind::Function(FunctionEvent::End(_)) => "fn_end",
        EventKind::Llm(LlmEvent::Request(_)) => "llm_request",
        EventKind::Llm(LlmEvent::RawRequest(_)) => "llm_raw_request",
        EventKind::Llm(LlmEvent::RawResponse(_)) => "llm_raw_response",
        EventKind::Llm(LlmEvent::Response(_)) => "llm_response",
        _ => "other",
    }).collect();

    // Top-level fn_start, then nested LLM sequence, then top-level fn_end
    // (exact order depends on whether top-level is expression or LLM)
    assert!(kinds.contains(&"llm_request"));
    assert!(kinds.contains(&"llm_raw_request"));
    assert!(kinds.contains(&"llm_raw_response"));
    assert!(kinds.contains(&"llm_response"));

    // All LLM events share the same span (the LLM function's span)
    let llm_events: Vec<_> = events.iter().filter(|e| matches!(&e.event, EventKind::Llm(_))).collect();
    let llm_span = &llm_events[0].ctx.span_id;
    for e in &llm_events {
        assert_eq!(&e.ctx.span_id, llm_span);
    }
}
```

---

## 12. Milestone 6: Publishing Events to Host Languages (CFFI)

### Goal

Python/TS can receive events and query the Collector via the existing CFFI
callback mechanism.

### What This Requires

#### 1. `bridge_cffi`: Accept collectors in `call_function_from_c`

**File**: `bridge_cffi/src/ffi/functions.rs`

Current code silently ignores collectors:

```rust
// Silently ignore collectors and type_builder (not supported)
// TODO: Support collectors when bex_engine adds support
```

New code:

```rust
fn call_function_inner(
    function_name: *const libc::c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    let engine = get_engine()?.clone();
    let func_name = /* parse function name */;
    let args = /* decode protobuf args */;

    // Extract collectors from protobuf
    let collectors: Vec<Arc<Collector>> = extract_collectors(&args);

    // ── Host-language span context (M8) ──
    // If the host language has an active @trace span, it passes its
    // span_id and root_span_id so this BAML call becomes a child.
    // If no host span is active, we create a fresh root.
    let host_ctx: Option<HostSpanContext> = extract_host_span_context(&args);

    let (root_span_id, parent_span_id) = match &host_ctx {
        Some(ctx) => {
            // BAML call is a child of the host-language @trace span.
            // Inherit the host's root and use the host's current span as parent.
            (ctx.root_span_id.clone(), Some(ctx.span_id.clone()))
        }
        None => {
            // No host span — this BAML call is a root.
            let root = SpanId::new();
            (root.clone(), None)
        }
    };

    for collector in &collectors {
        collector.track_call(root_span_id.clone());
    }

    // Reorder kwargs to match params
    let bex_args = /* ... existing reorder logic ... */;

    let rt = get_runtime().clone();
    rt.spawn(async move {
        // Execute function — no channel, no reader task needed.
        // The engine calls event_bus::emit() directly, which stores
        // events in the global EventStore and forwards to the publisher.
        let result = AssertUnwindSafe(async {
            engine.call_function(
                &func_name,
                &bex_args,
                Some(root_span_id),
                parent_span_id,     // from host @trace context (M8)
            ).await
        })
        .catch_unwind()
        .await;

        // Collectors can now be queried by the host language.
        // Events are in the global EventStore, indexed by root_span_id.
        // On collector drop, their tracked spans are untracked
        // (ref count decremented, events purged at zero).

        // Send result via callback
        match result {
            Ok(Ok(value)) => send_result_to_callback(id, true, &value),
            Ok(Err(e)) => send_error_to_callback(id, &format!("{}", e)),
            Err(panic) => send_error_to_callback(id, &format!("Panic: {:?}", panic)),
        }
    });

    Ok(())
}
```

#### 2. Expose Collector to host languages

**File**: `bridge_cffi/src/ffi/objects.rs`

Add FFI exports for Collector:

```rust
#[unsafe(no_mangle)]
pub extern "C" fn collector_new(name: *const libc::c_char) -> *mut Collector {
    let name = if name.is_null() {
        None
    } else {
        Some(unsafe { CStr::from_ptr(name) }.to_str().unwrap_or("collector").to_string())
    };
    let collector = Collector::new(name);
    Box::into_raw(Box::new(collector))
}

#[unsafe(no_mangle)]
pub extern "C" fn collector_logs(collector: *const Collector) -> Buffer {
    let collector = unsafe { &*collector };
    let logs = collector.function_logs();
    // Encode logs to protobuf
    Buffer::from(encode_function_logs(&logs))
}

#[unsafe(no_mangle)]
pub extern "C" fn collector_last_log(collector: *const Collector) -> Buffer {
    let collector = unsafe { &*collector };
    match collector.last_function_log() {
        Some(log) => Buffer::from(encode_function_log(&log)),
        None => Buffer::empty(),
    }
}

#[unsafe(no_mangle)]
pub extern "C" fn collector_usage(collector: *const Collector) -> Buffer {
    let collector = unsafe { &*collector };
    let usage = collector.usage();
    Buffer::from(encode_usage(&usage))
}

#[unsafe(no_mangle)]
pub extern "C" fn collector_clear(collector: *mut Collector) {
    let collector = unsafe { &*collector };
    collector.clear();
}

#[unsafe(no_mangle)]
pub extern "C" fn collector_free(collector: *mut Collector) {
    if !collector.is_null() {
        unsafe { drop(Box::from_raw(collector)) };
    }
}
```

#### 3. Protobuf schema for FunctionLog

**File**: `bridge_cffi/types/baml/cffi/v1/baml_outbound.proto`

Add messages for collector query results:

```protobuf
message FunctionLogMessage {
    string span_id = 1;
    string function_name = 2;
    optional CffiValueHolder args = 3;
    optional CffiValueHolder result = 4;
    optional string error = 5;
    optional uint64 duration_ms = 6;
    optional TokenUsageMessage usage = 7;
}

message TokenUsageMessage {
    optional uint64 input_tokens = 1;
    optional uint64 output_tokens = 2;
    optional uint64 cached_input_tokens = 3;
}
```

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bridge_cffi/src/ffi/functions.rs` | Root span creation, host span context extraction, collector registration, `call_function` with `root_span_id` + `parent_span_id` | ~25 |
| `bridge_cffi/src/ffi/objects.rs` | Collector FFI exports (new, logs, last, usage, clear, free) | ~60 |
| `bridge_cffi/types/baml/cffi/v1/baml_outbound.proto` | FunctionLog, TokenUsage protobuf messages | ~15 |
| `bridge_cffi/src/ctypes/mod.rs` | `encode_function_logs()`, `encode_usage()` helpers | ~40 |
| `bridge_cffi/Cargo.toml` | Add `baml_events` dependency | ~1 |

### Test

Python integration test:

```python
import baml

collector = baml.Collector("test")
result = await b.ExtractResume(resume_text, baml_options={"collectors": [collector]})

logs = collector.logs
assert len(logs) >= 1
assert logs[0].function_name == "ExtractResume"
assert logs[0].usage.input_tokens > 0
```

---

## 13. Milestone 7: Header Events

### Goal

Emit `Header(Enter)` and `Header(Exit)` events for hierarchical execution
visualization (IDE, Boundary dashboard).

### Background: How Headers Work Today

The VM already has `VizEnter` and `VizExit` instructions that are compiled
from `//# header` annotations. When the VM executes these instructions,
it yields `VmExecState::Notify(WatchNotification::Viz { ... })`.

The engine currently ignores these:

```rust
VmExecState::Notify(_notification) => {
    // Ignore watch notifications for now
}
```

### What This Requires

#### 1. Convert `Notify(Viz)` to `Header` events

**File**: `bex_engine/src/lib.rs`

Replace the `Notify` handler:

```rust
VmExecState::Notify(notification) => {
    match notification {
        WatchNotification::Viz { function_name, event: viz_event } => {
            // VizExecEvent has: delta (Enter/Exit), node_id, node_type,
            // label, header_level. We map these to HeaderEvent.
            let header_event = match viz_event.delta {
                VizExecDelta::Enter => EventKind::Header(
                    HeaderEvent::Enter(HeaderEnter {
                        name: viz_event.label.clone(),
                        node_id: viz_event.node_id,
                        node_type: viz_event.node_type,
                        header_level: viz_event.header_level,
                    })
                ),
                VizExecDelta::Exit => EventKind::Header(
                    HeaderEvent::Exit(HeaderExit {
                        name: viz_event.label.clone(),
                        node_id: viz_event.node_id,
                    })
                ),
            };
            // Direct global call — no channel needed
            event_bus::emit(RuntimeEvent {
                ctx: span_stack.current(),
                timestamp: web_time::SystemTime::now(),
                event: header_event,
            });
        }
        WatchNotification::Variables(_) => {
            // Watch variable notifications (M9)
        }
        WatchNotification::Block(_) => {
            // Block notifications (M9)
        }
    }
}
```

#### 2. Implicit headers for LLM functions

Add `baml.events.send("header_enter/exit")` to `call_llm_function()` in
`llm.baml`. This gives every LLM function call a visual "header" in the
IDE without requiring explicit `//# header` annotations:

**File**: `baml_language/crates/baml_builtins/baml/llm.baml`

```baml
function call_llm_function(function_name: string, args: map<string, unknown>) -> string {
    baml.events.send("function_start", { "name": function_name, "args": args })
    baml.events.send("header_enter", { "name": function_name })    // ← NEW

    // ... LLM calling sequence with intermediate events (M5) ...

    baml.events.send("header_exit", { "name": function_name })     // ← NEW
    baml.events.send("function_end", { "name": function_name, "result": result })
    return result;
}
```

No compiler changes needed — this is just editing the `.baml` file.

#### 3. Engine: Handle `header_enter` / `header_exit` in `build_event_kind()`

Already included in the M5 `build_event_kind()` implementation. For
implicit headers from `llm.baml`, these create `HeaderEnter`/`HeaderExit`
with just a `name` and a `node_id` of 0 (since they don't originate from
compiled `VizEnter`/`VizExit` instructions):

```rust
"header_enter" => EventKind::Header(HeaderEvent::Enter(HeaderEnter {
    name: get_string(payload, "name"),
    node_id: 0,       // implicit header, no compiled viz node
    node_type: VizNodeType::HeaderContextEnter,
    header_level: None,
})),
"header_exit" => EventKind::Header(HeaderEvent::Exit(HeaderExit {
    name: get_string(payload, "name"),
    node_id: 0,
})),
```

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_engine/src/lib.rs` | Handle `Notify(Viz)` → `Header` events | ~20 |
| `bex_engine/src/lib.rs` | Handle `header_enter`/`header_exit` in `build_event_kind()` | ~10 |
| `baml_builtins/baml/llm.baml` | Add `header_enter`/`header_exit` to `call_llm_function()` | ~2 |
| `baml_events/src/header.rs` | Header event types (already defined in Section 5.5) | 0 |

### Test

```rust
#[tokio::test]
async fn test_header_events() {
    // Function with explicit //# header annotations
    let engine = /* test engine */;
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    engine.call_function("AnnotatedFunc", &[], Some(root_span.clone())).await.unwrap();
    let events = event_store::events_for_span(&root_span).unwrap();

    let header_events: Vec<_> = events
        .iter()
        .filter(|e| matches!(&e.event, EventKind::Header(_)))
        .collect();

    // Verify Enter/Exit pairing
    assert!(header_events.len() >= 2);
    assert!(matches!(&header_events[0].event, EventKind::Header(HeaderEvent::Enter(_))));
    assert!(matches!(&header_events.last().unwrap().event, EventKind::Header(HeaderEvent::Exit(_))));
}

#[tokio::test]
async fn test_llm_implicit_headers() {
    // LLM function gets implicit headers
    let engine = /* test engine with LLM function */;
    let root_span = SpanId::new();
    let collector = Collector::new(None);
    collector.track_call(root_span.clone());

    engine.call_function("LlmFunc", &[], Some(root_span.clone())).await.unwrap();
    let events = event_store::events_for_span(&root_span).unwrap();

    let headers: Vec<_> = events.iter().filter(|e| matches!(&e.event, EventKind::Header(_))).collect();
    assert!(headers.len() >= 2); // At least Enter + Exit
}
```

---

## 14. Milestone 8: Host-Language Span Tracking (`@trace` in Python/TS)

### Goal

User-defined Python/TS functions decorated with `@trace` create spans
that are parents of BAML function calls and of each other, producing
a unified call tree across both host-language code and BAML execution.

```python
@trace
async def my_pipeline(text: str):
    set_tags(pipeline="v1")
    result = await b.ExtractResume(text)   # child of my_pipeline
    summary = await b.Summarize(result)    # child of my_pipeline
    return summary
```

Expected span tree:

```
my_pipeline                         ← host-language span (Python @trace)
  ExtractResume                     ← BAML engine span (child)
    call_llm_function               ← engine-internal span
  Summarize                         ← BAML engine span (child)
    call_llm_function               ← engine-internal span
```

### How `engine/` Does It Today

The current system has three cooperating layers:

1. **`RuntimeContextManager`** (Rust, `context_manager.rs`): A mutable
   call stack — `Vec<(uuid, name, tags, FunctionCallId)>`. `enter()`
   pushes, `exit()` pops. Tags from the parent are cloned to the child
   on `enter()`.

2. **`CtxManager`** (Python, `ctx_manager.py`): Wraps a
   `contextvars.ContextVar[Dict[thread_id, RuntimeContextManager]]`.
   This two-level key (contextvar → thread_id) handles both async
   context isolation (contextvars) and thread isolation (thread_id lookup).

3. **`@trace` decorator** (`trace_fn`): Two paths:

   - **Async**: `deep_clone()` the `RuntimeContextManager`, set the
     clone in the contextvar, then create a `BamlSpan` against the clone.
     The clone is essential because Python's `asyncio` propagates a
     *snapshot* of contextvars when creating a Task — concurrent
     coroutines from `asyncio.gather` each get their own clone, preventing
     stack corruption.

   - **Sync**: Use the shared `RuntimeContextManager` directly (single
     thread, no fork needed). New threads get fresh managers via the
     thread_id lookup.

4. **`BamlSpan`** (Rust → PyO3): `BamlSpan::new()` calls
   `runtime.start_call()` which calls `ctx.enter()`, emits a
   `FunctionStart` trace event with the full `call_id_stack`, and returns
   a `TracingCall`. `finish()` calls `ctx.exit()`, emits `FunctionEnd`.

5. **Generated BAML client** (`baml_client/`): When the user calls
   `b.ExtractResume(text)`, the generated code grabs the **same**
   `RuntimeContextManager` from the contextvar. Because `@trace`
   already pushed onto its stack, the BAML call's `start_call()` sees
   the parent and builds the correct `call_id_stack`.

**Key files**:
- `engine/language_client_python/python_src/baml_py/ctx_manager.py` — Python `CtxManager`: contextvar + thread_id + deep_clone + `@trace` decorator
- `engine/baml-runtime/src/types/context_manager.rs` — `RuntimeContextManager`: Rust span stack with enter/exit/tags
- `engine/language_client_python/src/types/span.rs` — `BamlSpan`: PyO3 wrapper for `TracingCall` (start_call/finish_call)

### Design for `baml_language`

We keep the same user-facing API but change the internals to emit
events into the global `EventStore` (via `event_bus::emit()`) instead
of the old `BAML_TRACER`. The host-language context manager now
manages `SpanId`s instead of `FunctionCallId`s.

#### Rust-side: `HostSpanManager`

A new lightweight struct in `bridge_cffi` (or a new small crate) that
replaces `RuntimeContextManager` for host-language span tracking. It
does NOT go through the BAML engine — it emits events directly.

```rust
// bridge_cffi/src/host_spans.rs

use std::collections::HashMap;
use baml_events::*;

/// Manages the host-language span stack for a single "context"
/// (one Python async task or one thread).
///
/// Replaces engine/'s RuntimeContextManager for v2.
/// Does NOT interact with the BexEngine — emits events directly
/// to the global EventStore via event_bus::emit().
#[derive(Clone, Debug)]
pub struct HostSpanManager {
    /// The span stack. Each entry is a span we're currently "inside".
    stack: Vec<HostSpanEntry>,
    /// Tags inherited from the stack + global tags.
    tags: HashMap<String, BexExternalValue>,
}

#[derive(Clone, Debug)]
struct HostSpanEntry {
    span_id: SpanId,
    root_span_id: SpanId,
    function_name: String,
    started_at: web_time::Instant,
}

impl HostSpanManager {
    pub fn new() -> Self {
        Self {
            stack: Vec::new(),
            tags: HashMap::new(),
        }
    }

    /// Deep clone for async context forking. Same semantics as
    /// engine/'s RuntimeContextManager::deep_clone().
    pub fn deep_clone(&self) -> Self {
        Self {
            stack: self.stack.clone(),
            tags: self.tags.clone(),
        }
    }

    /// Enter a new host-language span (@trace function start).
    /// Returns the SpanId for later use by the bridge.
    pub fn enter(&mut self, function_name: &str, args: Vec<(String, BexExternalValue)>) -> SpanId {
        let span_id = SpanId::new();

        // Determine parent and root
        let (parent_span_id, root_span_id) = match self.stack.last() {
            Some(parent) => (Some(parent.span_id.clone()), parent.root_span_id.clone()),
            None => (None, span_id.clone()), // This span IS the root
        };

        let ctx = SpanContext {
            span_id: span_id.clone(),
            parent_span_id,
            root_span_id: root_span_id.clone(),
        };

        // Emit FunctionStart event directly to the global EventStore
        event_bus::emit(RuntimeEvent {
            ctx,
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: function_name.to_string(),
                args,
                is_stream: false,
            })),
        });

        self.stack.push(HostSpanEntry {
            span_id: span_id.clone(),
            root_span_id,
            function_name: function_name.to_string(),
            started_at: web_time::Instant::now(),
        });

        span_id
    }

    /// Exit the current host-language span (@trace function end).
    pub fn exit(&mut self, result: Result<BexExternalValue, String>) {
        let Some(entry) = self.stack.pop() else {
            log::warn!("exit() called with empty span stack");
            return;
        };

        let parent_span_id = self.stack.last().map(|e| e.span_id.clone());

        let ctx = SpanContext {
            span_id: entry.span_id,
            parent_span_id,
            root_span_id: entry.root_span_id,
        };

        event_bus::emit(RuntimeEvent {
            ctx,
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                name: entry.function_name,
                result,
                duration: entry.started_at.elapsed(),
            })),
        });
    }

    /// Get the current span context (for passing to call_function).
    /// Returns None if no @trace span is active.
    pub fn current_context(&self) -> Option<HostSpanContext> {
        let entry = self.stack.last()?;
        Some(HostSpanContext {
            span_id: entry.span_id.clone(),
            root_span_id: entry.root_span_id.clone(),
        })
    }

    /// Set tags on the current span. Emits a SetTags event.
    pub fn upsert_tags(&mut self, tags: Vec<(String, BexExternalValue)>) {
        // Merge into local tag map
        for (k, v) in &tags {
            self.tags.insert(k.clone(), v.clone());
        }

        // If we're inside a span, emit a SetTags event
        if let Some(entry) = self.stack.last() {
            let parent_span_id = if self.stack.len() > 1 {
                self.stack.get(self.stack.len() - 2).map(|e| e.span_id.clone())
            } else {
                None
            };
            event_bus::emit(RuntimeEvent {
                ctx: SpanContext {
                    span_id: entry.span_id.clone(),
                    parent_span_id,
                    root_span_id: entry.root_span_id.clone(),
                },
                timestamp: web_time::SystemTime::now(),
                event: EventKind::SetTags(tags),
            });
        }
    }

    pub fn depth(&self) -> usize {
        self.stack.len()
    }
}

/// Lightweight context passed from host language to bridge_cffi
/// so call_function() can nest under the current @trace span.
#[derive(Clone, Debug)]
pub struct HostSpanContext {
    pub span_id: SpanId,
    pub root_span_id: SpanId,
}
```

#### Python-side: Updated `CtxManager`

The Python `CtxManager` class changes minimally — it swaps
`RuntimeContextManager` for `HostSpanManager` and stops going through
`BamlSpan`/`start_call()`/`finish_call()`. The contextvar + thread_id
+ deep_clone pattern stays the same.

**File**: Generated `globals.py` (template)

```python
from baml_py import BamlRuntime, HostSpanManager

DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME = BamlRuntime.from_files(
    "baml_src", get_baml_files(), os.environ.copy()
)

# HostSpanManager replaces BamlCtxManager.
# One per context (contextvar) × thread (thread_id).
DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX = CtxManager(
    DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME
)
```

**File**: Updated `ctx_manager.py`

```python
import asyncio
import contextvars
import functools
import inspect
import os
import threading
import typing

from .baml_py import HostSpanManager, BamlRuntime

F = typing.TypeVar("F", bound=typing.Callable[..., typing.Any])


def current_thread_id() -> int:
    t = threading.current_thread()
    return getattr(t, "native_id", None) or t.ident or 0


class CtxManager:
    """
    Manages host-language span context for @trace.

    Architecture (unchanged from engine/):
    - contextvars.ContextVar provides async isolation (each asyncio.Task
      gets a snapshot of the contextvar on creation).
    - Dict[thread_id, HostSpanManager] provides thread isolation
      (ThreadPoolExecutor workers get fresh managers).
    - deep_clone() on async @trace entry forks the span stack so
      concurrent coroutines don't corrupt each other.
    """

    def __init__(self, rt: BamlRuntime):
        self.rt = rt
        self.ctx: contextvars.ContextVar[dict[int, HostSpanManager]] = (
            contextvars.ContextVar("baml_ctx", default={})
        )

    def __mgr(self) -> HostSpanManager:
        ctx = self.ctx.get()
        tid = current_thread_id()
        if tid not in ctx:
            ctx[tid] = HostSpanManager()
        return ctx[tid]

    # ── @trace decorator ──

    def trace_fn(self, func: F) -> F:
        func_name = func.__name__
        param_names = list(inspect.signature(func).parameters.keys())

        if asyncio.iscoroutinefunction(func):

            @functools.wraps(func)
            async def async_wrapper(*args, **kwargs):
                params = _build_params(args, kwargs, param_names)

                # Fork the span manager (same deep_clone semantics as engine/)
                mgr = self.__mgr()
                clone = mgr.deep_clone()
                self.ctx.set({current_thread_id(): clone})

                clone.enter(func_name, params)
                try:
                    response = await func(*args, **kwargs)
                    clone.exit(response)
                    return response
                except BaseException as e:
                    clone.exit_error(str(e))
                    raise

            return typing.cast(F, async_wrapper)

        else:

            @functools.wraps(func)
            def sync_wrapper(*args, **kwargs):
                params = _build_params(args, kwargs, param_names)
                mgr = self.__mgr()

                mgr.enter(func_name, params)
                try:
                    response = func(*args, **kwargs)
                    mgr.exit(response)
                    return response
                except BaseException as e:
                    mgr.exit_error(str(e))
                    raise

            return typing.cast(F, sync_wrapper)

    # ── Tags ──

    def upsert_tags(self, **tags: str):
        self.__mgr().upsert_tags(tags)

    # ── Context for call_function ──

    def current_host_context(self) -> typing.Optional["HostSpanContext"]:
        """
        Called by the generated BAML client before call_function().
        Returns the current @trace span context (if any) so the BAML
        call can be nested as a child.
        """
        return self.__mgr().current_context()

    # ── Flush / lifecycle ──

    def flush(self):
        self.rt.flush()

    def on_log_event(self, handler):
        self.rt.set_log_event_callback(handler)
```

#### Generated BAML Client: Passing Host Context to `call_function`

The generated Python client functions (e.g., `b.ExtractResume()`) need
to pass the current host-language span context so the bridge can nest
the BAML call.

```python
# Generated baml_client (e.g., async_client.py)

async def ExtractResume(self, text: str, baml_options=None):
    # Get host-language span context (from @trace, if active)
    host_ctx = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_CTX.current_host_context()

    raw = self.__runtime.call_function(
        "ExtractResume",
        {"text": text},
        self.__ctx_manager.get(),
        host_ctx,           # NEW: passed to bridge_cffi
        baml_options or {},
    )
    # ... parse and return ...
```

### Sequence Diagram: `HostSpanManager` Lifecycle

The following diagram shows how a `@trace`-decorated Python function
creates a host-language span, how that span context flows into a BAML
function call, and how the `EventStore` receives events from both layers.

```
 Python User Code        CtxManager / HostSpanManager        EventStore (global)        bridge_cffi             BexEngine
 ────────────────        ────────────────────────────        ───────────────────        ───────────             ─────────
       │                            │                              │                       │                       │
  @trace my_pipeline()              │                              │                       │                       │
       │                            │                              │                       │                       │
       │──── trace_fn(async) ──────▶│                              │                       │                       │
       │                            │                              │                       │                       │
       │     [async path]           │                              │                       │                       │
       │     deep_clone() mgr       │                              │                       │                       │
       │     set clone in ctx_var   │                              │                       │                       │
       │                            │                              │                       │                       │
       │                  enter("my_pipeline", args)               │                       │                       │
       │                            │                              │                       │                       │
       │                            │  span_id = A (new UUID)      │                       │                       │
       │                            │  parent = None (stack empty) │                       │                       │
       │                            │  root = A (self is root)     │                       │                       │
       │                            │                              │                       │                       │
       │                            │── emit(FunctionStart ───────▶│                       │                       │
       │                            │    span=A, parent=∅, root=A) │                       │                       │
       │                            │                              │  [store if tracked]   │                       │
       │                            │                              │  [forward to pub sink]│                       │
       │                            │                              │                       │                       │
       │                            │  push A onto stack           │                       │                       │
       │                            │  stack: [A]                  │                       │                       │
       │                            │                              │                       │                       │
       │                            │                              │                       │                       │
  set_tags(pipeline="v1")           │                              │                       │                       │
       │──── upsert_tags ─────────▶│                              │                       │                       │
       │                            │── emit(SetTags ─────────────▶│                       │                       │
       │                            │    span=A, {"pipeline":"v1"})│                       │                       │
       │                            │                              │                       │                       │
       │                            │                              │                       │                       │
  await b.ExtractResume(text)       │                              │                       │                       │
       │                            │                              │                       │                       │
       │──── current_host_context()▶│                              │                       │                       │
       │◀── HostSpanContext ────────│                              │                       │                       │
       │    {span=A, root=A}        │                              │                       │                       │
       │                            │                              │                       │                       │
       │──── call_function("ExtractResume", args, host_ctx) ──────────────────────────────▶│                       │
       │                            │                              │                       │                       │
       │                            │                              │    root = SpanId(B)   │                       │
       │                            │                              │    parent = Some(A)   │                       │
       │                            │                              │       │               │                       │
       │                            │                              │       │── call_function("ExtractResume",      │
       │                            │                              │       │      args, root=A, parent=A) ────────▶│
       │                            │                              │       │               │                       │
       │                            │                              │       │               │  SpanStack::new_with_parent(B, Some(A))
       │                            │                              │       │               │  stack: [B], parent_of_root: A
       │                            │                              │       │               │                       │
       │                            │                              │◀──────│───────────────│── emit(FunctionStart  │
       │                            │                              │       │               │    span=B, parent=A,  │
       │                            │                              │       │               │    root=A)            │
       │                            │                              │       │               │                       │
       │                            │                              │       │               │  [VM executes...]     │
       │                            │                              │       │               │  span_stack.push()→C  │
       │                            │                              │◀──────│───────────────│── emit(FunctionStart  │
       │                            │                              │       │               │    span=C,parent=B)   │
       │                            │                              │◀──────│───────────────│── emit(LlmRequest,    │
       │                            │                              │       │               │    LlmResponse, etc.) │
       │                            │                              │◀──────│───────────────│── emit(FunctionEnd    │
       │                            │                              │       │               │    span=C,parent=B)   │
       │                            │                              │       │               │  span_stack.pop()     │
       │                            │                              │       │               │                       │
       │                            │                              │◀──────│───────────────│── emit(FunctionEnd    │
       │                            │                              │       │               │    span=B, parent=A,  │
       │                            │                              │       │               │    root=A)            │
       │                            │                              │       │               │                       │
       │◀──── result ──────────────────────────────────────────────────────│               │                       │
       │                            │                              │       │               │                       │
       │                            │                              │                       │                       │
  return result                     │                              │                       │                       │
       │                            │                              │                       │                       │
       │──── end_trace ───────────▶│                              │                       │                       │
       │                  exit(Ok(result))                         │                       │                       │
       │                            │                              │                       │                       │
       │                            │  pop A from stack            │                       │                       │
       │                            │  stack: []                   │                       │                       │
       │                            │  duration = now - A.started  │                       │                       │
       │                            │                              │                       │                       │
       │                            │── emit(FunctionEnd ─────────▶│                       │                       │
       │                            │    span=A, parent=∅, root=A  │                       │                       │
       │                            │    duration=...)             │                       │                       │
       │                            │                              │                       │                       │
       ▼                            ▼                              ▼                       ▼                       ▼

 Final EventStore contents (all share root=A):

   Event #  │ Type            │ span │ parent │ root │ Source
   ─────────┼─────────────────┼──────┼────────┼──────┼────────────────────
   1        │ FunctionStart   │ A    │ ∅      │ A    │ HostSpanManager
   2        │ SetTags         │ A    │ ∅      │ A    │ HostSpanManager
   3        │ FunctionStart   │ B    │ A      │ A    │ BexEngine
   4        │ FunctionStart   │ C    │ B      │ A    │ BexEngine (LLM)
   5        │ LlmRequest      │ C    │ B      │ A    │ BexEngine (LLM)
   6        │ LlmResponse     │ C    │ B      │ A    │ BexEngine (LLM)
   7        │ FunctionEnd     │ C    │ B      │ A    │ BexEngine (LLM)
   8        │ FunctionEnd     │ B    │ A      │ A    │ BexEngine
   9        │ FunctionEnd     │ A    │ ∅      │ A    │ HostSpanManager

 Reconstructed span tree:
   A ── my_pipeline (host @trace)
   └─ B ── ExtractResume (BAML engine)
      └─ C ── call_llm_function (compiler-inserted)
```

### How Spans Nest: End-to-End Flow (Compact)

```
Python user code                    bridge_cffi                    BexEngine
─────────────────                   ───────────                    ─────────
@trace my_pipeline()
│ HostSpanManager.enter()
│   → emit FunctionStart(span=A, parent=None, root=A)
│   → push A onto stack
│
│ await b.ExtractResume(text)
│   │ current_host_context()
│   │   → returns HostSpanContext{span=A, root=A}
│   │
│   └──→ call_function_inner()
│          root_span_id = SpanId::new()  // B
│          parent_span_id = Some(A)      // from host ctx
│          │
│          └──→ engine.call_function("ExtractResume", args,
│                    root=A, parent=Some(A))
│                 SpanStack::new_with_parent(B, Some(A))
│                 emit FunctionStart(span=B, parent=A, root=A)
│                 │
│                 │ (LLM call inside)
│                 │ span_stack.push() → span C
│                 │ emit FunctionStart(span=C, parent=B, root=A)
│                 │ emit LlmRequest(span=C, ...)
│                 │ emit LlmResponse(span=C, ...)
│                 │ emit FunctionEnd(span=C, parent=B, root=A)
│                 │ span_stack.pop()
│                 │
│                 emit FunctionEnd(span=B, parent=A, root=A)
│
│ HostSpanManager.exit()
│   → emit FunctionEnd(span=A, parent=None, root=A)
│   → pop A from stack

Result: All events share root=A. The tree is:
  A (my_pipeline)
    B (ExtractResume)
      C (call_llm_function)
```

### Async Context Isolation (`asyncio.gather`)

The `deep_clone()` on async entry is critical for `asyncio.gather`:

```python
@trace
async def pipeline():                    # span P (root)
    await asyncio.gather(
        process("a"),                    # gets clone₁ of manager
        process("b"),                    # gets clone₂ of manager
    )

@trace
async def process(x):                   # span Q₁ or Q₂
    await b.Classify(x)                 # span R₁ or R₂
```

Without `deep_clone()`, both coroutines share one `HostSpanManager` and
corrupt each other's stacks. With it:

- `process("a")` enters → clone₁ has stack `[P, Q₁]`
- `process("b")` enters → clone₂ has stack `[P, Q₂]`
- `b.Classify(x)` in each coroutine sees the correct parent

This matches the existing behavior proven by `test_tracing_root_with_children_parallel`
and `test_tracing_complex_async`.

### Thread Pool Behavior (Documented Limitation)

`ThreadPoolExecutor` workers get **fresh** `HostSpanManager` instances
because `__mgr()` creates a new one for unknown `thread_id`s. This means
worker spans are independent roots, not children of the submitting function.

This is the same behavior as `engine/` today, validated by
`test_tracing_thread_pool_simple` and `test_tracing_thread_pool_complex`.

A future improvement could allow explicit context propagation to threads —
see [Deferred Work](#15-deferred-work).

### Code Changes Summary

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bridge_cffi/src/host_spans.rs` | **New** — `HostSpanManager`, `HostSpanContext`, enter/exit/tags | ~120 |
| `bridge_cffi/src/ffi/objects.rs` | Expose `HostSpanManager` to Python via FFI (new/enter/exit/tags/clone/current_context) | ~80 |
| `bridge_cffi/src/ffi/functions.rs` | Accept `HostSpanContext`, pass `parent_span_id` to engine | ~15 |
| `bex_engine/src/lib.rs` | `call_function()` accepts `parent_span_id`, `SpanStack::new_with_parent()` | ~20 |
| `generators/.../tracing.py` | Update template: `trace` → `CtxManager.trace_fn`, `set_tags`, etc. | ~10 |
| `generators/.../globals.py` | Update template: create `CtxManager` with `HostSpanManager` | ~5 |
| `ctx_manager.py` (baml_py) | Replace `RuntimeContextManager` with `HostSpanManager`, keep contextvar+thread_id+deep_clone | ~80 |

### Test

```python
@pytest.mark.asyncio
async def test_host_span_nests_baml_call():
    """BAML calls are children of @trace-decorated Python functions."""
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    @trace
    async def my_pipeline(text: str):
        set_tags(source="test")
        result = await b.FnOutputClass(text)
        return result

    flush()
    _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

    await my_pipeline("hello")
    flush()

    reader = TraceFileReader(trace_file)
    reader.print_trace_hierarchy(show_ids=True, show_tags=True)

    # my_pipeline should be root
    root = reader.find_root("my_pipeline")
    assert_that(root).is_not_none()
    assert_that(root.is_root()).is_true()

    # FnOutputClass should be a child of my_pipeline
    children = reader.find_children(root.call_id, "FnOutputClass")
    assert_that(len(children)).is_equal_to(1)
    reader.verify_parent_child(root, children[0])

    # Tags should propagate
    reader.verify_tags(root, {"source": "test"})
    reader.verify_tags(children[0], {"source": "test"})


@pytest.mark.asyncio
async def test_host_span_parallel_gather():
    """Parallel @trace functions maintain correct parent-child relationships."""
    trace_file = os.environ["BAML_TRACE_FILE"]
    if os.path.exists(trace_file):
        os.remove(trace_file)

    @trace
    async def child_task(task_id: int):
        await b.FnOutputClass(f"arg-{task_id}")
        return f"done-{task_id}"

    @trace
    async def root_pipeline():
        await asyncio.gather(
            child_task(1),
            child_task(2),
            child_task(3),
        )

    flush()
    _ = DO_NOT_USE_DIRECTLY_UNLESS_YOU_KNOW_WHAT_YOURE_DOING_RUNTIME.drain_stats()

    await root_pipeline()
    flush()

    reader = TraceFileReader(trace_file)
    reader.print_trace_hierarchy(show_ids=True, show_depth=True)

    # root_pipeline should be root
    root = reader.find_root("root_pipeline")
    assert_that(root).is_not_none()

    # 3 child_task children
    children = reader.find_children(root.call_id, "child_task")
    assert_that(len(children)).is_equal_to(3)
    for child in children:
        reader.verify_parent_child(root, child)

    # Each child_task should have FnOutputClass as a grandchild
    child_ids = [c.call_id for c in children]
    fn_outputs = reader.find_by_function_name("FnOutputClass")
    assert_that(len(fn_outputs)).is_equal_to(3)
    for fn_output in fn_outputs:
        assert_that(fn_output.parent_id).is_in(*child_ids)
```

---

## 15. Deferred Work

The following items are explicitly deferred to future milestones and are
**not** addressed in M1–M8:

1. **Retry orchestration** (future `llm.baml` work): `engine/` handles
   retries in its orchestrator layer. In `baml_language`, retries will be
   implemented as a loop in `llm.baml` that wraps the HTTP call sequence.
   Each retry iteration will generate a new `RequestId` so
   `build_function_log` can group attempts correctly. The retry policy
   will be read from the function's client metadata. This will be
   implemented directly in `llm.baml` alongside the event sends.

2. **Error handling in `call_llm_function()`** (future `llm.baml` work):
   Currently, if `baml.http.send()` or `primitive_client.parse()` throws,
   the `function_end` event is never emitted. A future update to
   `llm.baml` will wrap the LLM sequence in error handling (BAML
   try/catch or equivalent) to guarantee `function_end` is always emitted
   with an error result, and that `llm_response` is emitted with
   `is_success: false` and an `error_message`. Until then, the
   `Collector` must tolerate missing `FunctionEnd` events
   (`build_function_log` already handles this by only caching after
   `FunctionEnd` arrives).

3. **Client strategy support** (fallback/round-robin): Strategy clients
   that try multiple providers will be handled as part of the retry/
   orchestration work in `llm.baml`. Each provider attempt gets its own
   `RequestId`.

4. **Explicit thread context propagation**: `ThreadPoolExecutor` workers
   currently get fresh `HostSpanManager` instances (independent root spans).
   A future API could allow explicit context passing:
   `ctx = get_trace_context()` → `pool.submit(fn, ctx)` →
   `@trace(context=ctx)`. This requires `HostSpanManager` serialization
   or a context token pattern. Deferred because the current behavior
   (independent roots) matches `engine/` and is acceptable for most use
   cases.

---

## 16. Open Questions

1. **`BexValue` to `BexExternalValue` in events**: The
   `baml.events.send()` call receives VM-internal `BexValue` args. The
   engine converts them via `vm_args_to_bex_values()`. Need to ensure
   this conversion handles all types (classes, enums, maps, arrays).

2. **Multiple collectors per call**: `engine/` supports passing multiple
   collectors to a single call. Our design supports this — each collector
   calls `track_call(root_span_id)` to register with the global
   `EventStore`. But do we need per-function-call collector targeting,
   or is one collector per `call_function()` sufficient?

3. **Collector lifetime across multiple calls**: In `engine/`, a collector
   can span multiple `call_function()` calls. In our design, the collector
   calls `track_call()` for each new root_span_id. The collector's
   `tracked_spans` list grows across calls, and each root_span_id is
   ref-counted independently in the `EventStore`.

4. **`SetTags` event production**: `engine/` has a `SetTags` trace data
   variant emitted by `ctx.upsert_tags()`. In the new compiler, how do
   tags get set? Options: (a) a `baml.tags.set(key, value)` builtin that
   emits `EventKind::SetTags`, (b) the engine sets initial tags from
   `baml_options`, (c) both. Need to define the tag-setting API.

5. **SSE stream events in FunctionLog**: `engine/` tracks
   `RawLLMResponseStream` events (SSE chunks) in `LLMStreamCall.sse_chunks`.
   Our M1–M7 design defers streaming to M8 but the `FunctionLog` / `LLMCall`
   structure should anticipate it. Currently `LLMCall` doesn't have an
   `sse_chunks` field — we'll need to add `LLMStreamCall` or extend
   `LLMCall` in M8.

6. **`primitive_client.parse()` return shape**: The current SysOp
   `LlmParseResponse` is unimplemented. It needs to return a map
   containing `content`, `model`, `finish_reason`, `usage`, and
   `client_stack`. The `client_stack` specifically requires the
   `PrimitiveClient` to know its own name and any parent strategy
   clients — this may require plumbing from the client resolution step.

7. **Host-language span context propagation via protobuf**: The
   `HostSpanContext` (span_id + root_span_id) needs to be passed from
   Python/TS to `bridge_cffi` on every `call_function()`. Options:
   (a) add fields to the existing `CallFunctionRequest` protobuf message,
   (b) pass as separate FFI arguments alongside the encoded args,
   (c) store in a thread-local on the Rust side that the Python CtxManager
   sets before each call. Option (a) is cleanest but requires proto changes.

8. **Collector tracking for host-language spans**: When a `@trace` function
   is the root span, should collectors passed to `b.ExtractResume()` also
   track the host root_span_id (so `collector.logs` includes the Python
   function's span)? Or should collectors only track the BAML engine's
   spans? In `engine/`, collectors track whatever call_id the `start_call()`
   produced — which includes host `@trace` spans. We should match this.

9. **Host span events in the Publisher**: Host-language `@trace` spans
   emit `FunctionStart`/`FunctionEnd` events with `function_type: Native`
   (not `BamlLlm`). The Publisher and Boundary dashboard need to handle
   these. Should they be included in S3 uploads? Filtered? The answer
   is yes — `engine/` publishes them today and the dashboard renders them.

10. **`HostSpanManager` vs reusing engine's `SpanStack`**: An alternative
    design would have the host language create a `SpanId`, pass it to
    bridge_cffi, and let bridge_cffi manage a parallel span stack. But
    this requires two FFI round-trips per `@trace` (one to get a span_id,
    one to call the function). The `HostSpanManager` approach keeps span
    lifecycle in the host language with a single FFI call for events,
    which is simpler and matches `engine/`'s proven pattern.

---

## 17. Implementation Checklists

> Audited against the actual `baml_language/` codebase. Items marked
> ✅ already exist; items marked ⬚ need to be built.

### Milestone 1: FunctionStart / FunctionEnd for Top-Level Calls

| # | Item | Status | Notes |
|---|------|--------|-------|
| 1.1 | Create `baml_events` crate with `Cargo.toml` | ⬚ | Crate does not exist yet |
| 1.2 | Define `RuntimeEvent`, `EventKind`, `SpanContext`, `SpanId` in `baml_events/src/lib.rs` | ⬚ | No event types exist anywhere in `baml_language/` |
| 1.3 | Define `FunctionEvent`, `FunctionStart`, `FunctionEnd` in `baml_events/src/function.rs` | ⬚ | |
| 1.4 | Implement global `EventStore` with `emit()`, `track()`/`untrack()`, `set_publisher_sink()` | ⬚ | No global event store exists |
| 1.5 | Create `baml_collector` crate with `Collector`, `track_call()`, `function_logs()`, `Drop` | ⬚ | Crate does not exist yet |
| 1.6 | Add `root_span_id: Option<SpanId>` + `parent_span_id: Option<SpanId>` to `BexEngine::call_function()` | ⬚ | Current signature: `call_function(&self, function_name: &str, args: &[BexValue])` — no tracing params |
| 1.7 | Add `baml_events` dependency to `bex_engine/Cargo.toml` | ⬚ | Not present |
| 1.8 | Emit `FunctionStart`/`FunctionEnd` via `event_bus::emit()` in `call_function()` for non-LLM functions | ⬚ | No event emission code in the engine |
| 1.9 | Add `function_has_llm_meta()` helper to check `FunctionMeta::Llm` | ⬚ | `FunctionMeta::Llm` is accessed in `llm.rs` but no `function_has_llm_meta` helper exists on `BexEngine` |
| 1.10 | Register `baml_events` and `baml_collector` in workspace `Cargo.toml` | ⬚ | Neither crate is a workspace member |

**Gaps found**: None — the doc correctly identifies everything as new. The codebase has zero event infrastructure in `baml_language/`.

### Milestone 2: Global Publisher for S3 / Boundary API

| # | Item | Status | Notes |
|---|------|--------|-------|
| 2.1 | Create `baml_publisher` crate with `Cargo.toml` | ⬚ | Crate does not exist |
| 2.2 | Implement `start_publisher()` with `OnceCell<mpsc::Sender>` pattern | ⬚ | |
| 2.3 | Implement `TracePublisher::run()` background loop (batch + timer + flush) | ⬚ | |
| 2.4 | Implement `upload_batch()` (JSON serialize → gzip → presigned S3 URL → PUT) | ⬚ | |
| 2.5 | Register publisher as `publisher_sink` on `EventStore` via `set_publisher_sink()` | ⬚ | |
| 2.6 | Call `start_publisher()` during CFFI init in `bridge_cffi` | ⬚ | No initialization code in bridge_cffi for publishing |
| 2.7 | Add `baml_publisher` dependency to `bridge_cffi/Cargo.toml` | ⬚ | Not present |
| 2.8 | Implement `flush()` and `shutdown()` publisher messages | ⬚ | |
| 2.9 | Read `BOUNDARY_API_KEY`, `BOUNDARY_API_URL`, `BAML_TRACE_BATCH_SIZE` env vars | ⬚ | |

**Gaps found**:
- **Missing from doc**: The doc doesn't mention how/where the `RuntimeEvent` → RPC format conversion works. `engine/` has `TraceEventWithMeta` → `baml_rpc` conversion. The doc mentions "reusing `baml_rpc` types" but doesn't detail the mapping. Should add a note about the serialization format — do we reuse `engine/`'s `baml_rpc` protos or define new ones?
- **Missing from doc**: No mention of `baml_publisher` being added to the workspace `Cargo.toml`.

### Milestone 3: FunctionStart / FunctionEnd for Nested LLM Calls

| # | Item | Status | Notes |
|---|------|--------|-------|
| 3.1 | Add `SysOp::EventSend` variant to `bex_vm_types/src/types.rs` | ⬚ | Not in the current 18-variant `SysOp` enum |
| 3.2 | Add `SysOp::NewRequestId` variant | ⬚ | Not in the enum |
| 3.3 | Add `baml.events.send()` builtin in `baml_builtins/src/lib.rs` | ⬚ | No `events` module in the `with_builtins!` macro |
| 3.4 | Add `baml.events.new_request_id()` builtin | ⬚ | |
| 3.5 | Map `baml.events.send` → `SysOp::EventSend` in compiler | ⬚ | |
| 3.6 | Handle `SysOp::EventSend` in engine's `ScheduleFuture` arm | ⬚ | Current `execute_sys_op()` has no `EventSend` case |
| 3.7 | Handle `SysOp::NewRequestId` in engine | ⬚ | |
| 3.8 | Add `baml.events.send("function_start/end")` calls to `call_llm_function()` in `llm.baml` | ⬚ | Current `llm.baml` has no event sends |
| 3.9 | Change `FunctionBody::Llm` compilation from stub to delegation to `call_llm_function()` | ⬚ | Currently compiles as empty bytecode + `SysOp::RenderPrompt` stub with TODO |
| 3.10 | Implement return type casting (string → declared return type) | ⬚ | |
| 3.11 | Implement `build_event_from_args()` helper in engine | ⬚ | |

**Gaps found**:
- **Missing from doc**: The doc says "the engine does not double-emit" for LLM functions but doesn't explain exactly how `call_function()` detects this. The doc mentions `function_has_llm_meta()` but item 3.9 changes LLM functions to delegate to `call_llm_function()` — once delegation is compiled, the top-level call is *still* a function with `FunctionMeta::Llm`. The engine needs to know: "should I emit top-level FunctionStart for this?" The answer in M1 is "if not LLM, emit." But after M3, LLM functions delegate to `call_llm_function()` which emits its own FunctionStart. **However**, the top-level function now has real bytecode (the delegation call), not a stub. The engine check `function_has_llm_meta()` should still work since `body_meta: Some(FunctionMeta::Llm{..})` is preserved. Worth clarifying in the doc.
- **Missing from doc**: `LlmParseResponse` SysOp currently panics with `"not yet implemented - TODO"`. M3 depends on `primitive_client.parse()` working. The doc mentions this is "unimplemented" in Open Question 6, but M3's test assumes it works. Should note M3 has a prerequisite: `LlmParseResponse` must be implemented first (or mocked for tests).
- **Missing from doc**: After M3, the engine's `execute_sys_op()` dispatch needs to NOT dispatch `SysOp::EventSend` through the normal path — it should be handled inline in the event loop BEFORE `execute_sys_op()`. The doc shows this correctly in the M3/M4 code but doesn't call it out as a change to the existing dispatch flow.

### Milestone 4: Span Context and Nested Call IDs

| # | Item | Status | Notes |
|---|------|--------|-------|
| 4.1 | Implement `SpanStack` struct with `SpanEntry`, `parent_of_root` | ⬚ | No span tracking exists in the engine |
| 4.2 | Implement `push()`, `pop()`, `current()`, `new_with_parent()` on `SpanStack` | ⬚ | |
| 4.3 | Create `SpanStack` in `call_function()`, pass to `run_event_loop_with_epoch()` | ⬚ | `run_event_loop_with_epoch()` currently takes `(vm, my_epoch)` only |
| 4.4 | Add `span_stack: &mut SpanStack` parameter to `run_event_loop_with_epoch()` | ⬚ | |
| 4.5 | In `EventSend` handler: push on `function_start`, pop on `function_end`, current for others | ⬚ | |
| 4.6 | Compute `FunctionEnd.duration` from `SpanStack` pop | ⬚ | |

**Gaps found**: None — straightforward.

### Milestone 5: Intermediate LLM Events

| # | Item | Status | Notes |
|---|------|--------|-------|
| 5.1 | Define `LlmEvent`, `LlmRequest`, `LlmRawRequest`, `LlmRawResponse`, `LlmResponse` in `baml_events/src/llm.rs` | ⬚ | |
| 5.2 | Define `RequestId` type | ⬚ | |
| 5.3 | Define `TokenUsage` type | ⬚ | |
| 5.4 | Add `baml.events.send()` calls for `llm_request`, `llm_raw_request`, `llm_raw_response`, `llm_response` to `llm.baml` | ⬚ | Current `llm.baml` has no event sends |
| 5.5 | Add `request_id = baml.events.new_request_id()` to `llm.baml` | ⬚ | |
| 5.6 | Implement full `build_event_kind()` for all LLM event types | ⬚ | |
| 5.7 | Implement `execute_parse_response()` to return map with `content`/`model`/`finish_reason`/`usage`/`client_stack` | ⬚ | Currently panics: `"LlmParseResponse SysOp not yet implemented"` |

**Gaps found**:
- **Missing from doc**: `execute_parse_response()` is a hard prerequisite for M5 (and also M3). This function currently panics. The doc should list this as a prerequisite or as part of M3/M5's work items. It requires porting `llm_ops::parse_response` from the legacy engine, which is non-trivial (provider-specific parsing for OpenAI, Anthropic, etc.).
- **Missing from doc**: The doc shows `primitive_client.parse()` returning a map with `model`, `finish_reason`, `usage`, `client_stack`. But the current builtin signature is `fn parse(self: PrimitiveClient, response: Response, function_name: String) -> Any`. The `Any` return type needs to actually be a map. Need to verify the BAML type system handles this correctly — or change the builtin signature.

### Milestone 6: Publishing Events to Host Languages (CFFI)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 6.1 | Extract collectors from protobuf in `call_function_inner()` | ⬚ | Currently ignored with TODO comment |
| 6.2 | Create `root_span_id` and register collectors | ⬚ | No span ID creation |
| 6.3 | Extract `HostSpanContext` from call args | ⬚ | No host span context handling |
| 6.4 | Pass `root_span_id` + `parent_span_id` to `engine.call_function()` | ⬚ | Current call: `engine.call_function(&func_name, &bex_args)` |
| 6.5 | Add Collector FFI exports (`collector_new`, `collector_logs`, etc.) in `objects.rs` | ⬚ | `objects.rs` only has stub `call_object_constructor` and `call_object_method` |
| 6.6 | Add `FunctionLogMessage` and `TokenUsageMessage` to protobuf schema | ⬚ | Not in current proto |
| 6.7 | Implement `encode_function_logs()` helper | ⬚ | |
| 6.8 | Add `baml_events`, `baml_collector` dependencies to `bridge_cffi/Cargo.toml` | ⬚ | Not present |

**Gaps found**:
- **Missing from doc**: The doc doesn't mention updating the protobuf `HostFunctionArguments` message to carry `HostSpanContext` (span_id + root_span_id). Currently the proto has `repeated BamlObjectHandle collectors = 4;` but no span context fields. This is listed in Open Question 7 but should also be noted as a concrete code change.
- **Missing from doc**: The `call_function_stream_from_c` is currently a stub that returns an error. The doc doesn't mention streaming at all for M6 — should note that streaming support is deferred and the stream FFI entry point will need the same span context plumbing when implemented.
- **Missing from doc**: `call_object_constructor` and `call_object_method` are stubs. If `Collector` is exposed as a host-language object, the doc should note whether it goes through these stubs or gets its own dedicated FFI exports (the doc shows dedicated exports, which is correct — but should note the stubs remain unrelated).

### Milestone 7: Header Events

| # | Item | Status | Notes |
|---|------|--------|-------|
| 7.1 | Define `HeaderEvent`, `HeaderEnter`, `HeaderExit` in `baml_events/src/header.rs` | ⬚ | |
| 7.2 | Handle `VmExecState::Notify(Viz)` → emit `Header` events | ⬚ | Currently: `VmExecState::Notify(_notification) => { // Ignore watch notifications for now }` |
| 7.3 | Add `header_enter`/`header_exit` to `call_llm_function()` in `llm.baml` | ⬚ | |
| 7.4 | Handle `header_enter`/`header_exit` in `build_event_kind()` | ⬚ | |

**Gaps found**:
- The doc correctly identifies that `VmExecState::Notify` is currently ignored. The VM already produces `WatchNotification::Viz` with `VizExecEvent` (confirmed: `VizEnter`/`VizExit` instructions exist in `bex_vm/src/vm.rs`). The types `VizExecEvent`, `VizNodeType`, `VizExecDelta` all exist in `bex_vm_types/src/bytecode.rs`. The mapping should be straightforward.
- **Missing from doc**: The doc's `HeaderEnter` struct references `node_id: u32` and `node_type: VizNodeType`. The existing `VizExecEvent` has `node_id: u32`, `node_type: VizNodeType`, `label: String`, `header_level: Option<u8>`. The doc matches this, but should explicitly note that `VizNodeType` is re-exported from `bex_vm_types::bytecode` (it already exists, doesn't need to be created).

### Milestone 8: Host-Language Span Tracking (`@trace`)

| # | Item | Status | Notes |
|---|------|--------|-------|
| 8.1 | Create `HostSpanManager` struct in `bridge_cffi/src/host_spans.rs` | ⬚ | File doesn't exist |
| 8.2 | Implement `enter()`, `exit()`, `current_context()`, `deep_clone()`, `upsert_tags()` | ⬚ | |
| 8.3 | Create `HostSpanContext` struct | ⬚ | |
| 8.4 | Expose `HostSpanManager` to Python via FFI in `objects.rs` | ⬚ | |
| 8.5 | Update Python `CtxManager` to use `HostSpanManager` instead of `RuntimeContextManager` | ⬚ | Current `ctx_manager.py` uses `RuntimeContextManager` from engine |
| 8.6 | Update generated `tracing.py` template | ⬚ | Current template delegates to `BamlCtxManager.trace_fn` |
| 8.7 | Update generated `globals.py` template | ⬚ | Current template creates `BamlCtxManager(BamlRuntime)` |
| 8.8 | Update generated client functions to pass `host_ctx` to `call_function` | ⬚ | Current generated clients don't pass span context |
| 8.9 | Add `HostSpanContext` fields to the inbound protobuf (`HostFunctionArguments`) | ⬚ | Not in current proto |

**Gaps found**:
- **Missing from doc**: The doc doesn't mention updating the Python `baml_py` package's `.pyi` stub file (`baml_py.pyi`) which exposes the type signatures for IDE completion. When `HostSpanManager` replaces `RuntimeContextManager`, the `.pyi` needs updating.
- **Missing from doc**: The doc doesn't mention the TypeScript equivalent. The TS client has `async_context_vars.ts` with its own `BamlCtxManager` class. M8 describes Python but the same pattern needs to be applied to TS. Should at least note this as follow-up or include a TS subsection.
- **Missing from doc**: The generated Python client code is produced by Rust code in `engine/generators/languages/python/src/`. The `_templates/tracing.py` and `_templates/globals.py` are literal Python templates that get copied. But the generated `async_client.py` / `sync_client.py` are generated by Rust code, not templates. The doc should note that the client generation Rust code needs to emit the `current_host_context()` call.

---

## 18. References

> (Previously section 17 — renumbered after adding Implementation Checklists.)

### engine/ Files

| File | What |
|------|------|
| `engine/baml-runtime/src/tracingv2/storage/storage.rs` | `TraceStorage`, `Collector`, `FunctionLog`, ref counting |
| `engine/baml-runtime/src/tracing/mod.rs` | `start_call()`, `finish_call()`, event emission |
| `engine/baml-lib/baml-types/src/tracing/events.rs` | `TraceEvent`, `TraceData`, `FunctionStart/End` |
| `engine/baml-runtime/src/lib.rs` | `TracingCallGuard`, `call_id_stack` |
| `engine/language_client_python/src/types/log_collector.rs` | Python Collector wrapper |
| `engine/language_client_cffi/src/ffi/functions.rs` | CFFI function call entry points |
| `engine/language_client_python/python_src/baml_py/ctx_manager.py` | Python `CtxManager`: contextvar + thread_id + deep_clone + `@trace` decorator |
| `engine/baml-runtime/src/types/context_manager.rs` | `RuntimeContextManager`: Rust span stack with enter/exit/tags |
| `engine/language_client_python/src/types/span.rs` | `BamlSpan`: PyO3 wrapper for `TracingCall` (start_call/finish_call) |

### baml_language/ Files

| File | What |
|------|------|
| `bex_engine/src/lib.rs` | `BexEngine`, `call_function()`, `run_event_loop_with_epoch()`, `execute_sys_op()` |
| `bex_engine/src/llm.rs` | `execute_get_jinja_template()`, `execute_get_client_function()` |
| `bex_vm/src/vm.rs` | `VmExecState`, `BexVm`, instruction execution, `DispatchFuture`/`Await` |
| `bex_vm_types/src/types.rs` | `SysOp`, `FunctionKind`, `FunctionMeta`, `Function` |
| `baml_builtins/src/lib.rs` | `with_builtins!` macro, builtin definitions |
| `baml_compiler_emit/src/lib.rs` | LLM function compilation, `sys_op_for_builtin()` |
| `bridge_cffi/src/ffi/functions.rs` | `call_function_from_c()`, `call_function_stream_from_c()` |
| `bridge_cffi/src/ffi/callbacks.rs` | `send_result_to_callback()`, `send_error_to_callback()` |
| `bridge_cffi/src/ctypes/value_encode.rs` | `external_to_cffi_value()` |
| `bex_external_types/src/bex_external_value.rs` | `BexExternalValue` enum |

### New Files (to be created)

| File | What |
|------|------|
| `baml_events/Cargo.toml` | New types + EventStore crate manifest |
| `baml_events/src/lib.rs` | `RuntimeEvent`, `EventKind`, `SpanContext` |
| `baml_events/src/function.rs` | `FunctionEvent`, `FunctionStart`, `FunctionEnd` |
| `baml_events/src/llm.rs` | `LlmEvent`, `LlmRequest`, `LlmResponse`, etc. |
| `baml_events/src/header.rs` | `HeaderEvent`, `HeaderEnter`, `HeaderExit` |
| `baml_events/src/event_store.rs` | Global `EventStore`, `emit()`, `track()`/`untrack()`, `set_publisher_sink()` |
| `baml_collector/Cargo.toml` | New collector crate manifest |
| `baml_collector/src/lib.rs` | `Collector`, `track_call()`, `function_logs()`, `Drop` impl |
| `baml_publisher/Cargo.toml` | New publisher crate manifest |
| `baml_publisher/src/lib.rs` | `start_publisher()`, `publish_event()`, `flush()`, `TracePublisher` |
| `bridge_cffi/src/host_spans.rs` | `HostSpanManager`, `HostSpanContext`, host-language span lifecycle |
