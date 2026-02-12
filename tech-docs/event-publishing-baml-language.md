# Event Publishing System for baml_language Compiler

**Status:** Draft  
**Authors:** BAML Team  
**Created:** 2026-02-05

## Overview

This document proposes an event publishing system for the new `baml_language` compiler that mirrors the functionality in `engine/baml-runtime`. The goal is to enable:

1. **Trace Events** — Runtime function execution tracing (start, end, LLM requests/responses)
2. **Stream Events** — Incremental streaming updates during LLM function execution
3. **Header Context Events** — Hierarchical execution context tracking (`header-enter`, `header-exit`)

## Background: Existing Implementation in `engine/`

### Current Architecture

The existing event publishing system in `engine/baml-runtime` consists of three main components:

#### 1. Publisher (`engine/baml-runtime/src/tracingv2/publisher/publisher.rs`)

A global singleton that handles async event batching and upload to Boundary API:

```rust
enum PublisherMessage {
    Trace(Arc<TraceEventWithMeta>),
    Flush(tokio::sync::oneshot::Sender<()>),
    UpdateRuntime(Arc<RuntimeAST>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

static PUBLISHING_CHANNEL: OnceCell<mpsc::Sender<PublisherMessage>> = OnceCell::new();
```

Key behaviors:
- **Batching**: Collects events into batches (default 500 events, configurable via `BAML_TRACE_BATCH_SIZE`)
- **Periodic flush**: Flushes every 2 seconds or when batch is full
- **Compression**: Compresses payloads > 2MB before upload
- **S3 Upload**: Uploads batched events via presigned S3 URLs

#### 2. Storage (`engine/baml-runtime/src/tracingv2/storage/storage.rs`)

Reference-counted event storage for collectors:

```rust
pub struct TraceStorage {
    call_map: HashMap<FunctionCallId, Vec<Arc<TraceEventWithMeta>>>,
    ref_counts: HashMap<FunctionCallId, usize>,
    function_inners: Arc<Mutex<HashMap<FunctionCallId, Arc<Mutex<FunctionLogInner>>>>>,
}
```

Key behaviors:
- Events are immediately published AND stored locally if collectors are active
- Reference counting tracks active collectors per function call
- Events are grouped by `FunctionCallId` for retrieval

#### 3. TraceEvent Types (`engine/baml-lib/baml-types/src/tracing/events.rs`)

```rust
pub struct TraceEvent<'a, T: HasType<type_meta::NonStreaming>> {
    pub call_id: FunctionCallId,
    pub function_event_id: FunctionEventId,
    pub content: TraceData<'a, T>,
    pub call_stack: Vec<FunctionCallId>,
    pub timestamp: web_time::SystemTime,
}

pub enum TraceData<'a, T: HasType<type_meta::NonStreaming>> {
    FunctionStart(FunctionStart<T>),
    FunctionEnd(FunctionEnd<'a, T>),
    SetTags(TraceTags),
    LLMRequest(Arc<LoggedLLMRequest>),
    RawLLMRequest(Arc<HTTPRequest>),
    RawLLMResponse(Arc<HTTPResponse>),
    RawLLMResponseStream(Arc<HTTPResponseStream>),
    LLMResponse(Arc<LoggedLLMResponse>),
}
```

### Header Context Events (`engine/baml-runtime/src/control_flow.rs`)

Headers create hierarchical execution contexts for visualization and filtering:

```rust
pub enum NodeType {
    FunctionRoot,
    HeaderContextEnter,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

fn enter_header(&mut self, header: &hir::HeaderContext) {
    let level = header.level.max(1);
    self.pop_headers_to_level(level - 1);  // Close deeper headers
    
    let node = Node::new(
        node_id,
        parent_id,
        log_filter_key,
        header.title.clone(),
        header.span.clone(),
        NodeType::HeaderContextEnter,
    );
    self.graph.add_node(node);
    
    self.frames.push(Frame::new(
        FrameEntry::Header { level },
        node_id,
        Some(segment),
    ));
}
```

### Stream Events

Stream events are handled separately via language-specific callbacks:

```rust
// TypeScript native.d.ts
export interface StreamEvent {
    streamId: string
    eventType: string  // "start" | "update" | "end"
    value?: string     // For "update" events
}
```

## Proposed Design for `baml_language`

### Design Goals

1. **Unified Event System** — Single abstraction for all event types
2. **Decoupled from Salsa** — Events should work independently of compilation
3. **Pluggable Consumers** — Support multiple event destinations (publisher, collectors, LSP)
4. **Zero-cost when disabled** — No overhead when event publishing is not needed
5. **WASM-compatible** — Work in both native and WASM contexts
6. **One event type everywhere** — No intermediate enums that just get translated

### Key Design Decision: Eliminate `WatchNotification`, Use `RuntimeEvent` Directly

Today the VM defines its own notification type:

```rust
// bex_vm/src/vm.rs — CURRENT
pub enum VmExecState {
    Await(HeapPtr),
    ScheduleFuture(HeapPtr),
    Complete(Value),
    Notify(WatchNotification),   // ← VM-specific enum
}

pub enum WatchNotification {
    Variables(Vec<NodeId>),
    Block(BlockNotification),
    Viz { function_name: String, event: VizExecEvent },
}
```

The engine then has to translate each variant into a `RuntimeEvent`. This is
redundant — every `WatchNotification` maps 1:1 to a `RuntimeEvent` variant, and
the only consumer (`bex_engine`) currently **discards them entirely**.

**Proposal: replace `WatchNotification` with `RuntimeEvent` in the VM yield.**

```rust
// bex_vm/src/vm.rs — PROPOSED
pub enum VmExecState {
    Await(HeapPtr),
    ScheduleFuture(HeapPtr),
    Complete(Value),
    Event(RuntimeEvent),   // ← unified event type
}
```

#### Why the VM must still yield (not fire-and-forget)

The yield semantics matter. When the VM hits an event it **pauses** and
returns control to the engine. The engine then:

1. Dispatches the `RuntimeEvent` to all registered sinks.
2. Calls the host-language callback (Python/TS/etc.).
3. Resumes the VM.

This synchronous yield is required by **multiple** consumers:

- **Streaming in Python/TS** — When a user writes `async for partial in
  stream(b.MyLlmFunction("hello"))`, each `Stream(Update)` or
  `Watch(VariableChanged)` yield is the moment the host language delivers
  the next partial result to the caller's `for` loop. If the VM
  fire-and-forgot, the host would have no synchronization point to feed
  the iterator — events would pile up in a queue and the user's loop
  would either see nothing or see them all at the end.

- **Playground / IDE preview** — The playground renders each watch update
  and header enter/exit before the next instruction executes. Without
  the yield, the UI would only see the final state.

- **Backpressure** — If a sink (e.g., the Boundary publisher) is slow or
  the host callback is expensive, the yield naturally throttles the VM.
  Fire-and-forget would require a separate bounded channel + drop policy
  just to avoid unbounded memory growth.

A fire-and-forget model would lose all of this. Keeping the yield but
changing the payload from `WatchNotification` to `RuntimeEvent` gives us
the best of both worlds:

- **No translation layer** — the VM produces the final event type directly.
- **Synchronous delivery** — the engine controls when to resume.
- **Single type** — sinks, engine, host callbacks all speak `RuntimeEvent`.
- **Streaming for free** — the same yield that powers watch also powers
  `Stream(Update)` delivery to Python/TS iterators.

#### What changes in the VM

Each place that currently constructs a `WatchNotification` instead constructs
a `RuntimeEvent`:

```rust
// BEFORE (bex_vm/src/vm.rs)
return Ok(VmExecState::Notify(WatchNotification::Variables(notifications)));

// AFTER
return Ok(VmExecState::Event(RuntimeEvent::Watch(WatchEvent {
    meta: EventMeta::from_vm(self),
    data: WatchEventData::VariableChanged(WatchVariableChanged {
        variable_name: var_name.to_string(),
        channel: channel.to_string(),
        old_value: old_val.to_json(),
        new_value: new_val.to_json(),
        change_path: Some(change_path),
        notified_roots: root_ids.iter().map(|id| format!("{id:?}")).collect(),
    }),
})));
```

```rust
// BEFORE
return Ok(VmExecState::Notify(WatchNotification::Viz { function_name, event }));

// AFTER (header enter)
return Ok(VmExecState::Event(RuntimeEvent::Header(HeaderEvent {
    meta: EventMeta::from_vm(self),
    data: HeaderEventData::Enter(HeaderEnter {
        level: event.header_level,
        title: event.label.clone(),
        span: None,
        node_id: event.node_id.to_string(),
        parent_node_id: None,
    }),
})));
```

```rust
// BEFORE
return Ok(VmExecState::Notify(WatchNotification::Block(notification)));

// AFTER
return Ok(VmExecState::Event(RuntimeEvent::Block(BlockEvent {
    meta: EventMeta::from_vm(self),
    data: if notification.is_enter {
        BlockEventData::Enter(notification.into())
    } else {
        BlockEventData::Exit(notification.into())
    },
})));
```

#### What changes in the engine

The engine loop becomes trivially simple — no mapping needed:

```rust
// bex_engine/src/lib.rs — PROPOSED
VmExecState::Event(event) => {
    // 1. Dispatch to all registered sinks (publisher, collector, etc.)
    if let Some(dispatcher) = &self.event_dispatcher {
        dispatcher.dispatch(event.clone());
    }

    // 2. Call host-language callback if registered
    if let Some(on_event) = &self.on_event_callback {
        on_event(&event);
    }

    // 3. VM automatically continues on next loop iteration
}
```

Compare to the translation layer that would otherwise be needed:

```rust
// bex_engine/src/lib.rs — AVOIDED (the WatchNotification approach)
VmExecState::Notify(notification) => {
    match notification {
        WatchNotification::Variables(nodes) => {
            for node_id in nodes {
                let (var_name, channel) = self.vm.watched_vars.get(...);
                let state = self.vm.watch.root_state(node_id);
                dispatcher.dispatch(RuntimeEvent::Watch(WatchEvent {
                    // ... 15 lines of field mapping ...
                }));
            }
        }
        WatchNotification::Block(block) => {
            dispatcher.dispatch(RuntimeEvent::Block(BlockEvent {
                // ... more mapping ...
            }));
        }
        WatchNotification::Viz { function_name, event } => {
            match event.node_type {
                RuntimeNodeType::HeaderContextEnter => {
                    // ... even more mapping ...
                }
                _ => { /* ... */ }
            }
        }
    }
}
```

#### Dependency implications

This means `bex_vm` takes a dependency on the `baml_events` crate. This is
acceptable because:

- `baml_events` is a leaf crate with no heavy dependencies (just `serde_json`).
- The VM already knows everything needed to construct events (variable names,
  values, header metadata, function names).
- The alternative (keeping `WatchNotification`) just moves the same logic to
  `bex_engine` with more boilerplate.

#### EventMeta helper on the VM

Add a helper to construct `EventMeta` from VM state:

```rust
impl EventMeta {
    /// Construct from VM state. Call stack comes from the engine's
    /// TraceContext which is passed to the VM at creation.
    pub fn from_vm(vm: &BexVm) -> Self {
        Self {
            call_id: vm.trace_ctx.current_call_id(),
            event_id: EventId::new(),
            call_stack: vm.trace_ctx.call_stack(),
            timestamp: std::time::SystemTime::now(),
        }
    }
}
```

### Key Design Decision: Channel-Based Event Delivery (Not Callbacks)

Instead of passing `on_event` callback closures (as `engine/` does), events
are delivered through **channels**. When calling a function, the caller can
pass in a `tokio::sync::mpsc::UnboundedSender<RuntimeEvent>` (or bounded).
The engine writes events to that channel; the caller reads from the receiver.

**Why channels instead of callbacks:**
- Callbacks create lifetime problems — the closure must outlive the async
  execution, leading to `'static` bounds and `Arc` gymnastics.
- Callbacks in CFFI require `block_in_place` to avoid deadlocks.
- Channels decouple producer from consumer — the engine writes, the host
  language reads at its own pace.
- Channels are composable — you can fan-out, filter, or buffer.
- Channels work naturally with async iterators in Python/TS.

```rust
// Caller creates channel, passes sender to engine
let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel::<RuntimeEvent>();

let result = engine.call_function("MyFunc", &args, Some(event_tx)).await?;

// Meanwhile, another task reads events:
while let Some(event) = event_rx.recv().await {
    match event {
        RuntimeEvent::Stream(StreamEvent { data: StreamEventData::Update { value, .. }, .. }) => {
            // encode to protobuf, send to host language
        }
        _ => { /* dispatch to sinks */ }
    }
}
```

In `bridge_cffi`, the C FFI layer creates the channel internally and spawns
a reader task that encodes events to protobuf and calls the C callback:

```rust
fn call_function_stream_inner(..., id: u32) -> Result<(), BridgeError> {
    let (event_tx, mut event_rx) = tokio::sync::mpsc::unbounded_channel();

    // Spawn event consumer that forwards to C callbacks
    let rt = get_runtime().clone();
    rt.spawn(async move {
        while let Some(event) = event_rx.recv().await {
            match &event {
                RuntimeEvent::Stream(StreamEvent { data: StreamEventData::Update { value, .. }, .. }) => {
                    let cffi = stream_value_to_cffi(value).unwrap();
                    let buf = cffi.encode_to_vec();
                    tokio::task::block_in_place(|| {
                        get_result_callback()(id, 0, buf.as_ptr() as *const i8, buf.len());
                    });
                }
                _ => {} // other events dispatched elsewhere
            }
        }
    });

    // Spawn function execution
    rt.spawn(async move {
        let result = engine.call_function("MyFunc", &args, Some(event_tx)).await;
        // send final result via callback with is_done=1
    });
    Ok(())
}
```

### Key Design Decision: Events via `baml.events.send()` Builtin (Not Special Opcodes)

Instead of special opcodes like `LlmFunctionEnter` / `LlmFunctionExit`,
event emission uses a **builtin function**: `baml.events.send(event)`.

**How LLM functions work today in `bex_engine`:**

LLM functions are compiled with `FunctionKind::SysOp(SysOp::RenderPrompt)`
and `FunctionMeta::Llm { prompt_template, client }`. The LLM call sequence
is orchestrated through a chain of builtin SysOp calls:

```
1. baml.llm.get_jinja_template(function_name) → template string
2. baml.llm.get_client_function(function_name) → client resolve fn
3. client_resolve_fn() → PrimitiveClient
4. PrimitiveClient.render_prompt(template, args) → PromptAst
5. PrimitiveClient.specialize_prompt(prompt) → specialized PromptAst
6. PrimitiveClient.build_request(prompt) → HttpRequest map
7. baml.http.send(request) → HttpResponse     ← async, yields to engine
8. PrimitiveClient.parse(response, function_name) → typed result
```

**Key insight**: Steps 1–8 are all either SysOps or builtin function calls
that the VM already knows how to execute. We don't need special tracing
opcodes — we just need the ability to emit events at the right points in
this sequence. A `baml.events.send()` builtin lets us do exactly that.

**How `baml.events.send()` works:**

```rust
// In baml_builtins — add a new builtin
mod baml {
    mod events {
        /// Send a runtime event. The engine picks it up from the channel.
        fn send(event_type: String, payload: BexExternalValue) -> null;
        // This is a SysOp — it yields to the engine which writes to the channel.
    }
}
```

The compiler can insert `baml.events.send(...)` calls at the right points.
For example, wrapping an LLM function call:

```
// Compiled bytecode for an LLM function (pseudocode):

// 1. Emit function start event
baml.events.send("function_start", { name: "MyFunc", args: { ... } })

// 2. The actual LLM call sequence
let template = baml.llm.get_jinja_template("MyFunc")
let client_fn = baml.llm.get_client_function("MyFunc")
let client = client_fn()

baml.events.send("llm_request_start", { client: client.name, ... })

let prompt = client.render_prompt(template, args)
let prompt = client.specialize_prompt(prompt)
let request = client.build_request(prompt)
let response = baml.http.send(request)

baml.events.send("llm_request_end", { ... })

let result = client.parse(response, "MyFunc")

// 3. Emit function end event
baml.events.send("function_end", { name: "MyFunc", result: result })

return result
```

**Why this is better than special opcodes:**
- **No new opcodes needed** — uses existing builtin/SysOp infrastructure.
- **Composable** — any function (not just LLM) can emit events.
- **The compiler controls placement** — the compiler inserts `send()` calls
  at the right points during emission.
- **Engine is simple** — it just writes the event to the channel.
- **Testable** — you can test event emission with the existing VM test infra.
- **All values are `BexExternalValue`** — no `serde_json::Value` anywhere.

**Implementation as a SysOp:**

```rust
// In bex_vm_types/src/types.rs — add new SysOp
pub enum SysOp {
    // ... existing ops ...
    /// Emit a runtime event: `baml.events.send(event_type, payload) -> null`
    EventSend,
}

// In bex_engine/src/lib.rs — handle the SysOp
SysOp::EventSend => {
    // args[0] = event type string, args[1] = payload (BexExternalValue)
    let event_type = args[0].as_str()?;
    let payload = args[1].clone();
    let event = RuntimeEvent::from_raw(event_type, payload);
    if let Some(tx) = &self.event_channel {
        let _ = tx.send(event);
    }
    SysOpResult::Ready(Ok(BexExternalValue::Null))
}
```

**Note on `VmExecState::Event`**: We still change `Notify` to `Event` for
watch variable events (which are VM-internal). But for tracing/LLM events,
the emission goes through `baml.events.send()` → SysOp → engine writes to
channel. The VM doesn't need to yield for these — they're fire-and-forget
through the channel.

### Architecture

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                              Event Sources                                   │
├───────────────────┬─────────────────────┬───────────────────────────────────┤
│   VM Execution    │   Compiler-inserted │         Engine-level              │
│   (watch yields)  │   baml.events.send()│         (SysOp handler)          │
└────────┬──────────┴──────────┬──────────┴────────────────┬──────────────────┘
         │                     │                           │
         │     All write to the same channel:              │
         ▼                     ▼                           ▼
┌─────────────────────────────────────────────────────────────────────────────┐
│              mpsc::UnboundedSender<RuntimeEvent>                              │
│                                                                             │
│  Caller creates (tx, rx) pair. Passes tx into engine.call_function().       │
│  Engine writes events. Caller reads from rx.                                │
└──────────────────────────────┬──────────────────────────────────────────────┘
                               │
                     mpsc::UnboundedReceiver
                               │
         ┌─────────────────────┼─────────────────────┐
         │                     │                     │
         ▼                     ▼                     ▼
┌─────────────────┐  ┌─────────────────┐  ┌─────────────────┐
│   Publisher     │  │   Collector     │  │   LSP Notifier  │
│   (Boundary)    │  │   (Local)       │  │   (IDE)         │
└─────────────────┘  └─────────────────┘  └─────────────────┘

   (Consumers can be tasks reading from cloned rx, or a dispatcher
    task that fans out to multiple sinks from a single rx)
```

### Signal-to-Event Translation Map

Each component in the system produces its own internal signals. These must be translated into the unified `RuntimeEvent` enum before reaching sinks. The following diagram and tables show every signal origin, what it currently does, and what `RuntimeEvent` it becomes.

#### Complete Signal Flow

The diagram below reflects the proposed design: events are written to a
**channel** (`mpsc::UnboundedSender<RuntimeEvent>`). There are two emission
paths: VM-internal yields (`VmExecState::Event`) for watch/viz events, and
`baml.events.send()` SysOp for tracing/LLM/stream events.

```
┌──────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                    PRODUCERS (construct RuntimeEvent directly — no intermediate types)                │
│                                                                                                      │
│  ┌────────────────────────────────────────────────────────────────────────┐                          │
│  │  bex_vm  (yields VmExecState::Event(RuntimeEvent))                    │                          │
│  │                                                                        │                          │
│  │  watch let x = ...          ──▶  RuntimeEvent::Watch(Registered)       │                          │
│  │  x = newVal / x.f = v      ──▶  RuntimeEvent::Watch(VariableChanged)  │                          │
│  │  watched var out of scope   ──▶  RuntimeEvent::Watch(Unregistered)    │                          │
│  │  $watch.options(...)        ──▶  RuntimeEvent::Watch(OptionsChanged)  │                          │
│  │  VizEnter (header node)     ──▶  RuntimeEvent::Header(Enter)          │                          │
│  │  VizExit  (header node)     ──▶  RuntimeEvent::Header(Exit)           │                          │
│  │  VizEnter/Exit (other)      ──▶  RuntimeEvent::Viz(...)               │                          │
│  │  BlockNotification          ──▶  RuntimeEvent::Block(Enter/Exit)      │                          │
│  └────────────────────────────────────────────────────────────────────────┘                          │
│                                                                                                      │
│  ┌────────────────────────────────────────────────────────────────────────┐                          │
│  │  bex_engine  (emits directly to dispatcher)                           │                          │
│  │                                                                        │                          │
│  │  call_function() entry      ──▶  RuntimeEvent::Function(Start)        │                          │
│  │  call_function() exit (ok)  ──▶  RuntimeEvent::Function(End::Success) │                          │
│  │  call_function() exit (err) ──▶  RuntimeEvent::Function(End::Error)   │                          │
│  │  LLM function detected      ──▶  RuntimeEvent::Header(Enter) [synth]  │                          │
│  │  LLM function completed     ──▶  RuntimeEvent::Header(Exit)  [synth]  │                          │
│  │  Stream opened              ──▶  RuntimeEvent::Stream(Start)          │                          │
│  │  Stream partial value       ──▶  RuntimeEvent::Stream(Update)         │                          │
│  │  Stream completed           ──▶  RuntimeEvent::Stream(End)            │                          │
│  │  Tags set                   ──▶  RuntimeEvent::Tags(...)              │                          │
│  └────────────────────────────────────────────────────────────────────────┘                          │
│                                                                                                      │
│  ┌────────────────────────────────────────────────────────────────────────┐                          │
│  │  sys_native / llm_ops  (emits directly to dispatcher)                 │                          │
│  │                                                                        │                          │
│  │  Prompt rendered            ──▶  RuntimeEvent::Llm(Request)           │                          │
│  │  HTTP request sent          ──▶  RuntimeEvent::Llm(RawRequest)        │                          │
│  │  HTTP response received     ──▶  RuntimeEvent::Llm(RawResponse)       │                          │
│  │  SSE chunk received         ──▶  RuntimeEvent::Llm(RawResponseStream) │                          │
│  │  Response parsed            ──▶  RuntimeEvent::Llm(Response)          │                          │
│  └────────────────────────────────────────────────────────────────────────┘                          │
│                                                                                                      │
└───────────────────────────────────────────────────────┬──────────────────────────────────────────────┘
                                                        │
                    ┌───────────────────────────────────┘
                    │  bex_engine dispatch loop:
                    │
                    │  VmExecState::Event(event) => {
                    │      dispatcher.dispatch(event.clone());  // fan out to sinks
                    │      on_event_callback(&event);           // host lang callback
                    │      // VM resumes on next iteration
                    │  }
                    │
                    └───────────────────────────────────┐
                                                        │
                                                        ▼
┌──────────────────────────────────────────────────────────────────────────────────────────────────────┐
│                                     SINKS (receive RuntimeEvent)                                     │
│                                                                                                      │
│   Sink                        │ Cares About                │ Ignores                                 │
│   ────────────────────────────┼────────────────────────────┼──────────────────────────────            │
│   BoundaryPublisher           │ Function, Llm, Header,     │ Watch (configurable),                   │
│                               │ Stream, Tags               │ Block, Viz                              │
│   ────────────────────────────┼────────────────────────────┼──────────────────────────────            │
│   LocalCollector              │ Function, Llm, Header,     │ (retains all if tracked)                │
│                               │ Stream, Watch, Tags        │                                         │
│   ────────────────────────────┼────────────────────────────┼──────────────────────────────            │
│   WatchProjection             │ Watch                      │ Everything else                         │
│   ────────────────────────────┼────────────────────────────┼──────────────────────────────            │
│   StreamProjection            │ Stream, Watch (if bound    │ Llm, Header, Block, Viz                 │
│                               │ to stream var)             │                                         │
│   ────────────────────────────┼────────────────────────────┼──────────────────────────────            │
│   LspNotifier                 │ Header, Viz, Block,        │ Llm raw events, Tags                   │
│                               │ Function, Watch            │                                         │
│                                                                                                      │
└──────────────────────────────────────────────────────────────────────────────────────────────────────┘
```

Note: there is **no translation layer** and **no callback**. Every producer
constructs `RuntimeEvent` directly and writes it to the channel. The engine
loop either writes to the channel (for `VmExecState::Event` yields) or the
SysOp handler writes to it (for `baml.events.send()` calls). Consumers read
from the channel receiver.

#### Table 1: RuntimeEvent Produced by Each Component

| Source | Trigger | RuntimeEvent | Priority |
|---|---|---|---|
| **`bex_vm`** | `watch let x = ...` executed | `Watch(Registered)` | Normal |
| **`bex_vm`** | Assignment to watched var or nested field | `Watch(VariableChanged)` | BestEffort |
| **`bex_vm`** | Watched var goes out of scope | `Watch(Unregistered)` | Normal |
| **`bex_vm`** | `$watch.options(...)` called | `Watch(OptionsChanged)` | Normal |
| **`bex_vm`** | `VizEnter` instruction (header node) | `Header(Enter)` | Critical |
| **`bex_vm`** | `VizExit` instruction (header node) | `Header(Exit)` | Critical |
| **`bex_vm`** | `VizEnter`/`VizExit` (branch, loop, etc.) | `Viz(...)` | Normal |
| **`bex_vm`** | `BlockNotification` instruction | `Block(Enter/Exit)` | Normal |
| **`bex_engine`** | `call_function()` entry | `Function(Start)` | Critical |
| **`bex_engine`** | `call_function()` exit (ok) | `Function(End::Success)` | Critical |
| **`bex_engine`** | `call_function()` exit (err) | `Function(End::Error)` | Critical |
| **`bex_engine`** | LLM function detected (`FunctionMeta::Llm`) | `Header(Enter)` [synthetic] | Critical |
| **`bex_engine`** | LLM function completed | `Header(Exit)` [synthetic] | Critical |
| **`bex_engine`** | Stream context opened | `Stream(Start)` | Normal |
| **`bex_engine`** | Typed partial value available | `Stream(Update)` | BestEffort |
| **`bex_engine`** | Stream completed | `Stream(End)` | Normal |
| **`bex_engine`** | User-defined tags | `Tags(...)` | Normal |
| **`sys_native`** | HTTP request dispatched | `Llm(RawRequest)` | Normal |
| **`sys_native`** | HTTP response received | `Llm(RawResponse)` | Normal |
| **`sys_native`** | SSE chunk received | `Llm(RawResponseStream)` | BestEffort |
| **`llm_ops`** | `RenderPrompt` sys op completes | `Llm(Request)` | Normal |
| **`llm_ops`** | `LlmParseResponse` sys op completes | `Llm(Response)` | Normal |

#### Table 3: RuntimeEvent → Sink Routing

| RuntimeEvent | Publisher | Collector | WatchProjection | StreamProjection | LspNotifier |
|---|:---:|:---:|:---:|:---:|:---:|
| `Function(Start)` | **yes** | **yes** | - | - | **yes** |
| `Function(End)` | **yes** | **yes** | - | - | **yes** |
| `Header(Enter)` | **yes** | **yes** | - | - | **yes** |
| `Header(Exit)` | **yes** | **yes** | - | - | **yes** |
| `Llm(Request)` | **yes** | **yes** | - | - | - |
| `Llm(RawRequest)` | **yes** | **yes** | - | - | - |
| `Llm(RawResponse)` | **yes** | **yes** | - | - | - |
| `Llm(RawResponseStream)` | **yes** | **yes** | - | - | - |
| `Llm(Response)` | **yes** | **yes** | - | - | - |
| `Stream(Start)` | **yes** | **yes** | - | **yes** | **yes** |
| `Stream(Update)` | opt-in | **yes** | - | **yes** | **yes** |
| `Stream(End)` | **yes** | **yes** | - | **yes** | **yes** |
| `Watch(Registered)` | - | **yes** | **yes** | - | **yes** |
| `Watch(VariableChanged)` | opt-in | **yes** | **yes** | **yes** (\*) | **yes** |
| `Watch(Unregistered)` | - | **yes** | **yes** | - | - |
| `Watch(OptionsChanged)` | - | **yes** | **yes** | - | - |
| `Block(Enter/Exit)` | - | opt-in | - | - | **yes** |
| `Viz(...)` | - | opt-in | - | - | **yes** |
| `Tags(...)` | **yes** | **yes** | - | - | - |

(\*) `StreamProjection` receives `Watch(VariableChanged)` only when the changed variable is bound to an active stream output.

#### Table 4: Event Lifecycle for a Typical LLM Function Call

This shows every event fired in order for a single `MyLlmFunction("hello")` call:

| # | RuntimeEvent | Emitted By | Description |
|---|---|---|---|
| 1 | `Function(Start)` | `bex_engine` | Function call begins |
| 2 | `Header(Enter)` [synthetic] | `bex_engine` | Implicit LLM function header |
| 3 | `Header(Enter)` | `bex_vm` via Viz | Explicit `# Step 1` header (if any) |
| 4 | `Llm(Request)` | `llm_ops` | Rendered prompt + params |
| 5 | `Llm(RawRequest)` | `sys_native` | Raw HTTP POST to provider |
| 6 | `Llm(RawResponseStream)` | `sys_native` | SSE chunk 1 (if streaming) |
| 7 | `Stream(Start)` | `bex_engine` | Stream context opened |
| 8 | `Watch(VariableChanged)` | `bex_vm` | Partial result assigned to watched var |
| 9 | `Stream(Update)` | `bex_engine` | Typed partial value |
| 10 | `Llm(RawResponseStream)` | `sys_native` | SSE chunk 2 |
| 11 | `Watch(VariableChanged)` | `bex_vm` | Updated partial result |
| 12 | `Stream(Update)` | `bex_engine` | Updated typed partial value |
| 13 | `Llm(RawResponse)` | `sys_native` | Final HTTP response |
| 14 | `Llm(Response)` | `llm_ops` | Parsed output + usage |
| 15 | `Stream(End)` | `bex_engine` | Final typed value |
| 16 | `Header(Exit)` | `bex_vm` via Viz | Explicit header closed |
| 17 | `Header(Exit)` [synthetic] | `bex_engine` | Implicit LLM header closed |
| 18 | `Function(End::Success)` | `bex_engine` | Function returns |

#### Table 5: Event Lifecycle for Chained Expression → LLM → Expression

```baml
function Pipeline(input: string) -> Output {
  let preprocessed = preprocess(input);    // expression fn
  let result = callLlm(preprocessed);      // LLM fn
  let final = postprocess(result);         // expression fn
  final
}
```

| # | RuntimeEvent | Source | Scope |
|---|---|---|---|
| 1 | `Function(Start{Pipeline})` | engine | Pipeline |
| 2 | — | — | `preprocess()` runs, no tracing (expression fn) |
| 3 | `Function(Start{callLlm})` | engine | callLlm |
| 4 | `Header(Enter)` [synthetic] | engine | callLlm |
| 5 | `Llm(Request)` | llm_ops | callLlm |
| 6 | `Llm(RawRequest)` | sys_native | callLlm |
| 7 | `Llm(RawResponse)` | sys_native | callLlm |
| 8 | `Llm(Response)` | llm_ops | callLlm |
| 9 | `Header(Exit)` [synthetic] | engine | callLlm |
| 10 | `Function(End{callLlm})` | engine | callLlm |
| 11 | — | — | `postprocess()` runs, no tracing (expression fn) |
| 12 | `Function(End{Pipeline})` | engine | Pipeline |

### Core Types

Create a new crate: `baml_events` (in `baml_language/crates/baml_events/`)

```rust
//! baml_events/src/lib.rs
//!
//! Unified event system for the BAML compiler and runtime.

use std::sync::Arc;

/// Unique identifier for a function call invocation.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct CallId(pub Arc<str>);

impl CallId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string().into())
    }
}

/// Unique identifier for a specific event within a call.
#[derive(Clone, Debug, Hash, Eq, PartialEq)]
pub struct EventId(pub Arc<str>);

impl EventId {
    pub fn new() -> Self {
        Self(uuid::Uuid::new_v4().to_string().into())
    }
}

/// All runtime events emitted by the BAML system.
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    /// Function execution events
    Function(FunctionEvent),
    /// LLM-specific events
    Llm(LlmEvent),
    /// Header context events for hierarchical execution tracking
    Header(HeaderEvent),
    /// Stream events for incremental updates
    Stream(StreamEvent),
    /// Custom tags/metadata
    Tags(TagsEvent),
}

#[derive(Clone, Debug)]
pub struct EventMeta {
    pub call_id: CallId,
    pub event_id: EventId,
    pub call_stack: Vec<CallId>,
    pub timestamp: std::time::SystemTime,
}

impl EventMeta {
    pub fn new(call_stack: Vec<CallId>) -> Self {
        Self {
            call_id: call_stack.last().cloned().unwrap_or_else(CallId::new),
            event_id: EventId::new(),
            call_stack,
            timestamp: std::time::SystemTime::now(),
        }
    }
}
```

### Function Events

```rust
//! baml_events/src/function.rs

use super::*;

#[derive(Clone, Debug)]
pub struct FunctionEvent {
    pub meta: EventMeta,
    pub data: FunctionEventData,
}

#[derive(Clone, Debug)]
pub enum FunctionEventData {
    Start(FunctionStart),
    End(FunctionEnd),
}

#[derive(Clone, Debug)]
pub struct FunctionStart {
    pub name: String,
    pub function_type: FunctionType,
    pub is_stream: bool,
    pub args: Vec<(String, BexExternalValue)>,
}

#[derive(Clone, Debug)]
pub enum FunctionEnd {
    Success { value: BexExternalValue },
    Error { message: String, error_type: String },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum FunctionType {
    BamlLlm,
    Native,
}
```

### Header Events

```rust
//! baml_events/src/header.rs

use super::*;

#[derive(Clone, Debug)]
pub struct HeaderEvent {
    pub meta: EventMeta,
    pub data: HeaderEventData,
}

#[derive(Clone, Debug)]
pub enum HeaderEventData {
    /// Entering a new header context (markdown-style hierarchy)
    Enter(HeaderEnter),
    /// Exiting a header context
    Exit(HeaderExit),
}

#[derive(Clone, Debug)]
pub struct HeaderEnter {
    /// Header nesting level (1 = top level, 2 = nested, etc.)
    pub level: u32,
    /// Header title/label
    pub title: String,
    /// Source location span
    pub span: Option<Span>,
    /// Unique node ID in the control flow graph
    pub node_id: String,
    /// Parent node ID (if any)
    pub parent_node_id: Option<String>,
}

#[derive(Clone, Debug)]
pub struct HeaderExit {
    /// Level being exited
    pub level: u32,
    /// Node ID being closed
    pub node_id: String,
}

#[derive(Clone, Debug)]
pub struct Span {
    pub file: String,
    pub start_line: u32,
    pub start_col: u32,
    pub end_line: u32,
    pub end_col: u32,
}
```

### Stream Events

```rust
//! baml_events/src/stream.rs

use super::*;

#[derive(Clone, Debug)]
pub struct StreamEvent {
    pub meta: EventMeta,
    pub data: StreamEventData,
}

#[derive(Clone, Debug)]
pub enum StreamEventData {
    /// Stream started
    Start {
        stream_id: String,
    },
    /// Incremental update during streaming
    Update {
        stream_id: String,
        /// Partial value
        value: BexExternalValue,
    },
    /// Stream completed
    End {
        stream_id: String,
        /// Final value
        final_value: Option<BexExternalValue>,
    },
}
```

### LLM Events

```rust
//! baml_events/src/llm.rs

use super::*;
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub struct LlmEvent {
    pub meta: EventMeta,
    pub data: LlmEventData,
}

#[derive(Clone, Debug)]
pub enum LlmEventData {
    /// Formatted request before sending
    Request(LlmRequest),
    /// Raw HTTP request
    RawRequest(HttpRequest),
    /// Raw HTTP response
    RawResponse(HttpResponse),
    /// Raw SSE stream chunk
    RawResponseStream(HttpResponseStream),
    /// Parsed LLM response
    Response(LlmResponse),
}

#[derive(Clone, Debug)]
pub struct LlmRequest {
    pub request_id: String,
    pub client_name: String,
    pub client_provider: String,
    pub params: Vec<(String, BexExternalValue)>,
    pub prompt: Vec<ChatMessage>,
}

#[derive(Clone, Debug)]
pub struct ChatMessage {
    pub role: String,
    pub content: Vec<ChatMessagePart>,
}

#[derive(Clone, Debug)]
pub enum ChatMessagePart {
    Text(String),
    Media { media_type: String, data: String },
}

#[derive(Clone, Debug)]
pub struct HttpRequest {
    pub id: String,
    pub url: String,
    pub method: String,
    pub headers: HashMap<String, String>,  // Redacted on serialization
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct HttpResponse {
    pub request_id: String,
    pub status: u16,
    pub headers: Option<HashMap<String, String>>,
    pub body: String,
}

#[derive(Clone, Debug)]
pub struct HttpResponseStream {
    pub request_id: String,
    pub event: SseEvent,
}

#[derive(Clone, Debug)]
pub struct SseEvent {
    pub timestamp_ms: i64,
    pub event: String,
    pub data: String,
    pub id: String,
}

#[derive(Clone, Debug)]
pub struct LlmResponse {
    pub request_id: String,
    pub client_stack: Vec<String>,
    pub model: Option<String>,
    pub finish_reason: Option<String>,
    pub usage: Option<LlmUsage>,
    pub raw_text_output: Option<String>,
    pub error_message: Option<String>,
}

#[derive(Clone, Debug)]
pub struct LlmUsage {
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub cached_input_tokens: Option<u64>,
}
```

### Event Sink Trait (Optional, for Fan-Out)

The channel receiver is the primary consumer. But when multiple subsystems
need events (publisher + collector + LSP), a fan-out task can read from the
channel and dispatch to `EventSink` implementations:

```rust
//! baml_events/src/sink.rs

use super::RuntimeEvent;

/// Trait for event consumers. Used by the optional fan-out task.
///
/// The primary event delivery mechanism is the mpsc channel.
/// EventSink is for secondary consumers that want to process events:
/// - `BoundaryPublisher`: Batches and uploads to Boundary API
/// - `LocalCollector`: Stores events for local retrieval
/// - `LspNotifier`: Sends notifications to IDE
pub trait EventSink: Send + Sync {
    /// Handle an incoming event.
    fn on_event(&self, event: RuntimeEvent);
    
    /// Flush any buffered events.
    fn flush(&self) -> impl std::future::Future<Output = ()> + Send;
    
    /// Shutdown the sink gracefully.
    fn shutdown(&self) -> impl std::future::Future<Output = ()> + Send;
}

/// A no-op sink that discards all events.
pub struct NoopSink;

impl EventSink for NoopSink {
    fn on_event(&self, _event: RuntimeEvent) {}
    
    async fn flush(&self) {}
    
    async fn shutdown(&self) {}
}
```

### Event Dispatcher

```rust
//! baml_events/src/dispatcher.rs

use std::sync::Arc;
use parking_lot::RwLock;
use super::{EventSink, RuntimeEvent};

/// Central event dispatcher that routes events to registered sinks.
pub struct EventDispatcher {
    sinks: RwLock<Vec<Arc<dyn EventSink>>>,
}

impl EventDispatcher {
    pub fn new() -> Self {
        Self {
            sinks: RwLock::new(Vec::new()),
        }
    }
    
    /// Register an event sink.
    pub fn register(&self, sink: Arc<dyn EventSink>) {
        self.sinks.write().push(sink);
    }
    
    /// Unregister all sinks.
    pub fn clear(&self) {
        self.sinks.write().clear();
    }
    
    /// Dispatch an event to all registered sinks.
    pub fn dispatch(&self, event: RuntimeEvent) {
        let sinks = self.sinks.read();
        for sink in sinks.iter() {
            sink.on_event(event.clone());
        }
    }
    
    /// Flush all sinks.
    pub async fn flush_all(&self) {
        let sinks = self.sinks.read().clone();
        for sink in sinks {
            sink.flush().await;
        }
    }
    
    /// Shutdown all sinks.
    pub async fn shutdown_all(&self) {
        let sinks = self.sinks.read().clone();
        for sink in sinks {
            sink.shutdown().await;
        }
    }
}

impl Default for EventDispatcher {
    fn default() -> Self {
        Self::new()
    }
}
```

### Streaming Architecture

Streaming in the new compiler must solve the same problems as `engine/`:
raw SSE tokens arrive faster than parsing can keep up, so we need buffering,
throttled parsing, deduplication, and coalescing before delivering partial
results to the host language.

#### How `engine/` does it today

The existing streaming pipeline has three concurrent layers:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                        engine/ Streaming Pipeline                           │
│                                                                             │
│  Layer 1: SSE Consumer (sse_future)                                         │
│  ───────────────────────────────────                                        │
│  • Reads raw SSE chunks from HTTP stream                                    │
│  • Accumulates text into LLMCompleteResponse.content                        │
│  • Publishes latest snapshot via tokio::watch channel                       │
│  • Handles timeouts (time_to_first_token, idle)                             │
│                                                                             │
│                    tokio::watch::channel                                     │
│                    ┌────────────────────┐                                   │
│                    │  Option<Arc<LLM    │                                   │
│                    │  CompleteResponse>> │                                   │
│                    └─────────┬──────────┘                                   │
│                              │                                              │
│  Layer 2: Parser Loop (run_parser_loop)                                     │
│  ──────────────────────────────────────                                     │
│  • Wakes every 50ms OR when snapshot changes                                │
│  • Parses latest snapshot with partial_parse_fn (allow_partials: true)      │
│  • Serializes result, compares to last-sent (deduplication)                 │
│  • Calls on_event(FunctionResult) only if value actually changed            │
│                                                                             │
│                    queue.Queue (Python) / channel (TS)                       │
│                    ┌────────────────────┐                                   │
│                    │  FunctionResult    │                                   │
│                    │  (partial)         │                                   │
│                    └─────────┬──────────┘                                   │
│                              │                                              │
│  Layer 3: Host Language Consumer                                            │
│  ────────────────────────────────                                           │
│  • Python: BamlStream.__aiter__ drains queue                                │
│  • Coalesces: keeps latest successful event, drops stale partials           │
│  • Yields typed partial to user's `async for` loop                          │
│  • Final: on_event(None) signals completion, get_final_response() returns   │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

Key types from `engine/`:

```rust
// engine/baml-runtime/src/internal/llm_client/orchestrator/stream.rs

/// Deduplication state between SSE consumer and parser
struct ParserState {
    last_sent_partial_serialized: Option<String>,   // string-compare dedup
    last_processed_snapshot_ptr: Option<usize>,      // pointer-compare skip
}

/// 50ms throttled parsing loop
async fn run_parser_loop(
    scope: OrchestrationScope,
    parse_state: Arc<Mutex<ParserState>>,
    partial_parse_fn: &ParseFn,     // parse with allow_partials: true
    on_event: &EventFn,             // callback to host language
    snapshot_rx: watch::Receiver<Option<Arc<LLMCompleteResponse>>>,
);
```

And the Python consumer:

```python
# engine/language_client_python/python_src/baml_py/stream.py

class BamlStream:
    def __init__(self, ffi_stream, partial_coerce, final_coerce, ctx_manager):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)  # register callback
        self.__event_queue = queue.Queue()                        # buffer

    async def __aiter__(self):
        self.__drive_to_completion_in_bg()  # start Rust execution on a thread
        while True:
            event = self.__event_queue.get_nowait()  # non-blocking poll
            if event is None: break

            # Coalesce: drain queue, keep only the latest successful partial
            latest_ok = event if event.is_ok() else None
            while True:
                nxt = self.__event_queue.get_nowait()  # drain
                if nxt is None: break
                if nxt.is_ok(): latest_ok = nxt        # keep latest

            if latest_ok is not None:
                yield self.__partial_coerce(latest_ok)  # deliver to user
```

#### Proposed streaming architecture for `baml_language`

In the new compiler, streaming uses the same `VmExecState::Event(RuntimeEvent)`
yield mechanism as everything else. But we need the same buffering/dedup/coalesce
layers. The key difference: in `engine/`, the SSE consumer and parser run as
separate tokio tasks joined by a `watch::channel`. In `baml_language`, the VM
is the central executor and it already yields on events.

```
┌─────────────────────────────────────────────────────────────────────────────┐
│               baml_language Streaming Pipeline (Proposed)                    │
│                                                                             │
│  Layer 1: SSE Consumer (sys_native HTTP op)                                 │
│  ──────────────────────────────────────────                                 │
│  • HTTP streaming happens as an async sys_op                                │
│  • Each SSE chunk fulfils a future, waking the VM                           │
│  • VM accumulates raw text into a buffer                                    │
│  • VM yields: Event(Llm(RawResponseStream)) for each chunk                 │
│                                                                             │
│  Layer 2: Throttled Parsing (in VM or engine)                               │
│  ────────────────────────────────────────────                               │
│  • After accumulating chunks, periodically parse with partial mode          │
│  • Compare serialized partial to last-sent (dedup)                          │
│  • If changed → VM yields: Event(Stream(Update { value }))                  │
│  • If unchanged → skip, continue accumulating                               │
│                                                                             │
│  Layer 3: Engine Dispatch                                                   │
│  ────────────────────────                                                   │
│  • Engine receives VmExecState::Event(Stream(Update))                       │
│  • Dispatches to all sinks                                                  │
│  • Calls host-language on_event callback                                    │
│  • Resumes VM                                                               │
│                                                                             │
│  Layer 4: Host Language Consumer                                            │
│  ────────────────────────────────                                           │
│  • Same BamlStream pattern: queue + coalesce + yield                        │
│  • Or: direct async iterator fed by on_event callback                       │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### Streaming state machine in the VM

The VM needs internal state to manage the parse-throttle-dedup cycle. This
is analogous to `engine/`'s `ParserState` but lives inside the VM:

```rust
//! bex_vm/src/stream_state.rs (new)

/// Per-active-stream state, held by the VM during an LLM streaming call.
pub struct StreamState {
    /// Unique stream identifier
    pub stream_id: String,
    /// Accumulated raw text from SSE chunks
    pub accumulated_text: String,
    /// Last serialized partial that was emitted (for dedup)
    pub last_emitted_serialized: Option<String>,
    /// Timestamp of last parse attempt (for throttle)
    pub last_parse_time: std::time::Instant,
    /// Minimum interval between parse attempts
    pub parse_interval: std::time::Duration,  // default: 50ms
    /// Whether the first token has arrived
    pub first_token_received: bool,
    /// Whether Stream(Start) has been emitted
    pub started: bool,
}

impl StreamState {
    pub fn new(stream_id: String) -> Self {
        Self {
            stream_id,
            accumulated_text: String::new(),
            last_emitted_serialized: None,
            last_parse_time: std::time::Instant::now(),
            parse_interval: std::time::Duration::from_millis(50),
            first_token_received: false,
            started: false,
        }
    }

    /// Append raw SSE chunk text.
    pub fn push_chunk(&mut self, text: &str) {
        self.accumulated_text.push_str(text);
        if !self.first_token_received {
            self.first_token_received = true;
        }
    }

    /// Should we attempt a parse right now?
    pub fn should_parse(&self) -> bool {
        self.last_parse_time.elapsed() >= self.parse_interval
    }

    /// After parsing, check if the result differs from last emission.
    /// Returns true if we should emit a Stream(Update).
    pub fn should_emit(&mut self, serialized: &str) -> bool {
        let dominated = self.last_emitted_serialized.as_deref() == Some(serialized);
        if !dominated {
            self.last_emitted_serialized = Some(serialized.to_string());
            self.last_parse_time = std::time::Instant::now();
            true
        } else {
            self.last_parse_time = std::time::Instant::now();
            false
        }
    }
}
```

#### Chunk arrival → event emission flow

```rust
//! bex_vm/src/vm.rs — streaming execution pseudocode

impl BexVm {
    /// Called when an SSE chunk arrives (async future fulfilled).
    fn handle_stream_chunk(
        &mut self,
        stream: &mut StreamState,
        chunk_text: &str,
        parse_partial: impl Fn(&str) -> Result<serde_json::Value>,
    ) -> Result<VmExecState> {
        // 1. Accumulate raw text
        stream.push_chunk(chunk_text);

        // 2. Emit raw chunk event (always, for tracing)
        //    The engine dispatches this but host language typically ignores it.
        //    BestEffort priority — can be dropped under backpressure.

        // 3. Check parse throttle
        if !stream.should_parse() {
            // Too soon since last parse — continue VM execution,
            // wait for next chunk.
            return Ok(VmExecState::Continue);
        }

        // 4. Attempt partial parse
        let parsed = match parse_partial(&stream.accumulated_text) {
            Ok(value) => value,
            Err(_) => {
                // Parse failed (incomplete JSON etc.) — continue accumulating.
                return Ok(VmExecState::Continue);
            }
        };

        // 5. Serialize and dedup
        let serialized = serde_json::to_string(&parsed)
            .unwrap_or_default();

        if !stream.should_emit(&serialized) {
            // Same as last emission — skip.
            return Ok(VmExecState::Continue);
        }

        // 6. Emit Stream(Start) on first successful parse
        if !stream.started {
            stream.started = true;
            // Yield start event first — engine dispatches + resumes VM
            return Ok(VmExecState::Event(RuntimeEvent::Stream(StreamEvent {
                meta: EventMeta::from_vm(self),
                data: StreamEventData::Start {
                    stream_id: stream.stream_id.clone(),
                },
            })));
            // Note: after resume, we re-enter and fall through to emit Update.
        }

        // 7. Yield the typed partial
        Ok(VmExecState::Event(RuntimeEvent::Stream(StreamEvent {
            meta: EventMeta::from_vm(self),
            data: StreamEventData::Update {
                stream_id: stream.stream_id.clone(),
                value: parsed,
            },
        })))
    }

    /// Called when the HTTP stream ends (final future fulfilled).
    fn handle_stream_end(
        &mut self,
        stream: &mut StreamState,
        parse_final: impl Fn(&str) -> Result<serde_json::Value>,
    ) -> Result<VmExecState> {
        // Final parse (non-partial mode)
        let final_value = parse_final(&stream.accumulated_text).ok();

        Ok(VmExecState::Event(RuntimeEvent::Stream(StreamEvent {
            meta: EventMeta::from_vm(self),
            data: StreamEventData::End {
                stream_id: stream.stream_id.clone(),
                final_value,
            },
        })))
    }
}
```

#### Engine dispatch during streaming

The engine loop handles stream events identically to any other event —
the only special behavior is that the host callback knows how to route
`Stream(Update)` into the user's iterator:

```rust
//! bex_engine/src/lib.rs — streaming dispatch (same as all events)

// This is the SAME match arm as everything else. No special stream logic.
VmExecState::Event(event) => {
    // Fan out to sinks (publisher, collector, etc.)
    if let Some(dispatcher) = &self.event_dispatcher {
        dispatcher.dispatch(event.clone());
    }

    // Host callback — for streaming, this enqueues into BamlStream's queue
    if let Some(on_event) = &self.on_event_callback {
        on_event(&event);
    }

    // VM resumes on next loop iteration
}
```

#### Host language consumer (Python, proposed)

The Python `BamlStream` stays structurally the same, but consumes
`RuntimeEvent` instead of `FunctionResult`:

```python
# Proposed: baml_language Python stream consumer

class BamlStream:
    def __init__(self, engine_handle, partial_coerce, final_coerce):
        self.__event_queue = queue.Queue()
        self.__engine_handle = engine_handle
        # Register callback: engine calls this on every RuntimeEvent
        engine_handle.on_event(self.__enqueue)

    def __enqueue(self, event: RuntimeEvent) -> None:
        # Only buffer stream-relevant events
        if event.is_stream_update() or event.is_stream_end():
            self.__event_queue.put_nowait(event)

    async def __aiter__(self):
        # Drive the Rust engine to completion in background
        self.__drive_to_completion_in_bg()
        while True:
            try:
                event = self.__event_queue.get_nowait()
            except queue.Empty:
                await asyncio.sleep(0.010)  # 10ms poll
                continue

            if event.is_stream_end():
                break

            # Coalesce: drain queue, keep only latest Update
            latest = event
            while True:
                try:
                    nxt = self.__event_queue.get_nowait()
                    if nxt.is_stream_end():
                        # Put it back — we'll see it next iteration
                        self.__event_queue.put_nowait(nxt)
                        break
                    latest = nxt  # keep latest
                except queue.Empty:
                    break

            yield self.__partial_coerce(latest.value)

    async def get_final_response(self):
        result = await self.__engine_handle.done()
        return self.__final_coerce(result)
```

#### Retry behavior during streaming

When a stream fails mid-way (e.g., connection drop), the engine retries
with the next orchestrator node. This creates a subtlety:

```
Attempt 1:  Stream(Start) → Update(A) → Update(B) → [connection error]
Attempt 2:  Stream(Start) → Update(A') → Update(B') → Update(C') → Stream(End)
```

The user's iterator sees partials from **both** attempts. The stream appears
to "reset" after the failure. This matches `engine/` behavior (see the TODO
comment at line 483 of `stream.rs`).

Options for the new compiler:
1. **Same as engine/** (recommended for now): Emit partials eagerly, accept
   the reset on retry. `get_final_response()` returns the correct final value.
2. **Buffer until success**: Hold all partials until the stream completes
   successfully. Better UX but adds latency — the user sees nothing until
   the first chunk of a successful attempt.
3. **Reset event**: Emit a `Stream(Reset { stream_id })` event when retrying,
   so the host language can clear its state.

#### Comparison: `engine/` vs `baml_language` streaming

| Aspect | `engine/` (current) | `baml_language` (proposed) |
|---|---|---|
| SSE → parse decoupling | `tokio::watch` channel | VM accumulates + throttled parse |
| Parse throttle | 50ms `tokio::interval` in parser task | `StreamState.should_parse()` in VM |
| Deduplication | String comparison in `ParserState` | String comparison in `StreamState` |
| Event delivery | `on_event(FunctionResult)` callback | `VmExecState::Event(Stream(Update))` yield |
| Host buffering | `queue.Queue` + coalesce in Python | Same pattern |
| Retry mid-stream | Partials from both attempts visible | Same (option to add Reset event) |
| Concurrency model | Two tokio tasks joined by `watch` | Single VM yield loop |

### Integration with VM (`bex_vm`)

The VM should accept an optional event dispatcher:

```rust
//! bex_vm/src/vm.rs (modifications)

use baml_events::{EventDispatcher, RuntimeEvent, HeaderEvent, HeaderEventData};

pub struct VmConfig {
    pub event_dispatcher: Option<Arc<EventDispatcher>>,
    // ... other config
}

impl Vm {
    /// Called when entering a header context during execution.
    fn enter_header(&mut self, header: &HeaderContext) {
        if let Some(dispatcher) = &self.config.event_dispatcher {
            let event = RuntimeEvent::Header(HeaderEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: HeaderEventData::Enter(HeaderEnter {
                    level: header.level,
                    title: header.title.clone(),
                    span: header.span.clone().map(Into::into),
                    node_id: self.allocate_node_id(),
                    parent_node_id: self.current_parent_id(),
                }),
            });
            dispatcher.dispatch(event);
        }
        
        // ... existing header handling logic
    }
    
    /// Called when exiting a header context.
    fn exit_header(&mut self, level: u32, node_id: String) {
        if let Some(dispatcher) = &self.config.event_dispatcher {
            let event = RuntimeEvent::Header(HeaderEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: HeaderEventData::Exit(HeaderExit { level, node_id }),
            });
            dispatcher.dispatch(event);
        }
    }
}
```

### Integration with LLM Clients (`llm_ops`)

```rust
//! llm_ops/src/lib.rs (modifications)

use baml_events::{EventDispatcher, RuntimeEvent, LlmEvent, LlmEventData};

pub struct LlmClientConfig {
    pub event_dispatcher: Option<Arc<EventDispatcher>>,
    // ... other config
}

impl LlmClient {
    async fn send_request(&self, request: &LlmRequest) -> Result<LlmResponse> {
        // Emit request event
        if let Some(dispatcher) = &self.config.event_dispatcher {
            dispatcher.dispatch(RuntimeEvent::Llm(LlmEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: LlmEventData::Request(request.clone()),
            }));
        }
        
        // ... send HTTP request
        
        // Emit response event
        if let Some(dispatcher) = &self.config.event_dispatcher {
            dispatcher.dispatch(RuntimeEvent::Llm(LlmEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: LlmEventData::Response(response.clone()),
            }));
        }
        
        Ok(response)
    }
}
```

### Boundary Publisher Sink

```rust
//! baml_events/src/sinks/boundary_publisher.rs

use std::sync::Arc;
use tokio::sync::mpsc;
use super::{EventSink, RuntimeEvent};

pub struct BoundaryPublisher {
    tx: mpsc::Sender<PublisherMessage>,
}

enum PublisherMessage {
    Event(RuntimeEvent),
    Flush(tokio::sync::oneshot::Sender<()>),
    Shutdown(tokio::sync::oneshot::Sender<()>),
}

impl BoundaryPublisher {
    pub fn new(config: BoundaryConfig) -> Self {
        let (tx, rx) = mpsc::channel(config.queue_capacity);
        
        // Spawn background task
        tokio::spawn(async move {
            let mut worker = PublisherWorker::new(rx, config);
            worker.run().await;
        });
        
        Self { tx }
    }
}

impl EventSink for BoundaryPublisher {
    fn on_event(&self, event: RuntimeEvent) {
        let _ = self.tx.try_send(PublisherMessage::Event(event));
    }
    
    async fn flush(&self) {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(PublisherMessage::Flush(ack_tx)).await;
        let _ = ack_rx.await;
    }
    
    async fn shutdown(&self) {
        let (ack_tx, ack_rx) = tokio::sync::oneshot::channel();
        let _ = self.tx.send(PublisherMessage::Shutdown(ack_tx)).await;
        let _ = ack_rx.await;
    }
}

struct PublisherWorker {
    rx: mpsc::Receiver<PublisherMessage>,
    config: BoundaryConfig,
    buffer: Vec<RuntimeEvent>,
}

impl PublisherWorker {
    async fn run(&mut self) {
        let mut interval = tokio::time::interval(Duration::from_secs(2));
        
        loop {
            tokio::select! {
                Some(msg) = self.rx.recv() => {
                    match msg {
                        PublisherMessage::Event(event) => {
                            self.buffer.push(event);
                            if self.buffer.len() >= self.config.batch_size {
                                self.flush_buffer().await;
                            }
                        }
                        PublisherMessage::Flush(ack) => {
                            self.flush_buffer().await;
                            let _ = ack.send(());
                        }
                        PublisherMessage::Shutdown(ack) => {
                            self.flush_buffer().await;
                            let _ = ack.send(());
                            break;
                        }
                    }
                }
                _ = interval.tick() => {
                    if !self.buffer.is_empty() {
                        self.flush_buffer().await;
                    }
                }
            }
        }
    }
    
    async fn flush_buffer(&mut self) {
        if self.buffer.is_empty() {
            return;
        }
        
        let batch = std::mem::take(&mut self.buffer);
        // ... serialize and upload to Boundary API
    }
}

pub struct BoundaryConfig {
    pub api_url: String,
    pub api_key: String,
    pub batch_size: usize,
    pub queue_capacity: usize,
}
```

### Local Collector Sink

```rust
//! baml_events/src/sinks/local_collector.rs

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use super::{CallId, EventSink, RuntimeEvent};

/// Collects events locally for retrieval.
pub struct LocalCollector {
    events: Arc<Mutex<HashMap<CallId, Vec<RuntimeEvent>>>>,
    ref_counts: Arc<Mutex<HashMap<CallId, usize>>>,
}

impl LocalCollector {
    pub fn new() -> Self {
        Self {
            events: Arc::new(Mutex::new(HashMap::new())),
            ref_counts: Arc::new(Mutex::new(HashMap::new())),
        }
    }
    
    /// Start tracking events for a call.
    pub fn track(&self, call_id: CallId) {
        let mut ref_counts = self.ref_counts.lock();
        *ref_counts.entry(call_id.clone()).or_insert(0) += 1;
        
        let mut events = self.events.lock();
        events.entry(call_id).or_insert_with(Vec::new);
    }
    
    /// Stop tracking events for a call.
    pub fn untrack(&self, call_id: &CallId) {
        let mut ref_counts = self.ref_counts.lock();
        if let Some(count) = ref_counts.get_mut(call_id) {
            *count = count.saturating_sub(1);
            if *count == 0 {
                ref_counts.remove(call_id);
                self.events.lock().remove(call_id);
            }
        }
    }
    
    /// Get all events for a call.
    pub fn get_events(&self, call_id: &CallId) -> Vec<RuntimeEvent> {
        self.events.lock().get(call_id).cloned().unwrap_or_default()
    }
}

impl EventSink for LocalCollector {
    fn on_event(&self, event: RuntimeEvent) {
        let call_id = event.call_id();
        let ref_counts = self.ref_counts.lock();
        
        if ref_counts.get(&call_id).map(|&c| c > 0).unwrap_or(false) {
            drop(ref_counts);
            self.events.lock()
                .entry(call_id)
                .or_insert_with(Vec::new)
                .push(event);
        }
    }
    
    async fn flush(&self) {}
    
    async fn shutdown(&self) {}
}
```

## Automatic LLM Function Instrumentation

In `baml_language`, LLM functions and expression functions can be chained together in arbitrary ways. Every LLM function should automatically have tracing enabled without requiring explicit opt-in from the user.

### Function Types in the New Compiler

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Function Call Chain                                  │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   user code                                                                  │
│       │                                                                      │
│       ▼                                                                      │
│   ┌─────────────────┐     ┌─────────────────┐     ┌─────────────────┐       │
│   │  Expression Fn  │────▶│    LLM Fn       │────▶│  Expression Fn  │       │
│   │  (no tracing)   │     │  (auto-traced)  │     │  (no tracing)   │       │
│   └─────────────────┘     └─────────────────┘     └─────────────────┘       │
│                                   │                                          │
│                                   ▼                                          │
│                           ┌─────────────────┐                               │
│                           │  Nested LLM Fn  │                               │
│                           │  (auto-traced)  │                               │
│                           └─────────────────┘                               │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Bytecode Instrumentation Strategy

The compiler should automatically inject tracing instructions around LLM function calls. This can be done at the bytecode emission phase (`baml_compiler_emit`).

#### Option A: Bytecode Wrapping

Wrap LLM function calls with explicit trace instructions in the emitted bytecode:

```rust
//! baml_compiler_emit/src/llm_instrumentation.rs

/// Bytecode instructions for tracing
pub enum TraceInstruction {
    /// Push a new call frame onto the trace stack
    TraceFunctionEnter {
        function_name: String,
        function_type: FunctionType,
        is_llm: bool,
    },
    /// Pop the current call frame and emit end event
    TraceFunctionExit,
    /// Emit a header context enter event
    TraceHeaderEnter {
        level: u32,
        title: String,
    },
    /// Emit a header context exit event  
    TraceHeaderExit {
        level: u32,
    },
}

/// When emitting bytecode for an LLM function call, wrap it:
fn emit_llm_function_call(
    &mut self,
    func: &LlmFunction,
    args: Vec<Value>,
) -> Vec<Instruction> {
    let mut instructions = Vec::new();
    
    // 1. Emit function enter trace
    instructions.push(Instruction::Trace(TraceInstruction::TraceFunctionEnter {
        function_name: func.name.clone(),
        function_type: FunctionType::BamlLlm,
        is_llm: true,
    }));
    
    // 2. Emit implicit header enter (LLM functions have implicit headers)
    instructions.push(Instruction::Trace(TraceInstruction::TraceHeaderEnter {
        level: 1,
        title: func.name.clone(),
    }));
    
    // 3. Push arguments
    for arg in args {
        instructions.push(Instruction::Push(arg));
    }
    
    // 4. Call the LLM function
    instructions.push(Instruction::CallLlm {
        function_id: func.id,
        arg_count: args.len(),
    });
    
    // 5. Emit header exit
    instructions.push(Instruction::Trace(TraceInstruction::TraceHeaderExit {
        level: 1,
    }));
    
    // 6. Emit function exit trace (handles both success and error)
    instructions.push(Instruction::Trace(TraceInstruction::TraceFunctionExit));
    
    instructions
}
```

#### Option B: VM-Level Instrumentation

Alternatively, the VM itself can detect LLM function calls and automatically emit events:

```rust
//! bex_vm/src/vm.rs

impl Vm {
    fn execute_instruction(&mut self, instruction: &Instruction) -> Result<()> {
        match instruction {
            Instruction::CallLlm { function_id, arg_count } => {
                let func_meta = self.get_function_metadata(*function_id);
                
                // Automatic tracing for LLM functions
                self.emit_function_enter(&func_meta);
                self.emit_header_enter(1, &func_meta.name);
                
                // Execute the LLM call
                let result = self.execute_llm_call(*function_id, *arg_count);
                
                // Emit exit events (even on error)
                self.emit_header_exit(1);
                self.emit_function_exit(&func_meta, &result);
                
                result
            }
            Instruction::Call { function_id, arg_count } => {
                // Expression functions - no automatic tracing
                self.execute_call(*function_id, *arg_count)
            }
            // ... other instructions
        }
    }
}
```

### Recommended Approach: Hybrid

Use **Option A (bytecode wrapping)** for flexibility, but make it transparent:

1. **Compiler marks LLM functions** — During HIR → MIR lowering, mark functions that require tracing
2. **MIR contains trace markers** — The MIR representation includes trace enter/exit points
3. **Bytecode emission injects instructions** — When emitting bytecode, inject trace instructions
4. **VM executes trace instructions** — The VM has dedicated opcodes for trace events

```rust
//! baml_compiler_mir/src/lower.rs

/// MIR representation includes trace markers for LLM functions
pub enum MirStatement {
    // ... existing statements
    
    /// Marker: entering a traced context
    TraceEnter(TraceContext),
    /// Marker: exiting a traced context
    TraceExit,
}

#[derive(Clone, Debug)]
pub struct TraceContext {
    pub kind: TraceContextKind,
    pub name: String,
    pub span: Span,
}

#[derive(Clone, Debug)]
pub enum TraceContextKind {
    /// LLM function - always traced, has implicit header
    LlmFunction,
    /// Explicit header in code (markdown-style)
    Header { level: u32 },
    /// Branch/match arm
    BranchArm { arm_name: String },
    /// Loop iteration
    Loop,
}
```

### LLM Function Events

Every LLM function automatically emits these events:

```rust
//! baml_events/src/llm_function.rs

/// Events specific to LLM function execution
#[derive(Clone, Debug)]
pub enum LlmFunctionEvent {
    /// Function execution started
    Enter(LlmFunctionEnter),
    /// Prompt rendered (before LLM call)
    PromptRendered(PromptRendered),
    /// LLM request sent
    RequestSent(LlmRequest),
    /// LLM response received (or streaming started)
    ResponseReceived(LlmResponse),
    /// Streaming update (if streaming)
    StreamUpdate(StreamUpdate),
    /// Output parsed successfully
    OutputParsed(OutputParsed),
    /// Function execution completed
    Exit(LlmFunctionExit),
}

#[derive(Clone, Debug)]
pub struct LlmFunctionEnter {
    pub function_name: String,
    pub args: Vec<(String, serde_json::Value)>,
    pub is_stream: bool,
    /// Implicit header for this LLM function
    pub header: HeaderEnter,
}

#[derive(Clone, Debug)]
pub struct PromptRendered {
    pub template_name: Option<String>,
    pub rendered_prompt: Vec<ChatMessage>,
}

#[derive(Clone, Debug)]
pub struct OutputParsed {
    pub raw_output: String,
    pub parsed_value: serde_json::Value,
    pub type_name: String,
}

#[derive(Clone, Debug)]
pub struct LlmFunctionExit {
    pub duration_ms: u64,
    pub result: LlmFunctionResult,
    /// Implicit header exit
    pub header_exit: HeaderExit,
}

#[derive(Clone, Debug)]
pub enum LlmFunctionResult {
    Success {
        value: serde_json::Value,
        usage: Option<LlmUsage>,
    },
    Error {
        error_type: String,
        message: String,
        /// Was this a retryable error that was retried?
        retried: bool,
    },
}
```

### Nested LLM Calls and Call Stack

When LLM functions call other LLM functions (directly or through expression functions), the call stack must be properly maintained:

```rust
//! bex_vm/src/trace_context.rs

/// Maintains trace context during VM execution
pub struct TraceContext {
    /// Stack of active call IDs
    call_stack: Vec<CallId>,
    /// Stack of active headers (for proper nesting)
    header_stack: Vec<ActiveHeader>,
    /// Event dispatcher reference
    dispatcher: Option<Arc<EventDispatcher>>,
}

#[derive(Clone, Debug)]
struct ActiveHeader {
    level: u32,
    node_id: String,
    function_name: Option<String>,  // Set if this is an implicit LLM function header
}

impl TraceContext {
    /// Enter an LLM function - creates both a call frame and implicit header
    pub fn enter_llm_function(&mut self, function_name: &str, args: &[(String, Value)]) {
        // 1. Create new call ID and push onto stack
        let call_id = CallId::new();
        self.call_stack.push(call_id.clone());
        
        // 2. Create implicit header for this LLM function
        let header_node_id = self.allocate_node_id();
        let header_level = self.next_header_level();
        
        self.header_stack.push(ActiveHeader {
            level: header_level,
            node_id: header_node_id.clone(),
            function_name: Some(function_name.to_string()),
        });
        
        // 3. Emit events
        if let Some(dispatcher) = &self.dispatcher {
            // Function enter event
            dispatcher.dispatch(RuntimeEvent::Function(FunctionEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: FunctionEventData::Start(FunctionStart {
                    name: function_name.to_string(),
                    function_type: FunctionType::BamlLlm,
                    is_stream: false, // Set properly based on call context
                    args: args.iter().map(|(k, v)| (k.clone(), v.to_json())).collect(),
                }),
            }));
            
            // Header enter event (implicit for LLM function)
            dispatcher.dispatch(RuntimeEvent::Header(HeaderEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: HeaderEventData::Enter(HeaderEnter {
                    level: header_level,
                    title: function_name.to_string(),
                    span: None, // Could include source span
                    node_id: header_node_id,
                    parent_node_id: self.current_parent_header_id(),
                }),
            }));
        }
    }
    
    /// Exit an LLM function - closes header and call frame
    pub fn exit_llm_function(&mut self, result: &Result<Value, Error>) {
        // Pop header
        if let Some(header) = self.header_stack.pop() {
            if let Some(dispatcher) = &self.dispatcher {
                dispatcher.dispatch(RuntimeEvent::Header(HeaderEvent {
                    meta: EventMeta::new(self.call_stack.clone()),
                    data: HeaderEventData::Exit(HeaderExit {
                        level: header.level,
                        node_id: header.node_id,
                    }),
                }));
            }
        }
        
        // Emit function end event
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.dispatch(RuntimeEvent::Function(FunctionEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: FunctionEventData::End(match result {
                    Ok(value) => FunctionEnd::Success { value: value.to_json() },
                    Err(e) => FunctionEnd::Error {
                        message: e.to_string(),
                        error_type: e.type_name().to_string(),
                    },
                }),
            }));
        }
        
        // Pop call ID
        self.call_stack.pop();
    }
    
    /// Enter an explicit header (from BAML code markdown headers)
    pub fn enter_explicit_header(&mut self, level: u32, title: &str) {
        // Pop any headers at same or deeper level (markdown semantics)
        while self.header_stack.last().map(|h| h.level >= level).unwrap_or(false) {
            self.exit_header_implicit();
        }
        
        let node_id = self.allocate_node_id();
        self.header_stack.push(ActiveHeader {
            level,
            node_id: node_id.clone(),
            function_name: None,
        });
        
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.dispatch(RuntimeEvent::Header(HeaderEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: HeaderEventData::Enter(HeaderEnter {
                    level,
                    title: title.to_string(),
                    span: None,
                    node_id,
                    parent_node_id: self.current_parent_header_id(),
                }),
            }));
        }
    }
}
```

### Bytecode Opcodes

New trace-related opcodes for the VM:

```rust
//! bex_vm_types/src/bytecode.rs

#[derive(Clone, Debug)]
pub enum Opcode {
    // ... existing opcodes
    
    /// Enter an LLM function context (creates call frame + implicit header)
    /// Stack: [..., arg_n, ..., arg_1] -> [...]
    LlmFunctionEnter {
        function_id: FunctionId,
        arg_count: u16,
        is_stream: bool,
    },
    
    /// Exit an LLM function context (closes header + call frame)
    /// Stack: [..., result] -> [..., result]
    LlmFunctionExit,
    
    /// Enter an explicit header context
    /// Stack: [...] -> [...]
    HeaderEnter {
        level: u8,
        title_constant_idx: u16,  // Index into constant pool
    },
    
    /// Exit header context(s) up to the specified level
    /// Stack: [...] -> [...]  
    HeaderExit {
        level: u8,
    },
    
    /// Mark the start of streaming
    StreamStart {
        stream_id_register: u8,
    },
    
    /// Emit a stream update
    StreamUpdate {
        stream_id_register: u8,
    },
    
    /// Mark the end of streaming
    StreamEnd {
        stream_id_register: u8,
    },
}
```

### Compiler Pipeline Integration

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                         Compilation Pipeline                                 │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   Source (BAML)                                                              │
│       │                                                                      │
│       ▼                                                                      │
│   ┌─────────────────┐                                                       │
│   │   HIR           │  Function marked with `is_llm: true`                  │
│   └────────┬────────┘                                                       │
│            │                                                                 │
│            ▼                                                                 │
│   ┌─────────────────┐                                                       │
│   │   TIR           │  Type information, call graph analysis                │
│   └────────┬────────┘                                                       │
│            │                                                                 │
│            ▼                                                                 │
│   ┌─────────────────┐                                                       │
│   │   MIR           │  TraceEnter/TraceExit markers inserted for LLM funcs  │
│   └────────┬────────┘                                                       │
│            │                                                                 │
│            ▼                                                                 │
│   ┌─────────────────┐                                                       │
│   │   Bytecode      │  LlmFunctionEnter/Exit opcodes emitted                │
│   └────────┬────────┘                                                       │
│            │                                                                 │
│            ▼                                                                 │
│   ┌─────────────────┐                                                       │
│   │   VM Execution  │  Events dispatched when trace opcodes execute         │
│   └─────────────────┘                                                       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

### Tracing Configuration

Users can configure tracing behavior at the function or project level:

```baml
// Project-level configuration (baml.toml or generator block)
generator default {
  tracing {
    enabled = true           // Master switch
    llm_functions = true     // Trace all LLM functions (default: true)
    expression_functions = false  // Trace expression functions (default: false)
    include_args = true      // Include function arguments in events
    include_results = true   // Include function results in events
    redact_pii = false       // Redact PII from traces
  }
}

// Per-function override
function MyLlmFunction(input: string) -> Output {
  @trace(enabled = false)  // Disable tracing for this function
  client "openai/gpt-4"
  prompt #"..."#
}
```

### Summary

| Function Type | Auto-Traced | Has Implicit Header | Events Emitted |
|--------------|-------------|---------------------|----------------|
| LLM Function | Yes | Yes | Enter, PromptRendered, Request, Response, [StreamUpdates], Exit |
| Expression Function | No (opt-in) | No | (None by default) |
| Nested LLM Call | Yes | Yes (nested level) | Full event tree with proper call stack |

## Watch Variable Notifications

The `watch let` syntax allows variables to emit events whenever their value changes. This is a reactive programming pattern built into BAML for real-time UI updates, debugging, and streaming partial results.

### Syntax

```baml
function MyFunction() -> Output {
  // Declare a watched variable
  watch let result = initialValue;
  
  // Configure watch behavior (optional)
  result.$watch.options(baml.WatchOptions { 
    when: filterFunction,  // Custom filter: only notify when this returns true
    // or: when: "manual"  // Disable automatic notifications
    // or: when: "never"   // Pause all notifications
  });
  
  // This assignment triggers a WatchNotification
  result = newValue;
  
  result
}
```

### Existing VM Implementation

The VM already has a sophisticated watch system in `bex_vm/src/watch.rs`:

```rust
/// State associated with a watched root.
pub struct RootState {
    /// Current value.
    pub value: Value,
    /// Last assigned value.
    pub last_assigned: Option<Value>,
    /// Last notified value.
    pub last_notified: Option<Value>,
    /// Channel name.
    pub channel: String,
    /// Pointer to filter function.
    pub filter: WatchFilter,
}

pub enum WatchFilter {
    Default,           // Notify on any value change (deep equals comparison)
    Manual,            // Skip automatic notifications
    Paused,            // Notifications disabled
    Function(HeapPtr), // Custom filter function that returns bool
}

/// Notification types from the VM
pub enum WatchNotification {
    /// Watched variables changed
    Variables(Vec<NodeId>),
    /// Block enter/exit notification
    Block(BlockNotification),
    /// Visualization event (header enter/exit, etc.)
    Viz { function_name: String, event: VizExecEvent },
}
```

### Watch Dependency Graph

The watch system maintains a reachability graph to track which objects can affect watched variables:

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                    Watch Dependency Graph Example                            │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│   watch let user = User { name: "Alice", profile: Profile { ... } }          │
│                                                                              │
│                    ┌──────────────┐                                          │
│                    │ LocalVar(0)  │◄─── Root (user)                          │
│                    │   [user]     │                                          │
│                    └──────┬───────┘                                          │
│                           │ Binding                                          │
│                           ▼                                                  │
│                    ┌──────────────┐                                          │
│                    │ HeapObject   │                                          │
│                    │   [User]     │                                          │
│                    └──────┬───────┘                                          │
│                           │ InstanceField(1)                                 │
│                           ▼                                                  │
│                    ┌──────────────┐                                          │
│                    │ HeapObject   │                                          │
│                    │  [Profile]   │                                          │
│                    └──────────────┘                                          │
│                                                                              │
│   Changes to User or Profile fields will notify the "user" watch root       │
│                                                                              │
└─────────────────────────────────────────────────────────────────────────────┘
```

Key behaviors:
- When any field in the dependency graph changes, the root is notified
- Deep equality comparison determines if a notification should fire (`WatchFilter::Default`)
- Custom filter functions can control notification behavior
- Variables going out of scope are automatically unregistered

### Watch Events in the Event System

Watch notifications should be integrated into the unified event system:

```rust
//! baml_events/src/watch.rs

use super::*;

#[derive(Clone, Debug)]
pub struct WatchEvent {
    pub meta: EventMeta,
    pub data: WatchEventData,
}

#[derive(Clone, Debug)]
pub enum WatchEventData {
    /// A watched variable's value changed
    VariableChanged(WatchVariableChanged),
    /// Watch was registered (variable declared with `watch let`)
    Registered(WatchRegistered),
    /// Watch was unregistered (variable went out of scope)
    Unregistered(WatchUnregistered),
    /// Watch options were changed
    OptionsChanged(WatchOptionsChanged),
}

#[derive(Clone, Debug)]
pub struct WatchVariableChanged {
    /// Name of the watched variable
    pub variable_name: String,
    /// Channel name (for grouping related watches)
    pub channel: String,
    /// Previous value (before change)
    pub old_value: serde_json::Value,
    /// New value (after change)
    pub new_value: serde_json::Value,
    /// Path within the object that changed (if nested)
    pub change_path: Option<WatchChangePath>,
    /// Node IDs of all roots that were notified
    pub notified_roots: Vec<String>,
}

#[derive(Clone, Debug)]
pub enum WatchChangePath {
    /// Direct binding changed: `x = newValue`
    Binding,
    /// Instance field changed: `x.field = newValue`
    InstanceField { field_index: usize, field_name: String },
    /// Array element changed: `x[i] = newValue`
    ArrayIndex(usize),
    /// Map entry changed: `x[key] = newValue`
    MapKey(String),
}

#[derive(Clone, Debug)]
pub struct WatchRegistered {
    pub variable_name: String,
    pub channel: String,
    pub initial_value: serde_json::Value,
    pub filter_mode: WatchFilterMode,
}

#[derive(Clone, Debug)]
pub struct WatchUnregistered {
    pub variable_name: String,
    pub channel: String,
    pub final_value: serde_json::Value,
}

#[derive(Clone, Debug)]
pub struct WatchOptionsChanged {
    pub variable_name: String,
    pub old_filter: WatchFilterMode,
    pub new_filter: WatchFilterMode,
}

#[derive(Clone, Debug)]
pub enum WatchFilterMode {
    /// Default: notify on value change (deep equals)
    Default,
    /// Manual: user controls when to notify
    Manual,
    /// Paused: no notifications
    Never,
    /// Custom function filter
    Function { function_name: String },
}
```

### Integration with Stream Events

Watch variables are often used to power streaming UI updates. When a `watch let` variable changes during streaming, it should emit both:

1. **WatchEvent** — The low-level variable change notification
2. **StreamEvent::Update** — The high-level streaming update (if in a streaming context)

```rust
//! Example: Watch + Stream integration

impl TraceContext {
    /// Handle a watched variable change during streaming
    pub fn on_watch_change(
        &mut self,
        var_name: &str,
        old_value: Value,
        new_value: Value,
        change_path: WatchChangePath,
    ) {
        // 1. Emit WatchEvent
        if let Some(dispatcher) = &self.dispatcher {
            dispatcher.dispatch(RuntimeEvent::Watch(WatchEvent {
                meta: EventMeta::new(self.call_stack.clone()),
                data: WatchEventData::VariableChanged(WatchVariableChanged {
                    variable_name: var_name.to_string(),
                    channel: self.get_watch_channel(var_name),
                    old_value: old_value.to_json(),
                    new_value: new_value.to_json(),
                    change_path: Some(change_path),
                    notified_roots: vec![], // Filled by watch system
                }),
            }));
        }
        
        // 2. If we're in a streaming context and this variable is the stream output,
        //    also emit a StreamEvent::Update
        if let Some(stream_id) = self.active_stream_for_var(var_name) {
            if let Some(dispatcher) = &self.dispatcher {
                dispatcher.dispatch(RuntimeEvent::Stream(StreamEvent {
                    meta: EventMeta::new(self.call_stack.clone()),
                    data: StreamEventData::Update {
                        stream_id,
                        value: new_value.to_json(),
                    },
                }));
            }
        }
    }
}
```

### Engine Integration: Handling `VmExecState::Event`

With `WatchNotification` eliminated, the engine loop is trivial — no
match-and-translate needed:

```rust
//! bex_engine/src/lib.rs (modifications)

// BEFORE (current code — discards everything):
VmExecState::Notify(_notification) => {
    // Ignore watch notifications for now
}

// AFTER (proposed — forward-only, no mapping):
VmExecState::Event(event) => {
    // 1. Fan out to all registered sinks
    if let Some(dispatcher) = &self.event_dispatcher {
        dispatcher.dispatch(event.clone());
    }

    // 2. Call host-language watch callback (Python/TS)
    if let Some(on_event) = &self.on_event_callback {
        on_event(&event);
    }

    // VM resumes automatically on next loop iteration
}
```

That's it. The engine never inspects the `RuntimeEvent` variant.
The VM already did all the work of constructing the right event type.

### WatchProjection Sink

A dedicated projection for watch notifications:

```rust
//! baml_events/src/sinks/watch_projection.rs

use std::collections::HashMap;
use std::sync::Arc;
use parking_lot::Mutex;
use super::{EventSink, RuntimeEvent, WatchEvent};

/// Callback type for watch notifications
pub type WatchCallback = Box<dyn Fn(&WatchEvent) + Send + Sync>;

/// Projection that routes watch events to registered callbacks.
pub struct WatchProjection {
    /// Callbacks by channel name
    channel_callbacks: Arc<Mutex<HashMap<String, Vec<WatchCallback>>>>,
    /// Global callbacks (receive all watch events)
    global_callbacks: Arc<Mutex<Vec<WatchCallback>>>,
}

impl WatchProjection {
    pub fn new() -> Self {
        Self {
            channel_callbacks: Arc::new(Mutex::new(HashMap::new())),
            global_callbacks: Arc::new(Mutex::new(Vec::new())),
        }
    }
    
    /// Subscribe to watch events on a specific channel.
    pub fn subscribe_channel(&self, channel: &str, callback: WatchCallback) {
        self.channel_callbacks
            .lock()
            .entry(channel.to_string())
            .or_default()
            .push(callback);
    }
    
    /// Subscribe to all watch events.
    pub fn subscribe_all(&self, callback: WatchCallback) {
        self.global_callbacks.lock().push(callback);
    }
}

impl EventSink for WatchProjection {
    fn on_event(&self, event: RuntimeEvent) {
        let RuntimeEvent::Watch(watch_event) = event else {
            return;
        };
        
        // Invoke global callbacks
        for callback in self.global_callbacks.lock().iter() {
            callback(&watch_event);
        }
        
        // Invoke channel-specific callbacks
        if let WatchEventData::VariableChanged(ref changed) = watch_event.data {
            if let Some(callbacks) = self.channel_callbacks.lock().get(&changed.channel) {
                for callback in callbacks {
                    callback(&watch_event);
                }
            }
        }
    }
    
    async fn flush(&self) {}
    
    async fn shutdown(&self) {}
}
```

### RuntimeEvent Enum Update

Add `Watch`, `Block`, and `Viz` to the top-level event enum:

```rust
/// All runtime events emitted by the BAML system.
#[derive(Clone, Debug)]
pub enum RuntimeEvent {
    /// Function execution events
    Function(FunctionEvent),
    /// LLM-specific events
    Llm(LlmEvent),
    /// Header context events for hierarchical execution tracking
    Header(HeaderEvent),
    /// Stream events for incremental updates
    Stream(StreamEvent),
    /// Watch variable change events
    Watch(WatchEvent),
    /// Block enter/exit events
    Block(BlockEvent),
    /// Visualization events (non-header)
    Viz(VizEvent),
    /// Custom tags/metadata
    Tags(TagsEvent),
}
```

### Summary: Watch System

| Event Type | Trigger | Use Case |
|------------|---------|----------|
| `Watch::Registered` | `watch let x = ...` | Track when watches are created |
| `Watch::VariableChanged` | Assignment to watched var or nested field | Real-time UI updates, debugging |
| `Watch::Unregistered` | Variable goes out of scope | Cleanup, lifecycle tracking |
| `Watch::OptionsChanged` | `x.$watch.options(...)` | Track filter mode changes |

Key design decisions:
1. **Watch events are separate from Stream events** — but can trigger stream updates
2. **Watch events include the change path** — so consumers know what specifically changed
3. **WatchProjection supports channel-based routing** — for organizing related watches
4. **Existing VM watch system is preserved** — we just consume its notifications instead of discarding them

## Migration Path

### Phase 1: Core Event Types and Dispatcher

1. Create `baml_events` crate with core types
2. Implement `EventDispatcher` and `NoopSink`
3. Add dispatcher to `VmConfig` (optional)

### Phase 2: Bytecode Trace Opcodes

1. Add `LlmFunctionEnter`/`LlmFunctionExit` opcodes to `bex_vm_types`
2. Add `HeaderEnter`/`HeaderExit` opcodes
3. Add `StreamStart`/`StreamUpdate`/`StreamEnd` opcodes
4. Implement opcode execution in `bex_vm`

### Phase 3: MIR Trace Markers

1. Add `TraceEnter`/`TraceExit` markers to MIR
2. Mark LLM functions during HIR → MIR lowering
3. Emit trace opcodes during MIR → bytecode emission

### Phase 4: LLM Function Instrumentation

1. Auto-wrap all LLM function calls with trace enter/exit
2. Add implicit header events for LLM functions
3. Test nested LLM calls with proper call stack

### Phase 5: Header Events

1. Emit `HeaderEnter`/`HeaderExit` events from VM for explicit headers
2. Ensure proper nesting with implicit LLM function headers
3. Test with local collector
4. Ensure parity with `engine/` header tracking

### Phase 6: Watch Variable Events

1. Stop discarding `VmExecState::Notify` in `bex_engine`
2. Map `WatchNotification::Variables` to `WatchEvent::VariableChanged`
3. Map `WatchNotification::Viz` to `HeaderEvent` or `VizEvent`
4. Map `WatchNotification::Block` to `BlockEvent`
5. Implement `WatchProjection` with channel-based routing
6. Test watch + stream integration

### Phase 7: LLM Request/Response Events

1. Integrate with `llm_ops` for request/response events
2. Add stream events for incremental updates
3. Test with both Boundary publisher and local collector

### Phase 8: Publisher Integration

1. Implement `BoundaryPublisher` sink
2. Add configuration via environment variables
3. Test end-to-end with Boundary API

### Phase 9: LSP Integration

1. Implement `LspNotifier` sink for IDE updates
2. Wire up to existing LSP messaging system
3. Enable real-time execution visualization

## Configuration

Environment variables (matching `engine/` behavior):

| Variable | Default | Description |
|----------|---------|-------------|
| `BOUNDARY_API_URL` | `https://api.boundaryml.com` | API endpoint |
| `BOUNDARY_API_KEY` | (none) | API key for authentication |
| `BAML_TRACE_BATCH_SIZE` | `500` | Events per batch |
| `BAML_BLOB_BATCH_SIZE` | `10` | Blobs per batch |
| `BAML_TRACE_COMPRESSION_THRESHOLD_MB` | `2.0` | Compress if larger |
| `BAML_MAX_TRACE_UPLOAD_MB` | `10` | Max upload size |

## WASM Considerations

For WASM targets:
- Use `web_time` instead of `std::time`
- Use `wasm_bindgen_futures::spawn_local` instead of `tokio::spawn`
- Consider channel alternatives that work in WASM (e.g., `async-channel`)

## Testing Strategy

1. **Unit tests**: Test event creation and serialization
2. **Integration tests**: Test dispatcher with multiple sinks
3. **Mock publisher tests**: Test batching and flush behavior
4. **End-to-end tests**: Test with real Boundary API (optional, CI-gated)

## Open Questions

1. Should we share event types between `engine/` and `baml_language/`?
   - Pro: Single source of truth
   - Con: Coupling between old and new compilers

2. How to handle backpressure when publisher queue is full?
   - Current `engine/` behavior: Log warning and drop events
   - Alternative: Apply backpressure to caller

3. Should collectors be scoped to specific call trees?
   - Current design: Yes, via `track()`/`untrack()` API
   - Consider: Automatic cleanup via RAII guards

4. **Bytecode vs VM-level instrumentation for LLM functions?**
   - **Option A (Bytecode)**: More explicit, compiler controls trace points
   - **Option B (VM)**: Simpler bytecode, VM handles all tracing
   - **Hybrid (Recommended)**: Bytecode contains trace opcodes, VM executes them
   - Decision affects: debugging, performance, flexibility

5. Should expression functions be opt-in traceable?
   - Default: No tracing for expression functions (they're cheap)
   - Consider: `@trace` attribute to enable tracing for debugging
   - Consider: Tracing expression functions that call LLM functions

6. How should implicit LLM headers interact with explicit headers?
   - Current proposal: LLM functions get level based on current header depth
   - Alternative: LLM function headers are always at a fixed level (e.g., 1)
   - Alternative: LLM functions don't get implicit headers, only explicit ones

7. What happens when streaming fails mid-stream?
   - Need to emit proper error events
   - Need to close any open headers
   - Need to update call stack correctly

8. Should watch events be included in Boundary uploads?
   - Pro: Full execution trace for debugging
   - Con: Could be very high volume, increase costs
   - Consider: Make it configurable, default to off for publisher

9. How should watch channels map to event filtering?
   - Current: `channel` is a string in `RootState`
   - Consider: First-class channel support in projections
   - Consider: Channel hierarchies (e.g., `llm.streaming.result`)

10. Should `Watch(VariableChanged)` include the change path?
    - Currently: VM only knows which roots were affected, not the exact mutation path
    - Needed: Which field/index changed for efficient partial updates
    - Consider: Thread mutation context (`WatchChangePath`) through `process_notifications()`

## References

### Existing Implementation (engine/)
- `engine/baml-runtime/src/tracingv2/publisher/publisher.rs` — Current publisher implementation
- `engine/baml-runtime/src/tracingv2/storage/storage.rs` — Current storage implementation
- `engine/baml-runtime/src/control_flow.rs` — Header context handling
- `engine/baml-lib/baml-types/src/tracing/events.rs` — Event type definitions

### New Compiler (baml_language/)
- `baml_language/crates/baml_project/src/db.rs` — Salsa event callback pattern
- `baml_language/crates/baml_compiler_mir/` — MIR definitions (trace markers go here)
- `baml_language/crates/baml_compiler_emit/` — Bytecode emission (trace opcodes emitted here)
- `baml_language/crates/bex_vm_types/src/bytecode.rs` — Bytecode opcode definitions
- `baml_language/crates/bex_vm/src/vm.rs` — VM execution (trace opcode handling, WatchNotification)
- `baml_language/crates/bex_vm/src/watch.rs` — Watch dependency graph and reachability tracking
- `baml_language/crates/bex_engine/src/lib.rs` — Engine loop (currently discards WatchNotification)
- `baml_language/crates/llm_ops/` — LLM client operations (request/response events)

### Related Design Doc
- `tech-docs-codex/compiler-event-publishing.md` — Runtime Event Bus design with projections

---

## Full Design Path: Exposing Typed Stream Values to Host Languages

This section traces the **complete path** from an LLM SSE chunk arriving over
HTTP all the way to a typed `Partial<MyClass>` value being yielded to a Python
`async for` loop. We first document how `engine/` does it today, then design
the equivalent path for `baml_language/` via `bridge_cffi`.

### Part 1: How `engine/` Does It Today

#### 1.1 The Stack

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                       engine/ Typed Stream Path                              │
│                                                                             │
│  HTTP SSE chunk (raw text)                                                  │
│    │                                                                        │
│    ▼                                                                        │
│  LLMCompleteResponse.content  (accumulated raw text)                        │
│    │                                                                        │
│    ▼  tokio::watch channel                                                  │
│  run_parser_loop()  (50ms throttle)                                         │
│    │  ├─ partial_parse_fn(content) → ResponseBamlValue                      │
│    │  ├─ serialize_partial() → dedup string compare                         │
│    │  └─ on_event(FunctionResult { ..., Some(Ok(ResponseBamlValue)) })      │
│    │                                                                        │
│    ▼  on_event callback                                                     │
│  FunctionResult  (wraps ResponseBamlValue)                                  │
│    │                                                                        │
│    │  ┌─ Python path ────────────────────────────────────────────┐          │
│    ▼  │  safe_trigger_callback(id, false, Ok(result), runtime)   │          │
│    │  │    │                                                     │          │
│    │  │    ▼                                                     │          │
│    │  │  send_result_to_callback(id, false, &content, runtime)   │          │
│    │  │    │  content.0.map_meta() → EncodeMeta                  │          │
│    │  │    │  .encode_to_c_buffer(ir, StreamingMode::Streaming)  │          │
│    │  │    │  → protobuf CffiValueHolder bytes                   │          │
│    │  │    │                                                     │          │
│    │  │    ▼  CallbackFn(id, 0, buf_ptr, buf_len)                │          │
│    │  │  Python CFFI receives bytes, decodes CffiValueHolder     │          │
│    │  │    → typed Partial<T> Python object                      │          │
│    │  └──────────────────────────────────────────────────────────┘          │
│    │                                                                        │
│    │  ┌─ TypeScript path ────────────────────────────────────────┐          │
│    ▼  │  on_event closure (ThreadsafeFunction)                   │          │
│    │  │    ▼                                                     │          │
│    │  │  FunctionResult.parsed(true)                             │          │
│    │  │    → serialize_partial() → serde_json::Value             │          │
│    │  │    → NAPI callback(null, FunctionResult)                 │          │
│    │  │    → TypeScript: event.parsed(true) → JSON               │          │
│    │  │    → partialCoerce(json) → typed Partial<T>              │          │
│    │  └──────────────────────────────────────────────────────────┘          │
│    │                                                                        │
│  Final: stream complete                                                     │
│    ▼  safe_trigger_callback(id, true, final_result, runtime)                │
│  send_result_to_callback(id, true, &content, runtime)                       │
│    → encode_to_c_buffer(ir, StreamingMode::NonStreaming)                     │
│    → CallbackFn(id, 1, buf_ptr, buf_len)                                   │
│    → typed T (final) Python/TS object                                       │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 1.2 Key Types

**FunctionResult** — `engine/baml-runtime/src/types/response.rs`

```rust
pub struct FunctionResult {
    event_chain: Vec<(
        OrchestrationScope,            // which LLM client/retry
        LLMResponse,                   // raw HTTP response
        Option<Result<ResponseBamlValue>>,  // parsed typed value (if any)
    )>,
}

impl FunctionResult {
    /// Create from a single streaming event
    pub fn new(
        scope: OrchestrationScope,
        response: LLMResponse,
        parsed: Option<Result<ResponseBamlValue>>,
    ) -> Self { ... }

    /// Get the parsed value from the last event
    pub fn result_with_constraints_content(&self) -> Result<&ResponseBamlValue> { ... }
}
```

**ResponseBamlValue** — `engine/baml-lib/jsonish/src/lib.rs`

```rust
/// A parsed BAML value with response metadata (flags, checks, completion state, type info).
pub struct ResponseBamlValue(pub BamlValueWithMeta<ResponseValueMeta>);

pub struct ResponseValueMeta(
    pub Vec<Flag>,              // parsing quality flags
    pub Vec<ResponseCheck>,     // constraint checks
    pub Completion,             // streaming completion state (PENDING/STARTED/DONE)
    pub TypeIR,                 // type information for encoding
);

impl ResponseBamlValue {
    /// Serialize for streaming — includes StreamState wrapper for incomplete fields
    pub fn serialize_partial(&self) -> SerializeResponseBamlValue { ... }

    /// Serialize for final result — no streaming metadata
    pub fn serialize_final(&self) -> SerializeResponseBamlValue { ... }
}
```

#### 1.3 CFFI Encoding Path (for Python/Ruby/Go via C FFI)

```rust
// engine/language_client_cffi/src/ffi/callbacks.rs

pub fn send_result_to_callback(id: u32, is_done: bool, content: &ResponseBamlValue, runtime: &BamlRuntime) {
    if is_done {
        // Final: encode with non-streaming types (Types namespace)
        let meta = content.0.map_meta(|f| EncodeMeta {
            field_type: f.3.to_non_streaming_type(runtime.ir.as_ref()),
            checks: &f.1,
        });
        meta.encode_to_c_buffer(runtime.ir.as_ref(), StreamingMode::NonStreaming)
    } else {
        // Streaming: encode with streaming types (StreamTypes namespace)
        // Top level types in streaming always have `not_null` set to true.
        let mut content = content.0.clone();
        content.meta_mut().3.meta_mut().streaming_behavior.needed = true;
        let meta = content.map_meta(|f| EncodeMeta {
            field_type: f.3.to_streaming_type(runtime.ir.as_ref()),
            checks: &f.1,
        });
        meta.encode_to_c_buffer(runtime.ir.as_ref(), StreamingMode::Streaming)
    }
    // → protobuf CffiValueHolder bytes sent via callback_fn(id, is_done, buf, len)
}
```

The **protobuf types** are defined in `engine/language_client_cffi/types/baml/cffi/v1/baml_outbound.proto`:

```protobuf
message CFFIValueHolder {
  oneof value {
    string string_value = 2;
    int64 int_value = 3;
    double float_value = 4;
    bool bool_value = 5;
    CFFIValueClass class_value = 9;
    CFFIValueEnum enum_value = 10;
    CFFIValueList list_value = 11;
    CFFIValueMap map_value = 12;
    CFFIValueUnionVariant union_variant_value = 13;
    CFFIValueChecked checked_value = 14;
    CFFIValueStreamingState streaming_state_value = 15;  // ← streaming!
  }
}

// Wraps a value that may not have arrived yet
message CFFIValueStreamingState {
  CFFITypeName name = 1;      // e.g. "Partial<MyClass>"
  CFFIValueHolder value = 2;  // the partial value (may have null fields)
}
```

**Key insight**: The `CffiValueStreamingState` wrapper is what tells the host
language "this field is still streaming." The host language codegen generates
`Partial<T>` types where every field is `Optional<Partial<FieldType>>`.

#### 1.4 NAPI Path (TypeScript)

```rust
// engine/language_client_typescript/src/types/function_results.rs

#[napi]
pub fn parsed(&self, allow_partials: bool) -> napi::Result<serde_json::Value> {
    let parsed = self.inner.result_with_constraints_content()?;
    let response = serde_json::to_value(if allow_partials {
        parsed.serialize_partial()   // → includes StreamState wrappers
    } else {
        parsed.serialize_final()     // → clean types, no stream metadata
    })?;
    Ok(response)
}
```

TypeScript side:
```typescript
// engine/language_client_typescript/typescript_src/stream.ts
async *[Symbol.asyncIterator]() {
    for await (const event of this.events) {
        if (event.isOk()) {
            yield this.partialCoerce(event.parsed(true));  // allow_partials=true
        }
    }
}
```

#### 1.5 Python BamlStream Consumer

```python
# engine/language_client_python/python_src/baml_py/stream.py

class BamlStream:
    def __init__(self, ffi_stream, partial_coerce, final_coerce, ctx_manager):
        self.__ffi_stream = ffi_stream.on_event(self.__enqueue)  # register callback
        self.__event_queue = queue.Queue()

    def __enqueue(self, data: FunctionResult) -> None:
        self.__event_queue.put_nowait(data)

    async def __aiter__(self):
        self.__drive_to_completion_in_bg()  # spawns thread → calls ffi.done()
        while True:
            event = self.__event_queue.get_nowait()
            if event is None: break                     # stream done

            # Coalesce: drain queue, keep latest successful partial
            latest_ok = event if event.is_ok() else None
            while True:
                nxt = self.__event_queue.get_nowait()
                if nxt is None: break
                if nxt.is_ok(): latest_ok = nxt         # keep latest only

            if latest_ok is not None:
                yield self.__partial_coerce(latest_ok)  # → typed Partial<T>
```

#### 1.6 Streaming Call Flow (Python CFFI)

```
Python user code                          Rust (engine/)
═══════════════                          ══════════════

stream = b.stream.MyFunc(args)
                                          ┌ call_function_stream_from_c(runtime, name, args, id)
                                          │   runtime.stream_function(name, kwargs, ...)
                                          │   → FunctionResultStream { ... }
                                          │   RUNTIME.spawn(async {
                                          │       stream.run(
                                          │           on_tick = || trigger_on_tick_callback(id),
                                          │           on_event = |result| {
                                          │               safe_trigger_callback(id, false, result, runtime)
                                          │               // encodes as protobuf, calls C callback
                                          │           },
                                          │       ).await
                                          │       safe_trigger_callback(id, true, final_result, runtime)
                                          │   })
                                          └

async for partial in stream:              stream.run() internally:
    print(partial.field_a)                  SSE chunks → accumulate → watch channel
                                            parser loop (50ms) → partial parse
                                            → on_event(FunctionResult)
                                            → safe_trigger_callback(id, false, ...)
                                            → encode protobuf (StreamingMode::Streaming)
                                            → C callback → Python queue.put(event)
    ← yield partial_coerce(event)           Python drains queue, coalesces, yields

final = await stream.get_final_response()
    ← final_coerce(result)                → safe_trigger_callback(id, true, ...)
                                          → encode protobuf (StreamingMode::NonStreaming)
```

---

### Part 2: Proposed Path for `baml_language/` via `bridge_cffi`

#### 2.1 Current State of `bridge_cffi`

`bridge_cffi` (at `baml_language/crates/bridge_cffi/`) is a work-in-progress
replacement for `engine/language_client_cffi/`. It uses `bex_engine` instead
of `baml-runtime`. Current status:

- `call_function_from_c()` ✅ — works, returns `BexExternalValue`
- `call_function_stream_from_c()` ❌ — returns error "Streaming not implemented"
- `call_function_parse_from_c()` ❌ — returns error "not implemented"
- `cancel_function_call()` ❌ — placeholder

The return type is `BexExternalValue` (not `FunctionResult`). Currently there
is **no partial/streaming concept** in `BexExternalValue`.

#### 2.2 The Gap: What's Missing

| Feature | `engine/` has | `bridge_cffi` has | Needed |
|---|---|---|---|
| Parsed result type | `ResponseBamlValue` with `Completion` state | `BexExternalValue` (no completion) | Add streaming metadata |
| Streaming state per field | `Completion { display, required_done }` | None | Add `StreamingState` |
| Partial serialization | `serialize_partial()` → `CffiValueStreamingState` | N/A | Encode partials |
| Stream function entry | `stream_function()` → `FunctionResultStream` | N/A | `bex_engine` stream API |
| Throttled parsing | `run_parser_loop` (50ms) | `StreamState` (proposed) | VM-level throttle |
| Event callback | `on_event: Fn(FunctionResult)` | N/A | `VmExecState::Event` |
| Protobuf streaming type | `CffiValueStreamingState` | Exists in proto (shared) | Wire up |

#### 2.3 The Proposed Stack

```
┌─────────────────────────────────────────────────────────────────────────────┐
│               baml_language/ Typed Stream Path (Proposed)                    │
│                                                                             │
│  HTTP SSE chunk (raw text from sys_native)                                  │
│    │                                                                        │
│    ▼                                                                        │
│  StreamState.accumulated_text  (in VM)                                      │
│    │                                                                        │
│    ▼  VM throttled parse (50ms)                                             │
│  partial_parse(accumulated_text) → BexStreamValue                           │
│    │  ├─ serialize + dedup check                                            │
│    │  └─ VmExecState::Event(Stream(Update { value: BexStreamValue }))       │
│    │                                                                        │
│    ▼  bex_engine event loop                                                 │
│  match VmExecState::Event(event) {                                          │
│    │  dispatch to sinks                                                     │
│    │  call on_event callback                                                │
│    │  resume VM                                                             │
│    │ }                                                                      │
│    │                                                                        │
│    ▼  bridge_cffi callback                                                  │
│  encode_stream_result(id, false, &BexStreamValue)                           │
│    │  → external_to_cffi_stream_value(&value)                               │
│    │  → protobuf CffiValueHolder bytes (with CffiValueStreamingState)       │
│    │  → CallbackFn(id, 0, buf_ptr, buf_len)                                │
│    │                                                                        │
│    ▼  Host language                                                         │
│  Python/TS decodes protobuf → typed Partial<T>                              │
│    │  queue.put(event)                                                      │
│    │  coalesce → yield to user's for loop                                   │
│    │                                                                        │
│  Final: Stream(End { final_value })                                         │
│    ▼  encode_stream_result(id, true, &BexExternalValue)                     │
│    → protobuf CffiValueHolder (no StreamingState wrappers)                  │
│    → CallbackFn(id, 1, buf_ptr, buf_len)                                   │
│    → typed T (final)                                                        │
└─────────────────────────────────────────────────────────────────────────────┘
```

#### 2.4 New Type: `BexStreamValue`

We need a value type that carries streaming completion metadata, analogous to
`ResponseBamlValue` but fitting into the BEX type system:

```rust
//! bex_external_types/src/stream_value.rs (new)

use super::{BexExternalValue, Ty};

/// Streaming completion state for a single field.
#[derive(Clone, Debug, PartialEq)]
pub enum StreamingCompletion {
    /// Field has not received any data yet.
    Pending,
    /// Field is receiving data (partially complete).
    Started,
    /// Field is fully received.
    Done,
}

/// A BexExternalValue annotated with per-field streaming completion state.
///
/// Analogous to `ResponseBamlValue` in engine/, but uses the BEX type system.
/// This is the type that flows through the streaming pipeline.
#[derive(Clone, Debug)]
pub struct BexStreamValue {
    /// The (possibly incomplete) value tree.
    pub value: BexExternalValue,
    /// Per-field streaming completion.
    /// Keys are dot-paths (e.g., "field_a", "field_a.nested_b").
    /// Missing keys default to `Pending`.
    pub completions: indexmap::IndexMap<String, StreamingCompletion>,
    /// The declared return type (for encoding).
    pub return_type: Ty,
}

impl BexStreamValue {
    /// Create from a partial parse result.
    pub fn from_partial(
        value: BexExternalValue,
        completions: indexmap::IndexMap<String, StreamingCompletion>,
        return_type: Ty,
    ) -> Self {
        Self { value, completions, return_type }
    }

    /// Create a "final" stream value where all fields are Done.
    pub fn from_final(value: BexExternalValue, return_type: Ty) -> Self {
        Self {
            value,
            completions: indexmap::IndexMap::new(), // empty = all done
            return_type,
        }
    }

    /// Check if all fields are Done.
    pub fn is_complete(&self) -> bool {
        self.completions.values().all(|c| matches!(c, StreamingCompletion::Done))
    }
}
```

#### 2.5 Updated `RuntimeEvent::Stream(Update)`

The stream update event carries a `BexStreamValue` instead of raw JSON:

```rust
//! baml_events/src/stream.rs (updated)

#[derive(Clone, Debug)]
pub enum StreamEventData {
    Start {
        stream_id: String,
        return_type: Ty,                     // declared return type
    },
    Update {
        stream_id: String,
        value: BexStreamValue,               // typed partial with completions
    },
    End {
        stream_id: String,
        final_value: Option<BexExternalValue>,  // fully typed final value
    },
}
```

#### 2.6 `bridge_cffi` Encoding: `BexStreamValue` → Protobuf

```rust
//! bridge_cffi/src/ctypes/value_encode.rs (additions)

use bex_external_types::{BexStreamValue, StreamingCompletion};
use crate::baml::cffi::{
    CffiValueHolder, CffiValueStreamingState, CffiTypeName, CffiTypeNamespace,
    cffi_value_holder::Value as CffiValueVariant,
};

/// Convert a streaming BexStreamValue to CffiValueHolder with streaming state.
pub fn stream_value_to_cffi(value: &BexStreamValue) -> Result<CffiValueHolder, BridgeError> {
    stream_value_to_cffi_inner(&value.value, &value.completions, "", &value.return_type)
}

fn stream_value_to_cffi_inner(
    value: &BexExternalValue,
    completions: &indexmap::IndexMap<String, StreamingCompletion>,
    path: &str,
    ty: &Ty,
) -> Result<CffiValueHolder, BridgeError> {
    // Check if this field needs a StreamingState wrapper
    let completion = completions.get(path).unwrap_or(&StreamingCompletion::Done);

    match completion {
        StreamingCompletion::Pending => {
            // Field hasn't started — wrap in StreamingState with null inner
            Ok(CffiValueHolder {
                value: Some(CffiValueVariant::StreamingStateValue(Box::new(
                    CffiValueStreamingState {
                        name: Some(CffiTypeName {
                            namespace: CffiTypeNamespace::StreamStateTypes as i32,
                            name: ty_to_stream_type_name(ty),
                        }),
                        value: None,  // no value yet
                    },
                ))),
            })
        }
        StreamingCompletion::Started => {
            // Field is partially complete — wrap inner value in StreamingState
            let inner = external_to_cffi_value(value)?;
            Ok(CffiValueHolder {
                value: Some(CffiValueVariant::StreamingStateValue(Box::new(
                    CffiValueStreamingState {
                        name: Some(CffiTypeName {
                            namespace: CffiTypeNamespace::StreamStateTypes as i32,
                            name: ty_to_stream_type_name(ty),
                        }),
                        value: Some(Box::new(inner)),
                    },
                ))),
            })
        }
        StreamingCompletion::Done => {
            // Field is complete — encode normally (no streaming wrapper)
            external_to_cffi_value(value)
        }
    }
}
```

#### 2.7 `bridge_cffi` Stream Function Implementation

```rust
//! bridge_cffi/src/ffi/functions.rs (additions)

/// Stream a function call.
///
/// Like call_function_from_c, but delivers partial results via the
/// on_event callback (is_done=0) during execution, and the final
/// result via the callback (is_done=1) when done.
#[unsafe(no_mangle)]
pub extern "C" fn call_function_stream_from_c(
    _runtime: *const libc::c_void,
    function_name: *const libc::c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Buffer {
    match call_function_stream_inner(function_name, encoded_args, length, id) {
        Ok(()) => encode_success_response(),
        Err(e) => encode_error_response(&e),
    }
}

fn call_function_stream_inner(
    function_name: *const libc::c_char,
    encoded_args: *const libc::c_char,
    length: usize,
    id: u32,
) -> Result<(), BridgeError> {
    let engine = get_engine()?.clone();
    let func_name = parse_function_name(function_name)?;
    let args = HostFunctionArguments::from_c_buffer(encoded_args as *const u8, length)?;
    let kwargs = kwargs_to_bex_values(args.kwargs)?;

    let params = engine.function_params(&func_name)
        .ok_or_else(|| BridgeError::FunctionNotFound { name: func_name.clone() })?;
    let bex_args = reorder_kwargs(&func_name, &params, &kwargs)?;

    let rt = get_runtime().clone();
    rt.spawn(async move {
        // call_function_stream is a new method on BexEngine that returns
        // partial results via a callback.
        let result = engine.call_function_stream(
            &func_name,
            &bex_args,
            // on_event: called for each partial result
            |stream_value: &BexStreamValue| {
                match stream_value_to_cffi(stream_value) {
                    Ok(cffi_value) => {
                        let buf = cffi_value.encode_to_vec();
                        tokio::task::block_in_place(|| {
                            get_result_callback()(id, 0, buf.as_ptr() as *const i8, buf.len());
                        });
                    }
                    Err(e) => {
                        log::error!("Stream encoding error: {}", e);
                    }
                }
            },
            // on_tick: called periodically for liveness
            || trigger_on_tick_callback(id),
        ).await;

        // Final result
        match result {
            Ok(final_value) => {
                send_result_to_callback(id, true, &final_value);
            }
            Err(e) => {
                send_error_to_callback(id, &format!("{}", e));
            }
        }
    });

    Ok(())
}
```

#### 2.8 `bex_engine` Stream API

```rust
//! bex_engine/src/lib.rs (additions)

impl BexEngine {
    /// Execute a streaming function call.
    ///
    /// Returns partial results via on_event callback during execution.
    /// Returns the final BexExternalValue when the function completes.
    pub async fn call_function_stream<F, T>(
        &self,
        function_name: &str,
        args: &[BexValue],
        on_event: F,           // called for each Stream(Update)
        on_tick: T,            // called periodically
    ) -> Result<BexExternalValue, EngineError>
    where
        F: Fn(&BexStreamValue) + Send + 'static,
        T: Fn() + Send + 'static,
    {
        // Same setup as call_function...
        let entry_point = self.resolve_function(function_name)?;
        let mut vm = self.create_vm(entry_point, args)?;

        let mut final_value = None;

        loop {
            match vm.step() {
                VmExecState::Done(value) => {
                    final_value = Some(self.externalize_value(value)?);
                    break;
                }

                VmExecState::Event(event) => {
                    match &event {
                        RuntimeEvent::Stream(StreamEvent {
                            data: StreamEventData::Update { value, .. }, ..
                        }) => {
                            on_event(value);
                            on_tick();
                        }
                        RuntimeEvent::Stream(StreamEvent {
                            data: StreamEventData::End { final_value: Some(v), .. }, ..
                        }) => {
                            // LLM stream completed — final value will come
                            // from VmExecState::Done after remaining bytecode
                        }
                        _ => {
                            // Other events: dispatch to sinks, continue
                        }
                    }

                    // Dispatch to all registered sinks
                    if let Some(dispatcher) = &self.event_dispatcher {
                        dispatcher.dispatch(event);
                    }

                    // Resume VM
                    continue;
                }

                VmExecState::Yield(sys_op) => {
                    // Handle I/O (HTTP, etc.) — same as call_function
                    let result = self.execute_sys_op(sys_op).await?;
                    vm.resume(result);
                }

                VmExecState::Continue => continue,

                VmExecState::Error(e) => return Err(e.into()),
            }
        }

        final_value.ok_or_else(|| EngineError::NoReturnValue)
    }
}
```

#### 2.9 VM Partial Parsing

Inside the VM, when an LLM function is streaming, the `handle_stream_chunk`
method (described in the Streaming Architecture section) produces
`BexStreamValue` instead of raw JSON:

```rust
//! bex_vm/src/stream_state.rs (extended)

impl StreamState {
    /// Parse accumulated text into a BexStreamValue.
    ///
    /// Uses the function's return type to determine field completion states.
    pub fn parse_partial(
        &mut self,
        return_type: &Ty,
        parse_fn: impl Fn(&str, &Ty) -> Result<(BexExternalValue, CompletionMap)>,
    ) -> Option<BexStreamValue> {
        if !self.should_parse() {
            return None;
        }

        let (value, completions) = match parse_fn(&self.accumulated_text, return_type) {
            Ok(result) => result,
            Err(_) => return None,  // parse failed, continue accumulating
        };

        // Dedup check
        let serialized = format!("{:?}", value);  // or proper serialization
        if !self.should_emit(&serialized) {
            return None;
        }

        Some(BexStreamValue::from_partial(value, completions, return_type.clone()))
    }
}
```

#### 2.10 Complete Flow Diagram

```
Python user code                          Rust (baml_language/)
═══════════════                          ══════════════════════

stream = b.stream.MyFunc(args)
                                          ┌ call_function_stream_from_c(_, name, args, id)
                                          │   engine.call_function_stream(name, args, on_event, on_tick)
                                          │     vm = create_vm(entry_point, args)
                                          │     loop {
                                          │       match vm.step() {
                                          │
                                          │   ┌─ VmExecState::Yield(SysOp::HttpStream)
                                          │   │   result = execute_sys_op(http_stream).await
                                          │   │   vm.resume(result)  // SSE chunk arrived
                                          │   │
                                          │   │   VM internally:
                                          │   │     stream_state.push_chunk(text)
                                          │   │     stream_state.parse_partial(return_type, parse_fn)
                                          │   │       → Some(BexStreamValue { value, completions })
                                          │   │     return VmExecState::Event(Stream(Update { value }))
                                          │   │
                                          │   ├─ VmExecState::Event(Stream(Update { value }))
                                          │   │   on_event(&value)
                                          │   │     → stream_value_to_cffi(&value)
                                          │   │     → protobuf bytes (with CffiValueStreamingState)
                                          │   │     → CallbackFn(id, 0, buf, len)
                                          │   │   dispatcher.dispatch(event)
                                          │   │   continue (resume VM)
                                          │   │
async for partial in stream:              │   │   Python receives protobuf bytes
    print(partial.field_a)                │   │     → decode CffiValueHolder
    ← yield partial_coerce(event)         │   │     → queue.put(event)
                                          │   │     → coalesce, yield typed Partial<T>
                                          │   │
                                          │   │   ... repeat for each throttled parse ...
                                          │   │
                                          │   ├─ VmExecState::Event(Stream(End { final_value }))
                                          │   │   dispatcher.dispatch(event)
                                          │   │
                                          │   ├─ VmExecState::Done(value)
                                          │   │   final_value = externalize_value(value)
                                          │   │   break
                                          │   └─
                                          │
                                          │   send_result_to_callback(id, true, &final_value)
                                          │     → external_to_cffi_value(&value)
                                          │     → protobuf bytes (no StreamingState wrappers)
                                          │     → CallbackFn(id, 1, buf, len)
                                          └

final = await stream.get_final_response()
    ← final_coerce(result)                → typed T (final)
```

#### 2.11 Comparison: `engine/` vs `baml_language/` Full Path

| Layer | `engine/` | `baml_language/` (proposed) |
|---|---|---|
| **HTTP streaming** | tokio SSE stream → `LLMCompleteResponse` | `sys_native` async op → `StreamState` in VM |
| **Buffering** | `tokio::watch` channel between SSE & parser tasks | VM-local `StreamState.accumulated_text` |
| **Parse throttle** | 50ms `tokio::interval` in `run_parser_loop` | 50ms check in `StreamState.should_parse()` |
| **Parsed type** | `ResponseBamlValue` (BamlValueWithMeta) | `BexStreamValue` (BexExternalValue + completions) |
| **Completion tracking** | `Completion { display, required_done }` per meta | `StreamingCompletion` per field path |
| **Deduplication** | String comparison of serialized partial | Same, in `StreamState.should_emit()` |
| **Event delivery** | `on_event(FunctionResult)` callback | `VmExecState::Event(Stream(Update))` yield |
| **CFFI encoding** | `EncodeMeta` + `encode_to_c_buffer(ir, StreamingMode)` | `stream_value_to_cffi()` → protobuf |
| **Protobuf wire format** | `CffiValueHolder` with `CffiValueStreamingState` | **Same** (shared proto definition) |
| **Host language consumer** | `BamlStream` (queue + coalesce) | **Same** (identical Python/TS wrapper) |
| **Final result** | `FunctionResult` → `result_with_constraints_content()` | `BexExternalValue` (direct) |
| **Final encoding** | `encode_to_c_buffer(ir, NonStreaming)` | `external_to_cffi_value()` (already exists) |

#### 2.12 Implementation Phases

**Phase A: Core streaming types** (no CFFI changes)
1. Add `BexStreamValue` and `StreamingCompletion` to `bex_external_types`
2. Add `StreamState` to `bex_vm`
3. Update `StreamEventData::Update` to carry `BexStreamValue`

**Phase B: VM streaming execution**
4. Implement `handle_stream_chunk` / `handle_stream_end` in VM
5. Wire up `VmExecState::Event(Stream(...))` emission
6. Add partial parse function (leveraging existing `jsonish` or new parser)

**Phase C: Engine stream API**
7. Add `call_function_stream` to `BexEngine`
8. Handle `VmExecState::Event(Stream(Update))` in engine loop
9. Add on_event + on_tick callback plumbing

**Phase D: CFFI streaming**
10. Add `stream_value_to_cffi()` encoding in `bridge_cffi`
11. Implement `call_function_stream_from_c()` (replace placeholder)
12. Wire up callbacks for partial and final results

**Phase E: Host language**
13. Verify existing `BamlStream` (Python) / `BamlStream` (TS) works with
    new protobuf format (should be unchanged — same proto)
14. Test end-to-end: BAML function → SSE → partial → Python `async for`

### References (Typed Stream Path)

**engine/ (existing)**
- `engine/baml-runtime/src/types/response.rs` — `FunctionResult` definition
- `engine/baml-runtime/src/types/stream.rs` — `FunctionResultStream` and `run()`
- `engine/baml-runtime/src/internal/llm_client/orchestrator/stream.rs` — Streaming orchestration, `ParserState`, `run_parser_loop`
- `engine/baml-lib/jsonish/src/lib.rs` — `ResponseBamlValue`, `serialize_partial()`, `serialize_final()`
- `engine/language_client_cffi/src/ffi/callbacks.rs` — `safe_trigger_callback`, `send_result_to_callback`
- `engine/language_client_cffi/src/ffi/functions.rs` — `call_function_stream_from_c`, `on_event`
- `engine/language_client_cffi/src/ctypes/baml_value_with_meta_encode.rs` — Protobuf encoding with `StreamingState`
- `engine/language_client_cffi/types/baml/cffi/v1/baml_outbound.proto` — `CffiValueStreamingState`
- `engine/language_client_python/python_src/baml_py/stream.py` — Python `BamlStream` consumer
- `engine/language_client_python/src/types/function_result_stream.rs` — Python FFI stream bindings
- `engine/language_client_python/src/types/function_results.rs` — Python `FunctionResult` with `cast_to()`
- `engine/language_client_typescript/src/types/function_result_stream.rs` — TS NAPI stream bindings
- `engine/language_client_typescript/src/types/function_results.rs` — TS `FunctionResult.parsed()`

**baml_language/ (new)**
- `baml_language/crates/bex_external_types/src/bex_external_value.rs` — `BexExternalValue` definition
- `baml_language/crates/bex_external_types/src/lib.rs` — External types crate
- `baml_language/crates/bex_engine/src/lib.rs` — Engine event loop (needs `call_function_stream`)
- `baml_language/crates/bridge_cffi/src/ffi/functions.rs` — `call_function_stream_from_c` (placeholder)
- `baml_language/crates/bridge_cffi/src/ffi/callbacks.rs` — Callback registration (ready)
- `baml_language/crates/bridge_cffi/src/ctypes/value_encode.rs` — `external_to_cffi_value` (needs streaming extension)
