# Call Stack Tracking in `baml_language` — Design Document

> Unifying VM call frames, observability spans, watch scopes, and exception traces into a coherent architecture.

---

## Table of Contents

1. [Introduction](#1-introduction)
2. [The Three Call Stacks Today](#2-the-three-call-stacks-today)
3. [Architecture Overview](#3-architecture-overview)
4. [VM Call Stack (Source of Truth)](#4-vm-call-stack-source-of-truth)
5. [Per-Function Tracing Control](#5-per-function-tracing-control)
6. [Observability Span Stack (Engine-Side)](#6-observability-span-stack-engine-side)
7. [Watch Dependency Tracking](#7-watch-dependency-tracking)
8. [Exception Call Stacks](#8-exception-call-stacks)
9. [Unification: VM as the Single Source of Truth](#9-unification-vm-as-the-single-source-of-truth)
10. [Detailed Flows](#10-detailed-flows)
11. [Data Structures](#11-data-structures)
12. [Implementation Plan](#12-implementation-plan)
13. [Open Questions](#13-open-questions)

---

## 1. Introduction

**What**: A design for how `baml_language` tracks call stacks across three
distinct subsystems — the VM's execution frames, the engine's observability
spans, and the watch system's scope tracking — and how these relate to
exception stack traces surfaced to users.

**Why**: Today, `baml_language` has three independent mechanisms that each
partially track "where we are in the call tree":

1. **VM frames** (`Vec<Frame>`) — the real call stack, used for execution
2. **Engine SpanStack** (proposed in event-publishing-design-v2) — UUID-based
   span hierarchy for observability events
3. **Watch `watched_vars`** — scope-based tracking of which variables are
   watched, cleaned up on `Return`

These operate independently. The VM doesn't know about spans. The engine
doesn't inspect the VM's frames for span management. Exception stack traces
are built ad-hoc from frames. This document proposes a unified architecture
where the **VM call stack is the single source of truth** and all other
systems derive their state from it, with **per-function tracing control**
so that each function call can independently decide whether to be
span-tracked.

**Scope**: This document covers the `bex_vm`, `bex_engine`, and
`bex_vm_types` crates. It references the event publishing design (v2)
for context but focuses specifically on call stack mechanics.

**Relationship to event-publishing-design-v2.md**: That document proposes
a `SpanStack` maintained by the engine alongside the VM. This document
argues that the VM's own `frames: Vec<Frame>` should be the authoritative
call stack, with the engine deriving span context from it — eliminating
the need for a parallel stack that can drift out of sync.

---

## 2. The Three Call Stacks Today

### 2.1 Current State: Three Independent Trackers

```
┌─────────────────────────────────────────────────────────────────────────────┐
│                          Current Architecture                               │
├─────────────────────────────────────────────────────────────────────────────┤
│                                                                             │
│  ┌─────────────────────┐                                                    │
│  │  VM (bex_vm)        │                                                    │
│  │                     │                                                    │
│  │  frames: Vec<Frame> │ ← real call stack (execution)                      │
│  │    Frame {           │                                                    │
│  │      function: Ptr   │                                                    │
│  │      ip: isize       │                                                    │
│  │      locals_offset   │                                                    │
│  │    }                 │                                                    │
│  │                     │                                                    │
│  │  watched_vars:      │ ← scope tracking for @watch cleanup                │
│  │    HashMap<StackIdx, │                                                    │
│  │      (String,String)>│                                                    │
│  │                     │                                                    │
│  │  watch: Watch       │ ← dependency graph (not a stack)                   │
│  │                     │                                                    │
│  │  stack_trace()      │ ← ad-hoc frame→ErrorLocation conversion            │
│  └──────────┬──────────┘                                                    │
│             │                                                               │
│             │ VmExecState                                                   │
│             ▼                                                               │
│  ┌─────────────────────┐                                                    │
│  │  Engine (bex_engine) │                                                    │
│  │                     │                                                    │
│  │  SpanStack (proposed)│ ← parallel span tracking (event-pub v2 M4)        │
│  │    Vec<SpanEntry> {  │                                                    │
│  │      span_id: UUID   │                                                    │
│  │      started_at      │                                                    │
│  │    }                 │                                                    │
│  │                     │                                                    │
│  │  (duplicates frame  │                                                    │
│  │   push/pop logic)   │                                                    │
│  └─────────────────────┘                                                    │
│                                                                             │
│  Problem: The SpanStack must be manually kept in sync with VM frames.       │
│  If the VM pushes a frame (Call) or pops (Return), the engine must          │
│  independently push/pop its SpanStack. Drift = broken traces.               │
│                                                                             │
└─────────────────────────────────────────────────────────────────────────────┘
```

### 2.2 The Synchronization Problem

The event-publishing-design-v2 proposes that the engine maintains a
`SpanStack` by intercepting `SysOp::EventSend` calls from the
compiler-inserted `baml.events.send("function_start/end")` in `llm.baml`.
This works for **LLM functions** (which explicitly send these events) but
has gaps:

| Call type | VM knows? | Engine SpanStack knows? | How? |
|-----------|-----------|------------------------|------|
| Bytecode function call | Yes (Frame push) | Only if `baml.events.send()` is inserted | Compiler must instrument |
| Native function call | Yes (no frame, inline) | No | Invisible to spans |
| SysOp (LLM, HTTP) | Yes (DispatchFuture) | Only if `llm.baml` sends events | Manual instrumentation |
| `interrupt()` (watch filter) | Yes (Frame push) | No | Invisible |
| Expression function (top-level) | Yes | Yes (engine wraps) | Engine-side |

**The core issue**: The SpanStack is a *shadow* of the VM's real call
stack, maintained by a completely different mechanism (SysOp interception
vs. frame push/pop). They can diverge if:

- A bytecode function is called but doesn't have `baml.events.send()` inserted
- An exception interrupts execution between `function_start` and `function_end`
- `interrupt()` runs a filter function, pushing a frame the SpanStack doesn't see
- Future control flow (try/catch, generators) adds frames the SpanStack doesn't expect

---

## 3. Architecture Overview

### 3.1 Proposed: VM-Centric Call Stack

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                         Proposed Architecture                                    │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  ┌─────────────────────────────────────────────────────┐                         │
│  │                   VM (bex_vm)                        │                         │
│  │                                                     │                         │
│  │  frames: Vec<Frame>    ← SINGLE SOURCE OF TRUTH     │                         │
│  │    Frame {                                          │                         │
│  │      function: HeapPtr                              │                         │
│  │      instruction_ptr: isize                         │                         │
│  │      locals_offset: StackIndex                      │                         │
│  │      span_id: Option<SpanId>  ← NEW                 │                         │
│  │      started_at: Instant      ← NEW                 │                         │
│  │    }                                                │                         │
│  │                                                     │                         │
│  │  tracing_active: bool                               │                         │
│  │  tracing_policy: TracingPolicy  ← NEW                │                         │
│  │                                                     │                         │
│  │  should_track_call(callee) → bool   ← PER-FUNCTION   │                         │
│  │    Checks: tracing_active → TracePolicy → TracingPolicy                       │
│  │    Some frames get span_id, others get None          │                         │
│  │                                                     │                         │
│  │  call_stack_snapshot() → CallStackSnapshot  ← NEW   │                         │
│  │    Lightweight: Vec of (function_name, span_id,     │                         │
│  │      source_line) extracted from current frames      │                         │
│  │                                                     │                         │
│  │  stack_trace(error) → StackTrace  (existing, richer)│                         │
│  │    Always includes ALL frames (tracked + untracked)  │                         │
│  │                                                     │                         │
│  └──────────┬──────────────────────────────────────────┘                         │
│             │                                                                    │
│             │ VmExecState (unchanged)                                            │
│             │ + call_stack_snapshot() when needed                                │
│             ▼                                                                    │
│  ┌──────────────────────────────────────────────────────┐                        │
│  │              Engine (bex_engine)                      │                        │
│  │                                                      │                        │
│  │  NO SpanStack needed.                                │                        │
│  │                                                      │                        │
│  │  TracingConfig per call_function() invocation:       │                        │
│  │    { enabled, policy, root_span_id, parent_span_id } │                        │
│  │                                                      │                        │
│  │  When emitting events:                               │                        │
│  │    let ctx = span_context_from_vm(vm, host_parent);  │                        │
│  │    // Walks frames, skips untracked (span_id=None)   │                        │
│  │    event_bus::emit(RuntimeEvent { ctx, ... });       │                        │
│  │                                                      │                        │
│  │  When handling errors:                               │                        │
│  │    let trace = vm.stack_trace(error);                │                        │
│  │    // Rich trace with ALL frames + span_ids          │                        │
│  │                                                      │                        │
│  └──────────────────────────────────────────────────────┘                        │
│                                                                                  │
│  Benefits:                                                                       │
│  • No parallel stack to keep in sync                                             │
│  • Per-function control: each Call decides its own span_id                        │
│  • Untracked frames are invisible to span tree but visible in exceptions         │
│  • Exception traces get span_ids for free (where available)                      │
│  • Watch cleanup uses the same frames                                            │
│  • interrupt() frames are visible when tracing policy allows                     │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 3.2 Design Principles

**Principle 1: Enrich the Frame, Don't Shadow It**

Instead of maintaining a parallel `SpanStack` in the engine, we **enrich
`Frame`** with two small fields:

```rust
pub struct Frame {
    // Existing fields (unchanged)
    pub function: HeapPtr,
    pub instruction_ptr: isize,
    pub locals_offset: StackIndex,

    // NEW: Observability metadata
    /// Span ID for this call frame. Assigned per-function decision.
    /// None if this function is not being traced.
    pub span_id: Option<SpanId>,

    /// When this frame was pushed (for computing duration on pop).
    /// None if this function is not being traced.
    pub started_at: Option<web_time::Instant>,
}
```

**Principle 2: Per-Function Opt-In, Not Global All-or-Nothing**

Every `Instruction::Call` consults `should_track_call(callee)` to decide
whether the new frame gets a `span_id`. This means `span_id` is `Some`
on some frames and `None` on others within the **same execution**. The
decision is driven by a priority chain: global kill switch → per-function
`TracePolicy` → VM-level `TracingPolicy` → heuristics. See Section 5.

**Cost analysis**: `SpanId` is a `Uuid` (16 bytes). `Instant` is 8 bytes
on most platforms. Total overhead per frame: ~24 bytes, but only for
tracked frames. Untracked frames store `None`/`None` (0 bytes effective).
With `MAX_FRAMES = 256`, worst case is ~6 KB. The VM already clones
`Function` objects (which contain entire `Bytecode` structs with
`Vec<Instruction>`) — 24 bytes per frame is negligible.

---

## 4. VM Call Stack (Source of Truth)

### 4.1 Current Frame Lifecycle

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                     Frame Lifecycle (Current)                                 │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Instruction::Call(arg_count)                                                │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  1. Compute locals_offset from stack                                    │ │
│  │  2. Verify function object on stack                                     │ │
│  │  3. Check arity                                                         │ │
│  │  4. Check MAX_FRAMES                                                    │ │
│  │  5. Match on FunctionKind:                                              │ │
│  │     ┌─────────────────────────────────────────────────────────────────┐ │ │
│  │     │ Bytecode:                                                       │ │ │
│  │     │   frames.push(Frame {                                           │ │ │
│  │     │     function: index,                                            │ │ │
│  │     │     instruction_ptr: 0,                                         │ │ │
│  │     │     locals_offset,                                              │ │ │
│  │     │   });                                                           │ │ │
│  │     │   frame_idx = frames.len() - 1;                                 │ │ │
│  │     │   // execution continues in new frame                           │ │ │
│  │     ├─────────────────────────────────────────────────────────────────┤ │ │
│  │     │ Native(func_ptr):                                               │ │ │
│  │     │   // NO frame push                                              │ │ │
│  │     │   let result = func(self, &args)?;                              │ │ │
│  │     │   stack.drain(locals_offset..);                                 │ │ │
│  │     │   stack.push(result);                                           │ │ │
│  │     │   // execution continues in SAME frame                          │ │ │
│  │     ├─────────────────────────────────────────────────────────────────┤ │ │
│  │     │ SysOp(_):                                                       │ │ │
│  │     │   // ERROR: SysOps are dispatched via DispatchFuture             │ │ │
│  │     └─────────────────────────────────────────────────────────────────┘ │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Instruction::Return                                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  1. Pop result from eval stack                                          │ │
│  │  2. Clean up watched_vars in this frame's scope                         │ │
│  │  3. Drain eval stack to locals_offset                                   │ │
│  │  4. Push result back on eval stack                                      │ │
│  │  5. frames.pop()                                                        │ │
│  │  6. Check interrupt_frame                                               │ │
│  │  7. If frames.is_empty() → VmExecState::Complete                        │ │
│  │  8. Otherwise: resume previous frame                                    │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  interrupt() (watch filter execution)                                        │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  1. Set interrupt_frame = Some(frames.len())                            │ │
│  │  2. Push function + args onto eval stack                                │ │
│  │  3. frames.push(Frame { function_ptr, ip: 0, locals_offset })           │ │
│  │  4. Call exec() recursively (nested execution)                          │ │
│  │  5. On Return: interrupt_frame detected → return Complete               │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 4.2 Proposed: Enriched Frame Lifecycle

The key change: every `Call` instruction consults a **per-function decision**
to determine whether this particular call should be span-tracked. The VM
doesn't blindly track everything — it asks "should I track this one?"

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                     Frame Lifecycle (Proposed)                                │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Instruction::Call(arg_count)                                                │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  ... same steps 1-4 ...                                                 │ │
│  │                                                                         │ │
│  │  Bytecode path:                                                         │ │
│  │    // Per-function decision: should this call be span-tracked?          │ │
│  │    let should_track = self.should_track_call(&callee);                  │ │
│  │                                                                         │ │
│  │    let span_id = if should_track {                                      │ │
│  │        Some(SpanId::new())                                              │ │
│  │    } else {                                                             │ │
│  │        None                                                             │ │
│  │    };                                                                   │ │
│  │                                                                         │ │
│  │    frames.push(Frame {                                                  │ │
│  │      function: index,                                                   │ │
│  │      instruction_ptr: 0,                                                │ │
│  │      locals_offset,                                                     │ │
│  │      span_id,                       // ← NEW (None if not tracked)      │ │
│  │      started_at: span_id.map(|_| Instant::now()),  // ← NEW             │ │
│  │    });                                                                  │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Instruction::Return                                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  ... same steps 1-4 (watch cleanup, stack drain) ...                    │ │
│  │                                                                         │ │
│  │  // NEW: Capture frame info before popping                              │ │
│  │  let popped_frame = &self.frames[frame_idx];                            │ │
│  │  let frame_info = FrameExitInfo {                                       │ │
│  │      span_id: popped_frame.span_id.clone(),                             │ │
│  │      duration: popped_frame.started_at.map(|t| t.elapsed()),            │ │
│  │      function_name: function.name.clone(),                              │ │
│  │  };                                                                     │ │
│  │                                                                         │ │
│  │  frames.pop();                                                          │ │
│  │                                                                         │ │
│  │  // frame_info is available to the engine via pending_frame_exit        │ │
│  │  self.last_frame_exit = Some(frame_info);                               │ │
│  │  ... same steps 6-8 ...                                                 │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 4.3 VM Call Stack Snapshot

The VM provides a zero-copy-friendly snapshot of its call stack that
the engine can use for span context construction:

```rust
/// A lightweight snapshot of the VM's call stack at a point in time.
/// Used by the engine to construct SpanContext for events.
pub struct CallStackSnapshot {
    pub entries: Vec<CallStackEntry>,
}

pub struct CallStackEntry {
    pub function_name: String,
    pub span_id: Option<SpanId>,
    pub source_line: usize,
    pub function_span: baml_type::Span,
}

impl BexVm {
    /// Capture a snapshot of the current call stack.
    ///
    /// This is called by the engine at yield points (ScheduleFuture, Await,
    /// Notify) to get the current span context for event emission.
    ///
    /// Cost: O(n) where n = number of frames (typically < 10).
    pub fn call_stack_snapshot(&self) -> CallStackSnapshot {
        let entries = self.frames.iter().map(|frame| {
            let function = self.get_object(frame.function)
                .as_function()
                .expect("frame.function must point to a Function");

            let last_ip = frame.instruction_ptr.saturating_sub(1) as usize;
            let source_line = function.bytecode.source_lines
                .get(last_ip)
                .copied()
                .unwrap_or(0);

            CallStackEntry {
                function_name: function.name.clone(),
                span_id: frame.span_id.clone(),
                source_line,
                function_span: function.span,
            }
        }).collect();

        CallStackSnapshot { entries }
    }
}
```

---

## 5. Per-Function Tracing Control

### 5.1 The Core Idea

Not every function call needs to generate observability events. A BAML
program may have hundreds of calls — utility functions, internal helpers,
list comprehensions, watch filters — and tracing all of them would be
noisy, expensive, and unhelpful. Instead, **every `Call` instruction in
the VM can independently decide whether to assign a `span_id`** to the
new frame.

The boolean decision is made per-call, not globally. This means:

- An LLM-calling function like `ExtractResume` gets tracked.
- An internal helper like `format_prompt` does not (unless explicitly opted in).
- The engine can override at any time (e.g., "trace everything in debug mode").
- A user-annotated function can always be tracked regardless of engine policy.

### 5.2 Decision Hierarchy

The per-function tracking decision is evaluated as a **priority chain**:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                Per-Function Tracing Decision Hierarchy                        │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  On Instruction::Call(arg_count):                                            │
│                                                                              │
│  ┌─────────────────────────────────────────────┐                             │
│  │  1. Is tracing_active == false on the VM?   │                             │
│  │     YES → span_id = None (global kill switch)│                            │
│  │     NO  → continue ↓                        │                             │
│  └───────────────────┬─────────────────────────┘                             │
│                      │                                                       │
│  ┌───────────────────▼─────────────────────────┐                             │
│  │  2. Does the Function have TracePolicy::     │                            │
│  │     Never?                                   │                            │
│  │     YES → span_id = None                     │                            │
│  │     NO  → continue ↓                        │                             │
│  └───────────────────┬─────────────────────────┘                             │
│                      │                                                       │
│  ┌───────────────────▼─────────────────────────┐                             │
│  │  3. Does the Function have TracePolicy::     │                            │
│  │     Always?                                  │                            │
│  │     YES → span_id = Some(SpanId::new())      │                            │
│  │     NO  → continue ↓                        │                             │
│  └───────────────────┬─────────────────────────┘                             │
│                      │                                                       │
│  ┌───────────────────▼─────────────────────────┐                             │
│  │  4. Consult the VM's tracing_policy:         │                            │
│  │     TraceAll  → span_id = Some(...)          │                            │
│  │     TraceNone → span_id = None               │                            │
│  │     TraceAuto → see rule 5 below             │                            │
│  └───────────────────┬─────────────────────────┘                             │
│                      │                                                       │
│  ┌───────────────────▼─────────────────────────┐                             │
│  │  5. TraceAuto: default heuristics            │                            │
│  │     • FunctionKind::Bytecode → YES (it's     │                            │
│  │       a user-defined function)               │                            │
│  │     • Has SysOp calls → YES (it does I/O)    │                            │
│  │     • Marked @traceable → YES                │                            │
│  │     • Is a watch filter → NO (by default)    │                            │
│  │     • Otherwise → NO                         │                            │
│  └─────────────────────────────────────────────┘                             │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.3 Function-Level Metadata: `TracePolicy`

Each compiled `Function` object can carry a `TracePolicy` that the
compiler emits based on annotations, function kind, or static analysis:

```rust
/// Per-function policy for whether this function's calls should
/// be span-tracked.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TracePolicy {
    /// Always track this function (e.g., user wrote @trace or it's an LLM fn).
    Always,
    /// Never track this function (e.g., internal builtin, trivial helper).
    Never,
    /// Defer to the VM/engine's tracing_policy (default for most functions).
    #[default]
    Auto,
}

pub struct Function {
    // ... existing fields ...
    pub name: String,
    pub kind: FunctionKind,
    pub arity: usize,
    pub span: baml_type::Span,
    pub bytecode: Bytecode,

    // NEW
    /// Controls whether calls to this function generate span events.
    pub trace_policy: TracePolicy,
}
```

The compiler sets `TracePolicy` during emit:

| Source construct | `TracePolicy` |
|---|---|
| `function ExtractResume` (user-defined BAML function) | `Auto` (traced under `TraceAll` or `TraceAuto` default) |
| `function call_llm_function` (from `llm.baml`) | `Always` (it's the LLM boundary) |
| Internal helpers like `deep_equals`, `array.length` | `Never` |
| User writes `@trace` annotation on a function | `Always` |
| User writes `@notrace` annotation | `Never` |
| Watch filter functions (compiler-generated lambdas) | `Auto` (traced only under `TraceAll`) |

### 5.4 VM-Level Policy: `TracingPolicy`

The VM carries a `TracingPolicy` that the engine sets before execution begins.
This is the "global dial" that the engine turns based on whether the host
requested tracing:

```rust
/// VM-level tracing policy. Set by the engine before calling exec().
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingPolicy {
    /// All function calls generate spans (except TracePolicy::Never functions).
    /// Useful for debugging, "trace everything" mode.
    TraceAll,
    /// No function calls generate spans (even TracePolicy::Always is suppressed).
    /// The absolute kill switch.
    TraceNone,
    /// Use per-function TracePolicy + heuristics (the default runtime mode).
    /// Only functions that "matter" (user-defined, LLM-calling, @trace) are tracked.
    TraceAuto,
}

pub struct BexVm {
    // ... existing fields ...

    /// Whether tracing is active at all. Quick kill switch.
    pub tracing_active: bool,

    /// Fine-grained policy for which functions to trace.
    /// Only consulted when tracing_active == true.
    pub tracing_policy: TracingPolicy,
}
```

### 5.5 The `should_track_call` Decision Function

This is the per-call decision that replaces the flat `if self.tracing_active`
check:

```rust
impl BexVm {
    /// Decide whether a specific function call should be span-tracked.
    ///
    /// Called during Instruction::Call to determine if the new Frame
    /// gets a span_id.
    fn should_track_call(&self, callee: &Function) -> bool {
        // Global kill switch
        if !self.tracing_active {
            return false;
        }

        // Per-function policy takes precedence (unless globally suppressed)
        match callee.trace_policy {
            TracePolicy::Always => return true,
            TracePolicy::Never  => return false,
            TracePolicy::Auto   => {} // fall through to VM policy
        }

        // VM-level policy
        match self.tracing_policy {
            TracingPolicy::TraceAll  => true,
            TracingPolicy::TraceNone => false,
            TracingPolicy::TraceAuto => {
                // Default heuristic: track user-defined bytecode functions.
                // Native functions don't push frames anyway, so this
                // effectively means "track all BAML-authored function calls."
                matches!(callee.kind, FunctionKind::Bytecode { .. })
            }
        }
    }
}
```

### 5.6 How Untracked Frames Affect Span Context

When some frames have `span_id = None`, the span context derivation
must **skip over them** to find the nearest tracked ancestor. This is
important: an untracked utility frame shouldn't break the parent-child
chain.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│            Span Context with Mixed Tracked/Untracked Frames                  │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  VM frames (tracing_policy = TraceAuto):                                     │
│  ┌────────────┬──────────────┬────────────────┬────────────────────────────┐ │
│  │ Frame 0    │ Frame 1      │ Frame 2        │ Frame 3                    │ │
│  │ main()     │ format()     │ ExtractResume()│ call_llm_function()        │ │
│  │ span: aaa  │ span: None   │ span: ccc      │ span: ddd                  │ │
│  │ (tracked)  │ (untracked)  │ (tracked)      │ (tracked)                  │ │
│  └────────────┴──────────────┴────────────────┴────────────────────────────┘ │
│        ▲                            ▲               ▲                        │
│        │                            │               │                        │
│        │  format() has TracePolicy::Never, so no span_id.                    │
│        │                                                                     │
│  When deriving SpanContext at Frame 3 (call_llm_function):                   │
│                                                                              │
│    span_id:        ddd   (current frame — has span)                          │
│    parent_span_id: ccc   (walk backwards, skip Frame 1 which has no span,   │
│                           find Frame 2 which has span ccc)                   │
│    root_span_id:   aaa   (walk to Frame 0)                                   │
│                                                                              │
│  ❌ WRONG (naive approach): parent_span_id = None (Frame 2 is immediate     │
│     parent, but if we had used Frame 2 it would be correct; the danger is    │
│     if Frame 2 was the untracked one)                                        │
│                                                                              │
│  Consider: Frame 2 is untracked instead:                                     │
│  ┌────────────┬──────────────┬────────────────┬────────────────────────────┐ │
│  │ Frame 0    │ Frame 1      │ Frame 2        │ Frame 3                    │ │
│  │ main()     │ pipeline()   │ helper()       │ call_llm_function()        │ │
│  │ span: aaa  │ span: bbb    │ span: None     │ span: ddd                  │ │
│  └────────────┴──────────────┴────────────────┴────────────────────────────┘ │
│                                                                              │
│    span_id:        ddd                                                       │
│    parent_span_id: bbb   ← walks past Frame 2 (None) to Frame 1 (bbb)       │
│    root_span_id:   aaa                                                       │
│                                                                              │
│  The event tree still looks correct:                                         │
│    main (aaa)                                                                │
│      └─ pipeline (bbb)                                                       │
│           └─ call_llm_function (ddd)   ← helper() is invisible               │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.7 Updated `span_context_from_vm` with Skip Logic

```rust
impl BexEngine {
    /// Derive SpanContext from the VM's frames, skipping untracked frames
    /// when looking for parent and root.
    fn span_context_from_vm(
        &self,
        vm: &BexVm,
        host_parent: &Option<SpanId>,
    ) -> SpanContext {
        let frames = &vm.frames;

        // Current span = top frame's span_id (it should be tracked if
        // we're emitting an event, but handle None defensively)
        let span_id = frames.last()
            .and_then(|f| f.span_id.clone())
            .unwrap_or_else(SpanId::new);

        // Parent span = walk backwards from top, skip untracked frames
        let parent_span_id = frames.iter().rev()
            .skip(1)                           // skip current frame
            .find_map(|f| f.span_id.clone())   // first ancestor with a span
            .or_else(|| host_parent.clone());   // or the host @trace span

        // Root span = walk from bottom, find first tracked frame
        let root_span_id = frames.iter()
            .find_map(|f| f.span_id.clone())
            .unwrap_or_else(SpanId::new);

        SpanContext { span_id, parent_span_id, root_span_id }
    }
}
```

### 5.8 Engine Controls per `call_function` Invocation

The engine decides the tracing policy per top-level invocation. This
means different calls into the VM can have different policies:

```rust
impl BexEngine {
    pub async fn call_function(
        &self,
        name: &str,
        args: Vec<BexValue>,
        // NEW: the caller controls whether/how to trace
        tracing: TracingConfig,
    ) -> Result<BexValue, EngineError> {
        let vm = self.get_or_create_vm();

        // Set VM tracing state based on caller's config
        vm.tracing_active = tracing.enabled;
        vm.tracing_policy = tracing.policy;

        // If tracing is active, assign a root span to the entry frame
        if tracing.enabled {
            if let Some(frame) = vm.frames.last_mut() {
                frame.span_id = Some(tracing.root_span_id
                    .unwrap_or_else(SpanId::new));
                frame.started_at = Some(web_time::Instant::now());
            }
        }

        self.run_event_loop(vm, tracing).await
    }
}

/// Configuration for a single function invocation's tracing behavior.
pub struct TracingConfig {
    /// Master switch: is tracing on at all?
    pub enabled: bool,
    /// Which functions to trace (only consulted when enabled = true).
    pub policy: TracingPolicy,
    /// Root span ID (from host @trace or auto-generated).
    pub root_span_id: Option<SpanId>,
    /// Parent span ID from host language (if called inside @trace).
    pub parent_span_id: Option<SpanId>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            policy: TracingPolicy::TraceAuto,
            root_span_id: None,
            parent_span_id: None,
        }
    }
}
```

### 5.9 Use Cases for Per-Function Control

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                 Use Cases for Per-Function Tracing Control                    │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Case 1: Production (TraceAuto)                                         │ │
│  │                                                                         │ │
│  │  Only LLM functions and user-defined BAML functions are tracked.        │ │
│  │  Internal helpers, watch filters, builtins are silent.                  │ │
│  │                                                                         │ │
│  │    main()        → tracked (user-defined)                               │ │
│  │    format_prompt  → NOT tracked (helper, TracePolicy::Never)            │ │
│  │    ExtractResume → tracked (user-defined)                               │ │
│  │    call_llm_func → tracked (TracePolicy::Always from llm.baml)         │ │
│  │    json_parse     → NOT tracked (builtin)                               │ │
│  │                                                                         │ │
│  │  Resulting span tree:                                                   │ │
│  │    main                                                                 │ │
│  │      └─ ExtractResume                                                   │ │
│  │           └─ call_llm_function                                          │ │
│  │                ├─ LlmRequest                                            │ │
│  │                └─ LlmResponse                                           │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Case 2: Debug Mode (TraceAll)                                          │ │
│  │                                                                         │ │
│  │  Every function call is tracked (except TracePolicy::Never).            │ │
│  │  Useful for diagnosing issues or building a full flame graph.           │ │
│  │                                                                         │ │
│  │    main()        → tracked                                              │ │
│  │    format_prompt  → tracked (even though it's a helper)                 │ │
│  │    ExtractResume → tracked                                              │ │
│  │    call_llm_func → tracked                                              │ │
│  │    json_parse     → NOT tracked (TracePolicy::Never overrides)          │ │
│  │                                                                         │ │
│  │  Resulting span tree:                                                   │ │
│  │    main                                                                 │ │
│  │      └─ format_prompt                                                   │ │
│  │      └─ ExtractResume                                                   │ │
│  │           └─ call_llm_function                                          │ │
│  │                ├─ LlmRequest                                            │ │
│  │                └─ LlmResponse                                           │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Case 3: Silent / Batch Mode (TraceNone or tracing_active = false)      │ │
│  │                                                                         │ │
│  │  No functions are tracked. Zero overhead. Useful for batch inference    │ │
│  │  or tests where observability is not needed.                            │ │
│  │                                                                         │ │
│  │    All frames: span_id = None, started_at = None                        │ │
│  │    No events emitted. Exception stack traces still work (they           │ │
│  │    just won't have span_ids).                                           │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  Case 4: Selective @trace from Host Language                            │ │
│  │                                                                         │ │
│  │  Python/TS code wraps a call in @trace → engine sets tracing_active     │ │
│  │  = true for that call. Other calls in the same process may not          │ │
│  │  have tracing active.                                                   │ │
│  │                                                                         │ │
│  │    # Python                                                             │ │
│  │    @trace                                                               │ │
│  │    async def my_pipeline(text):                                         │ │
│  │        # This call: tracing_active=true, TraceAuto                      │ │
│  │        result = await b.ExtractResume(text)                             │ │
│  │        return result                                                    │ │
│  │                                                                         │ │
│  │    async def batch_process(texts):                                      │ │
│  │        # This call: tracing_active=false (no @trace)                    │ │
│  │        results = [await b.ExtractResume(t) for t in texts]              │ │
│  │        return results                                                   │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 5.10 Interaction with Exception Stack Traces

Per-function tracing does NOT affect exception stack traces. Even when
`span_id` is `None` on a frame, the frame itself still exists in the
VM's `frames` vector. The `stack_trace()` method always includes every
frame — it just reports `span_id: None` for untracked ones:

```
  Traceback (most recent call last):
    File "main.baml", line 5, in main [span:aaa]
    File "helpers.baml", line 12, in format_prompt       ← no span (untracked)
    File "extract.baml", line 3, in ExtractResume [span:ccc]
  TypeError: expected string, got int
    (trace root: aaa)
```

This is the right behavior: exception traces show the full call stack
regardless of whether individual functions were being observed.

---

## 6. Observability Span Stack (Engine-Side)

### 6.1 Deriving SpanContext from VM Frames

Instead of the engine maintaining its own `SpanStack` (as proposed in
event-publishing-design-v2 Milestone 4), it derives `SpanContext` from
the VM's frames:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                 SpanContext Derivation from VM Frames                         │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  VM frames:                                                                  │
│  ┌─────────────┬───────────────┬──────────────────┬────────────────────────┐ │
│  │ Frame 0     │ Frame 1       │ Frame 2          │ Frame 3                │ │
│  │ main()      │ pipeline()    │ ExtractResume()  │ call_llm_function()    │ │
│  │ span: aaa   │ span: bbb     │ span: ccc        │ span: ddd              │ │
│  └─────────────┴───────────────┴──────────────────┴────────────────────────┘ │
│                                                                              │
│  vm.call_stack_snapshot() at Frame 3:                                        │
│                                                                              │
│    entries: [                                                                │
│      { name: "main",              span_id: aaa },                            │
│      { name: "pipeline",          span_id: bbb },                            │
│      { name: "ExtractResume",     span_id: ccc },                            │
│      { name: "call_llm_function", span_id: ddd },                            │
│    ]                                                                         │
│                                                                              │
│  Engine derives SpanContext:                                                 │
│                                                                              │
│    SpanContext {                                                              │
│      span_id: ddd,            // current frame (top of stack)                │
│      parent_span_id: ccc,     // previous frame                              │
│      root_span_id: aaa,       // first frame (or engine-provided root)       │
│    }                                                                         │
│                                                                              │
│  This is ALWAYS correct because it reads directly from the VM's              │
│  authoritative call stack. No synchronization needed.                        │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 6.2 Engine Integration

The engine now uses `TracingConfig` (see Section 5.8) to configure the
VM before execution. The key difference from the earlier flat approach:
the engine no longer just sets `tracing_active = true/false`. It passes
a full `TracingConfig` that controls both the global switch and the
per-function policy.

```rust
impl BexEngine {
    async fn run_event_loop_with_epoch(
        &self,
        vm: &mut BexVm,
        my_epoch: u64,
        tracing: &TracingConfig,
    ) -> Result<BexValue, EngineError> {
        // Configure VM tracing from the caller's TracingConfig
        vm.tracing_active = tracing.enabled;
        vm.tracing_policy = tracing.policy;

        // Assign root span_id to the entry-point frame.
        if tracing.enabled {
            if let Some(frame) = vm.frames.last_mut() {
                frame.span_id = Some(tracing.root_span_id
                    .clone()
                    .unwrap_or_else(SpanId::new));
                frame.started_at = Some(web_time::Instant::now());
            }
        }

        loop {
            match vm.exec()? {
                VmExecState::ScheduleFuture(id) => {
                    let pending = vm.pending_future(id)?;

                    if pending.operation == SysOp::EventSend {
                        // Derive span context from VM frames, skipping untracked
                        let ctx = self.span_context_from_vm(vm, &tracing.parent_span_id);
                        // ... build and emit event using ctx ...
                        continue;
                    }
                    // ... existing SysOp dispatch ...
                }

                VmExecState::Complete(value) => {
                    return Ok(self.value_to_external(value));
                }

                // ... Await, Notify handlers ...
            }
        }
    }

    /// Derive SpanContext from the VM's current call stack.
    /// This replaces the SpanStack entirely.
    ///
    /// When frames have mixed span_id (Some/None due to per-function
    /// tracing), we walk backwards skipping untracked frames to find
    /// the true parent. See Section 5.7 for the full skip logic.
    fn span_context_from_vm(
        &self,
        vm: &BexVm,
        host_parent: &Option<SpanId>,
    ) -> SpanContext {
        let frames = &vm.frames;

        // Current span = top frame's span_id
        let span_id = frames.last()
            .and_then(|f| f.span_id.clone())
            .unwrap_or_else(SpanId::new);

        // Parent span = walk backwards, skip untracked frames
        let parent_span_id = frames.iter().rev()
            .skip(1)
            .find_map(|f| f.span_id.clone())
            .or_else(|| host_parent.clone());

        // Root span = first tracked frame
        let root = frames.iter()
            .find_map(|f| f.span_id.clone())
            .unwrap_or_else(SpanId::new);

        SpanContext { span_id, parent_span_id, root_span_id: root }
    }
}
```

### 6.3 Comparison: SpanStack vs. VM-Derived

| Aspect | SpanStack (v2 proposal) | VM-Derived (this proposal) |
|--------|------------------------|---------------------------|
| **Push/pop** | Manual in engine on `EventSend` interception | Automatic on `Call`/`Return` instructions |
| **Sync with VM** | Must be manually kept in sync | Always in sync (same data source) |
| **Native calls** | Invisible (no frame push) | Invisible (same — no frame push) |
| **`interrupt()`** | Invisible (engine doesn't know) | Visible (frame is in `frames`) |
| **Exception safety** | `function_end` may be skipped | Duration computed from `started_at` on frame pop, even during unwind |
| **Zero-cost when off** | `SpanStack` still exists (empty) | `span_id: None`, `started_at: None` — truly zero fields |
| **Cross-cutting** | Separate data structure | Reuses existing `Frame` |
| **Host @trace parent** | `parent_of_root` field on SpanStack | `parent_span_id` parameter at call site |

---

## 7. Watch Dependency Tracking

### 7.1 How Watch Interacts with Call Stack Today

The watch system doesn't use the call stack directly, but it uses
**scope-based cleanup** tied to frames:

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                     Watch ↔ Frame Interaction                                │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  Instruction::Watch(index)                                                   │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  let abs_index = frame.locals_offset + index;                           │ │
│  │  self.watched_vars.insert(abs_index, (channel, var_name));              │ │
│  │  let var_node = NodeId::LocalVar(abs_index);                            │ │
│  │  watch.register_root(var_node, RootState { ... });                      │ │
│  │  // Link to object graph via track_watch_dependencies()                 │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Instruction::Return                                                         │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  // Clean up watched vars in this frame's scope:                        │ │
│  │  for i in frame.locals_offset..stack.len() {                            │ │
│  │      if watched_vars.remove(&i).is_some() {                             │ │
│  │          watch.unregister_root(NodeId::LocalVar(i));                     │ │
│  │          // Unlink object edges                                         │ │
│  │      }                                                                  │ │
│  │  }                                                                      │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Instruction::StoreVar(index) when var is watched                            │
│  ┌─────────────────────────────────────────────────────────────────────────┐ │
│  │  // Update dependency graph:                                            │ │
│  │  update_watched_node(watched_node, Path::Binding, old_value, new_value) │ │
│  │  // Run filters:                                                        │ │
│  │  let notifications = process_notifications(watched_node)?;              │ │
│  │  // If any pass filter → yield Notify                                   │ │
│  └─────────────────────────────────────────────────────────────────────────┘ │
│                                                                              │
│  Key insight: watched_vars keys are ABSOLUTE stack indices                    │
│  (frame.locals_offset + relative_index). This ties them to specific          │
│  stack slots, not to frames. Cleanup happens by scanning the range           │
│  [frame.locals_offset..stack.len()].                                         │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 7.2 Watch Scope vs. Call Stack: No Conflict

The watch system does **not** need its own call stack. It operates on:

- **Dependency graph** (`Watch` struct): Nodes are `NodeId::LocalVar(StackIndex)`
  or `NodeId::HeapObject(HeapPtr)`. This is a graph, not a stack.
- **Scope cleanup**: Uses `frame.locals_offset` to know which stack range
  to scan on `Return`. This correctly handles nested calls because each
  frame owns a contiguous region of the eval stack.

**No change needed** for the watch system under this proposal. The
enriched `Frame` adds `span_id` and `started_at` which the watch system
simply ignores. The cleanup logic continues to use `locals_offset` exactly
as before.

### 7.3 Watch Notifications and Span Context

When a watched variable changes and the VM yields
`VmExecState::Notify(WatchNotification::Variables(...))`, the engine now
has access to the span context from the VM's frames:

```rust
VmExecState::Notify(WatchNotification::Variables(roots)) => {
    // The VM's frames tell us exactly where we are in the call tree.
    let ctx = self.span_context_from_vm(vm, &root_span_id, &host_parent);

    // Future: emit watch events with full span context
    // event_bus::emit(RuntimeEvent {
    //     ctx,
    //     event: EventKind::Watch(WatchEvent { roots, ... }),
    // });
}
```

This is a free bonus of the VM-centric approach — watch events
automatically get correct span context without any extra plumbing.

---

## 8. Exception Call Stacks

### 8.1 Current Exception Stack Trace

The VM already builds stack traces from frames:

```rust
// bex_vm/src/vm.rs (line 787-816)
pub fn stack_trace(&self, error: VmError) -> StackTrace {
    let trace = self.frames.iter().map(|frame| {
        let function = self.get_object(frame.function).as_function()?.clone();
        let last_executed = frame.instruction_ptr.saturating_sub(1);

        Ok(ErrorLocation {
            function_name: function.name.clone(),
            function_span: function.span,
            error_line: function.bytecode.source_lines[last_executed as usize],
        })
    }).collect::<Result<Vec<_>, _>>()
      .unwrap_or_default();

    StackTrace { error, trace }
}
```

Current output format:

```
Traceback (most recent call last):
  File "main.baml", line 5, in main
  File "pipeline.baml", line 12, in process
  File "extract.baml", line 3, in ExtractResume
stack overflow
```

### 8.2 Proposed: Rich Exception Stack Trace

With `span_id` on each frame, exception traces gain observability context:

```rust
/// An enhanced error location with span context.
#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub function_name: String,
    pub function_span: baml_type::Span,
    pub error_line: usize,
    // NEW:
    pub span_id: Option<SpanId>,
}

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub error: VmError,
    pub trace: Vec<ErrorLocation>,
    // NEW: root_span_id for correlating with observability events
    pub root_span_id: Option<SpanId>,
}

impl BexVm {
    pub fn stack_trace(&self, error: VmError) -> StackTrace {
        let root_span_id = self.frames.first()
            .and_then(|f| f.span_id.clone());

        let trace = self.frames.iter().map(|frame| {
            let function = self.get_object(frame.function)
                .as_function()?.clone();
            let last_ip = frame.instruction_ptr.saturating_sub(1) as usize;

            Ok(ErrorLocation {
                function_name: function.name.clone(),
                function_span: function.span,
                error_line: function.bytecode.source_lines
                    .get(last_ip).copied().unwrap_or(0),
                span_id: frame.span_id.clone(),
            })
        }).collect::<Result<Vec<_>, _>>()
          .unwrap_or_default();

        StackTrace { error, trace, root_span_id }
    }
}
```

### 8.3 Exception Flow: From VM to Host Language

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                        Exception Call Stack Flow                             │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  VM encounters error (e.g., DivisionByZero in ExtractResume)                 │
│                                                                              │
│  VM frames at error time:                                                    │
│  ┌───────────┬──────────────┬────────────────────┐                           │
│  │ Frame 0   │ Frame 1      │ Frame 2            │                           │
│  │ main()    │ pipeline()   │ ExtractResume()    │ ← error here              │
│  │ span:aaa  │ span:bbb     │ span:ccc           │                           │
│  │ line:5    │ line:12      │ line:3             │                           │
│  └───────────┴──────────────┴────────────────────┘                           │
│                                                                              │
│  Step 1: VM returns Err(VmError::RuntimeError(...))                          │
│                                                                              │
│  Step 2: Engine catches error, calls vm.stack_trace(error):                  │
│  ┌─────────────────────────────────────────────────────────────────────┐     │
│  │  StackTrace {                                                       │     │
│  │    error: DivisionByZero { left: 42, right: 0 },                    │     │
│  │    root_span_id: Some(aaa),                                         │     │
│  │    trace: [                                                         │     │
│  │      ErrorLocation {                                                │     │
│  │        function_name: "main",                                       │     │
│  │        function_span: Span { file: "main.baml", ... },              │     │
│  │        error_line: 5,                                               │     │
│  │        span_id: Some(aaa),                                          │     │
│  │      },                                                             │     │
│  │      ErrorLocation {                                                │     │
│  │        function_name: "pipeline",                                   │     │
│  │        function_span: Span { file: "pipeline.baml", ... },          │     │
│  │        error_line: 12,                                              │     │
│  │        span_id: Some(bbb),                                          │     │
│  │      },                                                             │     │
│  │      ErrorLocation {                                                │     │
│  │        function_name: "ExtractResume",                              │     │
│  │        function_span: Span { file: "extract.baml", ... },           │     │
│  │        error_line: 3,                                               │     │
│  │        span_id: Some(ccc),                                          │     │
│  │      },                                                             │     │
│  │    ],                                                               │     │
│  │  }                                                                  │     │
│  └─────────────────────────────────────────────────────────────────────┘     │
│                                                                              │
│  Step 3: Engine emits FunctionEnd with error + emits a trace event:          │
│  ┌─────────────────────────────────────────────────────────────────────┐     │
│  │  event_bus::emit(RuntimeEvent {                                     │     │
│  │    ctx: SpanContext {                                               │     │
│  │      span_id: ccc,           // where error occurred                │     │
│  │      parent_span_id: bbb,    // caller                              │     │
│  │      root_span_id: aaa,      // top-level call                      │     │
│  │    },                                                               │     │
│  │    event: EventKind::Function(FunctionEvent::End(FunctionEnd {      │     │
│  │      name: "ExtractResume",                                         │     │
│  │      result: Err("division by zero: 42 / 0"),                       │     │
│  │      duration: started_at.elapsed(),  // from Frame.started_at      │     │
│  │    })),                                                             │     │
│  │  });                                                                │     │
│  └─────────────────────────────────────────────────────────────────────┘     │
│                                                                              │
│  Step 4: Engine propagates error + unwinds:                                  │
│    For each remaining frame (from top to bottom), emit FunctionEnd           │
│    with the error. Duration is computed from each frame's started_at.        │
│                                                                              │
│  Step 5: Host language receives EngineError with rich StackTrace:            │
│    - Python/TS can format the trace                                          │
│    - root_span_id lets it correlate with Collector logs                      │
│    - span_ids let the Boundary dashboard link errors to trace tree           │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

### 8.4 Exception Unwinding and FunctionEnd Emission

When an error occurs, the engine must emit `FunctionEnd` events for all
frames in the call stack (from innermost to outermost) so the event tree
stays balanced:

```rust
// In BexEngine, after catching a VmError:

fn emit_unwind_events(
    &self,
    vm: &BexVm,
    error: &VmError,
    root_span_id: &Option<SpanId>,
    host_parent: &Option<SpanId>,
) {
    // Walk frames from top (innermost) to bottom (outermost)
    for i in (0..vm.frames.len()).rev() {
        let frame = &vm.frames[i];
        let Some(span_id) = &frame.span_id else { continue };

        let function = vm.get_object(frame.function)
            .as_function()
            .map(|f| f.name.clone())
            .unwrap_or_else(|_| "<unknown>".into());

        let parent_span_id = if i > 0 {
            vm.frames[i - 1].span_id.clone()
        } else {
            host_parent.clone()
        };

        let root = vm.frames.first()
            .and_then(|f| f.span_id.clone())
            .or_else(|| root_span_id.clone())
            .unwrap_or_else(SpanId::new);

        event_bus::emit(RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id,
                root_span_id: root,
            },
            timestamp: web_time::SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(FunctionEnd {
                name: function,
                result: Err(error.to_string()),
                duration: frame.started_at
                    .map(|t| t.elapsed())
                    .unwrap_or_default(),
            })),
        });
    }
}
```

### 8.5 Future: try/catch and Exception Handlers

When BAML adds `try`/`catch` syntax, the VM will need:

1. **Exception handler table per function**: Maps instruction ranges to
   handler offsets (similar to Java's exception table or Python's
   `except` blocks).

2. **Stack unwinding**: On error, the VM walks frames looking for a
   handler. For each frame without a handler, it performs the same cleanup
   as `Return` (watch cleanup, eval stack drain) plus emitting
   `FunctionEnd(Err(...))` via the enriched frame.

3. **`span_id` survival**: Because the span info is on the frame itself
   (not in a separate stack), unwinding naturally preserves span context
   — each popped frame's `span_id` is available for the error event.

```
┌──────────────────────────────────────────────────────────────────────────────┐
│                   Future: try/catch Exception Flow                            │
├──────────────────────────────────────────────────────────────────────────────┤
│                                                                              │
│  VM frames:                                                                  │
│  ┌────────┬──────────┬──────────────┬──────────────────┐                     │
│  │ main() │ try {    │ pipeline()   │ ExtractResume()  │ ← error             │
│  │ s:aaa  │ handler  │ s:bbb        │ s:ccc            │                     │
│  │        │ at ip:20 │              │                  │                     │
│  └────────┴──────────┴──────────────┴──────────────────┘                     │
│                                                                              │
│  Unwind sequence:                                                            │
│  1. Pop ExtractResume (s:ccc) → emit FunctionEnd(Err) with span ccc          │
│  2. Pop pipeline (s:bbb) → emit FunctionEnd(Err) with span bbb              │
│  3. Reach main() → find handler at ip:20 → jump to catch block              │
│  4. Push error value onto stack                                              │
│  5. Continue execution in catch block                                        │
│                                                                              │
│  Note: span_ids on frames mean we automatically get correct error            │
│  events during unwinding without any extra bookkeeping.                       │
│                                                                              │
└──────────────────────────────────────────────────────────────────────────────┘
```

---

## 9. Unification: VM as the Single Source of Truth

### 9.1 Complete Data Flow Diagram

```
┌──────────────────────────────────────────────────────────────────────────────────┐
│                        Unified Call Stack Architecture                            │
├──────────────────────────────────────────────────────────────────────────────────┤
│                                                                                  │
│  Host Language (Python/TS)                                                       │
│  ┌──────────────────────────────────────────────┐                                │
│  │  @trace my_pipeline()                         │                                │
│  │    HostSpanManager.enter()                    │                                │
│  │      → emit FunctionStart (host span)         │                                │
│  │      → host_span_id = SpanId(xxx)             │                                │
│  │                                               │                                │
│  │    b.ExtractResume(text)                      │                                │
│  │      └─ bridge_cffi passes host_span_id ──────│──┐                             │
│  │                                               │  │                             │
│  │    HostSpanManager.exit()                     │  │                             │
│  │      → emit FunctionEnd (host span)           │  │                             │
│  └──────────────────────────────────────────────┘  │                             │
│                                                    │                             │
│  ┌─────────────────────────────────────────────────┼────────────────────────────┐ │
│  │  Engine                                         │                            │ │
│  │                                                 ▼                            │ │
│  │  call_function("ExtractResume", args,                                        │ │
│  │    root_span_id: Some(yyy),                                                  │ │
│  │    parent_span_id: Some(xxx))    ← from host @trace                          │ │
│  │                                                                              │ │
│  │  ┌───────────────────────────────────────────────────────────────────────┐   │ │
│  │  │  VM                                                                   │   │ │
│  │  │                                                                       │   │ │
│  │  │  frames[0]: { fn: ExtractResume, span: yyy, started: t0 }             │   │ │
│  │  │  frames[1]: { fn: call_llm_function, span: zzz, started: t1 }        │   │ │
│  │  │                                                                       │   │ │
│  │  │  ── DispatchFuture(SysOp::EventSend) ──                               │   │ │
│  │  │  VM yields, engine intercepts:                                        │   │ │
│  │  │                                                                       │   │ │
│  │  └───────────────────────────────┬───────────────────────────────────────┘   │ │
│  │                                  │                                           │ │
│  │  Engine reads VM frames:         │                                           │ │
│  │    ctx = {                       │                                           │ │
│  │      span_id: zzz,              │  (top frame)                              │ │
│  │      parent_span_id: yyy,       │  (frame below)                            │ │
│  │      root_span_id: yyy,         │  (bottom frame)                           │ │
│  │    }                             │                                           │ │
│  │                                  │                                           │ │
│  │  event_bus::emit(RuntimeEvent {  │                                           │ │
│  │    ctx, event: LlmRequest{...}   │                                           │ │
│  │  })                              │                                           │ │
│  │                                  │                                           │ │
│  └──────────────────────────────────┼───────────────────────────────────────────┘ │
│                                     │                                             │
│  ┌──────────────────────────────────┼───────────────────────────────────────────┐ │
│  │  EventStore                      │                                           │ │
│  │                                  ▼                                           │ │
│  │  Events (all share root_span_id = yyy):                                      │ │
│  │    • FunctionStart(my_pipeline)     ctx: {span:xxx, parent:None, root:xxx}   │ │
│  │    • FunctionStart(ExtractResume)   ctx: {span:yyy, parent:xxx,  root:xxx}   │ │
│  │    • FunctionStart(call_llm_func)   ctx: {span:zzz, parent:yyy,  root:xxx}   │ │
│  │    • LlmRequest(...)               ctx: {span:zzz, parent:yyy,  root:xxx}   │ │
│  │    • LlmResponse(...)              ctx: {span:zzz, parent:yyy,  root:xxx}   │ │
│  │    • FunctionEnd(call_llm_func)     ctx: {span:zzz, parent:yyy,  root:xxx}   │ │
│  │    • FunctionEnd(ExtractResume)     ctx: {span:yyy, parent:xxx,  root:xxx}   │ │
│  │    • FunctionEnd(my_pipeline)       ctx: {span:xxx, parent:None, root:xxx}   │ │
│  │                                                                              │ │
│  │  Reconstructed tree:                                                         │ │
│  │    my_pipeline (xxx)                                                         │ │
│  │      └─ ExtractResume (yyy)                                                  │ │
│  │           └─ call_llm_function (zzz)                                         │ │
│  │                ├─ LlmRequest                                                 │ │
│  │                └─ LlmResponse                                                │ │
│  │                                                                              │ │
│  └──────────────────────────────────────────────────────────────────────────────┘ │
│                                                                                  │
└──────────────────────────────────────────────────────────────────────────────────┘
```

### 9.2 How Each Subsystem Uses the Unified Frame

| Subsystem | What it reads from `Frame` | When |
|-----------|--------------------------|------|
| **Execution** | `function`, `instruction_ptr`, `locals_offset` | Every instruction cycle |
| **Per-function decision** | `function` → `TracePolicy` + VM's `TracingPolicy` | On `Call` (to set `span_id`) |
| **Observability** | `span_id` (skipping `None` frames) | At yield points (ScheduleFuture, Await, Notify) |
| **Duration** | `started_at` (only on tracked frames) | On frame pop (Return) or error |
| **Watch cleanup** | `locals_offset` | On Return (scan stack range) |
| **Exception trace** | All fields (tracked + untracked frames) | On error (build StackTrace) |
| **Debug display** | `function` + `instruction_ptr` → `source_lines` | Debug builds |

---

## 10. Detailed Flows

### 10.1 Flow: Normal Bytecode Call (a() calls b())

```
Time  │  VM State                          │  Span Effect
──────┼────────────────────────────────────┼─────────────────────────────────
  t0  │  frames: [a{span:A}]              │  current span = A
      │  exec: LOAD_GLOBAL "b"            │
      │  exec: CALL 0                     │
  t1  │  frames: [a{span:A}, b{span:B}]   │  current span = B, parent = A
      │  exec: ... b's bytecode ...        │
      │  exec: RETURN                      │
  t2  │  frames: [a{span:A}]              │  current span = A
      │  (frame_exit_info: span=B, dur=t2-t1)
      │  exec: ... a continues ...         │
```

### 10.2 Flow: LLM Function via Delegation

```
Time  │  VM State                                        │  Span Effect
──────┼──────────────────────────────────────────────────┼──────────────────
  t0  │  frames: [main{span:M}]                         │  span = M
      │  exec: CALL ExtractResume                        │
  t1  │  frames: [main{M}, ExtractResume{span:E}]       │  span = E, parent = M
      │  ExtractResume delegates to call_llm_function    │
      │  exec: CALL call_llm_function                    │
  t2  │  frames: [main{M}, ExtractResume{E},             │  span = L, parent = E
      │           call_llm_function{span:L}]             │
      │  exec: DISPATCH_FUTURE(EventSend,"fn_start")     │
      │  ── VM yields ──                                 │
      │  Engine: ctx from frames = {span:L, parent:E}    │
      │  exec: DISPATCH_FUTURE(SysOp::HttpSend)          │
      │  ── VM yields ──                                 │
      │  Engine: ctx from frames = {span:L, parent:E}    │
  t3  │  exec: RETURN (call_llm_function)                │
      │  frames: [main{M}, ExtractResume{E}]             │  span = E
  t4  │  exec: RETURN (ExtractResume)                    │
      │  frames: [main{M}]                               │  span = M
```

### 10.3 Flow: Error During LLM Call

```
Time  │  VM State                                        │  Action
──────┼──────────────────────────────────────────────────┼──────────────────
  t0  │  frames: [main{M}, Extract{E}, call_llm{L}]     │  Executing
  t1  │  SysOp::HttpSend returns error                   │  VM yields Err
      │                                                  │
      │  Engine catches Err(EngineError):                │
      │    vm.stack_trace(error) builds:                 │
      │      trace = [                                   │
      │        {name:"main",     span:M, line:5},        │
      │        {name:"Extract",  span:E, line:3},        │
      │        {name:"call_llm", span:L, line:8},        │
      │      ]                                           │
      │                                                  │
      │  Engine emits unwind events:                     │
      │    FunctionEnd(call_llm, Err, dur=t1-t0)  ctx:{span:L, parent:E}
      │    FunctionEnd(Extract,  Err, dur=t1-t0)  ctx:{span:E, parent:M}
      │    FunctionEnd(main,     Err, dur=t1-t0)  ctx:{span:M, parent:host}
      │                                                  │
      │  Returns EngineError to host with StackTrace     │
      │    (includes span_ids for dashboard correlation) │
```

### 10.4 Flow: Watch Filter Interrupt

```
Time  │  VM State                                        │  Span Effect
──────┼──────────────────────────────────────────────────┼──────────────────
  t0  │  frames: [main{span:M}]                         │  span = M
      │  exec: STORE_FIELD email on watched var          │
      │  process_notifications() → filter function       │
      │  interrupt(filter_fn, [value]):                  │
  t1  │  frames: [main{M}, <filter>{span:F}]            │  span = F, parent = M
      │  exec: ... filter bytecode ...                   │
      │  exec: RETURN                                    │
  t2  │  frames: [main{M}]                              │  span = M
      │  interrupt_frame check → Complete(bool)          │
      │  Continue: yield Notify or skip                  │
      │                                                  │
      │  Note: The filter function gets its own span!    │
      │  If the filter errors, the stack trace includes  │
      │  the filter frame with its span_id.              │
```

### 10.5 Flow: Per-Function Tracing (Mixed Tracked/Untracked)

```
Policy: TracingPolicy::TraceAuto
Functions:
  main()             → TracePolicy::Auto   → tracked (user bytecode)
  format_prompt()    → TracePolicy::Never   → NOT tracked
  ExtractResume()    → TracePolicy::Auto   → tracked (user bytecode)
  call_llm_function  → TracePolicy::Always  → tracked

Time  │  VM State                                          │  Span Effect
──────┼────────────────────────────────────────────────────┼──────────────────
  t0  │  frames: [main{span:M}]                           │  span = M
      │  exec: CALL format_prompt                          │
      │  should_track_call(format_prompt) = false          │
      │  (TracePolicy::Never)                              │
  t1  │  frames: [main{M}, format_prompt{span:None}]      │  span = M (unchanged)
      │  exec: ... format_prompt bytecode ...              │
      │  exec: RETURN                                      │
  t2  │  frames: [main{M}]                                │  span = M
      │  (no frame exit event — span was None)             │
      │  exec: CALL ExtractResume                          │
      │  should_track_call(ExtractResume) = true           │
      │  (Auto + TraceAuto + Bytecode)                     │
  t3  │  frames: [main{M}, ExtractResume{span:E}]         │  span = E, parent = M
      │  exec: CALL call_llm_function                      │
      │  should_track_call(call_llm_function) = true       │
      │  (TracePolicy::Always)                             │
  t4  │  frames: [main{M}, ExtractResume{E},               │  span = L, parent = E
      │           call_llm_function{span:L}]               │
      │  exec: DISPATCH_FUTURE(EventSend)                  │
      │  ── VM yields ──                                   │
      │  Engine: ctx from frames (skip-over logic):        │
      │    span_id: L                                      │
      │    parent_span_id: E  (direct parent, has span)    │
      │    root_span_id: M    (first tracked frame)        │
  t5  │  exec: RETURN (call_llm_function)                  │
      │  exec: RETURN (ExtractResume)                      │
      │  frames: [main{M}]                                │  span = M
```

Note that `format_prompt` is **fully invisible** in the span tree, but
its frame is **fully present** in the VM's `frames` for exception traces.

---

## 11. Data Structures

### 11.1 Enhanced Frame

```rust
// bex_vm/src/vm.rs

/// Call frame with optional observability metadata.
///
/// When tracing is active (tracing_active == true), each frame push
/// assigns a new span_id and records the start time. This lets the
/// engine derive SpanContext from the frame stack without maintaining
/// a parallel SpanStack.
///
/// When tracing is inactive, span_id and started_at are None,
/// adding zero overhead to the hot path.
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// Pointer to the running function object.
    pub function: HeapPtr,
    /// Instruction pointer (program counter).
    pub instruction_ptr: isize,
    /// Local variables offset in the eval stack.
    pub locals_offset: StackIndex,

    // --- Observability (NEW) ---

    /// Span ID for this call frame. None when tracing is inactive.
    pub span_id: Option<SpanId>,
    /// When this frame was pushed. None when tracing is inactive.
    pub started_at: Option<web_time::Instant>,
}
```

**Note on `Copy`**: The existing `Frame` is `Copy`. `SpanId` wraps a
`uuid::Uuid` which is `Copy` (it's `[u8; 16]`). `Option<Instant>` is
also `Copy`. So `Frame` remains `Copy`.

### 11.2 Per-Function Trace Policy

```rust
// bex_vm_types/src/types.rs (on Function)

/// Per-function policy for whether calls to this function should be
/// span-tracked. Set by the compiler during emit.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum TracePolicy {
    /// Always track (e.g., @trace annotation, LLM boundary functions).
    Always,
    /// Never track (e.g., internal builtins, trivial helpers).
    Never,
    /// Defer to the VM's TracingPolicy (default for most functions).
    #[default]
    Auto,
}
```

### 11.3 VM Tracing State

```rust
// Added to BexVm

/// VM-level tracing policy dial.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TracingPolicy {
    /// Track all function calls (except TracePolicy::Never).
    TraceAll,
    /// Track no function calls (global suppress).
    TraceNone,
    /// Use per-function TracePolicy + heuristics (default runtime mode).
    TraceAuto,
}

pub struct BexVm {
    // ... existing fields ...

    /// Master switch: is tracing active at all?
    /// When false, no span_ids are assigned regardless of TracingPolicy.
    pub tracing_active: bool,

    /// Fine-grained policy for which functions to trace.
    /// Only consulted when tracing_active == true.
    pub tracing_policy: TracingPolicy,
}
```

### 11.4 Tracing Configuration (Engine → VM)

```rust
// bex_engine/src/lib.rs

/// Configuration for a single function invocation's tracing behavior.
/// Passed from bridge_cffi → engine → VM.
pub struct TracingConfig {
    /// Master switch: is tracing on at all?
    pub enabled: bool,
    /// Which functions to trace (only consulted when enabled = true).
    pub policy: TracingPolicy,
    /// Root span ID (from host @trace or auto-generated).
    pub root_span_id: Option<SpanId>,
    /// Parent span ID from host language (if called inside @trace).
    pub parent_span_id: Option<SpanId>,
}

impl Default for TracingConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            policy: TracingPolicy::TraceAuto,
            root_span_id: None,
            parent_span_id: None,
        }
    }
}
```

### 11.5 Call Stack Snapshot

```rust
// bex_vm/src/lib.rs (new public types)

/// Lightweight snapshot of the VM's call stack.
#[derive(Debug, Clone)]
pub struct CallStackSnapshot {
    pub entries: Vec<CallStackEntry>,
}

/// A single entry in the call stack snapshot.
#[derive(Debug, Clone)]
pub struct CallStackEntry {
    pub function_name: String,
    pub span_id: Option<SpanId>,
    pub source_line: usize,
    pub function_span: baml_type::Span,
    pub started_at: Option<web_time::Instant>,
}

impl CallStackSnapshot {
    /// Derive a SpanContext from the snapshot.
    ///
    /// `host_parent` is the host-language @trace span_id (if any).
    pub fn to_span_context(&self, host_parent: Option<&SpanId>) -> SpanContext {
        let current = self.entries.last();
        let parent_entry = if self.entries.len() > 1 {
            self.entries.get(self.entries.len() - 2)
        } else {
            None
        };

        SpanContext {
            span_id: current
                .and_then(|e| e.span_id.clone())
                .unwrap_or_else(SpanId::new),
            parent_span_id: parent_entry
                .and_then(|e| e.span_id.clone())
                .or_else(|| host_parent.cloned()),
            root_span_id: self.entries.first()
                .and_then(|e| e.span_id.clone())
                .unwrap_or_else(SpanId::new),
        }
    }
}
```

### 11.6 Enhanced StackTrace

```rust
// bex_vm/src/errors.rs

#[derive(Debug, Clone)]
pub struct ErrorLocation {
    pub function_name: String,
    pub function_span: baml_type::Span,
    pub error_line: usize,
    /// Span ID from the frame (for observability correlation).
    pub span_id: Option<SpanId>,
    /// Duration this frame was alive before the error.
    pub frame_duration: Option<std::time::Duration>,
}

#[derive(Debug, Clone)]
pub struct StackTrace {
    pub error: VmError,
    pub trace: Vec<ErrorLocation>,
    /// Root span ID for correlating with event store.
    pub root_span_id: Option<SpanId>,
}

impl std::fmt::Display for StackTrace {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        writeln!(f, "Traceback (most recent call last):")?;
        for location in &self.trace {
            write!(
                f,
                "  File \"{}\", line {}, in {}",
                location.function_span.file_id,
                location.error_line,
                location.function_name
            )?;
            if let Some(span_id) = &location.span_id {
                write!(f, " [span:{}]", span_id)?;
            }
            writeln!(f)?;
        }
        write!(f, "{}", self.error)?;
        if let Some(root) = &self.root_span_id {
            write!(f, "\n  (trace root: {})", root)?;
        }
        Ok(())
    }
}
```

---

## 12. Implementation Plan

### Phase 1: Enrich Frame + Per-Function Tracing (VM-only)

**Changes**:
- Add `TracePolicy` enum to `bex_vm_types` (on `Function`)
- Add `TracingPolicy` enum to `bex_vm`
- Add `span_id: Option<SpanId>` and `started_at: Option<web_time::Instant>` to `Frame`
- Add `tracing_active: bool` and `tracing_policy: TracingPolicy` to `BexVm`
- Implement `should_track_call(&self, callee: &Function) -> bool` on `BexVm`
- Modify `Instruction::Call` (Bytecode path) to use `should_track_call()` for per-function decision
- Modify `Instruction::Return` to capture `FrameExitInfo` (for engine)
- Add `call_stack_snapshot()` method
- Enhance `stack_trace()` to include `span_id` and `frame_duration`
- Update `BexVm::new()` to initialize `tracing_active = false`, `tracing_policy = TraceAuto`

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_vm_types/src/types.rs` | Add `TracePolicy` enum, `trace_policy` field on `Function` | ~15 |
| `bex_vm/src/vm.rs` | Add fields to `Frame`, `TracingPolicy`, `should_track_call()` | ~50 |
| `bex_vm/src/vm.rs` | Modify `Call`/`Return`, add `call_stack_snapshot()` | ~40 |
| `bex_vm/src/errors.rs` | Enhance `ErrorLocation`, `StackTrace` | ~20 |
| `bex_vm/src/lib.rs` | Export new types | ~5 |
| `bex_vm_types` (if SpanId lives there) | Import `uuid` for `SpanId` type | ~10 |

**Tests**:
- Unit test: `span_id` is `None` when `tracing_active == false`
- Unit test: `span_id` is `Some` when `tracing_active == true` and `TracePolicy::Auto`
- Unit test: `TracePolicy::Never` suppresses span even when `TracingPolicy::TraceAll`
- Unit test: `TracePolicy::Always` creates span even when `TracingPolicy::TraceAuto` wouldn't
- Unit test: `should_track_call()` respects the priority chain (Section 5.2)
- Unit test: `call_stack_snapshot()` returns correct hierarchy with mixed tracked/untracked frames
- Unit test: `stack_trace()` includes all frames (tracked and untracked)
- Unit test: `stack_trace()` includes `span_id` and `frame_duration`

### Phase 2: Engine Uses VM Frames + TracingConfig (replaces SpanStack)

**Changes**:
- Add `TracingConfig` struct to engine
- `BexEngine::call_function()` accepts `TracingConfig` and configures
  VM's `tracing_active`, `tracing_policy`, and root span
- `run_event_loop_with_epoch()` derives `SpanContext` from
  `vm.call_stack_snapshot()` with skip-over logic for untracked frames
- On error, call `emit_unwind_events()` using frame info (only for tracked frames)
- Remove any `SpanStack` if it was already implemented

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `bex_engine/src/lib.rs` | Add `TracingConfig`, configure VM | ~30 |
| `bex_engine/src/lib.rs` | `span_context_from_vm()` with skip logic | ~30 |
| `bex_engine/src/lib.rs` | `emit_unwind_events()` for error case | ~40 |
| `bex_engine/src/lib.rs` | Replace SpanStack usage in event loop | ~20 |
| `bridge_cffi` | Pass `TracingConfig` from host @trace | ~15 |

### Phase 3: Compiler Emit of TracePolicy

**Changes**:
- Compiler sets `TracePolicy::Always` on `call_llm_function` (from `llm.baml`)
- Compiler sets `TracePolicy::Never` on internal builtins
- Add `@trace` / `@notrace` annotation support to BAML syntax
- Compiler emits appropriate `TracePolicy` based on annotations

**Files**:

| File | Change | Lines (est.) |
|------|--------|-------------|
| `baml_compiler_emit` | Set `trace_policy` during function compilation | ~20 |
| `baml_builtins/src/lib.rs` | Set `TracePolicy::Always` for LLM fns, `Never` for builtins | ~10 |
| Grammar / parser (future) | `@trace` and `@notrace` annotations | ~30 |

### Phase 4: Exception Handling Infrastructure

**Changes** (future, when try/catch is added):
- Add `ExceptionTable` to `Function` (maps instruction ranges → handler offsets)
- Add `Instruction::Throw` and handler dispatch in `exec()`
- Unwind loop: pop frames, emit FunctionEnd(Err) for tracked frames, until handler found
- Reuse `span_id`/`started_at` for correct duration and span context during unwind

---

## 13. Open Questions

### Q1: Should `SpanId` be a VM-level type or engine-level type?

**Option A**: `SpanId` lives in `bex_vm_types` (or `baml_events`), imported
by `bex_vm`. This means the VM has a direct dependency on the event system's
ID type.

**Option B**: `Frame.span_id` is `Option<[u8; 16]>` (raw UUID bytes). The
engine wraps it in `SpanId` when constructing `SpanContext`. This keeps
the VM agnostic about the event system.

**Recommendation**: Option A. `SpanId` is trivial (a newtype around
`uuid::Uuid`), and having it in the VM avoids conversions. The `uuid`
crate is already used elsewhere.

### Q2: Should native function calls get span_ids?

Currently, native functions don't push frames (they execute inline and
return). This means they're invisible to span tracking. With per-function
tracing, this is moot: native functions don't have `Frame`s, so they
can't have `span_id`s regardless of `TracePolicy`.

**Recommendation**: Keep them invisible. If we ever need to trace a
native call, we'd need to push a synthetic frame — but that's a separate
design decision.

### Q3: How does this interact with the `interrupt()` mechanism?

The `interrupt()` function pushes a real frame for the filter function.
Under this proposal, that frame's `span_id` depends on the filter
function's `TracePolicy`:

- If the compiler sets `TracePolicy::Auto` on filter lambdas, they'll
  be tracked under `TraceAll` but silent under `TraceAuto`.
- If the compiler sets `TracePolicy::Never`, filters are always silent.

**Recommendation**: Set `TracePolicy::Auto` on filter functions. In
`TraceAuto` mode, they won't be tracked (they don't match the heuristic
for user-defined bytecode functions — they're compiler-generated). In
`TraceAll` mode, they show up for debugging.

### Q4: Performance impact of `Instant::now()` per frame push?

`Instant::now()` is a syscall on some platforms. On macOS it uses
`mach_absolute_time()` which is ~25ns. With per-function tracing,
the cost is only paid for frames where `should_track_call()` returns
`true`. In `TraceAuto` mode, internal helpers pay zero cost.

**Recommendation**: Accept the cost. It's only paid when a function is
actually being traced, and those are the functions users care about
(LLM calls, user-defined pipelines).

### Q5: What should the default `TracePolicy` be for user-defined functions?

**Option A**: `Auto` (default) — traced only when the VM/engine says so.
This is the safest default. Users who want explicit control use `@trace`.

**Option B**: `Always` for all user-defined functions — every BAML
function the user writes is always traced when tracing is active. This
is simpler but noisier.

**Recommendation**: Option A. `Auto` with the `TraceAuto` heuristic
(Section 5.5) covers the common case: all user-defined bytecode
functions are tracked, but the heuristic can be refined without changing
the `Function` metadata.

### Q6: Should `TracePolicy::Never` be overridable by `TraceAll`?

Currently the hierarchy in Section 5.2 has `TracePolicy::Never` always
suppressing spans, even under `TracingPolicy::TraceAll`. Is there a use
case where "I really want to see everything, including builtins"?

**Option A**: `Never` is absolute. No override. Keeps the contract simple.

**Option B**: Add a `TracingPolicy::TraceForce` that overrides even
`Never`. Useful for VM-level debugging but risks noise.

**Recommendation**: Option A for now. `TraceAll` is "trace everything
the compiler didn't explicitly exclude." `TraceForce` can be added later
if needed.

### Q7: Compatibility with event-publishing-design-v2

This document proposes replacing the `SpanStack` from v2 Milestone 4 with
VM-derived spans with per-function control. The rest of v2 (EventStore,
Collector, Publisher, EventSend SysOp, `llm.baml` instrumentation)
remains unchanged. The only difference is **where span context comes
from** and **which functions generate spans**:

- v2 M4: `SpanStack` maintained by engine, pushed/popped on `EventSend` interception
- This doc: VM `Frame.span_id` (per-function decision via `should_track_call()`),
  read by engine at yield points with skip-over logic for untracked frames

All other event types, the `baml.events.send()` builtin, and the
`build_event_kind()` conversion remain the same.

---

*This document should be read alongside
[event-publishing-design-v2.md](./event-publishing-design-v2.md)
which covers the event system architecture, collector design, and
milestone plan.*
