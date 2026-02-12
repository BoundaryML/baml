# Callstack Tracking Implementation Plan

Implementation plan for [callstack-tracking-design-v2.md](./callstack-tracking-design-v2.md).

---

## Current State

- **VM** (`bex_vm`): `BexVm` has `frames: Vec<Frame>`, `stack: EvalStack`, `watched_vars`, `interrupt_frame`. `VmExecState` has 4 variants: `Await`, `ScheduleFuture`, `Complete`, `Notify(WatchNotification)`. `Call(usize)` dispatches at `vm.rs:2085`. `Return` dispatches at `vm.rs:2201`.
- **Bytecode** (`bex_vm_types`): `Instruction` enum has 42 variants. `Call(usize)` at `bytecode.rs:290`. `Value` is `Copy` enum: `Null | Int(i64) | Float(f64) | Bool(bool) | Object(HeapPtr)`.
- **Engine** (`bex_engine`): `BexEngine` is a shared `Arc` singleton with epoch-based GC. `call_function()` creates a per-call VM, runs event loop matching `VmExecState`. `Notify` variant is currently **ignored** (line 833-835).
- **Compiler** (`baml_compiler_emit`): `emit_terminator()` emits `Instruction::Call(args.len())` for `Terminator::Call` in `emit.rs:910`. LLM functions compile to bytecode calling `call_llm_function` via regular `Call`.
- **Event infrastructure**: **None**. No `baml_events` crate, no `SpanId`, no `EventStore`, no `event_bus::emit()`. Zero event types exist in the codebase.

---

## Phases

### Phase 0: Minimal Event Types

Create the bare minimum event types needed for the engine to emit `FunctionStart`/`FunctionEnd`. No global `EventStore`, no collectors, no publisher — just types and a simple channel-based sink.

#### 0.1 Create `baml_events` crate

```
baml_language/crates/baml_events/
├── Cargo.toml
└── src/
    ├── lib.rs
    ├── span_id.rs       # SpanId newtype (Uuid)
    ├── span_context.rs  # SpanContext { span_id, parent_span_id, root_span_id }
    └── types.rs         # RuntimeEvent, EventKind, FunctionEvent, FunctionStart, FunctionEnd
```

**Types to define:**

```rust
// span_id.rs
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub struct SpanId(pub uuid::Uuid);

impl SpanId {
    pub fn new() -> Self { Self(uuid::Uuid::new_v4()) }
}

// span_context.rs
#[derive(Clone, Debug)]
pub struct SpanContext {
    pub span_id: SpanId,
    pub parent_span_id: Option<SpanId>,
    pub root_span_id: SpanId,
}

// types.rs
#[derive(Clone, Debug)]
pub struct RuntimeEvent {
    pub ctx: SpanContext,
    pub timestamp: web_time::SystemTime,
    pub event: EventKind,
}

#[derive(Clone, Debug)]
pub enum EventKind {
    Function(FunctionEvent),
}

#[derive(Clone, Debug)]
pub enum FunctionEvent {
    Start(FunctionStart),
    End(FunctionEnd),
}

#[derive(Clone, Debug)]
pub struct FunctionStart {
    pub name: String,
    pub args: Vec<BexExternalValue>,
}

#[derive(Clone, Debug)]
pub struct FunctionEnd {
    pub name: String,
    pub result: BexExternalValue,
    pub duration: std::time::Duration,
}
```

**Dependencies:** `uuid`, `web-time`, `bex_external_types`.

#### 0.2 Register in workspace

- Add `baml_events = { path = "crates/baml_events" }` to `baml_language/Cargo.toml` workspace deps.
- Add `baml_events` dependency to `bex_engine/Cargo.toml`.

#### 0.3 Verify

- `cargo build -p baml_events`
- `cargo build -p bex_engine` (with new dep, no code changes yet)

**Files modified:**
| File | Change |
|------|--------|
| `baml_language/Cargo.toml` | Add workspace member + dep |
| `baml_language/crates/baml_events/Cargo.toml` | New |
| `baml_language/crates/baml_events/src/lib.rs` | New |
| `baml_language/crates/baml_events/src/span_id.rs` | New |
| `baml_language/crates/baml_events/src/span_context.rs` | New |
| `baml_language/crates/baml_events/src/types.rs` | New |

---

### Phase 1: VM — `CallWithTrace` + `SpanNotification`

Add the `CallWithTrace` instruction and `SpanNotification` type to the VM. No engine changes yet — just the VM side.

#### 1.1 Add `CallWithTrace(usize)` to `Instruction` enum

**File:** `bex_vm_types/src/bytecode.rs`

- Add variant `CallWithTrace(usize)` after `Call(usize)` (near line 290).
- Add Display case: `Instruction::CallWithTrace(n) => write!(f, "CALL_WITH_TRACE {n}")` (near line 544).

#### 1.2 Add `SpanNotification` enum

**File:** `bex_vm/src/vm.rs`

Add a new enum near `WatchNotification` (line 245):

```rust
/// Notifications yielded by the VM for span tracking.
/// The VM provides args/result from the eval stack; the engine
/// handles span lifecycle and event emission.
#[derive(Clone, Debug)]
pub enum SpanNotification {
    FunctionEnter {
        function_name: String,
        frame_depth: usize,
        args: Vec<Value>,
    },
    FunctionExit {
        function_name: String,
        result: Value,
    },
}
```

#### 1.3 Extend `VmExecState` to carry `SpanNotification`

**File:** `bex_vm/src/vm.rs`

The existing `Notify(WatchNotification)` variant handles watch notifications. We need a way to yield span notifications too. Two options:

**Option A — Extend `WatchNotification`** (minimal change):
Add `SpanNotification(SpanNotification)` variant to `WatchNotification`.

**Option B — Separate `VmExecState` variant** (cleaner):
Add `SpanNotify(SpanNotification)` to `VmExecState`.

**Recommendation:** Option B — keeps span notifications separate from watch notifications. The engine already ignores `Notify(WatchNotification)` (line 833), so a separate variant means we don't need to modify watch handling.

```rust
pub enum VmExecState {
    Await(HeapPtr),
    ScheduleFuture(HeapPtr),
    Complete(Value),
    Notify(WatchNotification),
    SpanNotify(SpanNotification),  // NEW
}
```

#### 1.4 Add VM fields

**File:** `bex_vm/src/vm.rs`

Add to `BexVm` struct (after `interrupt_frame`, line 211):

```rust
/// Kill switch for tracing. When false, CallWithTrace behaves as Call.
pub tracing_enabled: bool,

/// Frame depths pushed via CallWithTrace. Always sorted ascending (LIFO).
traced_frames: Vec<usize>,
```

Initialize in `BexVm::new()` (line 376):
```rust
tracing_enabled: false,
traced_frames: Vec::new(),
```

#### 1.5 Dispatch `CallWithTrace`

**File:** `bex_vm/src/vm.rs`

Add a new match arm in the instruction dispatch (after `Call` at line 2085). The logic mirrors `Call` exactly, plus:

1. Before pushing the frame: if `tracing_enabled`, snapshot args from eval stack.
2. After pushing the frame: push `frame_idx` onto `traced_frames`.
3. Return `VmExecState::SpanNotify(SpanNotification::FunctionEnter { ... })`.
4. If `!tracing_enabled`, fall through to behave identically to `Call`.

**Key detail:** For `FunctionKind::Bytecode` only. Native calls don't push frames and cannot be traced.

```rust
Instruction::CallWithTrace(arg_count) => {
    // Same preamble as Call: locals_offset, callee lookup, arity check, MAX_FRAMES check
    // ...

    match callee.kind {
        FunctionKind::Bytecode => {
            let args = if self.tracing_enabled {
                let locals_offset_raw = locals_offset.into_raw();
                Some(self.stack.slice(locals_offset_raw + 1..self.stack.len()).to_vec())
            } else {
                None
            };

            // Push frame (identical to Call)
            self.frames.push(Frame { function: index, instruction_ptr: 0, locals_offset });
            frame_idx = self.frames.len() - 1;

            if self.tracing_enabled {
                self.traced_frames.push(frame_idx);
                return Ok(VmExecState::SpanNotify(SpanNotification::FunctionEnter {
                    function_name: callee.name.clone(),
                    frame_depth: frame_idx,
                    args: args.unwrap_or_default(),
                }));
            }

            // Tracing disabled — continue as normal Call
            function = self.get_object(self.frames[frame_idx].function)
                .as_function()?.clone();
        }
        FunctionKind::Native(_) => { /* same as Call — no tracing */ }
        FunctionKind::SysOp(_) => { /* same as Call */ }
        FunctionKind::NativeUnresolved => { /* same as Call */ }
    }
}
```

**Important:** After the engine handles `SpanNotify(FunctionEnter)`, it resumes by calling `vm.exec()` again. The VM's IP is already at instruction 0 of the new frame (set in the `Frame` push), so execution continues in the called function.

#### 1.6 Modify `Return` to detect traced frames

**File:** `bex_vm/src/vm.rs` (line 2201)

Insert **before** `self.frames.pop()` (line 2233):

```rust
// Check if this frame was traced
let span_exit = if self.tracing_enabled
    && self.traced_frames.last() == Some(&frame_idx)
{
    let func_name = self.get_object(self.frames[frame_idx].function)
        .as_function()
        .map(|f| f.name.clone())
        .ok();
    self.traced_frames.pop();
    func_name
} else {
    None
};
```

Then after the existing stack drain + push + frames.pop + interrupt check (but before the "normal: continue in caller frame" logic):

```rust
if let Some(name) = span_exit {
    return Ok(VmExecState::SpanNotify(SpanNotification::FunctionExit {
        function_name: name,
        result,
    }));
}
```

**Edge case — interrupt returns:** If `interrupt_frame` triggers `Complete`, we should NOT yield `FunctionExit` because interrupts are not traced. The interrupt check (line 2236) comes before our `FunctionExit` check, so this is naturally handled.

**Edge case — last frame returns `Complete`:** If `self.frames.is_empty()` after pop, we currently return `Complete(result)`. For traced entry-point frames, the engine handles the final `FunctionEnd` emission itself (it wraps `call_function()` with push/pop), so this is also fine.

#### 1.7 Export new types

**File:** `bex_vm/src/lib.rs`

Add `SpanNotification` to public exports.

**File:** `bex_vm_types/src/lib.rs`

Ensure `CallWithTrace` is visible through `Instruction` (already is, since `Instruction` is re-exported).

#### 1.8 Tests

**File:** `bex_vm/tests/` (new test file or extend existing)

Unit tests:
1. `CallWithTrace` yields `FunctionEnter` when `tracing_enabled = true`.
2. `CallWithTrace` acts as `Call` when `tracing_enabled = false`.
3. `Return` yields `FunctionExit` for traced frames.
4. `Return` does NOT yield for untraced frames.
5. Nested traced/untraced calls: correct `traced_frames` behavior.
6. `interrupt()` frames don't interfere with `traced_frames`.
7. Args are correctly captured from eval stack.
8. Result value is correct in `FunctionExit`.

**Files modified:**
| File | Change |
|------|--------|
| `bex_vm_types/src/bytecode.rs` | Add `CallWithTrace(usize)` variant + Display |
| `bex_vm/src/vm.rs` | `SpanNotification` enum, `VmExecState::SpanNotify`, `tracing_enabled`, `traced_frames`, `CallWithTrace` dispatch, `Return` modification |
| `bex_vm/src/lib.rs` | Export `SpanNotification` |

---

### Phase 2: Engine — Span Stack + Event Emission

Handle `SpanNotification` in the engine, maintain a per-invocation span stack, and emit `FunctionStart`/`FunctionEnd` events.

#### 2.1 Define `EngineSpan` and `TracingConfig`

**File:** `bex_engine/src/lib.rs` (or new `bex_engine/src/tracing.rs`)

```rust
use baml_events::{SpanId, SpanContext, RuntimeEvent, EventKind, FunctionEvent, FunctionStart, FunctionEnd};

#[derive(Clone, Debug)]
pub struct EngineSpan {
    pub span_id: SpanId,
    pub label: String,
    pub started_at: web_time::Instant,
    pub frame_depth: usize,
}

pub struct TracingConfig {
    pub enabled: bool,
    pub root_span_id: Option<SpanId>,
    pub parent_span_id: Option<SpanId>,
}
```

#### 2.2 Add `span_context()` helper

Free function (not on `BexEngine`):

```rust
fn span_context(
    span_stack: &[EngineSpan],
    host_parent: Option<&SpanId>,
) -> SpanContext {
    let current = span_stack.last();
    let parent = if span_stack.len() > 1 {
        span_stack.get(span_stack.len() - 2).map(|s| s.span_id.clone())
    } else {
        host_parent.cloned()
    };
    SpanContext {
        span_id: current.map(|s| s.span_id.clone()).unwrap_or_else(SpanId::new),
        parent_span_id: parent,
        root_span_id: span_stack.first().map(|s| s.span_id.clone()).unwrap_or_else(SpanId::new),
    }
}
```

#### 2.3 Add event channel to `call_function()`

**File:** `bex_engine/src/lib.rs`, in `call_function()` (line 538)

Add a new parameter for event output and create the per-invocation span stack:

```rust
pub async fn call_function(
    &self,
    function_name: &str,
    args: Vec<BexExternalValue>,
    tracing_config: Option<TracingConfig>,  // NEW
) -> Result<(BexExternalValue, Vec<RuntimeEvent>), EngineError>  // events returned
```

At the top of `call_function()`, after VM creation:

```rust
let mut span_stack: Vec<EngineSpan> = Vec::new();
let mut events: Vec<RuntimeEvent> = Vec::new();
let config = tracing_config.unwrap_or(TracingConfig {
    enabled: false, root_span_id: None, parent_span_id: None,
});
vm.tracing_enabled = config.enabled;
```

If tracing enabled, push entry-point span and emit `FunctionStart`:

```rust
if config.enabled {
    let span_id = config.root_span_id.clone().unwrap_or_else(SpanId::new);
    span_stack.push(EngineSpan {
        span_id: span_id.clone(),
        label: function_name.to_string(),
        started_at: web_time::Instant::now(),
        frame_depth: 0,
    });
    events.push(RuntimeEvent {
        ctx: SpanContext {
            span_id: span_id.clone(),
            parent_span_id: config.parent_span_id.clone(),
            root_span_id: span_id.clone(),
        },
        timestamp: web_time::SystemTime::now(),
        event: EventKind::Function(FunctionEvent::Start(FunctionStart {
            name: function_name.to_string(),
            args: args.clone(),  // entry-point args already in BexExternalValue form
        })),
    });
}
```

#### 2.4 Handle `SpanNotify` in event loop

**File:** `bex_engine/src/lib.rs`, in `run_event_loop_with_epoch()` (line 686)

Add match arms for `VmExecState::SpanNotify`:

```rust
VmExecState::SpanNotify(SpanNotification::FunctionEnter {
    function_name, frame_depth, args
}) => {
    let span_id = SpanId::new();
    let ctx = span_context(&span_stack, config.parent_span_id.as_ref());

    span_stack.push(EngineSpan {
        span_id: span_id.clone(),
        label: function_name.clone(),
        started_at: web_time::Instant::now(),
        frame_depth,
    });

    // Convert VM args to BexExternalValue for the event
    let ext_args: Vec<BexExternalValue> = args.iter()
        .map(|v| self.vm_arg_to_bex_value(v))
        .collect();

    events.push(RuntimeEvent {
        ctx: SpanContext {
            span_id,
            parent_span_id: Some(ctx.span_id),
            root_span_id: ctx.root_span_id,
        },
        timestamp: web_time::SystemTime::now(),
        event: EventKind::Function(FunctionEvent::Start(FunctionStart {
            name: function_name,
            args: ext_args,
        })),
    });

    // Continue VM execution
}

VmExecState::SpanNotify(SpanNotification::FunctionExit {
    function_name, result
}) => {
    if let Some(span) = span_stack.pop() {
        let duration = span.started_at.elapsed();
        let ctx = span_context(&span_stack, config.parent_span_id.as_ref());
        let ext_result = self.vm_arg_to_bex_value(&result);

        events.push(RuntimeEvent {
            ctx: SpanContext {
                span_id: span.span_id,
                parent_span_id: Some(ctx.span_id),
                root_span_id: ctx.root_span_id,
            },
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                name: function_name,
                result: ext_result,
                duration,
            })),
        });
    }

    // Continue VM execution
}
```

#### 2.5 Emit entry-point `FunctionEnd` on `Complete`

In the `Complete(value)` handler (line 700), after converting the result:

```rust
VmExecState::Complete(value) => {
    let result = self.heap.with_gc_protection(|protected| {
        self.convert_vm_value_to_external_with_type(&value, &return_type, &protected.epoch_guard())
    })?;

    // Pop entry-point span and emit FunctionEnd
    if let Some(span) = span_stack.pop() {
        let duration = span.started_at.elapsed();
        events.push(RuntimeEvent {
            ctx: SpanContext {
                span_id: span.span_id,
                parent_span_id: config.parent_span_id.clone(),
                root_span_id: span_stack.first()
                    .map(|s| s.span_id.clone())
                    .unwrap_or(span.span_id.clone()),
            },
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                name: span.label,
                result: result.clone(),
                duration,
            })),
        });
    }

    return Ok((result, events));
}
```

#### 2.6 Error unwind

Add `emit_unwind_events()` helper. On error paths, unwind the span stack:

```rust
fn emit_unwind_events(
    span_stack: &mut Vec<EngineSpan>,
    error: &EngineError,
    config: &TracingConfig,
    events: &mut Vec<RuntimeEvent>,
) {
    while let Some(span) = span_stack.pop() {
        let duration = span.started_at.elapsed();
        events.push(RuntimeEvent {
            ctx: SpanContext {
                span_id: span.span_id.clone(),
                parent_span_id: span_stack.last()
                    .map(|s| s.span_id.clone())
                    .or(config.parent_span_id.clone()),
                root_span_id: span_stack.first()
                    .map(|s| s.span_id.clone())
                    .unwrap_or(span.span_id.clone()),
            },
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                name: span.label,
                result: BexExternalValue::String(format!("Error: {error}")),
                duration,
            })),
        });
    }
}
```

#### 2.7 Thread `span_stack` and `events` through event loop

The event loop method signature changes:

```rust
async fn run_event_loop_with_epoch(
    &self,
    return_type: Ty,
    vm: &mut BexVm,
    my_epoch: u64,
    span_stack: &mut Vec<EngineSpan>,    // NEW
    events: &mut Vec<RuntimeEvent>,       // NEW
    config: &TracingConfig,               // NEW
) -> Result<BexExternalValue, EngineError>
```

#### 2.8 Update callers

**File:** `bridge_cffi/src/ffi/functions.rs`

`call_function_inner()` (line 53) calls `engine.call_function()`. Update to pass `None` for tracing config initially (tracing will be enabled via bridge_cffi in a future phase):

```rust
let (result, _events) = engine.call_function(function_name, args, None).await?;
```

**File:** `bridge_python/src/runtime.rs`

Same — pass `None` for now.

#### 2.9 Tests

Engine-level integration tests:
1. `call_function` with tracing enabled produces `FunctionStart` + `FunctionEnd` events.
2. Events have correct `SpanContext` (parent/root relationships).
3. Nested `CallWithTrace` produces correct span hierarchy.
4. Error mid-execution produces unwind events.
5. `call_function` with tracing disabled produces no events.

**Files modified:**
| File | Change |
|------|--------|
| `bex_engine/Cargo.toml` | Add `baml_events` dep |
| `bex_engine/src/lib.rs` | `EngineSpan`, `TracingConfig`, `span_context()`, `emit_unwind_events()`, `call_function()` signature change, `SpanNotify` handling in event loop, entry-point span push/pop |
| `bridge_cffi/src/ffi/functions.rs` | Update `call_function()` call |
| `bridge_python/src/runtime.rs` | Update `call_function()` call |

---

### Phase 3: Compiler — Emit `CallWithTrace`

Make the compiler emit `CallWithTrace` at traced call sites.

#### 3.1 LLM function delegation

**File:** `baml_compiler_emit/src/emit.rs`

In `emit_terminator()` for `Terminator::Call` (line 899), the compiler currently always emits `Instruction::Call(args.len())`. Change to emit `CallWithTrace` when the callee is `call_llm_function`:

```rust
Terminator::Call { callee, args, destination, target, unwind: _ } => {
    self.emit_operand_pull(callee, mir);
    for arg in args {
        self.emit_operand_pull(arg, mir);
    }

    let should_trace = self.should_trace_call(callee, mir);
    if should_trace {
        self.emit(Instruction::CallWithTrace(args.len()));
    } else {
        self.emit(Instruction::Call(args.len()));
    }

    self.emit_store_place(destination, mir);
    self.emit_jump_unless_fallthrough(*target);
}
```

#### 3.2 `should_trace_call()` helper

**File:** `baml_compiler_emit/src/emit.rs`

```rust
fn should_trace_call(&self, callee: &Operand, mir: &MirFunction) -> bool {
    // Resolve callee to a function name
    if let Some(name) = self.resolve_callee_name(callee, mir) {
        // Trace LLM function delegation
        if name == "baml.llm.call_llm_function" {
            return true;
        }
        // Future: check @trace annotation on the called function
    }
    false
}
```

The exact implementation of `resolve_callee_name` depends on how `Operand` resolves to a qualified name. The callee is typically a `LoadGlobal` of a function, so we can look up the global index in the globals map.

#### 3.3 Handle `CallWithTrace` in disassembler/debug tools

Anywhere that pattern-matches on `Instruction` exhaustively will need a `CallWithTrace` arm. Search for `Instruction::Call` matches across the codebase:

- `bex_vm/src/vm.rs` — instruction dispatch (Phase 1 already handles)
- `baml_compiler_emit/src/emit.rs` — Display/debug (handled by `bytecode.rs` Display impl)
- Any test helpers that construct/match instructions

#### 3.4 Tests

Compiler-level tests:
1. LLM function bodies emit `CallWithTrace` for `call_llm_function`.
2. Non-LLM function calls still emit `Call`.
3. Verify disassembly output includes `CALL_WITH_TRACE`.

**Files modified:**
| File | Change |
|------|--------|
| `baml_compiler_emit/src/emit.rs` | `should_trace_call()`, conditional `CallWithTrace` emission |
| `baml_compiler_emit/Cargo.toml` | (only if new deps needed) |

---

### Phase 4: Bridge Integration — Enable Tracing from Host

Wire tracing config from host languages through the bridge layer so callers can enable tracing and receive events.

#### 4.1 `bridge_cffi` — Pass `TracingConfig`

**File:** `bridge_cffi/src/ffi/functions.rs`

In `call_function_inner()`, construct a `TracingConfig` and pass it to the engine:

```rust
let tracing_config = TracingConfig {
    enabled: true,  // or from caller config
    root_span_id: Some(SpanId::new()),
    parent_span_id: None,  // or from HostSpanContext
};

let (result, events) = engine.call_function(
    &function_name, bex_args, Some(tracing_config),
).await?;

// For now: write events to JSONL trace file (same format as old system)
write_trace_events(&events, &trace_file_path)?;
```

#### 4.2 `bridge_python` — Pass `TracingConfig`

**File:** `bridge_python/src/runtime.rs`

Update `call_function` / `call_function_sync` to optionally accept tracing config and return events alongside the result.

#### 4.3 Trace file output

Write events to a JSONL file compatible with the existing trace format. This lets the `bridge_python` tests in `test_tracing.py` (the xfail tests) start passing.

**Files modified:**
| File | Change |
|------|--------|
| `bridge_cffi/src/ffi/functions.rs` | Construct `TracingConfig`, handle returned events |
| `bridge_python/src/runtime.rs` | Forward tracing config, expose events |

---

## Dependency Graph

```
Phase 0: baml_events crate (types only)
    │
    ├──▶ Phase 1: VM CallWithTrace + SpanNotification  (no deps on Phase 0)
    │        │
    │        ▼
    └──▶ Phase 2: Engine span stack + event emission  (needs Phase 0 + Phase 1)
             │
             ▼
         Phase 3: Compiler emits CallWithTrace  (needs Phase 1 for instruction)
             │
             ▼
         Phase 4: Bridge integration  (needs Phase 2 + Phase 3)
```

Phase 0 and Phase 1 can be done in parallel. Phase 2 requires both. Phase 3 only requires Phase 1 (the instruction definition). Phase 4 requires Phase 2 + Phase 3.

---

## Verification Checkpoints

| Checkpoint | How to verify |
|---|---|
| Phase 0 complete | `cargo build -p baml_events` succeeds |
| Phase 1 complete | `cargo test -p bex_vm` — new tests for `CallWithTrace`/`Return` pass |
| Phase 2 complete | `cargo test -p bex_engine` — events returned from `call_function()` with correct spans |
| Phase 3 complete | `cargo test -p baml_compiler_emit` — LLM calls produce `CALL_WITH_TRACE` in disassembly |
| Phase 4 complete | `cd baml_language/crates/bridge_python && uv run pytest tests/test_tracing.py -v` — xfail tests for event recording start passing |
| Full stack | Run bridge_python tests with tracing enabled, verify JSONL output has correct `function_start`/`function_end` events with parent-child relationships |

---

## What's NOT in This Plan

| Item | Why deferred |
|---|---|
| Global `EventStore` singleton | Not needed until collectors/publisher exist |
| `baml_collector` crate | Not needed until host languages query events |
| `baml_publisher` crate (S3/Boundary) | Separate milestone |
| `SysOp::EventSend` / `baml.events.send()` | Not needed — engine emits function events directly from `CallWithTrace` notifications |
| `@trace` / `@notrace` annotations | Compiler support can be added incrementally after Phase 3 |
| `StartSpan` / `EndSpan` instructions | Future: sub-function spans |
| Streaming support | `bex_engine` doesn't support streaming yet |
| `HostSpanManager` real implementation | Depends on full event system |
| Header events (`VizEnter`/`VizExit`) | Separate from callstack tracking |
