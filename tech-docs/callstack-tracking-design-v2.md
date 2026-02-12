# Call Stack & Span Tracking in `baml_language` — Design Document v2

> Spans are an engine concern. The VM has `CallWithTrace` for function-level
> spans and (future) `StartSpan`/`EndSpan` for arbitrary regions.
> The engine owns the span stack, generates IDs, records timing, and
> emits events.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [Why v2: Lessons from the Frame-Enrichment Approach](#2-why-v2-lessons-from-the-frame-enrichment-approach)
3. [Architecture Overview](#3-architecture-overview)
4. [VM Side: CallWithTrace + Return Detection](#4-vm-side-callwithtrace--return-detection)
5. [Engine Side: Span Stack and Event Emission](#5-engine-side-span-stack-and-event-emission)
6. [Integration with Event Publishing (FunctionStart/FunctionEnd)](#6-integration-with-event-publishing-functionstartfunctionend)
7. [Compiler-Driven Per-Function Control](#7-compiler-driven-per-function-control)
8. [Arbitrary (Non-Function) Spans (Future)](#8-arbitrary-non-function-spans-future)
9. [Exception Call Stacks](#9-exception-call-stacks)
10. [Detailed Flows](#10-detailed-flows)
11. [Python-Side Callstack Initialization](#11-python-side-callstack-initialization)
12. [Data Structures](#12-data-structures)
13. [Implementation Plan](#13-implementation-plan)
14. [Design Alternatives Considered](#14-design-alternatives-considered)
15. [Open Questions](#15-open-questions)

---

## 1. Introduction

**What**: A design for how `baml_language` tracks observability spans
using a clear separation between the VM (which executes bytecode) and
the engine (which owns observability).

**Why**: The [v1 design](./callstack-tracking-design.md) proposed
enriching the VM's `Frame` struct with `span_id` and `started_at` fields.
After evaluation, we identified fundamental problems (Section 2). This
v2 design draws a hard line:

- **The VM knows nothing about spans.** It has one new instruction
  (`CallWithTrace`) and a tiny side-table (`traced_frames: Vec<usize>`)
  to detect traced returns. No `SpanId`, no `Instant`, no span stack.
- **The engine owns span lifecycle.** It maintains the span stack,
  generates IDs, records timing, and emits events. All observability
  logic lives in one place.
- **Function-level spans piggyback on Call/Return.** The compiler emits
  `CallWithTrace` at call sites for traced functions. The VM yields a
  notification on entry (with args from the eval stack) and on return
  (with the return value). The engine emits `FunctionStart`/`FunctionEnd`
  events directly — no `baml.events.send()` needed for span tracking.
  No wrapping `SpanEnter`/`SpanExit` instructions around the function body.
- **Arbitrary spans are a future extension.** `StartSpan`/`EndSpan`
  instructions can be added later for sub-function regions, retries,
  streaming, etc. — using the same engine span stack.

**Scope**: `bex_vm`, `bex_vm_types`, `bex_engine`, `baml_compiler_emit`.

**Relationship to other docs**:
- [event-publishing-design-v2.md](./event-publishing-design-v2.md) — the
  event system. This document replaces the `SpanStack` proposed in M4 of
  that document with an engine-owned span stack driven by `CallWithTrace`.
- [callstack-tracking-design.md](./callstack-tracking-design.md) — the v1
  design (frame-enrichment). This document supersedes it.

---

## 2. Why v2: Lessons from the Frame-Enrichment Approach

The v1 design proposed adding `span_id: Option<SpanId>` and
`started_at: Option<Instant>` directly to `struct Frame`. Five problems:

### 2.1 Frame Size and Cache Pressure

`Frame` is 24 bytes (`HeapPtr` + `isize` + `StackIndex`), fits in one
cache line, and is `Copy`. Adding `Option<Uuid>` (17 bytes) +
`Option<Instant>` (16 bytes) nearly triples it to ~57 bytes. The VM's
hot loop reads `frames[frame_idx].instruction_ptr` on every instruction
dispatch.

### 2.2 Coupling the VM to the Observability System

`SpanId` on `Frame` creates a compile-time dependency from the VM to
the event system's ID type. If we change span identification later, we
change `Frame`.

### 2.3 Cannot Represent Non-Function Spans

Spans on `Frame` can only start on `Call` and end on `Return`. Sub-function
regions, retry attempts, streaming chunks, and user-defined `span("x") { }`
blocks all require spans without corresponding frames.

### 2.4 Per-Function Decisions on the Hot Path

The v1 design's `should_track_call()` was a multi-step priority chain
evaluated on every `Instruction::Call`. With `CallWithTrace`, per-function
control is a compile-time decision at the call site — the compiler emits
`CallWithTrace` or `Call`. Zero runtime decision cost.

### 2.5 The VM Already Uses Side-Tables for Optional Metadata

The watch system uses `watched_vars: HashMap<StackIndex, ...>` — a
side-table alongside the eval stack, not inline on `Value`. The same
principle applies: optional span metadata belongs next to the call stack,
not on `Frame`.

---

## 3. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                               v2 Architecture                                    │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐       │
│  │                          VM (bex_vm)                                   │       │
│  │                                                                       │       │
│  │  frames: Vec<Frame>          ← UNTOUCHED. 24 bytes, Copy.            │       │
│  │  stack: EvalStack            ← UNTOUCHED.                            │       │
│  │  watched_vars: HashMap       ← UNTOUCHED.                            │       │
│  │  watch: Watch                ← UNTOUCHED.                            │       │
│  │                                                                       │       │
│  │  tracing_enabled: bool       ← kill switch (one branch)              │       │
│  │  traced_frames: Vec<usize>   ← side-table of traced frame depths     │       │
│  │                                                                       │       │
│  │  Instructions:                                                        │       │
│  │    Call(n)          → push frame, continue (unchanged)                │       │
│  │    CallWithTrace(n) → push frame, mark depth, yield FunctionEnter    │       │
│  │    Return           → pop frame. If traced depth → yield FunctionExit│       │
│  │                                                                       │       │
│  └────────────┬──────────────────────────────────────────────────────────┘       │
│               │                                                                  │
│               │  VmExecState::Notify(SpanNotification)                           │
│               │  VmExecState::ScheduleFuture / Await / Complete                  │
│               ▼                                                                  │
│  ┌───────────────────────────────────────────────────────────────────────┐       │
│  │                        Engine (bex_engine)                             │       │
│  │                                                                       │       │
│  │  BexEngine is a shared Arc<BexEngine> singleton — immutable.          │       │
│  │  All per-call mutable state lives in call_function() locals:          │       │
│  │                                                                       │       │
│  │  call_function() creates per-invocation:                              │       │
│  │    span_stack: Vec<EngineSpan>  ← per-call, NOT on BexEngine          │       │
│  │    vm: BexVm                    ← per-call (already exists)           │       │
│  │                                                                       │       │
│  │  Entry-point function:                                                │       │
│  │    Engine pushes span BEFORE calling exec()                           │       │
│  │    Engine emits FunctionStart (with args) directly                    │       │
│  │    Engine pops span AFTER Complete                                    │       │
│  │    Engine emits FunctionEnd (with result) directly                    │       │
│  │                                                                       │       │
│  │  On Notify(FunctionEnter { args }):                                   │       │
│  │    Generate SpanId, record Instant::now(), push span_stack            │       │
│  │    Emit FunctionStart event (args provided by VM from eval stack)     │       │
│  │                                                                       │       │
│  │  On Notify(FunctionExit { result }):                                  │       │
│  │    Pop span_stack, compute duration                                   │       │
│  │    Emit FunctionEnd event (result provided by VM from eval stack)     │       │
│  │                                                                       │       │
│  │  On error:                                                            │       │
│  │    stack_trace from vm.frames (unchanged)                             │       │
│  │    Unwind span_stack, emit SpanEnd(Err) for each                      │       │
│  │                                                                       │       │
│  └───────────────────────────────────────────────────────────────────────┘       │
│                                                                                  │
│  Key properties:                                                                 │
│  • Frame untouched (24 bytes, Copy)                                              │
│  • VM has zero span state (no SpanId, no Instant)                                │
│  • Only new VM state: traced_frames (Vec<usize> of frame depths)                 │
│  • All observability logic in engine                                             │
│  • BexEngine is immutable/shared — span_stack is per-invocation local            │
│  • Concurrent call_function() invocations have isolated span stacks              │
│  • CallWithTrace = one instruction instead of SpanEnter + body + SpanExit        │
│  • Per-function control is compile-time (CallWithTrace vs Call)                  │
│  • Future: StartSpan/EndSpan for arbitrary regions                               │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### Design Principles

**Principle 1: The VM is a pure execution engine.**
It executes bytecode and manages frames. `CallWithTrace` is just a `Call`
variant that yields a notification. The VM doesn't know what happens with
that notification.

**Principle 2: The engine owns observability.**
SpanId generation, timing, the span stack, SpanContext derivation, event
emission, and error unwind — all in the engine.

**Principle 3: The caller decides what's traced.**
The compiler emits `CallWithTrace` at the call site. The called function's
bytecode is identical whether it's traced or not.

**Principle 4: Function spans piggyback on frames. Arbitrary spans are separate.**
`CallWithTrace` uses the natural frame push/pop lifecycle. Future
`StartSpan`/`EndSpan` instructions handle non-function regions.

---

## 4. VM Side: CallWithTrace + Return Detection

### 4.1 New Instruction

```rust
pub enum Instruction {
    // ... existing instructions ...

    /// Call a function AND notify the engine that this call is traced.
    ///
    /// Behaves exactly like Call(n): pushes a frame, sets up locals.
    /// Additionally:
    ///   1. Records the new frame's depth in traced_frames
    ///   2. Yields SpanNotification::FunctionEnter to the engine
    ///
    /// When tracing_enabled is false, behaves identically to Call(n).
    CallWithTrace(usize),
}
```

### 4.2 VM State: traced_frames Side-Table

Following the `watched_vars` pattern, the VM stores a small side-table
of which frame depths are traced:

```rust
pub struct BexVm {
    // Existing (ALL unchanged)
    pub frames: Vec<Frame>,
    pub stack: EvalStack,
    pub heap: Arc<BexHeap>,
    pub tlab: Tlab,
    pub globals: GlobalPool,
    pub watch: Watch,
    pub watched_vars: HashMap<StackIndex, (String, String)>,
    pub interrupt_frame: Option<usize>,
    // ...

    // NEW
    /// Kill switch for tracing. When false, CallWithTrace = Call.
    pub tracing_enabled: bool,

    /// Frame depths that were pushed via CallWithTrace.
    /// Always sorted ascending (frames are LIFO).
    /// Checked on Return to yield FunctionExit notifications.
    traced_frames: Vec<usize>,
}
```

### 4.3 Dispatch: CallWithTrace

```rust
Instruction::CallWithTrace(arg_count) => {
    // ── All the same logic as Call(arg_count) ──
    let locals_offset = self.stack.len() - arg_count;
    let callee_value = self.stack[locals_offset - 1];
    let callee_ptr = callee_value.as_object()
        .ok_or(RuntimeError::NotCallable(callee_value))?;
    let function = self.get_object(callee_ptr).as_function()?;

    // Arity check, MAX_FRAMES check...
    // (identical to Call)

    match function.kind {
        FunctionKind::Bytecode { .. } => {
            // ── NEW: snapshot args from the eval stack BEFORE pushing frame ──
            let args = if self.tracing_enabled {
                Some(self.stack[locals_offset..].to_vec())
            } else {
                None
            };

            self.frames.push(Frame {
                function: callee_ptr,
                instruction_ptr: 0,
                locals_offset: StackIndex::from_raw(locals_offset),
            });
            frame_idx = self.frames.len() - 1;

            // ── NEW: trace this frame ──
            if self.tracing_enabled {
                self.traced_frames.push(frame_idx);
                return Ok(VmExecState::Notify(SpanNotification::FunctionEnter {
                    function_name: function.name.clone(),
                    frame_depth: frame_idx,
                    args: args.unwrap_or_default(),
                }));
            }
            // If tracing disabled, just continue (it's a normal Call)
        }

        FunctionKind::Native(func_ptr) => {
            // Native calls: no frame push, same as Call.
            // Cannot be traced (no frame to track).
            let result = func_ptr(self, &args)?;
            self.stack.drain(locals_offset..);
            self.stack.push(result);
        }

        FunctionKind::SysOp(_) => {
            return Err(VmError::Internal(InternalError::SysOpDirectCall));
        }
    }
}
```

### 4.4 Dispatch: Return (Modified)

```rust
Instruction::Return => {
    let frame_depth = self.frames.len() - 1;

    // ── Normal Return logic (unchanged) ──
    let result = self.stack.ensure_pop()?;

    // Watch cleanup: unregister watched vars in this frame's scope
    let drain_start = self.frames[frame_depth].locals_offset;
    for i in drain_start.into_raw()..self.stack.len() {
        let index = StackIndex::from_raw(i);
        if self.watched_vars.remove(&index).is_some() {
            let var_node = NodeId::LocalVar(index);
            self.watch.unregister_root(var_node);
            // ... unlink edges ...
        }
    }

    // Capture function name before popping (for notification)
    let function_name = if self.tracing_enabled
        && self.traced_frames.last() == Some(&frame_depth)
    {
        let function = self.get_object(self.frames[frame_depth].function)
            .as_function()
            .map(|f| f.name.clone())
            .ok();
        self.traced_frames.pop();
        function
    } else {
        None
    };

    // Drain stack, push result, pop frame
    self.stack.drain(drain_start..);
    self.stack.push(result);
    self.frames.pop();

    // Check interrupt
    if let Some(interrupt_depth) = self.interrupt_frame {
        if self.frames.len() == interrupt_depth {
            self.interrupt_frame = None;
            return Ok(VmExecState::Complete(result));
        }
    }

    // ── NEW: yield FunctionExit for traced frames (with result value) ──
    if let Some(name) = function_name {
        return Ok(VmExecState::Notify(SpanNotification::FunctionExit {
            function_name: name,
            result,  // ← return value from the eval stack
        }));
    }

    // Normal: continue in caller frame
    if self.frames.is_empty() {
        return Ok(VmExecState::Complete(result));
    }
    frame_idx = self.frames.len() - 1;
}
```

### 4.5 Why traced_frames Works as a Vec (Stack)

Frame depths are always pushed in increasing order (each new frame is
deeper) and popped in decreasing order (LIFO). The `traced_frames` Vec
is naturally sorted, so `last() == Some(&current_depth)` is a correct
O(1) check:

```
Call main (traced by engine, not in traced_frames)
  Call helper (not traced)                  traced_frames: []
    CallWithTrace call_llm → push depth 2  traced_frames: [2]
    Return depth 2 → last()==2 → pop, yield traced_frames: []
  Return depth 1 → last()==None → continue  traced_frames: []
Return depth 0 → (engine handles)
```

Non-contiguous traced depths also work:

```
CallWithTrace a → push 1                   traced_frames: [1]
  Call b (not traced)                       traced_frames: [1]
    CallWithTrace c → push 3               traced_frames: [1, 3]
    Return depth 3 → last()==3 → pop, yield traced_frames: [1]
  Return depth 2 → last()==1 → continue     traced_frames: [1]
Return depth 1 → last()==1 → pop, yield     traced_frames: []
```

### 4.6 Notification Types

```rust
/// Span notifications yielded by the VM. Stateless — no SpanId, no timing.
/// The VM provides args and result values from the eval stack so the engine
/// can emit FunctionStart/FunctionEnd events without parsing event payloads.
#[derive(Clone, Debug)]
pub enum SpanNotification {
    /// A traced function call was entered (via CallWithTrace).
    /// `args` are snapshotted from the eval stack before the frame is pushed.
    FunctionEnter {
        function_name: String,
        frame_depth: usize,
        args: Vec<Value>,
    },
    /// A traced function call is returning.
    /// `result` is the return value popped from the eval stack.
    FunctionExit {
        function_name: String,
        result: Value,
    },
}
```

### 4.7 What the VM Does NOT Do

| Concern | VM's role |
|---|---|
| Generate SpanId | No — engine does this |
| Record start time | No — engine does this |
| Maintain span stack | No — engine does this |
| Know current span context | No — engine knows this |
| Emit events | No — engine does this |
| Decide which functions to trace | No — compiler decided (CallWithTrace vs Call) |

---

## 5. Engine Side: Span Stack and Event Emission

### 5.1 Engine Span Stack (Per-Invocation)

**Critical**: `BexEngine` is a shared `Arc<BexEngine>` singleton with
`&self` methods — it **cannot** hold per-call mutable state. The
`span_stack` is a local variable created inside `call_function()`,
just like the `BexVm`:

```rust
// BexEngine is IMMUTABLE and SHARED across concurrent calls.
// It holds only read-only compile-time data (heap, globals, function index).
pub struct BexEngine {
    heap: Arc<BexHeap>,
    globals: GlobalPool,
    resolved_function_names: HashMap<String, (HeapPtr, FunctionKind)>,
    // ... NO span_stack here.
}

// The span stack is created per-invocation in call_function().
// Each concurrent call gets its own isolated span stack.

#[derive(Clone, Debug)]
pub struct EngineSpan {
    pub span_id: SpanId,
    pub label: String,
    pub started_at: web_time::Instant,
    pub frame_depth: usize,
}
```

### 5.2 SpanContext Derivation

A free function (or associated function on `BexEngine`) that takes the
span stack by reference — no `self` state needed:

```rust
/// Derive SpanContext from a per-invocation span stack.
fn span_context(
    span_stack: &[EngineSpan],
    host_parent: Option<&SpanId>,
) -> SpanContext {
    let current = span_stack.last();
    let parent = if span_stack.len() > 1 {
        span_stack.get(span_stack.len() - 2)
    } else {
        None
    };

    SpanContext {
        span_id: current
            .map(|s| s.span_id.clone())
            .unwrap_or_else(SpanId::new),
        parent_span_id: parent
            .map(|s| s.span_id.clone())
            .or_else(|| host_parent.cloned()),
        root_span_id: span_stack.first()
            .map(|s| s.span_id.clone())
            .unwrap_or_else(SpanId::new),
    }
}
```

### 5.3 Engine Event Loop

The event loop takes the span stack as a `&mut` parameter — the stack
is created in `call_function()` and threaded through:

```rust
impl BexEngine {
    async fn run_event_loop(
        &self,
        vm: &mut BexVm,
        span_stack: &mut Vec<EngineSpan>,  // per-invocation, NOT on self
        config: &TracingConfig,
    ) -> Result<BexValue, EngineError> {
        vm.tracing_enabled = config.enabled;

        loop {
            match vm.exec()? {
                VmExecState::Notify(SpanNotification::FunctionEnter {
                    function_name, frame_depth, args
                }) => {
                    let span_id = SpanId::new();
                    let ctx = span_context(
                        span_stack,
                        config.parent_span_id.as_ref(),
                    );

                    span_stack.push(EngineSpan {
                        span_id: span_id.clone(),
                        label: function_name.clone(),
                        started_at: web_time::Instant::now(),
                        frame_depth,
                    });

                    // ── Emit FunctionStart directly from the notification ──
                    // Args come from the VM's eval stack (snapshotted in
                    // CallWithTrace before the frame was pushed).
                    let span_ctx = SpanContext {
                        span_id,
                        parent_span_id: ctx.span_id.into(),
                        root_span_id: ctx.root_span_id,
                    };
                    event_bus::emit(RuntimeEvent {
                        ctx: span_ctx,
                        timestamp: web_time::SystemTime::now(),
                        event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                            name: function_name,
                            args: self.values_to_external(args),
                            is_stream: false,
                        })),
                    });
                }

                VmExecState::Notify(SpanNotification::FunctionExit {
                    function_name, result
                }) => {
                    if let Some(span) = span_stack.pop() {
                        let duration = span.started_at.elapsed();
                        let ctx = span_context(
                            span_stack,
                            config.parent_span_id.as_ref(),
                        );

                        // ── Emit FunctionEnd directly from the notification ──
                        // Result comes from the VM's eval stack (the return
                        // value popped during Return instruction processing).
                        event_bus::emit(RuntimeEvent {
                            ctx: SpanContext {
                                span_id: span.span_id,
                                parent_span_id: ctx.span_id.into(),
                                root_span_id: ctx.root_span_id,
                            },
                            timestamp: web_time::SystemTime::now(),
                            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                                name: function_name,
                                result: self.value_to_external(result),
                                duration,
                            })),
                        });
                    }
                }

                VmExecState::ScheduleFuture(id) => {
                    let pending = vm.pending_future(id)?;
                    // ... existing SysOp dispatch ...
                }

                VmExecState::Complete(value) => {
                    return Ok(self.value_to_external(value));
                }

                // ... Await, other Notify handlers ...
            }
        }
    }
}
```

---

## 6. Integration with Event Publishing (FunctionStart/FunctionEnd)

This is how `CallWithTrace` integrates with the event system described in
[event-publishing-design-v2.md](./event-publishing-design-v2.md).

### 6.1 One Event Source: The Engine

All `FunctionStart`/`FunctionEnd` events are emitted by the engine in
response to VM notifications. There is no need for `baml.events.send()`
to emit function-level span events — the VM's `CallWithTrace`/`Return`
instructions provide everything the engine needs:

| Notification | Engine action | Data source |
|---|---|---|
| `FunctionEnter { name, args }` | Push span, emit `FunctionStart` | `args` from VM eval stack |
| `FunctionExit { name, result }` | Pop span, emit `FunctionEnd` | `result` from VM eval stack |
| Entry-point function enter | Push span, emit `FunctionStart` | `args` from `call_function()` params |
| Entry-point function exit (`Complete`) | Pop span, emit `FunctionEnd` | `result` from `Complete(value)` |

This is simpler than the event-publishing-design-v2's original M4 approach,
which proposed having functions self-emit via `baml.events.send("function_start")`
and the engine parsing those event payloads to push/pop the span stack.
With `CallWithTrace`, all function-level observability is driven by the
VM's natural Call/Return lifecycle.

`SysOp::EventSend` still exists for **non-function events** (e.g.,
`LlmRequest`, `LlmResponse`, intermediate events), and the engine still
attaches the correct `SpanContext` from the per-invocation `span_stack`
to those events.

### 6.2 End-to-End Flow

```
Python: result = await b.ExtractResume(text)
  │
  ▼
Engine::call_function("ExtractResume", args, tracing_config):
  │
  │  // 0. Create per-invocation span stack (local variable, NOT on BexEngine)
  │  let mut span_stack: Vec<EngineSpan> = Vec::new();
  │
  │  // 1. Engine pushes span for the entry-point function
  │  span_stack.push(EngineSpan { span_id: A, label: "ExtractResume", ... })
  │  emit FunctionStart { name: "ExtractResume", args, ctx: {span:A, parent:host} }
  │
  │  // 2. Start VM execution — span_stack passed by &mut to run_event_loop
  │  run_event_loop(&mut vm, &mut span_stack, &config)
  │    │
  │    │  ExtractResume bytecode:
  │    │    LOAD_GLOBAL "call_llm_function"
  │    │    LOAD_CONST "ExtractResume"
  │    │    LOAD_VAR text
  │    │    CallWithTrace 2
  │    │    ── yield FunctionEnter("call_llm_function", depth=1, args=[...]) ──
  │    │
  │    │  // 3. Engine pushes span AND emits FunctionStart for the LLM function
  │    │  span_stack.push(EngineSpan { span_id: B, label: "call_llm_function", ... })
  │    │  emit FunctionStart { name: "call_llm_function", args, ctx: {span:B, parent:A} }
  │    │  // span_stack is now [A, B]
  │    │  vm.exec()
  │    │    │
  │    │    │  call_llm_function bytecode (from llm.baml):
  │    │    │    ... render prompt, build request ...
  │    │    │    baml.http.send(request)
  │    │    │    ── yield ScheduleFuture(SysOp::HttpSend) ──
  │    │    │
  │    │    │  // 4. Engine attaches span context to HTTP call
  │    │    │  ctx = span_context(&span_stack, ...)
  │    │    │  // ctx = { span: B, parent: A, root: A }  ✅ CORRECT
  │    │    │
  │    │    │    ... parse response ...
  │    │    │
  │    │    │  RETURN (with result on eval stack)
  │    │    │  ── traced frame (depth 1) → yield FunctionExit("call_llm_function", result) ──
  │    │
  │    │  // 5. Engine pops span AND emits FunctionEnd for call_llm_function
  │    │  span_stack.pop()  // span_stack is now [A]
  │    │  emit FunctionEnd { name: "call_llm_function", result, ctx: {span:B, parent:A} }
  │    │  vm.exec()
  │    │
  │    │  ... ExtractResume continues, RETURN ...
  │    │  ── Complete(result) ──
  │
  │  // 6. Engine pops span for ExtractResume
  │  span_stack.pop()  // span_stack is now []
  │  emit FunctionEnd { name: "ExtractResume", result, ctx: {span:A, parent:host} }
  │
  │  // span_stack is dropped — per-invocation lifecycle complete
```

### 6.3 Why the Span Context Is Always Correct

The critical invariant: **the engine's span stack is updated and
FunctionStart is emitted BEFORE the function body starts executing.**

1. For the entry-point function: the engine pushes a span and emits
   FunctionStart before calling `exec()`.
2. For `CallWithTrace` calls: the VM pushes the frame, then yields
   `FunctionEnter` with args. The engine pushes a span and emits
   FunctionStart before resuming `exec()`.

And symmetrically for `FunctionEnd`:

1. For `Return` on traced frames: the VM yields `FunctionExit` with the
   return value. The engine pops the span and emits FunctionEnd.
2. For the entry-point function: the engine pops the span and emits
   FunctionEnd after `Complete`.

### 6.4 Why This Is Better Than Self-Emitting Functions

The previous design had functions self-emit via `baml.events.send()`:

| Previous (self-emit) | Current (engine-driven) |
|---|---|
| Function bytecode includes `baml.events.send("function_start", ...)` | No `baml.events.send()` needed for function spans |
| Engine parses event payloads to detect push/pop | Engine uses explicit VM notifications |
| Span pushed AFTER function_start event emitted | Span pushed and FunctionStart emitted BEFORE function body |
| Must duplicate function name/args in bytecode constants | Args come from the VM eval stack (zero-copy snapshot) |
| Only works for functions with `baml.events.send()` | Works for ANY function called via `CallWithTrace` |
| Two event sources (engine for entry-point, bytecode for inner) | One event source (engine handles all) |

Everything else in M4 (SpanContext structure, root_span_id, parent_of_root
for host @trace) remains the same.

---

## 7. Compiler-Driven Per-Function Control

### 7.1 Compile-Time Decision

The compiler decides at the **call site** whether to emit `Call` or
`CallWithTrace`:

```
┌──────────────────────────────────────────────────────────────────────────┐
│                 Compiler Tracing Decision (Call Site)                     │
├──────────────────────────────────────────────────────────────────────────┤
│                                                                          │
│  Source:                                                                 │
│    function ExtractResume(text: string) -> Resume {                      │
│      client "openai/gpt-4o"                                              │
│      prompt #"..."#                                                      │
│    }                                                                     │
│                                                                          │
│  Compiled bytecode for ExtractResume:                                    │
│    // Delegates to call_llm_function (from event-pub-v2 M3)             │
│    LOAD_GLOBAL "call_llm_function"                                       │
│    LOAD_CONST "ExtractResume"                                            │
│    LOAD_VAR text                                                         │
│    CallWithTrace 2     ← traced: call_llm_function is an LLM boundary  │
│    RETURN                                                                │
│                                                                          │
│  Compiled bytecode for a helper function:                                │
│    LOAD_GLOBAL "format_prompt"                                           │
│    LOAD_VAR text                                                         │
│    Call 1              ← NOT traced: internal helper                     │
│    RETURN                                                                │
│                                                                          │
│  The CALLED function's bytecode is IDENTICAL in both cases.              │
│  The CALLER decides.                                                     │
│                                                                          │
└──────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Default Tracing Decisions

| Call site | Instruction | Why |
|---|---|---|
| Calling an LLM function (`call_llm_function`) | `CallWithTrace` | LLM boundary is always interesting |
| Calling a `@trace`-annotated function | `CallWithTrace` | Explicit opt-in |
| Calling internal helpers | `Call` | Not interesting by default |
| Calling builtins (native) | `Call` | Native calls have no frame |
| Calling a `@notrace`-annotated function | `Call` | Explicit opt-out |

### 7.3 Runtime Kill Switch

```rust
pub tracing_enabled: bool
```

When `false`, `CallWithTrace` behaves identically to `Call` — one branch,
no yield, no `traced_frames` push. Set by the engine per invocation.

### 7.4 No `TracePolicy`, No `TracingPolicy`, No `should_track_call()`

The compiler decides. The runtime has one boolean. That's it.

---

## 8. Arbitrary (Non-Function) Spans (Future)

`CallWithTrace` handles function-level spans. For non-function regions,
future `StartSpan`/`EndSpan` instructions will use the same engine span
stack:

### 8.1 Sub-Function Regions (Future)

```baml
// In llm.baml — sub-function spans for LLM call phases:
// Note: FunctionStart/FunctionEnd are emitted by the engine via
// CallWithTrace notifications (Section 6). Sub-function spans use
// start_span/end_span for finer-grained observability:
function call_llm_function(function_name: string, args: map<string, unknown>) -> string {
    start_span("assemble_prompt")
    let prompt = ...
    end_span()

    start_span("http_request")
    let response = baml.http.send(request)
    end_span()

    start_span("parse_response")
    let result = primitive_client.parse(response)
    end_span()

    return result
}
```

These would compile to `StartSpan(idx)`/`EndSpan` instructions that
yield to the engine, which pushes/pops its span stack — same mechanism.

### 8.2 User-Defined Spans (Future)

```baml
function my_pipeline(text: string) -> Result {
    span("preprocessing") {
        let cleaned = clean_text(text)
    }
    span("inference") {
        let result = ExtractResume(cleaned)
    }
    return result
}
```

### 8.3 Two Mechanisms, One Span Stack

| Mechanism | Traces | Anchor | Instructions |
|---|---|---|---|
| Entry-point | Top-level function | Engine wraps `call_function()` | None (engine-side) |
| `CallWithTrace` | Specific function calls | Frame push/pop | `CallWithTrace(n)` + Return detection |
| `StartSpan`/`EndSpan` (future) | Arbitrary code regions | Explicit bytecode pairs | `StartSpan(idx)` + `EndSpan` |

All three push/pop the **same engine span stack**. SpanContext is always
derived from that one stack.

---

## 9. Exception Call Stacks

### 9.1 Exception Traces from Frames (Unchanged)

`stack_trace()` reads `self.frames`. Completely unchanged:

```rust
pub fn stack_trace(&self, error: VmError) -> StackTrace {
    let trace = self.frames.iter().map(|frame| {
        let function = self.get_object(frame.function).as_function()?.clone();
        let last_ip = frame.instruction_ptr.saturating_sub(1);
        Ok(ErrorLocation {
            function_name: function.name.clone(),
            function_span: function.span,
            error_line: function.bytecode.source_lines[last_ip as usize],
        })
    }).collect::<Result<Vec<_>, VmError>>()
      .unwrap_or_default();
    StackTrace { error, trace }
}
```

### 9.2 Engine Enriches with Span Context

```rust
pub struct EnrichedError {
    pub stack_trace: StackTrace,        // from VM frames (always complete)
    pub active_spans: Vec<EngineSpan>,  // from engine span_stack
    pub root_span_id: Option<SpanId>,
}
```

### 9.3 Error Unwind

When an error occurs, the engine unwinds its own span stack. The VM's
`traced_frames` is also stale (frames were never popped), but that's
fine — the engine doesn't read it. The engine just pops its own stack:

```rust
/// Unwinds the per-invocation span stack on error.
/// Takes span_stack by &mut — same local variable from call_function().
fn emit_unwind_events(
    span_stack: &mut Vec<EngineSpan>,
    error: &VmError,
    config: &TracingConfig,
) {
    while let Some(span) = span_stack.pop() {
        let duration = span.started_at.elapsed();
        event_bus::emit(RuntimeEvent {
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
            event: EventKind::SpanEnd(SpanEnd {
                label: span.label,
                result: Err(error.to_string()),
                duration,
            }),
        });
    }
}
```

### 9.4 Example Error Output

```
  Traceback (most recent call last):              ← from VM frames (complete)
    File "main.baml", line 5, in main
    File "helpers.baml", line 12, in format_prompt
    File "extract.baml", line 3, in ExtractResume

  Active spans at error time:                     ← from engine span_stack
    main [span:aaa, 250ms]
    call_llm_function [span:bbb, 12ms]

  TypeError: expected string, got int
  (trace root: aaa)
```

`format_prompt` is in the frame trace but not the span trace (it used
`Call`, not `CallWithTrace`). Correct.

---

## 10. Detailed Flows

### 10.1 Flow: Traced LLM Function Call

Note: `span_stack` in these flows is a per-invocation local variable
created in `call_function()`, not a field on the shared `BexEngine`.

```
Time │ VM action                           │ span_stack (per-invocation)
─────┼─────────────────────────────────────┼──────────────────────────────
     │ Engine: push {A, "ExtractResume"}   │ [A]
     │ Engine: emit FunctionStart(Extract) │
 t0  │ exec: CallWithTrace (call_llm)      │
     │ snapshot args from eval stack       │
     │ frames.push(call_llm), depth=1      │
     │ traced_frames.push(1)               │
     │ yield FunctionEnter("call_llm",     │
     │   depth=1, args=[...])              │
     │                                     │ Engine: push {B, "call_llm"}
     │                                     │ emit FunctionStart(call_llm, args)
     │                                     │ [A, B]
 t1  │ exec: ... render prompt ...          │
 t2  │ exec: DispatchFuture(HttpSend)       │
     │ yield ScheduleFuture                 │
     │                                     │ ctx = {span:B, parent:A} ✅
     │                                     │ schedule HTTP
 t3  │ exec: Await                          │
     │ yield Await                          │ await HTTP response
 t4  │ exec: ... parse response ...         │
 t5  │ exec: Return (result on eval stack)  │
     │ traced_frames.last()==1 → pop        │
     │ yield FunctionExit("call_llm",       │
     │   result=<parsed_value>)             │
     │                                     │ Engine: pop {B}
     │                                     │ emit FunctionEnd(call_llm, result)
     │                                     │ [A]
 t6  │ exec: Return (ExtractResume)         │
     │ Complete(result)                     │
     │                                     │ Engine: pop {A}
     │                                     │ emit FunctionEnd(ExtractResume, result)
     │                                     │ []
```

### 10.2 Flow: Untraced Function Call

```
Time │ VM action                           │ Engine span_stack
─────┼─────────────────────────────────────┼──────────────────────────────
 t0  │ exec: Call (format_prompt)           │ [A, B]  (unchanged)
     │ frames.push(format_prompt)           │
     │ (no yield — regular Call)            │
 t1  │ exec: ... format_prompt body ...     │ [A, B]  (unchanged)
 t2  │ exec: Return                         │
     │ traced_frames.last() != depth        │
     │ (no yield — regular Return)          │ [A, B]  (unchanged)
```

Zero yields. Zero engine involvement. The compiler emitted `Call`, not
`CallWithTrace`.

### 10.3 Flow: Error Mid-Execution

```
Time │ VM action                           │ Engine action
─────┼─────────────────────────────────────┼──────────────────────────────
 t0  │ frames: [main, Extract, call_llm]   │ span_stack: [A, B]
 t1  │ Error in call_llm                   │
     │ vm.exec() returns Err(VmError)       │
     │                                     │ stack_trace = vm.stack_trace(err)
     │                                     │   → [main:5, Extract:3, call_llm:8]
     │                                     │ emit_unwind_events():
     │                                     │   pop B → SpanEnd(Err, dur)
     │                                     │   pop A → SpanEnd(Err, dur)
     │                                     │ return EnrichedError
```

### 10.4 Flow: Watch Filter Interrupt

```
Time │ VM action                           │ Engine span_stack
─────┼─────────────────────────────────────┼──────────────────────────────
 t0  │ frames: [main], interrupt(filter)   │ [A]
     │ frames.push(<filter>)               │
     │ (regular frame push, not traced)     │ [A]  (unchanged)
 t1  │ exec: filter body ...               │ [A]  (unchanged)
 t2  │ Return, interrupt_frame check        │ [A]  (unchanged)
     │ Complete(bool)                       │
```

Watch filters use regular frame push (via `interrupt()`), not
`CallWithTrace`. The engine's span stack is unaffected.

---

## 11. Python-Side Callstack Initialization

This section describes in detail how callstacks and span contexts are
initialized from the Python side before VM execution begins. The engine's
span stack (Section 5) doesn't exist in a vacuum — it must be
bootstrapped with the right parent/root context so that BAML spans nest
correctly under host-language spans.

### 11.1 Two Entry Paths from Python

Python code enters the BAML engine through two paths:

| Path | Example | Span context on entry |
|---|---|---|
| **Direct call** | `await b.ExtractResume(text)` | No host span — BAML creates a fresh root |
| **`@trace`-decorated call** | `@trace` wraps a pipeline; BAML calls happen inside | Host span active — BAML call becomes a child |

Both paths converge at `bridge_cffi::call_function_inner()`, but with
different `HostSpanContext` values (present or absent).

### 11.2 Current System (`engine/`) — How It Works Today

The existing `engine/` system uses a 5-layer initialization chain:

```
Python user code
    │
    │  @trace decorator or direct b.FunctionName() call
    ▼
CtxManager (Python, ctx_manager.py)
    │  contextvars.ContextVar[Dict[thread_id, RuntimeContextManager]]
    │  Manages one RuntimeContextManager per (async context, thread) pair
    ▼
RuntimeContextManager (Rust, context_manager.rs)
    │  Arc<Mutex<Vec<(uuid, name, tags, FunctionCallId)>>>
    │  enter() pushes, exit() pops, deep_clone() forks for async
    ▼
BamlSpan (PyO3, span.rs)
    │  BamlSpan::new() → runtime.start_call() → ctx.enter()
    │  BamlSpan::finish() → runtime.finish_call() → ctx.exit()
    ▼
BamlTracer (Rust, tracing/mod.rs)
    │  start_call() → ctx.enter(name) → creates TracingCall
    │  finish_call() → ctx.exit() → emits TraceEvent to BAML_TRACER
    ▼
BAML_TRACER (global singleton, Lazy<Mutex<TraceStorage>>)
    Stores events keyed by FunctionCallId
```

#### How `@trace` Builds the Call Stack

When a `@trace`-decorated Python function is called, here is the precise
initialization sequence:

**Step 1: Python `CtxManager.trace_fn()` (async path)**

```python
# ctx_manager.py — inside the async_wrapper generated by @trace
params = {param_names[i]: arg for i, arg in enumerate(args)}
params.update(kwargs)
span = self.start_trace_async(func_name, params, os.environ.copy())
```

**Step 2: `start_trace_async()` deep-clones the context**

```python
def start_trace_async(self, name, args, env_vars):
    mng = self.__ctx()               # get RuntimeContextManager for this thread
    cln = mng.deep_clone()            # fork: new Arc<Mutex<Vec<...>>>
    self.ctx.set({current_thread_id(): cln})  # replace in contextvar
    return BamlSpan.new(self.rt, name, args, cln, env_vars)
```

The `deep_clone()` is critical for `asyncio.gather()` — each concurrent
coroutine gets its own copy of the call stack so they don't corrupt
each other.

**Step 3: `BamlSpan::new()` crosses into Rust via PyO3**

```rust
// span.rs
let span = runtime.inner.start_call(function_name, args_map, &ctx.inner, &env_vars);
```

**Step 4: `BamlTracer::start_call()` calls `ctx.enter()`**

```rust
// tracing/mod.rs
let (call_id, call_stack, ctx_tags, global_tags) = ctx.enter(function_name);
```

**Step 5: `RuntimeContextManager::enter()` pushes onto the stack**

```rust
// context_manager.rs
pub fn enter(&self, name: &str) -> (uuid, Vec<FunctionCallId>, ...) {
    let call = uuid::Uuid::new_v4();           // new UUID for this span
    let call_id = FunctionCallId::new();        // new FunctionCallId
    let mut ctx = self.context.lock().unwrap();
    ctx.push((call, name.to_string(), last_tags.clone(), call_id.clone()));

    let call_stack = ctx.iter()
        .map(|(.., call_id)| call_id.clone())
        .collect::<Vec<_>>();                   // full stack snapshot

    (call, call_stack, last_tags, global_tags)
}
```

At this point the `RuntimeContextManager` has a stack like:

```
[("my_pipeline", uuid_A, FunctionCallId_1)]
```

**Step 6: When the user calls `await b.ExtractResume(text)` inside @trace**

The generated BAML client code grabs the **same** `RuntimeContextManager`
from the contextvar (the deep-cloned one set in step 2):

```python
# Generated async_client.py
ctx = __ctx__manager__.get()              # same clone from step 2
return __runtime__.call_function(
    function_name, args, ctx, ...)        # ctx already has the @trace span
```

The PyO3 layer passes `ctx.inner` (the Rust `RuntimeContextManager`)
into `baml_runtime.call_function()`, which calls `tracer.start_call()`
again — this time for "ExtractResume". The stack grows to:

```
[("my_pipeline", uuid_A, FunctionCallId_1),
 ("ExtractResume", uuid_B, FunctionCallId_2)]
```

The `call_id_stack` (list of `FunctionCallId`s) is embedded in every
`TraceEvent`, allowing the Boundary dashboard to reconstruct the
parent-child tree.

#### How Direct Calls Work (No `@trace`)

When the user calls `await b.ExtractResume(text)` without `@trace`:

1. `CtxManager.__ctx()` returns a fresh `RuntimeContextManager`
   (stack is empty)
2. The generated client passes this empty-stack context to
   `call_function()`
3. `tracer.start_call()` calls `ctx.enter("ExtractResume")`
4. Stack becomes: `[("ExtractResume", uuid_A, FunctionCallId_1)]`
5. The BAML call is a root — no parent span

#### Thread Isolation via `thread_id` Key

The `CtxManager` uses a two-level key: `contextvars.ContextVar` for
async isolation, and `Dict[thread_id, RuntimeContextManager]` for
thread isolation:

```python
def __ctx(self) -> RuntimeContextManager:
    ctx = self.ctx.get()                    # contextvar snapshot
    thread_id = current_thread_id()
    if thread_id not in ctx:
        ctx[thread_id] = self.rt.create_context_manager()  # fresh for new threads
    return ctx[thread_id]
```

This means:
- **Same async task, same thread** → same `RuntimeContextManager` → correct nesting
- **`asyncio.gather` coroutines** → each gets a contextvar snapshot; `deep_clone()`
  ensures independent stacks
- **`ThreadPoolExecutor` workers** → new `thread_id` → fresh
  `RuntimeContextManager` → independent root spans

### 11.3 New System (`baml_language`) — Design

The new system replaces the multi-layered `RuntimeContextManager` →
`BamlTracer` → `BAML_TRACER` chain with a simpler model based on
`HostSpanManager` and direct `event_bus::emit()` calls.

See [event-publishing-design-v2.md, Milestone 8](./event-publishing-design-v2.md#14-milestone-8-host-language-span-tracking-trace-in-pythonts)
for the full `HostSpanManager` design. Here we focus on how the
callstack specifically gets initialized.

#### The New Initialization Chain

```
Python user code
    │
    │  @trace decorator or direct b.FunctionName() call
    ▼
CtxManager (Python, ctx_manager.py)
    │  Same contextvars + thread_id pattern
    │  Now wraps HostSpanManager instead of RuntimeContextManager
    ▼
HostSpanManager (Rust, bridge_cffi/src/host_spans.rs)
    │  Vec<HostSpanEntry> — lightweight span stack
    │  enter() pushes span, emits FunctionStart via event_bus::emit()
    │  exit() pops span, emits FunctionEnd via event_bus::emit()
    │  current_context() → Option<HostSpanContext>
    ▼
bridge_cffi::call_function_inner()
    │  Reads HostSpanContext from the HostSpanManager
    │  Creates root_span_id and parent_span_id
    │  Registers collectors to track root_span_id
    ▼
BexEngine::call_function()
    │  Receives root_span_id + parent_span_id
    │  Creates per-invocation span_stack (local variable)
    │  Pushes initial EngineSpan onto local span_stack
    │  Creates per-invocation BexVm
    │  Sets vm.tracing_enabled
    │  Runs VM event loop (span_stack passed by &mut)
    ▼
Per-invocation span_stack ← CallWithTrace notifications from VM
    All subsequent span context derivation happens here (Section 5)
```

#### Step-by-Step: `@trace` → BAML Call

**Step 1: Python `@trace` enters a host-language span**

```python
# Updated ctx_manager.py
@functools.wraps(func)
async def async_wrapper(*args, **kwargs):
    params = _build_params(args, kwargs, param_names)
    mgr = self.__mgr()
    clone = mgr.deep_clone()                    # fork for async isolation
    self.ctx.set({current_thread_id(): clone})
    clone.enter(func_name, params)              # ← push onto HostSpanManager
    # ...
```

**Step 2: `HostSpanManager::enter()` generates a SpanId and emits FunctionStart**

```rust
pub fn enter(&mut self, function_name: &str, args: Vec<(String, BexExternalValue)>) -> SpanId {
    let span_id = SpanId::new();

    // Determine parent and root from current stack
    let (parent_span_id, root_span_id) = match self.stack.last() {
        Some(parent) => (Some(parent.span_id.clone()), parent.root_span_id.clone()),
        None => (None, span_id.clone()),  // this span IS the root
    };

    // Emit FunctionStart directly — no engine involved
    event_bus::emit(RuntimeEvent {
        ctx: SpanContext { span_id, parent_span_id, root_span_id },
        timestamp: web_time::SystemTime::now(),
        event: EventKind::Function(FunctionEvent::Start(FunctionStart {
            name: function_name.to_string(), args, is_stream: false,
        })),
    });

    self.stack.push(HostSpanEntry { span_id, root_span_id, function_name, started_at });
    span_id
}
```

At this point the `HostSpanManager` stack is:

```
[HostSpanEntry { span_id: A, root_span_id: A, name: "my_pipeline" }]
```

And a `FunctionStart` event has already been emitted to the global
`EventStore` with `span=A, parent=None, root=A`.

**Step 3: User calls `await b.ExtractResume(text)` inside `@trace`**

The generated BAML client asks the `CtxManager` for the current host
span context:

```python
# Generated async_client.py
host_ctx = __ctx__manager__.current_host_context()
# → HostSpanContext { span_id: A, root_span_id: A }

raw = self.__runtime.call_function(
    "ExtractResume", {"text": text},
    self.__ctx_manager.get(),
    host_ctx,           # ← passed to bridge_cffi
    baml_options or {},
)
```

**Step 4: `bridge_cffi::call_function_inner()` extracts the host context**

```rust
let host_ctx: Option<HostSpanContext> = extract_host_span_context(&args);

let (root_span_id, parent_span_id) = match &host_ctx {
    Some(ctx) => {
        // BAML call is a child of the @trace span.
        // Inherit the host's root; use the host's span as parent.
        (ctx.root_span_id.clone(), Some(ctx.span_id.clone()))
    }
    None => {
        // No host span — this BAML call is its own root.
        let root = SpanId::new();
        (root.clone(), None)
    }
};

// Register collectors to track this root_span_id
for collector in &collectors {
    collector.track_call(root_span_id.clone());
}

// Call the engine with explicit span parentage
engine.call_function(&func_name, &bex_args, Some(root_span_id), parent_span_id).await
```

**Step 5: `BexEngine::call_function()` pushes the initial engine span**

```rust
// bex_engine/src/lib.rs
pub async fn call_function(
    &self, name: &str, args: &[BexValue],
    root_span_id: Option<SpanId>, parent_span_id: Option<SpanId>,
) -> Result<BexValue, EngineError> {
    let span_id = root_span_id.unwrap_or_else(SpanId::new);

    // Create per-invocation span stack (local variable, NOT on self)
    let mut span_stack: Vec<EngineSpan> = Vec::new();

    // Push the entry-point span BEFORE VM execution begins
    span_stack.push(EngineSpan {
        span_id: span_id.clone(),
        label: name.to_string(),
        started_at: web_time::Instant::now(),
        frame_depth: 0,
    });

    // Emit FunctionStart for the entry-point function
    event_bus::emit(RuntimeEvent {
        ctx: SpanContext {
            span_id: span_id.clone(),
            parent_span_id: parent_span_id.clone(),
            root_span_id: span_id.clone(),
        },
        // ...
    });

    // VM execution begins — span_stack passed by &mut to run_event_loop.
    // CallWithTrace notifications will push additional spans (Section 5.3)
    let result = self.run_event_loop(&mut vm, &mut span_stack, &tracing_config).await;
    // span_stack is dropped here — per-invocation lifecycle complete
    // ...
}
```

At this point the per-invocation `span_stack` has:

```
[EngineSpan { span_id: B, label: "ExtractResume", frame_depth: 0 }]
```

And the full span context chain is:

```
A (my_pipeline)           ← HostSpanManager, emitted by Python-side enter()
└─ B (ExtractResume)      ← Engine span_stack, parent=A from host_ctx
   └─ C (call_llm_function) ← Engine span_stack, pushed by CallWithTrace notification
```

#### Step-by-Step: Direct Call (No `@trace`)

When the user calls `await b.ExtractResume(text)` without `@trace`:

1. `CtxManager.current_host_context()` returns `None` (empty stack)
2. Generated client passes `host_ctx=None` to `call_function`
3. `bridge_cffi` creates a fresh `root_span_id = SpanId::new()` with
   `parent_span_id = None`
4. Engine pushes the entry-point span as the root
5. Span tree has no host-language parent:

```
B (ExtractResume)           ← Engine span_stack, root of tree
└─ C (call_llm_function)    ← Engine span_stack, pushed by CallWithTrace
```

### 11.4 Key Differences: Current vs. New System

| Aspect | Current (`engine/`) | New (`baml_language`) |
|---|---|---|
| **Host span state** | `RuntimeContextManager`: `Vec<(uuid, name, tags, FunctionCallId)>` behind `Arc<Mutex<>>` | `HostSpanManager`: `Vec<HostSpanEntry>` (lightweight, no tags in entries) |
| **Span identity** | `FunctionCallId` (opaque UUID wrapper) + separate `uuid::Uuid` call ID | `SpanId` (single UUID, used everywhere) |
| **How @trace emits events** | `BamlSpan::new()` → `runtime.start_call()` → `BamlTracer` → `BAML_TRACER` (4 hops) | `HostSpanManager::enter()` → `event_bus::emit()` (1 hop, direct) |
| **How context flows to engine** | `RuntimeContextManager` passed by reference; engine calls `ctx.enter()` internally, mutating the shared stack | `HostSpanContext { span_id, root_span_id }` passed as immutable data; engine doesn't touch the host stack |
| **Call stack as data** | `call_id_stack: Vec<FunctionCallId>` embedded in every `TraceEvent` | `SpanContext { span_id, parent_span_id, root_span_id }` — tree reconstructed from parent pointers |
| **Async isolation** | `deep_clone()` on `Arc<Mutex<Vec<...>>>` (clones the Vec inside a new Arc+Mutex) | `deep_clone()` on `HostSpanManager` (clones the Vec directly, no Arc/Mutex — stack is owned) |
| **Engine-internal spans** | Not tracked (engine/ doesn't have sub-function spans) | Per-invocation `span_stack: Vec<EngineSpan>` tracks all `CallWithTrace` spans |
| **Thread isolation** | `Dict[thread_id, RuntimeContextManager]` keyed by `threading.native_id` | Same pattern: `Dict[thread_id, HostSpanManager]` |
| **Where span lifecycle lives** | Split across `BamlTracer`, `RuntimeContextManager`, and `BAML_TRACER` global | `HostSpanManager` for host spans; per-invocation `span_stack` local in `call_function()` for engine spans |

### 11.5 Initialization Sequence Diagram

```
                     Python                          Rust (bridge_cffi)              Rust (BexEngine)
                     ──────                          ──────────────────              ────────────────
                        │                                    │                              │
  @trace my_pipeline()  │                                    │                              │
                        │                                    │                              │
   CtxManager.trace_fn()│                                    │                              │
     ┌──────────────────┤                                    │                              │
     │ deep_clone() mgr │                                    │                              │
     │ set clone in ctx │                                    │                              │
     │                  │                                    │                              │
     │ clone.enter(     │──── FFI ──────────────────────────▶│                              │
     │  "my_pipeline")  │  HostSpanManager::enter()          │                              │
     │                  │    span_id = A                     │                              │
     │                  │    parent = None (stack empty)     │                              │
     │                  │    root = A                        │                              │
     │                  │    event_bus::emit(FnStart, A)     │                              │
     │                  │    stack.push(A)                   │                              │
     │                  │◀───────────────────────────────────│                              │
     │                  │                                    │                              │
     │ await b.Extract  │                                    │                              │
     │  Resume(text)    │                                    │                              │
     │                  │                                    │                              │
     │ current_host_    │──── FFI ──────────────────────────▶│                              │
     │  context()       │  HostSpanManager::current_context()│                              │
     │                  │◀── HostSpanContext{span=A,root=A} ─│                              │
     │                  │                                    │                              │
     │ call_function(   │──── FFI (protobuf) ───────────────▶│                              │
     │  "ExtractResume",│                                    │                              │
     │   args,          │  call_function_inner():             │                              │
     │   host_ctx)      │    host_ctx present → {            │                              │
     │                  │      root = A (from host)          │                              │
     │                  │      parent = A (from host)        │                              │
     │                  │    }                               │                              │
     │                  │    register collectors for root=A  │                              │
     │                  │                                    │                              │
     │                  │    engine.call_function(            │──────────────────────────────▶│
     │                  │      "ExtractResume", args,         │                              │
     │                  │      root=A, parent=Some(A))        │  push EngineSpan{B, "Extract"}
     │                  │                                    │  emit FnStart(B, parent=A)   │
     │                  │                                    │  vm.tracing_enabled = true   │
     │                  │                                    │  vm.exec() begins            │
     │                  │                                    │    │                         │
     │                  │                                    │    │ CallWithTrace            │
     │                  │                                    │    │ → FunctionEnter          │
     │                  │                                    │    │                         │
     │                  │                                    │  push EngineSpan{C, "call_llm"}
     │                  │                                    │  span_stack: [B, C]          │
     │                  │                                    │    │                         │
     │                  │                                    │    │ baml.events.send(        │
     │                  │                                    │    │   LlmRequest, etc.)      │
     │                  │                                    │    │ ctx = {span:C, parent:B} │
     │                  │                                    │    │         ✅ correct       │
     │                  │                                    │    │                         │
     │                  │                                    │    │ ... LLM call ...         │
     │                  │                                    │    │                         │
     │                  │                                    │    │ Return → FunctionExit    │
     │                  │                                    │  pop C, span_stack: [B]      │
     │                  │                                    │    │                         │
     │                  │                                    │    │ Complete                 │
     │                  │                                    │  pop B, span_stack: []       │
     │                  │                                    │  emit FnEnd(B, parent=A)     │
     │                  │◀───────────────────────────────────│◀─────────────────────────────│
     │                  │                                    │                              │
     │ clone.exit(      │──── FFI ──────────────────────────▶│                              │
     │   Ok(result))    │  HostSpanManager::exit()           │                              │
     │                  │    pop A from stack                 │                              │
     │                  │    emit FnEnd(A, parent=None)       │                              │
     │                  │◀───────────────────────────────────│                              │
     └──────────────────┤                                    │                              │
                        │                                    │                              │
  return result         ▼                                    ▼                              ▼
```

### 11.6 The `deep_clone()` Boundary and Async Safety

A subtle but critical detail: `deep_clone()` determines which spans
see which parents in concurrent Python code.

**When `deep_clone()` happens**: At the entry of every `async @trace`
function, before `enter()` is called.

**What it clones**: The entire `HostSpanManager` stack — a `Vec<HostSpanEntry>`
with all accumulated spans from outer `@trace` calls.

**Why it's needed**: Python's `asyncio` propagates `contextvars` by
snapshot when creating a Task. Without `deep_clone()`, concurrent
coroutines from `asyncio.gather()` would share one `HostSpanManager`
and corrupt each other's stacks:

```python
@trace
async def pipeline():              # span P
    await asyncio.gather(
        branch("a"),               # should see parent P
        branch("b"),               # should see parent P
    )

@trace
async def branch(x):              # span Q₁ or Q₂
    await b.Classify(x)           # should see parent Q₁ or Q₂
```

Without clone: both coroutines push onto the same stack →
`branch("b")` might see `branch("a")` as parent instead of `pipeline`.

With clone:
- `branch("a")` gets clone₁ with stack `[P]` → pushes `Q₁` → stack `[P, Q₁]`
- `branch("b")` gets clone₂ with stack `[P]` → pushes `Q₂` → stack `[P, Q₂]`
- Each `b.Classify()` call sees the correct parent

**Sync path**: No `deep_clone()`. The `HostSpanManager` is used
directly — single thread, no fork needed. Nested sync `@trace` calls
naturally build up the stack.

### 11.7 What Happens When No `@trace` Is Active

If the user never uses `@trace`, the `HostSpanManager` stack is always
empty. The initialization path simplifies to:

```
Python: await b.ExtractResume(text)
  │
  │  current_host_context() → None
  │
  ▼
bridge_cffi::call_function_inner():
  host_ctx = None
  root_span_id = SpanId::new()     // fresh root (B)
  parent_span_id = None            // no parent
  │
  ▼
BexEngine::call_function("ExtractResume", args, root=B, parent=None):
  span_stack.push(EngineSpan { span_id: B, ... })
  emit FunctionStart(span=B, parent=None, root=B)
  // VM execution with CallWithTrace as normal
```

The BAML call is the root of the span tree. Events are still emitted
and stored — collectors and the publisher still work. The only difference
is there's no host-language parent span wrapping the BAML call.

### 11.8 Why Two Span Stacks (Host + Engine) Instead of One

The design uses two separate span stacks — `HostSpanManager` on the
Python side and a per-invocation `span_stack` inside `call_function()` — rather than a single
unified stack. This is intentional.

#### Different Lifecycles

The host stack persists across multiple BAML engine invocations. A
single `@trace my_pipeline` span wraps two sequential `call_function()`
invocations. The engine stack is ephemeral — created when
`call_function()` starts, destroyed when it returns. They don't overlap
for the same span; they represent different levels of the tree:

```
HostSpanManager stack: [A: my_pipeline]       ← lives across both calls
  Engine stack (call 1): [B: ExtractResume, C: call_llm]  ← ephemeral
  Engine stack (call 2): [D: Summarize, E: call_llm]      ← ephemeral
```

#### Different Concurrency Models

The host stack follows Python's concurrency rules — `deep_clone()` for
`asyncio.gather`, `contextvars` for task isolation, `thread_id` for
thread isolation. The engine stack is single-owner during synchronous
VM execution (no mutex, no cloning). Merging them would force one model
to accommodate the other.

The current `engine/` system does exactly this (one shared
`RuntimeContextManager` behind `Arc<Mutex<>>` that both Python and Rust
mutate), and it's one of the pain points — the engine has to call
`ctx.enter()`/`ctx.exit()` on a shared mutable object that Python also
deep-clones for async, creating subtle coordination requirements.

#### Clean Boundary

The connection between the two stacks is a simple immutable data
transfer: `HostSpanContext { span_id, root_span_id }`. The engine uses
this as `parent_span_id` for its root span. No shared mutable state, no
lock contention, no deep-clone coordination. Compare this to `engine/`
where the `RuntimeContextManager` is passed by reference and both sides
mutate the same `Arc<Mutex<Vec<...>>>`.

#### Independent Error Teardown

When the engine hits an error, it unwinds its own `span_stack` (emitting
`SpanEnd(Err)` for each). It doesn't touch the host stack. The host
stack unwinds independently when the Python `@trace` wrapper catches the
exception and calls `exit_error()`. If they were one stack, error unwind
would need to coordinate which entries belong to the host and which to
the engine.

#### Per-Invocation Isolation for Concurrent Calls

The engine's span stack is a **local variable** in `call_function()`,
not a field on the shared `BexEngine`. This is essential because
`BexEngine` is a shared `Arc<BexEngine>` singleton — multiple concurrent
Python calls (e.g., `asyncio.gather(b.Foo(), b.Bar())`) all share the
same engine instance. If the span stack were on `BexEngine`, concurrent
calls would interleave pushes/pops on the same stack and corrupt each
other's span context.

The per-invocation pattern mirrors how the VM is already handled:

```rust
pub async fn call_function(&self, ...) -> Result<...> {
    let mut span_stack: Vec<EngineSpan> = Vec::new();  // per-call
    let mut vm = BexVm::new(Arc::clone(&self.heap), ...);  // per-call
    self.run_event_loop(&mut vm, &mut span_stack, &config).await
    // Both dropped here — no shared mutable state
}
```

This means each concurrent call has its own span stack, its own VM,
and its own lifecycle. The shared `BexEngine` remains purely immutable
(heap, globals, function index) — no locks needed for span tracking.

#### Why Not a Unified Stack?

A unified stack would mean passing the `HostSpanManager` into
`call_function()` by mutable reference and having the engine push/pop
directly onto it. This is essentially what `engine/` does today with
`RuntimeContextManager`, and the resulting coupling is one of the reasons
the new design separates them:

- `deep_clone()` would need to be aware of engine spans
- Error unwind would need to distinguish host vs. engine entries
- The engine would depend on the host span manager's type
- Lock contention between host and engine for every span push/pop

The only scenario where a unified stack might matter is nesting
host-language `@trace` spans *inside* BAML function execution (e.g., a
BAML function calling back into Python). But that doesn't happen — BAML
functions execute entirely in the VM, and the host-to-engine boundary is
one-directional per invocation.

---

## 12. Data Structures

### 12.1 VM Side (Minimal)

```rust
// bex_vm_types/src/bytecode.rs — new instruction
pub enum Instruction {
    // ... existing ...
    Call(usize),           // unchanged
    CallWithTrace(usize),  // new
}

// bex_vm/src/vm.rs — notification type + VM state
#[derive(Clone, Debug)]
pub enum SpanNotification {
    FunctionEnter {
        function_name: String,
        frame_depth: usize,
        args: Vec<Value>,       // snapshotted from eval stack before frame push
    },
    FunctionExit {
        function_name: String,
        result: Value,          // return value from eval stack
    },
}

pub struct BexVm {
    // ... existing fields (ALL unchanged) ...
    pub tracing_enabled: bool,       // new
    traced_frames: Vec<usize>,       // new
}
```

### 12.2 Engine Side

**Important**: `BexEngine` is a shared `Arc<BexEngine>` singleton with
`&self` methods. It holds only immutable compile-time data. All
per-invocation mutable state — the span stack, the VM, tracing config —
lives as local variables in `call_function()`.

```rust
// bex_engine/src/lib.rs

// BexEngine is SHARED and IMMUTABLE — no span_stack field.
// pub struct BexEngine { heap, globals, resolved_function_names, ... }

/// Per-invocation span entry. Created as Vec<EngineSpan> local in call_function().
#[derive(Clone, Debug)]
pub struct EngineSpan {
    pub span_id: SpanId,
    pub label: String,
    pub started_at: web_time::Instant,
    pub frame_depth: usize,
}

/// Tracing configuration passed from bridge_cffi to call_function().
pub struct TracingConfig {
    pub enabled: bool,
    pub root_span_id: Option<SpanId>,
    pub parent_span_id: Option<SpanId>,
}

/// Error enriched with span context from the per-invocation span stack.
pub struct EnrichedError {
    pub stack_trace: StackTrace,
    pub active_spans: Vec<EngineSpan>,
    pub root_span_id: Option<SpanId>,
}
```

### 12.3 Existing Types (Unchanged)

```rust
// Frame — NOT modified
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    pub function: HeapPtr,
    pub instruction_ptr: isize,
    pub locals_offset: StackIndex,
}

// ErrorLocation, StackTrace — NOT modified
```

---

## 13. Implementation Plan

### Phase 1: VM CallWithTrace + Notifications

**Changes**:
- Add `CallWithTrace(usize)` to `Instruction` enum
- Add `tracing_enabled: bool` and `traced_frames: Vec<usize>` to `BexVm`
- Add `SpanNotification` enum (with `args: Vec<Value>` on `FunctionEnter`, `result: Value` on `FunctionExit`)
- Dispatch `CallWithTrace` in `exec()` (same as `Call` + snapshot args from eval stack + trace + yield)
- Modify `Return` to check `traced_frames` and yield `FunctionExit` with result value
- Display impl for `CallWithTrace` instruction

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_vm_types/src/bytecode.rs` | `CallWithTrace(usize)` variant + Display | ~5 |
| `bex_vm/src/vm.rs` | `SpanNotification`, `tracing_enabled`, `traced_frames` | ~10 |
| `bex_vm/src/vm.rs` | `CallWithTrace` dispatch (mirrors `Call`) | ~30 |
| `bex_vm/src/vm.rs` | `Return` modification (check traced_frames) | ~15 |

**Tests**:
- Unit test: `CallWithTrace` yields `FunctionEnter` when `tracing_enabled`
- Unit test: `CallWithTrace` acts as `Call` when `tracing_enabled == false`
- Unit test: `Return` yields `FunctionExit` for traced frames
- Unit test: `Return` does NOT yield for untraced frames
- Unit test: Nested traced/untraced calls with correct `traced_frames` behavior
- Unit test: `interrupt()` frames don't interfere with `traced_frames`

### Phase 2: Per-Invocation Span Stack + Integration with Event Publishing

**Changes**:
- Add `EngineSpan` struct and `TracingConfig` to `bex_engine`
- Create per-invocation `span_stack: Vec<EngineSpan>` as a local in `call_function()`
  (NOT on the shared `BexEngine` struct — concurrent calls need isolated stacks)
- Thread `&mut span_stack` through `run_event_loop()`
- Handle `SpanNotification::FunctionEnter` → push span onto local stack, emit `FunctionStart` (args from notification)
- Handle `SpanNotification::FunctionExit` → pop span from local stack, emit `FunctionEnd` (result from notification)
- `span_context()` as a free function taking `&[EngineSpan]` (replaces M4's SpanStack)
- Entry-point function: push/pop span around `exec()`
- `emit_unwind_events()` takes `&mut Vec<EngineSpan>` for error cleanup
- `EnrichedError`, `TracingConfig`
- `SysOp::EventSend` handler uses `span_context(&span_stack, ...)` for correct ctx

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_engine/src/lib.rs` | `EngineSpan`, per-invocation `span_stack` in `call_function()`, notification handlers | ~50 |
| `bex_engine/src/lib.rs` | `span_context()` free fn, entry-point span push/pop, `run_event_loop(&mut span_stack)` | ~30 |
| `bex_engine/src/lib.rs` | `emit_unwind_events(&mut span_stack)`, `EnrichedError` | ~30 |
| `bex_engine/src/lib.rs` | `TracingConfig`, integrate with `call_function()` | ~20 |
| `bridge_cffi` | Pass `TracingConfig` from host | ~10 |

### Phase 3: Compiler Emits CallWithTrace

**Changes**:
- LLM function delegation (`FunctionBody::Llm`) emits `CallWithTrace`
  when calling `call_llm_function()`
- `@trace` annotation support: emit `CallWithTrace` at annotated call sites
- `@notrace` annotation support: always emit `Call`

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_compiler_emit` | `CallWithTrace` for LLM delegation calls | ~10 |
| `baml_compiler_emit` | `@trace`/`@notrace` annotation handling | ~20 |

### Phase 4: Arbitrary Spans (Future)

- `StartSpan(idx)` / `EndSpan` instructions for non-function regions
- Sub-function spans in `llm.baml`
- User-defined `span("label") { ... }` syntax
- Same engine span stack, same notification pattern

---

## 14. Design Alternatives Considered

### 14.1 Alternative A: Frame Enrichment (v1)

Add `span_id` and `started_at` to `Frame`.

**Rejected**: Frame nearly triples in size. Cannot represent non-function
spans. Couples VM to event system. Runtime `should_track_call()` cost.

### 14.2 Alternative B: SpanEnter/SpanExit Wrapping Function Bodies

Compiler wraps every traced function body with `SpanEnter`/`SpanExit`.

**Compared to CallWithTrace**:
- 2 extra instructions per function (SpanEnter + SpanExit) vs 0 extra
  (CallWithTrace replaces Call at the call site)
- Function body is modified vs function body is untouched
- The callee decides (SpanEnter in body) vs the caller decides
  (CallWithTrace at call site)
- SpanExit must be on every exit path vs Return auto-detects traced frames

**Verdict**: `CallWithTrace` is simpler for function-level spans. For
arbitrary spans, `StartSpan`/`EndSpan` (equivalent to SpanEnter/SpanExit)
remains the right approach as a future addition.

### 14.3 Alternative C: VM-Level Span Stack

The VM maintains `span_stack: Vec<SpanInfo>` with SpanId and timing.

**Rejected**: Puts observability state in the wrong layer. VM depends on
SpanId and Instant types. Observability logic split between VM and engine.

### 14.4 Alternative D: Engine-Side SpanStack via Event Interception (M4 original)

Engine pushes/pops spans when it sees `EventSend("function_start")`/
`EventSend("function_end")`.

**Compared to CallWithTrace**:
- Span pushed AFTER function_start event vs span pushed BEFORE function
  body executes
- Must parse event payloads vs explicit notification
- Only works for functions with `baml.events.send()` vs works for any call

**Verdict**: `CallWithTrace` is strictly better. Span context is correct
from the first instruction of the function body.

### 14.5 Summary

| Criterion | v1 (Frame) | SpanEnter/Exit | VM span stack | Event interception | **CallWithTrace** |
|---|---|---|---|---|---|
| Frame untouched | No | Yes | Yes | Yes | **Yes** |
| VM stateless for spans | No | Depends | No | Yes | **Nearly (Vec<usize>)** |
| Function-level simplicity | Low | Medium | Medium | Low | **High** |
| Arbitrary spans | No | Yes | Yes | No | **Future (StartSpan)** |
| Per-fn control | Runtime | Compile-time | Compile-time | Runtime | **Compile-time** |
| Span timing correct | Yes | Yes | Yes | Late (after event) | **Yes (before body)** |
| Yield cost per traced fn | 0 | 2 | 2 | 0 | **2** |

---

## 15. Open Questions

### Q1: ~~Should CallWithTrace emit FunctionStart, or leave it to baml.events.send()?~~

**Resolved.** The engine now emits `FunctionStart` directly when it
receives `FunctionEnter` from the VM, and `FunctionEnd` when it receives
`FunctionExit`. The VM snapshots args from the eval stack before the
frame push and provides the return value on `FunctionExit`, so the
engine has everything it needs. `baml.events.send("function_start/end")`
is no longer needed for function-level spans.

### Q2: ~~Should the engine emit FunctionStart on FunctionEnter notification?~~

**Resolved — YES.** The concern about args not being available was
resolved by having `CallWithTrace` snapshot the args from the eval stack
before pushing the frame (Section 4.3). The `FunctionEnter` notification
now carries `args: Vec<Value>` and `FunctionExit` carries `result: Value`.
This gives us one event source (the engine) instead of two, with no loss
of information.

### Q3: traced_frames cleanup on error

When the VM returns an error, `traced_frames` may have stale entries
(frames that were never popped). The engine should clear the VM's
`traced_frames` after handling an error, or the VM should clear it in a
`reset()` method.

### Q4: SpanId generation strategy

Same as before: start with UUID v4, optimize later if needed. The engine
owns this, so changes don't touch the VM.

### Q5: Compatibility with event-publishing-design-v2

This replaces M4's `SpanStack` with engine-owned `span_stack` driven by
`CallWithTrace` notifications. M1-M3 (EventStore, Collector, FunctionStart/
FunctionEnd emission) remain unchanged. M5+ (intermediate LLM events,
host-language @trace) benefit from the correct span context.

---

*This document supersedes
[callstack-tracking-design.md](./callstack-tracking-design.md) (v1)
and should be read alongside
[event-publishing-design-v2.md](./event-publishing-design-v2.md)
which covers the event system architecture, collector design, and
milestone plan.*
