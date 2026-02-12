# BEAM-like Runtime: Live Introspection, Hot Reload, and AI-Driven Debugging

> Designing a control plane for `baml_language` that brings BEAM-style observability — live VM introspection, hot code reload, and AI agent-driven debugging — to the BEX runtime.

---

**Table of Contents**

1. [Introduction](#1-introduction)
2. [Design Principles](#2-design-principles)
3. [Core Design Decisions](#3-core-design-decisions)
   - [3.1 Protocol: gRPC with tonic](#31-protocol-grpc-with-tonic)
   - [3.2 Socket: Unix Domain Socket (Local) + TCP (Remote)](#32-socket-unix-domain-socket-local--tcp-remote)
   - [3.3 VM Registry on BexEngine (Not Global Static)](#33-vm-registry-on-bexengine-not-global-static)
   - [3.4 Breakpoints via VmExecState Extension (Not Instruction Patching)](#34-breakpoints-via-vmexecstate-extension-not-instruction-patching)
   - [3.5 Hot Reload via Engine Replacement (Not In-Place Patching)](#35-hot-reload-via-engine-replacement-not-in-place-patching)
4. [Architecture Overview](#4-architecture-overview)
5. [Control Plane / RPC Layer](#5-control-plane--rpc-layer)
   - [5.1 New Crate: bex_control_plane](#51-new-crate-bex_control_plane)
   - [5.2 Proto Definitions](#52-proto-definitions)
   - [5.3 Server Lifecycle](#53-server-lifecycle)
   - [5.4 Session Management and Auth](#54-session-management-and-auth)
6. [VM Introspection API](#6-vm-introspection-api)
   - [6.1 ListVMs / InspectVM](#61-listvms--inspectvm)
   - [6.2 Stack and Heap Inspection](#62-stack-and-heap-inspection)
   - [6.3 Disassemble](#63-disassemble)
   - [6.4 Event Subscription](#64-event-subscription)
7. [Live Tracing](#7-live-tracing)
   - [7.1 Dynamic Trace Attachment](#71-dynamic-trace-attachment)
   - [7.2 Conditional Tracing](#72-conditional-tracing)
   - [7.3 Breakpoints](#73-breakpoints)
8. [Hot Code Reload](#8-hot-code-reload)
   - [8.1 Incremental Compilation Pipeline](#81-incremental-compilation-pipeline)
   - [8.2 Epoch Coordination](#82-epoch-coordination)
   - [8.3 Schema Migrations](#83-schema-migrations)
9. [REPL / Expression Evaluation](#9-repl--expression-evaluation)
   - [9.1 Standalone Evaluation](#91-standalone-evaluation)
   - [9.2 At-Breakpoint Evaluation](#92-at-breakpoint-evaluation)
10. [AI Agent Integration](#10-ai-agent-integration)
    - [10.1 Tool Definitions](#101-tool-definitions)
    - [10.2 Agent Workflow Example](#102-agent-workflow-example)
    - [10.3 Safety Constraints](#103-safety-constraints)
11. [Implementation Milestones](#11-implementation-milestones)
12. [Deferred Work](#12-deferred-work)
13. [Open Questions](#13-open-questions)
14. [References](#14-references)

---

## 1. Introduction

**What**: A design for adding a BEAM-inspired control plane to `baml_language` — enabling live introspection of running VMs, hot code reload without dropping inflight requests, and a tool-use interface for AI agents to debug and operate the runtime.

**Why**: Today, debugging a running BAML system means restarting the process. There is no way to inspect a live VM's stack, attach tracing to a specific function after deployment, or swap in new BAML source without downtime. The BEAM (Erlang/OTP runtime) solved these problems decades ago. We can apply the same ideas to BEX, adapted for our bytecode VM and epoch-based GC.

**Scope**:
- Crates affected: `bex_engine`, `bex_vm`, `bex_vm_types`, `bex_events`, `bridge_cffi`
- New crate: `bex_control_plane`
- Does NOT change the bytecode format, the compiler pipeline, or the GC algorithm
- Does NOT require changes to BAML source language syntax

**Relationship to other docs**:
- [event-publishing-design-v2.md](event-publishing-design-v2.md) — The tracing and `RuntimeEvent` infrastructure this design extends
- [callstack-tracking-design-v2.md](callstack-tracking-design-v2.md) — The unified call stack model this design builds on
- [event-publishing-implementation-plan.md](event-publishing-implementation-plan.md) — The phased approach we follow for implementation

---

## 2. Design Principles

1. **Read-only by default** — All introspection operations are non-mutating. Write operations (breakpoints, reload, eval) require explicit opt-in and are gated behind session capabilities.

2. **Zero-cost when disconnected** — If no control plane client is connected, the runtime pays zero overhead. No polling, no channel allocation, no atomic checks on the hot path. Feature gates compile out the control plane entirely for embedded/WASM targets.

3. **Composable primitives** — Each capability (list VMs, inspect stack, subscribe to events, set breakpoint) is an independent RPC. Complex workflows (like "break on error, inspect locals, eval fix, resume") are composed client-side, not baked into the server.

4. **Existing patterns over novelty** — Reuse the epoch system for hot reload coordination. Reuse `VmExecState` for breakpoint yields. Reuse `RuntimeEvent` for trace subscriptions. Reuse `debug::display_instruction` for disassembly. Avoid inventing new mechanisms when existing ones can be extended.

5. **Engine-scoped, not process-scoped** — All state lives on `Arc<BexEngine>`, not in global statics. Multiple engines in the same process get independent control planes. This follows the pattern established by `bex_events::event_store` but scopes introspection to the engine that owns the VMs.

---

## 3. Core Design Decisions

### 3.1 Protocol: gRPC with tonic

**Context**: The control plane needs a protocol for clients (CLI tools, AI agents, IDE extensions) to communicate with the runtime. Options considered: custom TCP protocol, JSON-RPC over WebSocket, gRPC, Cap'n Proto RPC.

**Decision**: Use gRPC via `tonic` for the control plane protocol, with `prost` for message serialization.

**Rationale**:
- The workspace already depends on `prost` for protobuf serialization (see `bridge_cffi/Cargo.toml` — prost is used for FFI buffer encoding). Adding `tonic` is an incremental step.
- gRPC provides bidirectional streaming natively — required for event subscription (`SubscribeEvents` returns a stream) and breakpoint interaction (client sends resume/step commands on an open stream).
- Strong typing via `.proto` files means AI agents can parse the schema and generate tool-use definitions automatically.
- `tonic` compiles to efficient Rust with zero-copy deserialization where possible.

**Consequences**:
- New dependency: `tonic` (server) and `tonic-build` (build script). Adds ~200KB to binary.
- WASM targets cannot use gRPC directly — they use the existing FFI bridge and a separate in-process adapter (deferred).
- Proto files become a public API contract. Breaking changes require versioning.

### 3.2 Socket: Unix Domain Socket (Local) + TCP (Remote)

**Context**: The control plane server needs a transport. Local debugging should be zero-config. Remote debugging (e.g., attaching to a container) needs to work across networks.

**Decision**: Default to a Unix domain socket at a well-known path (`/tmp/bex-control-<pid>.sock`). Optionally listen on a TCP port when `BEX_CONTROL_ADDR` is set.

**Rationale**:
- UDS avoids port conflicts, is invisible to network scanners, and requires filesystem permissions for access — good security defaults.
- The PID-based path makes it trivial for a CLI tool to discover the socket: `ls /tmp/bex-control-*.sock`.
- TCP mode is opt-in for remote debugging, behind an explicit env var so it cannot be enabled accidentally.

**Consequences**:
- macOS and Linux only for UDS. Windows uses named pipes (same `tonic` transport layer).
- Discovery requires either the PID or a directory listing. A future registry service could centralize this.

### 3.3 VM Registry on BexEngine (Not Global Static)

**Context**: To list and inspect VMs, the control plane needs a registry of active VMs. Options: global `static` registry (like `bex_events::PUBLISHER_TX`), or per-engine registry on `Arc<BexEngine>`.

**Decision**: Add a `VmRegistry` field to `BexEngine`. Each VM registers on creation and deregisters on completion.

**Rationale**:
- `BexEngine` already owns the lifecycle of VMs — `call_function` and `call_function_traced` create VMs, run their event loops, and clean up. The engine is the natural owner of a VM registry.
- A global static would conflate VMs from different engines in the same process (e.g., test harnesses run multiple engines). The existing `epoch_states` array on `BexEngine` already demonstrates per-engine VM tracking.
- The `EpochState` struct already stores `parked_vms: Mutex<Vec<VmPtr>>` — the registry follows the same pattern with richer metadata.

**Consequences**:
- `BexEngine` gains a new field: `vm_registry: VmRegistry`. This is behind a `cfg(feature = "control-plane")` gate.
- VM registration adds one `Mutex` lock on the hot path (VM creation/destruction). This is negligible compared to the cost of running the VM itself.
- Registry entries are weak references — if the VM completes, the entry is removed.

### 3.4 Breakpoints via VmExecState Extension (Not Instruction Patching)

**Context**: Breakpoints must pause a VM at a specific instruction and yield control to the debugger. Two approaches: (a) patch `Instruction::Unreachable` into the bytecode at the breakpoint location, or (b) extend `VmExecState` with a new `Breakpoint` variant that the VM yields when it hits a flagged instruction.

**Decision**: Extend `VmExecState` with a `Breakpoint` variant. The VM checks a per-engine breakpoint set at each instruction dispatch.

**Rationale**:
- `VmExecState` already has five variants (`Await`, `ScheduleFuture`, `Complete`, `Notify`, `SpanNotify`) that cause the VM to yield control to the engine's event loop. A `Breakpoint` variant is a natural extension:
  ```rust
  // bex_vm/src/vm.rs — VmExecState
  pub enum VmExecState {
      Await(HeapPtr),
      ScheduleFuture(HeapPtr),
      Complete(Value),
      Notify(WatchNotification),
      SpanNotify(SpanNotification),
      // NEW
      Breakpoint {
          frame_depth: usize,
          instruction_ptr: isize,
          function_name: String,
      },
  }
  ```
- Instruction patching would mutate shared bytecode (`Arc<BexHeap>`) and require careful coordination with the GC's forwarding-pointer update pass. The yield approach avoids touching the heap entirely.
- The check is a single `HashSet::contains` lookup on the `(function_ptr, instruction_ptr)` pair, behind a `cfg` gate that compiles to nothing when the feature is disabled.

**Consequences**:
- The breakpoint check adds a branch to the VM's main `exec()` loop. When the feature is disabled (`cfg(not(feature = "control-plane"))`), this compiles away entirely.
- When enabled but no breakpoints are set, the cost is one atomic load (checking if the breakpoint set is non-empty) per instruction — comparable to the existing `interrupt_frame` check.
- Breakpoints are set by `(function_ptr, instruction_offset)`, not by source line. Source-line mapping uses the existing `Bytecode::source_lines` vector.

### 3.5 Hot Reload via Engine Replacement (Not In-Place Patching)

**Context**: Hot reload must swap in new BAML source without dropping inflight requests. Options: (a) patch individual functions in-place on the heap, or (b) create a new `BexEngine` from the new source and drain the old one.

**Decision**: Hot reload creates a new `Arc<BexEngine>` and atomically swaps the reference. Inflight VMs on the old engine run to completion. New requests go to the new engine.

**Rationale**:
- The existing FFI bridge already stores the engine as `Arc<RwLock<Option<Arc<BexEngine>>>>` (see `bridge_cffi/src/engine.rs`). An atomic swap of the inner `Arc` is the simplest possible reload mechanism.
- In-place function patching would require updating `HeapPtr` references across all live VMs, coordinating with the epoch-based GC, and handling schema changes (new fields, removed classes). This is extremely complex and error-prone.
- The epoch system naturally handles draining: the old engine's VMs complete their current epoch, then the old `Arc<BexEngine>` is dropped when the last reference (the last inflight VM) finishes.
- The Salsa-based incremental compilation in `tools_onionskin/src/compiler.rs` (`CompilerRunner`) already supports re-compilation from modified source files — we reuse this for the reload pipeline.

**Consequences**:
- Memory doubles briefly during reload (two engines, two heaps). This is acceptable for a debugging/development feature.
- Inflight VMs see the old schema. Schema-breaking changes (removed fields, changed types) cannot be applied to running VMs — they drain first.
- The reload is atomic from the perspective of new requests: no request ever sees a half-loaded state.

---

## 4. Architecture Overview

```
┌──────────────────────────────────────────────────────────────────────┐
│                        Process Boundary                              │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │                    bex_control_plane                          │    │
│  │                                                              │    │
│  │  ┌──────────────┐    ┌──────────────┐    ┌──────────────┐   │    │
│  │  │ gRPC Server   │    │ Session Mgr  │    │ Auth Gate    │   │    │
│  │  │ (tonic)       │    │              │    │              │   │    │
│  │  └──────┬───────┘    └──────┬───────┘    └──────┬───────┘   │    │
│  │         │                   │                   │            │    │
│  │         └───────────────────┼───────────────────┘            │    │
│  │                             │                                │    │
│  └─────────────────────────────┼────────────────────────────────┘    │
│                                │                                     │
│                                │ ControlPlaneHandle                  │
│                                │ (Arc<BexEngine> reference)          │
│                                │                                     │
│  ┌─────────────────────────────┼────────────────────────────────┐    │
│  │                    Arc<BexEngine>                             │    │
│  │                             │                                │    │
│  │  ┌──────────────┐  ┌───────┴──────┐  ┌──────────────┐      │    │
│  │  │ VmRegistry   │  │ BreakpointSet│  │ ReloadSlot   │      │    │
│  │  │              │  │              │  │              │      │    │
│  │  │ vm_id → meta │  │ {fn, ip} set │  │ next_engine  │      │    │
│  │  └──────┬───────┘  └──────────────┘  └──────────────┘      │    │
│  │         │                                                    │    │
│  │         │ Per-invocation VMs                                 │    │
│  │         │                                                    │    │
│  │  ┌──────┴───────────────────────────────────────────────┐   │    │
│  │  │  VM 1 (epoch 42)     VM 2 (epoch 42)     VM 3 ...   │   │    │
│  │  │  ┌─────────────┐    ┌─────────────┐                 │   │    │
│  │  │  │ BexVm       │    │ BexVm       │                 │   │    │
│  │  │  │ frames      │    │ frames      │                 │   │    │
│  │  │  │ stack       │    │ stack       │                 │   │    │
│  │  │  │ tlab        │    │ tlab        │                 │   │    │
│  │  │  └─────────────┘    └─────────────┘                 │   │    │
│  │  └──────────────────────────────────────────────────────┘   │    │
│  │                                                              │    │
│  │  ┌──────────────────────────────────────────────────────┐   │    │
│  │  │  Arc<BexHeap> (shared across all VMs)                │   │    │
│  │  └──────────────────────────────────────────────────────┘   │    │
│  │                                                              │    │
│  │  ┌──────────────────────────────────────────────────────┐   │    │
│  │  │  Epoch GC Coordinator                                │   │    │
│  │  │  current_epoch: AtomicU64                            │   │    │
│  │  │  epoch_states: [EpochState; 2]                       │   │    │
│  │  │  epoch_drained: Notify                               │   │    │
│  │  │  gc_complete: Notify                                 │   │    │
│  │  └──────────────────────────────────────────────────────┘   │    │
│  └──────────────────────────────────────────────────────────────┘    │
│                                                                      │
│  ┌──────────────────────────────────────────────────────────────┐    │
│  │  Clients (connect via UDS or TCP)                            │    │
│  │                                                              │    │
│  │  ┌────────────┐  ┌────────────┐  ┌────────────────────┐    │    │
│  │  │ CLI tool   │  │ IDE plugin │  │ AI Agent (Claude)  │    │    │
│  │  │ bex debug  │  │ VSCode ext │  │ tool-use client    │    │    │
│  │  └────────────┘  └────────────┘  └────────────────────┘    │    │
│  └──────────────────────────────────────────────────────────────┘    │
└──────────────────────────────────────────────────────────────────────┘
```

---

## 5. Control Plane / RPC Layer

### 5.1 New Crate: bex_control_plane

```
baml_language/crates/bex_control_plane/
├── Cargo.toml
├── build.rs              # tonic-build for proto compilation
├── proto/
│   └── bex_control.proto # Service + message definitions
├── src/
│   ├── lib.rs            # ControlPlaneHandle, start/stop
│   ├── server.rs         # tonic service implementation
│   ├── session.rs        # Session management
│   └── auth.rs           # Capability-based auth
```

Dependencies:

```toml
[dependencies]
bex_engine = { path = "../bex_engine" }
bex_vm = { path = "../bex_vm" }
bex_vm_types = { path = "../bex_vm_types" }
bex_events = { path = "../bex_events" }
tonic = "0.12"
prost = "0.13"
tokio = { version = "1", features = ["net", "sync", "signal"] }
uuid = { version = "1", features = ["v4"] }

[build-dependencies]
tonic-build = "0.12"
```

### 5.2 Proto Definitions

```protobuf
syntax = "proto3";
package bex.control;

service BexControl {
  // Introspection (read-only)
  rpc ListVMs(ListVMsRequest) returns (ListVMsResponse);
  rpc InspectVM(InspectVMRequest) returns (InspectVMResponse);
  rpc InspectStack(InspectStackRequest) returns (InspectStackResponse);
  rpc InspectHeapObject(InspectHeapObjectRequest) returns (InspectHeapObjectResponse);
  rpc Disassemble(DisassembleRequest) returns (DisassembleResponse);
  rpc ListFunctions(ListFunctionsRequest) returns (ListFunctionsResponse);

  // Event subscription (server-streaming)
  rpc SubscribeEvents(SubscribeEventsRequest) returns (stream RuntimeEventProto);
  rpc SubscribeVMState(SubscribeVMStateRequest) returns (stream VMStateChange);

  // Tracing (requires "trace" capability)
  rpc AttachTrace(AttachTraceRequest) returns (AttachTraceResponse);
  rpc DetachTrace(DetachTraceRequest) returns (DetachTraceResponse);

  // Debugging (requires "debug" capability)
  rpc SetBreakpoint(SetBreakpointRequest) returns (SetBreakpointResponse);
  rpc RemoveBreakpoint(RemoveBreakpointRequest) returns (RemoveBreakpointResponse);
  rpc ListBreakpoints(ListBreakpointsRequest) returns (ListBreakpointsResponse);
  rpc DebugSession(stream DebugCommand) returns (stream DebugEvent);

  // Evaluation (requires "eval" capability)
  rpc Eval(EvalRequest) returns (EvalResponse);

  // Hot reload (requires "reload" capability)
  rpc Reload(ReloadRequest) returns (ReloadResponse);
  rpc ReloadStatus(ReloadStatusRequest) returns (ReloadStatusResponse);

  // Session management
  rpc CreateSession(CreateSessionRequest) returns (CreateSessionResponse);
  rpc DestroySession(DestroySessionRequest) returns (DestroySessionResponse);
}

message VMSummary {
  string vm_id = 1;
  uint64 epoch = 2;
  string entry_function = 3;
  string state = 4;        // "running", "awaiting", "paused_at_breakpoint"
  uint32 frame_depth = 5;
  uint64 started_at_ms = 6;
}

message FrameInfo {
  uint32 depth = 1;
  string function_name = 2;
  int64 instruction_ptr = 3;
  uint32 source_line = 4;
  repeated LocalVariable locals = 5;
}

message LocalVariable {
  uint32 slot = 1;
  string name = 2;         // from debug info, empty if unavailable
  string type_name = 3;
  string value_preview = 4; // truncated display_value output
}

message DebugCommand {
  oneof command {
    ResumeCommand resume = 1;
    StepCommand step = 2;
    EvalAtBreakpointCommand eval = 3;
  }
}

message DebugEvent {
  oneof event {
    BreakpointHit breakpoint_hit = 1;
    VMCompleted vm_completed = 2;
    EvalResult eval_result = 3;
  }
}
```

### 5.3 Server Lifecycle

The control plane is started by the engine owner (the FFI bridge, the CLI, or a test harness). It is not started automatically.

```rust
// bex_control_plane/src/lib.rs

pub struct ControlPlaneHandle {
    engine: Arc<BexEngine>,
    shutdown_tx: tokio::sync::oneshot::Sender<()>,
    server_task: tokio::task::JoinHandle<()>,
    pub address: String,
}

impl ControlPlaneHandle {
    /// Start the control plane server.
    ///
    /// If `addr` is None, defaults to UDS at /tmp/bex-control-<pid>.sock.
    /// If `addr` is Some, listens on that TCP address.
    pub async fn start(
        engine: Arc<BexEngine>,
        addr: Option<String>,
    ) -> Result<Self, ControlPlaneError> {
        let (shutdown_tx, shutdown_rx) = tokio::sync::oneshot::channel();
        let address = addr.unwrap_or_else(|| {
            format!("/tmp/bex-control-{}.sock", std::process::id())
        });
        // ... spawn tonic server with graceful shutdown on shutdown_rx
        todo!()
    }

    /// Gracefully shut down the control plane.
    pub async fn shutdown(self) {
        let _ = self.shutdown_tx.send(());
        let _ = self.server_task.await;
    }
}
```

### 5.4 Session Management and Auth

Sessions use a capability-based model. Each session has a set of capabilities granted at creation time.

```rust
// bex_control_plane/src/session.rs

#[derive(Clone, Debug)]
pub struct Session {
    pub id: uuid::Uuid,
    pub capabilities: HashSet<Capability>,
    pub created_at: std::time::Instant,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum Capability {
    /// Read-only introspection (ListVMs, InspectStack, etc.)
    Inspect,
    /// Attach/detach traces on running VMs
    Trace,
    /// Set breakpoints, step, resume
    Debug,
    /// Evaluate expressions in VM context
    Eval,
    /// Hot-reload BAML source
    Reload,
}
```

The `CreateSession` RPC accepts a list of requested capabilities. The server grants them based on a policy (default: all capabilities in development mode, `Inspect` only in production). The session token is passed as gRPC metadata on subsequent calls.

---

## 6. VM Introspection API

### 6.1 ListVMs / InspectVM

**Building blocks**: `BexEngine` already tracks VMs via `EpochState::parked_vms` and the epoch counters. The registry extends this with metadata.

```rust
// bex_engine/src/lib.rs — new field (behind cfg gate)

pub struct BexEngine {
    // ... existing fields ...

    #[cfg(feature = "control-plane")]
    vm_registry: Mutex<HashMap<VmId, VmMetadata>>,
}

#[cfg(feature = "control-plane")]
pub struct VmMetadata {
    pub vm_id: VmId,
    pub epoch: u64,
    pub entry_function: String,
    pub state: VmState,
    pub frame_depth: usize,
    pub started_at: web_time::Instant,
    /// Raw pointer to the VM — only valid while the VM is alive.
    /// Access requires the VM to be paused (at breakpoint or safepoint).
    vm_ptr: *const BexVm,
}

#[derive(Clone, Debug)]
pub enum VmState {
    Running,
    Awaiting,       // yielded VmExecState::Await
    PausedBreakpoint { frame_depth: usize, instruction_ptr: isize },
    Completed,
}
```

**ListVMs** returns a snapshot of `vm_registry`. No locks are held beyond the HashMap read.

**InspectVM** returns full metadata for a single VM, including the current `Frame` stack (only accessible when the VM is paused or at a safepoint).

### 6.2 Stack and Heap Inspection

**Building blocks**: `BexVm::frames` is a `Vec<Frame>`. Each `Frame` has `function: HeapPtr`, `instruction_ptr: isize`, and `locals_offset: StackIndex`. The `EvalStack` holds all local variables.

```rust
// Pseudocode for InspectStack handler

fn inspect_stack(vm: &BexVm) -> Vec<FrameInfo> {
    vm.frames.iter().enumerate().map(|(depth, frame)| {
        let function = vm.get_object(frame.function);
        let function_name = match function {
            Object::Function(f) => f.name.clone(),
            _ => "<unknown>".to_string(),
        };

        let bytecode = match function {
            Object::Function(f) => &f.bytecode,
            _ => return FrameInfo::default(),
        };

        let source_line = bytecode.source_lines
            .get(frame.instruction_ptr as usize)
            .copied()
            .unwrap_or(0);

        // Locals: from frame.locals_offset to next frame's locals_offset (or stack top)
        let locals_end = vm.frames.get(depth + 1)
            .map(|f| f.locals_offset)
            .unwrap_or(vm.stack.len());

        let locals = (frame.locals_offset..locals_end)
            .map(|slot| {
                let value = &vm.stack[slot];
                LocalVariable {
                    slot: slot as u32,
                    name: String::new(), // populated from debug info if available
                    type_name: format!("{:?}", vm.type_of(value)),
                    value_preview: debug::display_value(value),
                }
            })
            .collect();

        FrameInfo { depth: depth as u32, function_name, instruction_ptr: frame.instruction_ptr, source_line: source_line as u32, locals }
    }).collect()
}
```

**Heap inspection** dereferences a `HeapPtr` and returns a structured representation. This reuses `debug::display_value` for the preview and adds type-specific field enumeration for classes and maps.

### 6.3 Disassemble

**Building blocks**: `debug::display_instruction` in `bex_vm/src/debug.rs` already formats instructions with context (variable names, constant values, jump targets). The `Disassemble` RPC wraps this for a given function.

```rust
// Pseudocode for Disassemble handler

fn disassemble(engine: &BexEngine, function_name: &str) -> DisassembleResponse {
    let (fn_ptr, _kind) = engine.resolved_function_names.get(function_name)
        .ok_or(EngineError::FunctionNotFound { name: function_name.to_string() })?;

    let function = engine.heap().get(*fn_ptr);
    let bytecode = match function {
        Object::Function(f) => &f.bytecode,
        _ => return Err(/* not a function */),
    };

    let instructions: Vec<DisassembledInstruction> = bytecode.instructions
        .iter()
        .enumerate()
        .map(|(ip, _instr)| {
            let (instr_str, meta_str) = debug::display_instruction(
                ip as isize,
                function,
                /* stack */ &EvalStack::default(),
                /* globals */ &engine.globals,
                Some(&bytecode.constants),
                None,
            );
            DisassembledInstruction {
                offset: ip as u32,
                instruction: instr_str,
                metadata: meta_str,
                source_line: bytecode.source_lines.get(ip).copied().unwrap_or(0) as u32,
            }
        })
        .collect();

    DisassembleResponse { function_name: function_name.to_string(), instructions }
}
```

### 6.4 Event Subscription

**Building blocks**: `bex_events::event_store::emit()` publishes `RuntimeEvent`s to a global publisher thread. `SubscribeEvents` taps into this stream.

The subscription model uses a `tokio::sync::broadcast` channel added to `BexEngine`. When the engine handles `VmExecState::SpanNotify`, it also sends the constructed `RuntimeEvent` to the broadcast channel (only if there are subscribers — checked via `broadcast::Sender::receiver_count()`).

```rust
// bex_engine/src/lib.rs — new field

#[cfg(feature = "control-plane")]
event_broadcast: tokio::sync::broadcast::Sender<RuntimeEvent>,
```

The `SubscribeEvents` RPC creates a `broadcast::Receiver` and streams events to the client with optional filtering (by function name, by span ID, by event kind).

---

## 7. Live Tracing

### 7.1 Dynamic Trace Attachment

Today, tracing is compile-time: `CallWithTrace` instructions are emitted by the compiler for `@trace`-annotated functions. Dynamic trace attachment enables tracing any function at runtime without recompilation.

**Mechanism**: The engine maintains a `HashSet<HeapPtr>` of dynamically-traced functions. In the VM's `Call` instruction handler, if the callee is in the trace set, the VM behaves as if it were a `CallWithTrace` — yielding `SpanNotification::FunctionEnter` and `SpanNotification::FunctionExit`.

```rust
// bex_engine/src/lib.rs — new field

#[cfg(feature = "control-plane")]
dynamic_traces: RwLock<HashSet<HeapPtr>>,
```

The check is a single `RwLock::read()` + `HashSet::contains()` on the `Call` instruction path. When the set is empty (no dynamic traces), the `RwLock` read is uncontended and nearly free.

### 7.2 Conditional Tracing

Conditional tracing extends dynamic trace attachment with a predicate. Instead of tracing every call, the trace fires only when a condition is met (e.g., "trace `classify` only when the input contains 'error'").

This is implemented by storing a compiled BAML expression alongside the trace entry. The expression is evaluated against the function's arguments before the call. If it returns true, tracing proceeds. If false, the call executes without tracing.

This reuses the REPL evaluation infrastructure (Section 9) — the condition is compiled to a small `BytecodeProgram` and evaluated in a throwaway VM.

### 7.3 Breakpoints

Breakpoints use the `VmExecState::Breakpoint` variant described in Section 3.4.

**Breakpoint set storage**:

```rust
// bex_engine/src/lib.rs — new field

#[cfg(feature = "control-plane")]
breakpoints: RwLock<HashMap<BreakpointId, Breakpoint>>,

#[cfg(feature = "control-plane")]
breakpoint_index: RwLock<HashSet<(HeapPtr, isize)>>,  // fast lookup in exec loop
```

**Breakpoint resolution**: The client specifies breakpoints by `(function_name, source_line)`. The server resolves this to `(HeapPtr, instruction_offset)` using the function's `Bytecode::source_lines` mapping.

**VM pause flow**:

1. VM's `exec()` loop checks `breakpoint_index` at each instruction dispatch.
2. If the current `(function_ptr, instruction_ptr)` is in the index, the VM yields `VmExecState::Breakpoint { ... }`.
3. The engine's event loop receives the breakpoint yield and notifies the control plane.
4. The control plane streams a `BreakpointHit` event to the client's `DebugSession` stream.
5. The client sends a `ResumeCommand` or `StepCommand` on the same stream.
6. The engine resumes the VM's `exec()` loop.

**Step implementation**: "Step over" removes the current breakpoint temporarily and sets a breakpoint at the next instruction in the same frame. "Step into" removes the breakpoint and sets one at the first instruction of any `Call`/`CallWithTrace` target. "Step out" sets a breakpoint at the `Return` instruction of the current frame.

---

## 8. Hot Code Reload

### 8.1 Incremental Compilation Pipeline

Hot reload reuses the Salsa-based incremental compilation already implemented in `tools_onionskin/src/compiler.rs`. The `CompilerRunner` supports `compile_from_filesystem` with modified source files — Salsa automatically reuses cached intermediate results for unchanged files.

**Reload pipeline**:

1. Client sends `ReloadRequest` with new source files (or a path to reload from disk).
2. The control plane creates a `CompilerRunner`, feeds it the new source, and compiles to bytecode.
3. If compilation succeeds, a new `BexEngine` is created from the bytecode.
4. The engine reference is atomically swapped (see Section 3.5).
5. The control plane returns `ReloadResponse` with the compilation result (success, warnings, errors).

```rust
// Pseudocode for reload handler

async fn reload(
    engine_slot: &Arc<RwLock<Option<Arc<BexEngine>>>>,
    source_files: HashMap<String, String>,
    env_vars: HashMap<String, String>,
) -> Result<ReloadResponse, ControlPlaneError> {
    // Step 1: Compile new source
    let snapshot = baml_compiler_emit::generate_project_bytecode(&source_files)?;

    // Step 2: Create new engine
    let new_engine = BexEngine::new(snapshot, env_vars, SysOps::native())?;
    let new_engine = Arc::new(new_engine);

    // Step 3: Atomic swap
    {
        let mut guard = engine_slot.write().await;
        let old_engine = guard.replace(Arc::clone(&new_engine));
        // old_engine is dropped when all inflight VMs complete
        // (they hold Arc<BexEngine> references)
    }

    Ok(ReloadResponse {
        success: true,
        new_epoch: new_engine.current_epoch.load(Ordering::Relaxed),
    })
}
```

### 8.2 Epoch Coordination

The epoch system already handles concurrent VM lifecycle:

- `current_epoch: AtomicU64` on `BexEngine` tracks the current epoch.
- Each `call_function` registers with the current epoch.
- `collect_garbage` increments the epoch and waits for old-epoch VMs to park.

Hot reload piggybacks on this: the old engine's VMs run to completion in their own epoch. The new engine starts its own epoch counter from 0. There is no cross-engine epoch coordination needed because each `Arc<BexEngine>` is independent.

**Drain monitoring**: The control plane can monitor the old engine's drain progress by watching `EpochState::active` counters. When both epoch slots reach zero active VMs, the old engine is fully drained.

### 8.3 Schema Migrations

Schema changes (new classes, removed fields, changed types) are handled by the engine-replacement strategy:

| Change Type | Inflight VMs (old engine) | New VMs (new engine) |
|---|---|---|
| New function added | Not visible | Available immediately |
| Function signature changed | Uses old signature | Uses new signature |
| Class field added | Uses old schema | Uses new schema |
| Class field removed | Uses old schema | Uses new schema |
| Enum variant added | Uses old variants | Uses new variants |

There is no attempt to migrate inflight VM state. This is intentional: a running VM has values on its stack and heap that conform to the old schema. Attempting to migrate them mid-execution would be unsafe and complex. The BEAM takes the same approach — old code runs to completion on the old module version.

---

## 9. REPL / Expression Evaluation

### 9.1 Standalone Evaluation

The REPL compiles a BAML expression to bytecode and runs it in a fresh VM against the current engine's heap and globals.

```rust
// Pseudocode for standalone eval

async fn eval(
    engine: &Arc<BexEngine>,
    expression: &str,
) -> Result<BexExternalValue, EvalError> {
    // Step 1: Wrap expression in a synthetic function
    let source = format!("function __repl__() -> _ {{ return {} }}", expression);

    // Step 2: Compile (reusing engine's schema for type resolution)
    let snapshot = compile_expression(&source, engine)?;

    // Step 3: Create a temporary engine with the expression
    // that shares the main engine's resolved names
    let result = engine.call_function("__repl__", vec![]).await?;

    Ok(result)
}
```

The `VmRunnerState` in `tools_onionskin/src/compiler.rs` already demonstrates this pattern — it tracks `available_functions`, selects one, and runs it via the engine.

### 9.2 At-Breakpoint Evaluation

When a VM is paused at a breakpoint, eval can access the VM's local variables and stack.

**Mechanism**: The paused VM's `frames` and `stack` are read-only accessible. The expression is compiled with the locals in scope, and a throwaway VM is created with a copy of the relevant stack segment. The throwaway VM runs the expression and returns the result without modifying the paused VM's state.

```rust
// Pseudocode for at-breakpoint eval

fn eval_at_breakpoint(
    paused_vm: &BexVm,
    frame_depth: usize,
    expression: &str,
) -> Result<String, EvalError> {
    let frame = &paused_vm.frames[frame_depth];

    // Extract locals from the paused VM's stack
    let locals_end = paused_vm.frames.get(frame_depth + 1)
        .map(|f| f.locals_offset)
        .unwrap_or(paused_vm.stack.len());

    let locals: Vec<Value> = (frame.locals_offset..locals_end)
        .map(|i| paused_vm.stack[i].clone())
        .collect();

    // Compile expression with local variable bindings
    // Run in throwaway VM
    // Return debug::display_value of result
    todo!()
}
```

---

## 10. AI Agent Integration

### 10.1 Tool Definitions

The control plane RPCs map directly to AI agent tool-use definitions. Each RPC becomes a tool with typed parameters and return values.

```json
{
  "tools": [
    {
      "name": "list_vms",
      "description": "List all active VMs in the BEX engine with their state, entry function, epoch, and frame depth.",
      "input_schema": {}
    },
    {
      "name": "inspect_stack",
      "description": "Inspect the call stack of a specific VM. Returns frame-by-frame function names, instruction pointers, source lines, and local variables.",
      "input_schema": {
        "type": "object",
        "properties": {
          "vm_id": { "type": "string" }
        },
        "required": ["vm_id"]
      }
    },
    {
      "name": "disassemble",
      "description": "Disassemble a function to its BEX bytecode instructions with source line mapping.",
      "input_schema": {
        "type": "object",
        "properties": {
          "function_name": { "type": "string" }
        },
        "required": ["function_name"]
      }
    },
    {
      "name": "eval",
      "description": "Evaluate a BAML expression in the engine context. Returns the result as a string.",
      "input_schema": {
        "type": "object",
        "properties": {
          "expression": { "type": "string" }
        },
        "required": ["expression"]
      }
    },
    {
      "name": "set_breakpoint",
      "description": "Set a breakpoint at a function and source line. The VM will pause when it reaches this point.",
      "input_schema": {
        "type": "object",
        "properties": {
          "function_name": { "type": "string" },
          "source_line": { "type": "integer" }
        },
        "required": ["function_name", "source_line"]
      }
    },
    {
      "name": "attach_trace",
      "description": "Dynamically attach tracing to a function. All calls to this function will emit RuntimeEvents until detached.",
      "input_schema": {
        "type": "object",
        "properties": {
          "function_name": { "type": "string" }
        },
        "required": ["function_name"]
      }
    },
    {
      "name": "reload",
      "description": "Hot-reload BAML source files. Compiles new source, creates a new engine, and atomically swaps. Inflight requests drain on the old engine.",
      "input_schema": {
        "type": "object",
        "properties": {
          "source_files": {
            "type": "object",
            "description": "Map of file path to file content"
          }
        },
        "required": ["source_files"]
      }
    }
  ]
}
```

### 10.2 Agent Workflow Example

An AI agent debugging a failing BAML function:

```
Agent: "The function 'classify_sentiment' is returning unexpected results. Let me investigate."

1. list_vms() → [VM #7: running classify_sentiment, epoch 42, frame depth 3]
2. attach_trace("classify_sentiment") → OK
3. <waits for next invocation>
4. <RuntimeEvent: FunctionStart { name: "classify_sentiment", args: ["This product is terrible"] }>
5. <RuntimeEvent: FunctionEnd { name: "classify_sentiment", result: "Positive" }>

Agent: "The function returned 'Positive' for a clearly negative input. Let me inspect the prompt template."

6. disassemble("classify_sentiment") → [bytecode listing with LLM call]
7. eval("get_prompt_template('classify_sentiment')") → "Classify the sentiment: {input}. Return Positive."

Agent: "Found it — the prompt template has a bias toward 'Positive' in its instruction. The template
        ends with 'Return Positive.' which biases the LLM. Let me fix it."

8. reload({
     "src/classify.baml": "...fixed source with neutral prompt template..."
   }) → { success: true, new_epoch: 1 }

9. detach_trace("classify_sentiment") → OK

Agent: "Reloaded with fixed prompt template. New requests will use the corrected version."
```

### 10.3 Safety Constraints

1. **Read-only default**: AI agents are granted `Inspect` capability only. `Trace`, `Debug`, `Eval`, and `Reload` require explicit user approval per session.

2. **Eval sandboxing**: Evaluated expressions run in a throwaway VM with no side effects. They cannot call `@trace`-annotated functions, write to globals, or invoke system operations (LLM calls, HTTP, file I/O). The `SysOps` for eval VMs is a no-op stub.

3. **Reload approval**: The `Reload` RPC requires a confirmation step. The server compiles the new source and returns a diff summary. The agent must then confirm the reload with the diff hash, preventing accidental or malicious reloads.

4. **Rate limiting**: The control plane enforces rate limits per session: max 100 RPCs/second for read operations, max 1 reload/minute, max 10 breakpoints per session.

5. **Audit log**: All mutating operations (breakpoint set/remove, trace attach/detach, eval, reload) are logged with the session ID, timestamp, and parameters. The audit log is a `RuntimeEvent` variant, feeding into the existing event infrastructure.

---

## 11. Implementation Milestones

### Milestone 1: Read-Only Introspection

| Goal | Requires | Test |
|---|---|---|
| `VmRegistry` on `BexEngine` behind `cfg(feature = "control-plane")` | Add `Mutex<HashMap<VmId, VmMetadata>>` to `BexEngine` | Unit test: register/deregister VM, verify list |
| `ListVMs` RPC returns active VM metadata | `bex_control_plane` crate, tonic server, proto definitions | Integration test: start engine, call function, ListVMs returns 1 entry |
| `InspectStack` RPC returns frame info for paused VM | VM pause mechanism (breakpoint not needed — use `Await` safepoint) | Integration test: VM awaiting future, InspectStack returns frames |
| `Disassemble` RPC returns bytecode for named function | Wire `debug::display_instruction` to proto response | Integration test: compile function, Disassemble returns instructions |
| UDS transport with PID-based path | `tonic` UDS listener | Integration test: connect to `/tmp/bex-control-<pid>.sock` |

**Exit criteria**: A CLI tool can connect to a running BAML process and list VMs, inspect stacks, and disassemble functions.

### Milestone 2: Event Subscription and Dynamic Tracing

| Goal | Requires | Test |
|---|---|---|
| `broadcast` channel on `BexEngine` for `RuntimeEvent` | `tokio::sync::broadcast::Sender` field | Unit test: subscribe, emit event, receive on subscriber |
| `SubscribeEvents` RPC streams events to client | Server-streaming gRPC | Integration test: trace function, subscribe, receive FunctionStart/End |
| `AttachTrace` / `DetachTrace` RPCs | `RwLock<HashSet<HeapPtr>>` on engine, check in `Call` handler | Integration test: attach trace, call function, verify events emitted |
| Conditional tracing with BAML expression predicate | Expression compilation, throwaway VM evaluation | Integration test: conditional trace fires only when predicate matches |

**Exit criteria**: An operator can attach tracing to any function at runtime and receive a live stream of events.

### Milestone 3: Breakpoints and Debug Sessions

| Goal | Requires | Test |
|---|---|---|
| `VmExecState::Breakpoint` variant | Extend enum in `bex_vm/src/vm.rs` | Unit test: VM yields Breakpoint when hitting flagged instruction |
| Breakpoint index on engine | `RwLock<HashSet<(HeapPtr, isize)>>` | Unit test: set breakpoint, verify index contains entry |
| Source-line to instruction resolution | `Bytecode::source_lines` reverse lookup | Unit test: resolve line 5 to instruction offset |
| `DebugSession` bidirectional stream | Client sends Resume/Step, server sends BreakpointHit | Integration test: set breakpoint, call function, receive hit, resume, complete |
| Step over / step into / step out | Temporary breakpoint manipulation | Integration test: step through 3-instruction sequence |

**Exit criteria**: A debugger client can set breakpoints, pause VMs, inspect state, step through bytecode, and resume.

### Milestone 4: REPL and Hot Reload

| Goal | Requires | Test |
|---|---|---|
| Standalone `Eval` RPC | Expression compilation, throwaway VM | Integration test: `eval("1 + 2")` returns `3` |
| At-breakpoint eval with local access | Stack segment copy, scoped compilation | Integration test: pause at breakpoint, eval local variable, get correct value |
| `Reload` RPC with compilation | `CompilerRunner` integration, engine swap | Integration test: reload with new source, new function available |
| Drain monitoring | `EpochState::active` counter observation | Integration test: start long-running function, reload, verify old VM completes |
| Reload confirmation step | Diff summary, hash verification | Integration test: reload returns diff, confirm with hash |

**Exit criteria**: Operators can evaluate expressions, inspect locals at breakpoints, and hot-reload source without dropping requests.

### Milestone 5: AI Agent Integration and Hardening

| Goal | Requires | Test |
|---|---|---|
| Tool-use JSON schema generation from proto | Proto reflection or codegen | Test: generated schema matches expected JSON |
| Session capability enforcement | Middleware gRPC interceptor | Test: session without `Debug` capability rejected on `SetBreakpoint` |
| Eval sandboxing (no-op SysOps) | `SysOps::noop()` variant | Test: eval with LLM call returns error, not actual LLM response |
| Rate limiting per session | Token bucket in session state | Test: exceed 100 RPCs/sec, get rate-limited response |
| Audit logging as `RuntimeEvent` | New `EventKind::ControlPlane` variant | Test: set breakpoint, verify audit event emitted |
| TCP transport with TLS | `tonic` TLS configuration | Test: connect over TCP with TLS, verify encrypted |

**Exit criteria**: An AI agent can connect with scoped capabilities, debug a running system, and all operations are audited.

---

## 12. Deferred Work

- **WASM target support** — The gRPC control plane cannot run in WASM. A future in-process adapter would expose the same API via function calls, usable from `baml_playground_wasm`.
- **Distributed tracing** — Correlating spans across multiple BAML processes (e.g., microservices). Requires a trace collector (Jaeger/Zipkin) integration.
- **Persistent breakpoints** — Breakpoints that survive process restarts. Requires a breakpoint config file.
- **Memory profiling** — Heap dump and allocation tracking beyond `heap_stats()`. Requires GC instrumentation.
- **Time-travel debugging** — Recording and replaying VM execution. Requires instruction-level logging with full state snapshots.
- **Multi-engine federation** — A single control plane managing multiple engines (e.g., in a test harness running parallel engines).

---

## 13. Open Questions

1. **Breakpoint check cost at scale** — The current design checks a `HashSet` per instruction. For tight loops with millions of iterations, is a per-instruction atomic load acceptable? Alternative: check only at function entry and backward jumps (reducing granularity but eliminating inner-loop overhead).

2. **Expression compilation scope** — When evaluating at a breakpoint, how much of the engine's schema should be available? Full schema (all classes, enums, functions) or only what's in scope at the current frame?

3. **Reload and dynamic traces** — When the engine is swapped, dynamic traces reference `HeapPtr`s on the old heap. Should traces auto-migrate to the new engine (by function name lookup), or should they be dropped on reload?

4. **GC interaction with breakpoints** — If a VM is paused at a breakpoint for a long time, it holds roots that prevent GC collection. Should there be a timeout that auto-resumes paused VMs?

5. **Proto versioning strategy** — How do we version the gRPC API? Options: URL path versioning (`/v1/BexControl`), proto package versioning (`bex.control.v1`), or feature negotiation at session creation.

---

## 14. References

- **Erlang/OTP `observer`** — [https://www.erlang.org/doc/apps/observer/observer_ug](https://www.erlang.org/doc/apps/observer/observer_ug) — Process introspection and tracing in the BEAM.
- **Erlang hot code loading** — [https://www.erlang.org/doc/system/code_loading](https://www.erlang.org/doc/system/code_loading) — The BEAM's approach to loading new module versions while old ones drain.
- **tonic** — [https://github.com/hyperium/tonic](https://github.com/hyperium/tonic) — Rust gRPC framework.
- **event-publishing-design-v2.md** — `RuntimeEvent` types and event infrastructure.
- **callstack-tracking-design-v2.md** — Unified call stack model and `Frame` structure.
- **bex_vm/src/vm.rs** — `BexVm`, `VmExecState`, `Frame` definitions.
- **bex_engine/src/lib.rs** — `BexEngine`, epoch system, `collect_garbage`, `call_function_traced`.
- **bex_vm/src/debug.rs** — `display_instruction`, `display_value` for introspection formatting.
- **bex_events/src/types.rs** — `RuntimeEvent`, `EventKind`, `SpanContext`.
- **bridge_cffi/src/engine.rs** — `Arc<RwLock<Option<Arc<BexEngine>>>>` pattern for engine management.
- **tools_onionskin/src/compiler.rs** — `CompilerRunner`, Salsa incremental compilation, `VmRunnerState`.
