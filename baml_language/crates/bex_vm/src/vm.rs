//! BEX VM - The synchronous bytecode interpreter.
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for direct heap access during instruction execution:
//! - `heap.get_object(idx)`: Reading objects for type checks, field access, method dispatch
//! - `get_object_mut()`: Mutating object fields through `&mut self`
//!
//! Safety is ensured by:
//! - Single-threaded execution: Each VM instance runs on one thread at a time
//! - TLAB exclusivity: VMs have exclusive write access to their allocation regions
//! - Controlled mutation: Only VM-owned runtime objects can be mutated, and only via `&mut self`
//! - GC coordination: Garbage collection only runs when VMs are at safepoints (yielded)

#![allow(unsafe_code)]

use std::{collections::HashMap, sync::Arc};

use ::bex_vm_types::{EarlyYieldCheck, RootHaver, types::ErrorClass};
use ::core::any::TypeId;
#[cfg(not(target_arch = "wasm32"))]
use ::core::sync::atomic::AtomicBool;
use bex_heap::{BexHeap, Generation, Tlab};
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, GlobalPool, HeapPtr, Instruction, Object, ObjectIndex, ObjectPool,
    ObjectType, PanicClass, StackIndex, UnaryOp, Value, Variant,
    bytecode::{self, BlockNotification},
    types::{
        BoundMethod, Cell, Closure, Function, FunctionType, Future, FutureType, Instance,
        PendingFuture, Type,
    },
};
use indexmap::IndexMap;

use crate::{
    errors::{StackFrame, VmBamlError, VmError, VmInternalError, VmPanic, VmRustFnError},
    indexable::{EvalStack, EvalStackTrait},
    package_baml::{BamlPackageBaml, NativeCallResult, NativeFunction},
    types::ObjectTrait,
    watch::{self, NodeId, RootState, Watch, WatchFilter},
};

/// Max call stack size.
pub const MAX_FRAMES: usize = 256;

/// Bytecode call frame — pushed when entering a bytecode function.
/// Deliberately Copy so the exec-loop can snapshot/restore cheaply.
#[derive(Clone, Copy, Debug)]
pub(crate) struct BytecodeFrame {
    /// Pointer to the running function (or closure) object.
    pub(crate) function: HeapPtr,
    /// Instruction pointer (IP). Points to the next instruction.
    pub(crate) instruction_ptr: usize,
    /// Local variables offset in the eval stack.
    pub(crate) locals_offset: StackIndex,
    /// Byte offset of the most recently dispatched opcode (compact path).
    /// In the legacy path this mirrors `instruction_ptr - 1` and is kept
    /// up-to-date before each `step()` call.
    /// Used by `capture_stack_trace`, `try_unwind_exception`, and event
    /// source location capture.
    pub(crate) faulting_pc: usize,
}

impl RootHaver for BytecodeFrame {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.function);
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.function = roots.get(&self.function).copied().unwrap_or(self.function);
    }
}

/// Native continuation frame — pushed when a native function yields via
/// `NativeCallResult::YieldToCall`. Sits below the callback's bytecode
/// frame on the call stack and is popped when the callback returns.
pub(crate) struct NativeFrame {
    /// Pointer to the native function object (for GC roots + stack traces).
    pub(crate) function: HeapPtr,
    /// The continuation to invoke with the callback's return value.
    pub(crate) continuation: Box<dyn crate::package_baml::Continuation>,
}

impl RootHaver for NativeFrame {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.function);
        roots.extend_from_slice(&self.continuation.gc_roots());
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.function = roots.get(&self.function).copied().unwrap_or(self.function);
        self.continuation.apply_forwarding(roots);
    }
}

/// Call frame — either a bytecode frame or a native continuation frame.
pub(crate) enum Frame {
    Bytecode(BytecodeFrame),
    Native(NativeFrame),
}

impl Frame {
    /// Get the function pointer (valid for both variants).
    pub(crate) fn function(&self) -> HeapPtr {
        match self {
            Frame::Bytecode(f) => f.function,
            Frame::Native(f) => f.function,
        }
    }
}

impl RootHaver for Frame {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        match self {
            Frame::Bytecode(f) => f.collect_roots(roots),
            Frame::Native(f) => f.collect_roots(roots),
        }
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        match self {
            Frame::Bytecode(f) => f.forward_roots(roots),
            Frame::Native(f) => f.forward_roots(roots),
        }
    }
}

/// The beast.
///
/// This is a stack based virtual machine. Stack based machines work by pushing
/// and popping values from an "evaluation stack". Picture this example from
/// [Crafting Interpreters](https://craftinginterpreters.com/a-virtual-machine.html):
///
/// ```ignore
/// fn echo(n) {
///     print(n)
///     return n
/// }
///
/// print(echo(echo(1) + echo(2)) + echo(echo(4) + echo(5)))
/// ```
///
/// Output should be:
///
/// ```text
/// 1
/// 2
/// 3
/// 4
/// 5
/// 9
/// 12
/// ```
///
/// The code above would create an AST similar to this:
///
/// ```text
///                 +-------+
///                 | print |
///                 +-------+
///                     |
///                   +---+
///          +--------| + |--------+
///          |        +---+        |
///      +------+               +------+
///      | echo |               | echo |
///      +------+               +------+
///          |                     |
///        +---+                 +---+
///        | + |                 | + |
///        +---+                 +---+
///          |                     |
///     +---------+           +----------+
///     |         |           |          |
/// +------+   +------+   +------+   +------+
/// | echo |   | echo |   | echo |   | echo |
/// +------+   +------+   +------+   +------+
///     |         |           |          |
///   +---+     +---+       +---+      +---+
///   | 1 |     | 2 |       | 4 |      | 5 |
///   +---+     +---+       +---+      +---+
/// ```
///
/// If we "flatten" the AST considering the "lifetime" of each value, we get
/// this structure:
///
/// ```text
///                   +---+
/// constant 1 ...... | 1 |
/// echo(1) ......... |   |---+
/// constant 2 ...... |   | 2 |
/// echo(2) ......... |   |   |
///                   +---+---+
/// add 1+2 ......... | 3 |
/// echo(3) ......... |   |---+
/// constant 4 ...... |   | 4 |
/// echo(4) ......... |   |   |---+
/// constant 5 ...... |   |   | 5 |
/// echo(5) ......... |   |   |   |
///                   |   |---+---+
/// add 4+5 ......... |   | 9 |
/// echo(9) ......... |   |   |
///                   +---+---+
/// add 3+9 ......... |12 |
/// print(12) ....... |   |
///                   +---+
/// ```
///
/// Looks like a stack doesn't it? That's the evaluation stack. All values in
/// the program flow through that stack, eliminating the need for instructions
/// with registers. Instead of `ADD r2, r0, r1` we just have `ADD`, which pops
/// two values from the stack, produces the result and pushes it back on top.
/// Simple, right? The drawback is that we need to execute more instructions to
/// achieve the same result as a register based VM. If we want to add two
/// variables, a register VM would run a single instruction:
///
/// ```text
/// ADD r2, r0, r1  // Add the contents of r0 and r1 and store the result in r2
///                 // r2 = r0 + r1
/// ```
///
/// Meanwhile a stack VM would run 4 instructions:
///
/// ```text
/// LOAD_VAR 0   // Push the contents of variable 0 on top of the stack
/// LOAD_VAR 1   // Push the contents of variable 1 on top of the stack
/// ADD          // Pop two values, add and push the result on top of the stack
/// STORE_VAR 2  // Store the top of the stack in variable 2
/// ```
///
/// Basically it's slower because it needs more cycles to do the same thing.
/// Other than that, pretty much everything is better in a stack VM, especially
/// simplicity (we don't even need to figure out which registers to use and when
/// to use them).
pub struct BexVm {
    /// Call stack.
    ///
    /// On each function call we create a new [`Frame`] and push it on this
    /// stack. On each return, we destroy the frame and pop it from the stack
    /// to resume the execution of the previous frame.
    pub(crate) frames: Vec<Frame>,

    /// Evaluation stack.
    ///
    /// This stack only stores values.
    pub stack: EvalStack,

    /// Reference to the shared heap (long-lived, shared across VMs).
    pub heap: Arc<BexHeap>,

    pub early_yield: EarlyYieldCheck,

    /// Thread-local allocation buffer (exclusive to this VM).
    pub tlab: Tlab,

    /// Global variables.
    ///
    /// This stores the functions and globally declared variables.
    pub globals: GlobalPool,

    /// Resolved class names mapping fully-qualified class names to their heap pointers.
    ///
    /// Used by `resolve_class()` for generated `copy::` struct `to_value()` methods.
    /// Populated at VM construction time from the compiled program's class index.
    pub resolved_class_names: HashMap<String, HeapPtr>,

    /// Pre-resolved heap pointers for `baml.errors.*` classes, indexed by
    /// `ErrorClass` discriminant.
    error_class_ptrs: Vec<HeapPtr>,

    /// Pre-resolved heap pointers for `baml.panics.*` classes, indexed by
    /// `PanicClass` discriminant.
    panic_class_ptrs: Vec<HeapPtr>,

    /// Emit dependency graph.
    pub watch: Watch,

    /// Tracks which local variables are watched (have @watch).
    pub(crate) watched_vars: HashMap<StackIndex, (String, String)>,

    pub interrupt_frame: Option<usize>,

    /// Frame depths for traced function calls. Always sorted ascending (LIFO).
    /// Checked on `Return` to yield `FunctionExit` notifications.
    traced_frames: Vec<usize>,

    /// Current span context, set by the engine before each VM execution step.
    /// Available to `//baml:mut_vm` native functions that need to emit events
    /// with the correct span context.
    pub current_span_context: Option<bex_events::SpanContext>,

    /// Process argv passed to the engine at startup. Exposed to BAML via
    /// `baml.sys.argv()`. Shared (cheap to clone) across VMs.
    pub argv: Arc<[String]>,
}

/// VM execution state.
///
/// The virtual machine cannot deal with futures, so when when it stumbles upon
/// future creation instructions, it returns control flow to the embedder,
/// expecting the embedder to schedule the future and yield back the control
/// flow to the VM.
///
/// Similarly, when the VM encounters an await point, it returns control flow to
/// the embedder, expecting the embedder to await the future and fulfil it with
/// the final result before yielding back control flow to the VM.
#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq)]
pub enum VmExecState {
    /// VM cannot proceed. It is awaiting a pending future to complete.
    Await(HeapPtr),

    /// VM notifies caller about a future that needs to be scheduled.
    ///
    /// Bytecode execution continues when control flow is handled back to the
    /// VM.
    ScheduleFuture(HeapPtr),

    /// VM has completed the execution of all available bytecode.
    Complete(Value),

    /// Notify about watched variables.
    Notify(WatchNotification),

    /// Notify about span lifecycle (from traced `Call` / `Return`).
    SpanNotify(SpanNotification),

    /// The VM is yielding a custom event to be emitted.
    ///
    /// The engine handles this by converting both values to `BexExternalValue`
    /// and emitting a `CustomEvent` with the current span context.
    Event {
        /// Name of the event (extracted from the String heap object).
        event_name: String,
        /// Event payload (raw VM value; engine converts to `BexExternalValue`).
        data: Value,
        /// Source location where the event was emitted:
        /// (`file_id`, line, column, `start_offset`, `end_offset`).
        source_location: Option<(u32, u32, u32, u32, u32)>,
    },

    /// We are still executing, but we should yield to allow other threads or the GC to run.
    EarlyYield,
}

#[allow(clippy::large_enum_variant)]
#[derive(Debug, PartialEq)]
pub enum WatchNotification {
    Variables(Vec<watch::NodeId>),
    Block(BlockNotification),
    Viz {
        function_name: String,
        event: bex_vm_types::bytecode::VizExecEvent,
    },
}

/// Span notifications yielded by the VM for callstack tracking.
///
/// The VM provides args and result values from the eval stack so the engine
/// can emit `FunctionStart`/`FunctionEnd` events without additional lookups.
/// The VM itself has no span state (no `SpanId`, no timing) — all observability
/// logic lives in the engine.
#[derive(Clone, Debug, PartialEq)]
pub enum SpanNotification {
    /// A traced function call was entered.
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

/// Intermediate representation of a compiled BAML program.
///
/// `BytecodeProgram` holds compile-time objects in an `ObjectPool` which are
/// transferred to the unified `BexHeap` when creating a `BexEngine`.
///
/// # Lifecycle
///
/// 1. **Creation**: `convert_program()` builds `BytecodeProgram` from raw bytecode
/// 2. **Object Transfer**: `BexEngine::new()` extracts `objects` into `BexHeap`
/// 3. **Discard**: The `ObjectPool` is consumed; runtime uses `BexHeap` exclusively
///
/// # Why `ObjectPool` Here?
///
/// `ObjectPool` is used here (not `Vec`) because:
/// - `convert_program()` builds objects incrementally with type-safe indexing
/// - Preserves phantom-typed `ObjectIndex` semantics during construction
/// - After transfer to `BexHeap`, runtime allocation uses TLABs instead
///
/// See `BexEngine::new()` for the handoff to unified heap architecture.
#[derive(Clone, Debug)]
pub struct BytecodeProgram {
    pub objects: ObjectPool,
    /// Compile-time globals (converted to runtime Values in `BexEngine::new`).
    pub globals: Vec<bex_vm_types::ConstValue>,
    pub resolved_function_names: HashMap<String, (ObjectIndex, FunctionKind)>,
    pub resolved_class_names: HashMap<String, ObjectIndex>,
    pub resolved_enums_names: HashMap<String, ObjectIndex>,
    /// Maps function names to their global indices.
    /// Used for dynamic function lookup at runtime.
    pub function_global_indices: HashMap<String, usize>,
    /// Pre-formatted Jinja `{% macro %}` definitions for all `template_strings`.
    pub template_strings_macros: String,
    /// Client build metadata, passed through to `SysOpContext`.
    pub client_metadata: HashMap<String, bex_vm_types::ClientBuildMeta>,
    /// Compiled test cases.
    pub test_cases: Vec<bex_vm_types::TestCase>,
    /// Recursive type alias definitions for output format rendering.
    pub recursive_type_alias_defs: indexmap::IndexMap<baml_type::TypeName, baml_type::Ty>,
}

/// Convert a compiled `Program` to a `BytecodeProgram` with native functions attached.
///
/// This is the bridge between compilation output and VM execution. It:
/// 1. Attaches native function implementations to builtin functions
/// 2. Builds resolved name lookups for functions, classes, and enums
pub fn convert_program(program: bex_vm_types::Program) -> Result<BytecodeProgram, VmInternalError> {
    // Convert objects, attaching native functions
    let objects: Vec<Object> = program
        .objects
        .into_iter()
        .map(crate::package_baml::attach_builtins)
        .collect::<Result<Vec<_>, _>>()?;

    // Build resolved name maps by scanning objects
    let mut resolved_function_names = HashMap::new();
    let mut resolved_class_names = HashMap::new();
    let mut resolved_enums_names = HashMap::new();

    for (idx, obj) in objects.iter().enumerate() {
        let obj_idx = ObjectIndex::from_raw(idx);
        match obj {
            Object::Function(func) => {
                resolved_function_names.insert(func.name.clone(), (obj_idx, func.kind));
            }
            Object::Class(class) => {
                resolved_class_names.insert(class.name.to_string(), obj_idx);
            }
            Object::Enum(enum_def) => {
                resolved_enums_names.insert(enum_def.name.to_string(), obj_idx);
            }
            _ => {}
        }
    }

    Ok(BytecodeProgram {
        objects: ObjectPool::from_vec(objects),
        globals: program.globals,
        resolved_function_names,
        resolved_class_names,
        resolved_enums_names,
        function_global_indices: program.function_global_indices,
        template_strings_macros: program.template_strings_macros,
        client_metadata: program.client_metadata,
        test_cases: program.test_cases,
        recursive_type_alias_defs: program.recursive_type_alias_defs,
    })
}

/// Get the type tag for any runtime value.
///
/// This is a free function to avoid borrow checker issues when called
/// from within the instruction dispatch loop.
fn value_type_tag(value: &Value) -> i64 {
    use bex_vm_types::types::type_tags;

    match value {
        Value::Int(_) => type_tags::INT,
        Value::Float(_) => type_tags::FLOAT,
        Value::Bool(_) => type_tags::BOOL,
        Value::Null => type_tags::NULL,
        Value::Object(ptr) => {
            // SAFETY: Reading type information from objects via HeapPtr.
            let obj = unsafe { ptr.get() };
            match obj {
                Object::String(_) => type_tags::STRING,
                Object::Uint8Array(_) => type_tags::UINT8ARRAY,
                Object::Variant(_) => type_tags::ENUM,
                Object::Array(_) => type_tags::LIST,
                Object::Map(_) => type_tags::MAP,
                Object::Function(_) => type_tags::FUNCTION,
                Object::Closure(_) => type_tags::FUNCTION,
                Object::BoundMethod(_) => type_tags::FUNCTION,
                Object::Cell(_) => type_tags::UNKNOWN,
                Object::Future(_) => type_tags::FUTURE,
                Object::Enum(_) => type_tags::ENUM,
                Object::RustData(_) => type_tags::UNKNOWN,
                Object::Collector(_) => type_tags::COLLECTOR,
                Object::Type(_) => type_tags::TYPE,
                Object::Class(_) => type_tags::UNKNOWN,
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => type_tags::UNKNOWN,
                Object::Instance(instance) => {
                    let class_obj = unsafe { instance.class.get() };
                    let Object::Class(class) = class_obj else {
                        unreachable!("Instance.class does not point to a Class object")
                    };
                    class.type_tag
                }
            }
        }
    }
}

impl BexVm {
    /// Create a new VM with a shared heap.
    ///
    /// The heap is shared across all VMs. Each VM gets its own TLAB
    /// for contention-free allocation.
    pub fn new(
        heap: Arc<BexHeap>,
        globals: GlobalPool,
        resolved_class_names: HashMap<String, HeapPtr>,
        #[cfg(not(target_arch = "wasm32"))] park_requested: Arc<AtomicBool>,
        argv: Arc<[String]>,
    ) -> Self {
        let tlab = Tlab::new(Arc::clone(&heap));

        // Pre-resolve error class pointers indexed by Error discriminant.
        let error_class_ptrs: Vec<HeapPtr> = ErrorClass::ALL
            .iter()
            .map(|ec| {
                *resolved_class_names.get(ec.fqn()).unwrap_or_else(|| {
                    panic!("error class {:?} not in resolved_class_names", ec.fqn())
                })
            })
            .collect();

        // Pre-resolve panic class pointers indexed by PanicClass discriminant.
        let panic_class_ptrs: Vec<HeapPtr> = PanicClass::ALL
            .iter()
            .map(|pc| {
                *resolved_class_names.get(pc.fqn()).unwrap_or_else(|| {
                    panic!("panic class {:?} not in resolved_class_names", pc.fqn())
                })
            })
            .collect();

        let early_yield = EarlyYieldCheck::new(
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
        );

        Self {
            frames: Vec::new(),
            stack: EvalStack::new(),
            heap,
            early_yield,
            tlab,
            globals,
            resolved_class_names,
            error_class_ptrs,
            panic_class_ptrs,
            watch: Watch::new(),
            watched_vars: HashMap::new(),
            interrupt_frame: None,
            traced_frames: Vec::new(),
            current_span_context: None,
            argv,
        }
    }

    /// Read an object from the heap via `HeapPtr`.
    ///
    /// # Safety
    ///
    /// This is safe for:
    /// - Compile-time objects (immutable)
    /// - Objects allocated by this VM's TLAB
    /// - Objects from other VMs when they're not being mutated
    #[inline]
    pub fn get_object(&self, ptr: HeapPtr) -> &Object {
        // SAFETY: Single-threaded execution within a VM. Objects are only
        // written during allocation or field writes, both controlled by this VM.
        unsafe { ptr.get() }
    }

    /// Get mutable access to an object via `HeapPtr`.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access (typically via TLAB ownership
    /// or single-threaded execution). Only runtime objects can be mutated.
    #[inline]
    pub fn get_object_mut(&mut self, ptr: HeapPtr) -> &mut Object {
        // SAFETY: We have &mut self, so no other code can access the VM.
        // The TLAB ensures this VM has exclusive access to its allocated objects.
        assert!(
            !self.heap.is_compile_time_ptr(ptr),
            "Cannot mutate compile-time object"
        );
        // SAFETY: We have &mut self, ensuring exclusive access to this VM's objects
        unsafe { ptr.get_mut() }
    }

    /// Write barrier for field/element/cell writes.
    ///
    /// Called *before* the actual field write at each mutation site. If `container_ptr`
    /// is in an older generation than the object being written (`written_value`), the
    /// card containing `container_ptr` is marked dirty so partial GC can discover
    /// the cross-generation reference.
    ///
    /// This is a no-op when either side is not a heap object, or when the container
    /// is in Gen0 (no card table for Gen0).
    #[inline]
    pub fn write_barrier(&self, container_ptr: HeapPtr, written_value: Value) {
        if let Value::Object(ref_ptr) = written_value {
            let container_gen = self.heap.generation_of(container_ptr);
            let ref_gen = self.heap.generation_of(ref_ptr);
            if container_gen > ref_gen {
                self.heap.mark_card_for_ptr(container_ptr);
            }
        }
    }

    /// Conservative write barrier for mutable accessor paths (builtin dispatch).
    ///
    /// Unconditionally marks the card dirty if `container_ptr` is in an older
    /// generation. Used by `as_array_mut` / `as_map_mut` where the actual written
    /// value is not yet known (it's supplied by the callee trait method).
    ///
    /// This over-marks (any mutable access to an older-gen object dirties the card),
    /// but it is always safe and the cost is negligible since most objects are Gen0.
    #[inline]
    pub(crate) fn conservative_write_barrier(&self, container_ptr: HeapPtr) {
        let container_gen = self.heap.generation_of(container_ptr);
        if container_gen > Generation::Gen0 {
            self.heap.mark_card_for_ptr(container_ptr);
        }
    }

    /// Collect all `HeapPtr`s stored in call frames.
    /// - For bytecode frames, the function pointer
    /// - For native frames, the continuation pointer as well as all native-held GC roots
    ///
    /// Used by `bex_engine` to include frame roots in GC root sets.
    pub fn collect_frame_roots(&self) -> Vec<HeapPtr> {
        let mut roots = Vec::new();
        for frame in &self.frames {
            roots.push(frame.function());
            if let Frame::Native(nf) = frame {
                roots.extend(nf.continuation.gc_roots());
            }
        }
        roots
    }

    /// Update heap pointers held by frames according to a GC forwarding map.
    ///
    /// Must be called after a GC cycle to keep frame pointers valid.
    pub fn apply_frame_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for frame in &mut self.frames {
            match frame {
                Frame::Bytecode(bf) => {
                    if let Some(&new_ptr) = forwarding.get(&bf.function) {
                        bf.function = new_ptr;
                    }
                }
                Frame::Native(nf) => {
                    if let Some(&new_ptr) = forwarding.get(&nf.function) {
                        nf.function = new_ptr;
                    }
                    nf.continuation.apply_forwarding(forwarding);
                }
            }
        }
    }

    /// Convert an `ObjectIndex` to `HeapPtr` (for compile-time objects).
    ///
    /// Used during the transition from index-based to pointer-based access.
    #[inline]
    pub fn idx_to_ptr(&self, idx: ObjectIndex) -> HeapPtr {
        self.heap.compile_time_ptr(idx.into_raw())
    }

    /// Helper method to get `HeapPtr` from a Value, with type checking.
    fn as_object_ptr(
        &self,
        value: &Value,
        object_type: ObjectType,
    ) -> Result<HeapPtr, VmInternalError> {
        let Value::Object(ptr) = value else {
            return Err(VmInternalError::TypeError {
                expected: object_type.into(),
                got: self.type_of(value),
            });
        };
        Ok(*ptr)
    }

    /// Get string from a Value.
    pub fn as_string(&self, value: &Value) -> Result<&String, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::String)?;
        self.get_object(ptr).as_string()
    }

    /// Get uint8array from a Value.
    pub fn as_uint8array(&self, value: &Value) -> Result<&Vec<u8>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Uint8Array)?;
        let obj = self.get_object(ptr);
        match obj {
            Object::Uint8Array(bytes) => Ok(bytes),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Uint8Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable uint8array from a Value.
    pub fn as_uint8array_mut(&mut self, value: &Value) -> Result<&mut Vec<u8>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Uint8Array)?;
        match self.get_object_mut(ptr) {
            Object::Uint8Array(bytes) => Ok(bytes),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Uint8Array.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    /// Get type of a value.
    pub fn type_of(&self, value: &Value) -> Type {
        Type::of(value, |ptr| ObjectType::of(self.get_object(ptr)))
    }

    /// Get mutable string from a Value.
    pub fn as_string_mut(&mut self, value: &Value) -> Result<&mut String, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::String)?;
        self.get_object_mut(ptr).as_string_mut()
    }

    /// Get array from a Value.
    pub fn as_array(&self, value: &Value) -> Result<&[Value], VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        let obj = self.get_object(ptr);
        match obj {
            Object::Array(arr) => Ok(arr.as_slice()),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable array from a Value.
    pub fn as_array_mut(&mut self, value: &Value) -> Result<&mut Vec<Value>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        // Conservative write barrier: any mutable access to an older-generation
        // array may introduce cross-generation references. Used by builtin dispatch
        // (Array.push, Array.pop, etc.) where the actual written values are not
        // visible at this call site.
        self.conservative_write_barrier(ptr);
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(ptr), Object::Array(_)) {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(self.get_object(ptr)).into(),
            });
        }
        match self.get_object_mut(ptr) {
            Object::Array(arr) => Ok(arr),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get map from a Value.
    pub fn as_map(&self, value: &Value) -> Result<&IndexMap<String, Value>, VmInternalError> {
        let index = self.as_object_ptr(value, ObjectType::Map)?;
        let obj = self.get_object(index);
        match obj {
            Object::Map(map) => Ok(map),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable map from a Value.
    pub fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<&mut IndexMap<String, Value>, VmInternalError> {
        let index = self.as_object_ptr(value, ObjectType::Map)?;
        // Conservative write barrier: any mutable access to an older-generation
        // map may introduce cross-generation references. Used by builtin dispatch
        // (Map.set, etc.) where the actual written values are not visible here.
        self.conservative_write_barrier(index);
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(index), Object::Map(_)) {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(self.get_object(index)).into(),
            });
        }
        match self.get_object_mut(index) {
            Object::Map(map) => Ok(map),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get Value reference (for generic types).
    #[allow(dead_code)]
    pub fn as_value_mut(&mut self, value: &Value) -> Result<&mut Value, VmInternalError> {
        // This is used by macro-generated code for generic type parameters.
        // For now, we don't support mutable access to generic values.
        let Value::Object(ptr) = value else {
            return Err(VmInternalError::InvalidObjectRef(0));
        };
        Err(VmInternalError::InvalidObjectRef(ptr.as_ptr() as usize))
    }

    /// TODO: We should remove this API in favor of using `bex_engine` only (vbv)
    /// Creates a VM from a compiled [`bex_vm_types::Program`].
    ///
    /// This is primarily for testing. In production, use `BexEngine` which
    /// manages the heap across multiple VM instances. On native targets the
    /// caller supplies the `park_requested` atomic so tests can simulate the
    /// coordination signal that `BexEngine::collect_garbage` uses.
    pub fn from_program(
        program: bex_vm_types::Program,
        #[cfg(not(target_arch = "wasm32"))] park_requested: Arc<AtomicBool>,
    ) -> Result<Self, VmInternalError> {
        let bytecode = convert_program(program)?;

        // Extract compile-time objects for the heap
        let compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Create heap with compile-time objects
        let heap = BexHeap::new(compile_time_objects);

        // Convert compile-time globals (ConstValue) to runtime globals (Value)
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| cv.to_value(|idx| heap.compile_time_ptr(idx.into_raw())))
            .collect();
        let globals = GlobalPool::from_vec(globals_vec);

        // Build resolved_class_names: convert ObjectIndex -> HeapPtr
        let resolved_class_names: HashMap<String, HeapPtr> = bytecode
            .resolved_class_names
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx.into_raw())))
            .collect();

        Ok(Self::new(
            heap,
            globals,
            resolved_class_names,
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            Arc::from(Vec::<String>::new()),
        ))
    }

    /// Bootstraps the VM preparing the given function to run.
    pub fn set_entry_point(&mut self, function: HeapPtr, args: &[Value]) {
        debug_assert!(
            matches!(self.get_object(function), Object::Function(_)),
            "expect function as entry point, got {:?}",
            self.get_object(function)
        );

        self.stack.extend(args.iter().copied());

        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            faulting_pc: 0,
        }));

        // Entry functions need the same frame-local pre-allocation as normal
        // bytecode calls now that INIT_LOCALS is gone from bytecode.
        self.allocate_real_locals_for_frame(function)
            .expect("entry point must be a valid function frame");
    }

    /// Restores the VM state and prepares it for the next execution.
    ///
    /// This is used to clear the stack and frames after execution.
    pub fn finalize(&mut self) {
        // If the VM returns correctly with VmExecState::Complete, the eval
        // stack and call stack should be empty.
        self.stack.clear();
        self.frames.clear();
    }

    /// Returns a reference to the pending future.
    ///
    /// Returns [`VmInternalError::TypeError`] if the future is not pending, or not a future.
    pub fn pending_future(&self, future_ptr: HeapPtr) -> Result<&PendingFuture, VmInternalError> {
        match self.get_object(future_ptr) {
            Object::Future(Future::Pending(future)) => Ok(future),
            other => Err(VmInternalError::TypeError {
                expected: FutureType::Pending.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    /// Set a future to Ready state without modifying the stack.
    ///
    /// Use this for sync operations that complete during `ScheduleFuture` handling,
    /// before the VM reaches the `Await` instruction. The `Await` instruction will
    /// extract the value from the Ready future.
    pub fn set_future_ready(
        &mut self,
        future_ptr: HeapPtr,
        value: Value,
    ) -> Result<(), VmInternalError> {
        // Write barrier before mutating the future object.
        self.write_barrier(future_ptr, value);

        let Object::Future(future) = self.get_object_mut(future_ptr) else {
            return Err(VmInternalError::TypeError {
                expected: FutureType::Any.into(),
                got: ObjectType::of(self.get_object(future_ptr)).into(),
            });
        };

        *future = Future::Ready(value);
        Ok(())
    }

    /// Fulfill a future and replace the stack top if the VM is awaiting it.
    ///
    /// Use this for async operations that complete while the VM is blocked at
    /// an `Await` instruction. This replaces the future on the stack with the
    /// ready value so execution can continue.
    pub fn fulfil_future(
        &mut self,
        future_ptr: HeapPtr,
        value: Value,
    ) -> Result<(), VmInternalError> {
        self.set_future_ready(future_ptr, value)?;

        // At any given moment, the VM can only await a single future, because
        // we can only call the AWAIT instruction on a future on top of the
        // stack. If that future being await is fulfilled, we need to replace
        // the future on the stack with the ready value so that the next
        // instruction that the VM runs can use the value, not the future
        // object.
        if let Some(Value::Object(ptr)) = self.stack.last() {
            if *ptr == future_ptr {
                self.stack.pop();
                self.stack.push(value);
            }
        }

        Ok(())
    }

    /// Allocates an array on the heap and returns it to the caller.
    pub fn alloc_array(&mut self, values: Vec<Value>) -> Value {
        Value::Object(self.tlab.alloc(Object::Array(values)))
    }

    pub fn alloc_map(&mut self, values: IndexMap<String, Value>) -> Value {
        Value::Object(self.tlab.alloc(Object::Map(values)))
    }

    pub fn alloc_string(&mut self, s: String) -> Value {
        Value::Object(self.tlab.alloc(Object::String(s)))
    }

    pub fn alloc_uint8array(&mut self, data: Vec<u8>) -> Value {
        Value::Object(self.tlab.alloc(Object::Uint8Array(data)))
    }

    /// TODO: Seems to low level for an embedder, provide an API that takes
    /// class name and mapping of field name => value instead.
    pub fn alloc_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::Object(
            self.tlab
                .alloc(Object::Instance(Instance { class, fields })),
        )
    }

    // TODO: Same problem as above. Ideally takes (&str, &str) instead.
    pub fn alloc_variant(&mut self, enm: HeapPtr, index: usize) -> Value {
        Value::Object(self.tlab.alloc(Object::Variant(Variant { enm, index })))
    }

    /// Allocate a future object.
    pub fn alloc_future(&mut self, future: Future) -> Value {
        Value::Object(self.tlab.alloc(Object::Future(future)))
    }

    /// Allocate a collector object on the heap.
    pub fn alloc_collector(&mut self, collector: bex_vm_types::CollectorRef) -> Value {
        Value::Object(self.tlab.alloc(Object::Collector(collector)))
    }

    /// Get collector ref from a Value.
    pub fn as_collector(
        &self,
        value: &Value,
    ) -> Result<&bex_vm_types::CollectorRef, VmInternalError> {
        let index = self.as_object_ptr(value, ObjectType::Collector)?;
        let obj = self.get_object(index);
        match obj {
            Object::Collector(c) => Ok(c),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Collector.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Allocate opaque Rust data on the heap, returning a `Value::Object(HeapPtr)`.
    ///
    /// Used by generated `copy::` structs for `$rust_type` fields.
    pub fn alloc_rust_data(&mut self, data: Arc<dyn std::any::Any + Send + Sync>) -> Value {
        Value::Object(self.tlab.alloc(Object::RustData(data)))
    }

    /// Downcast a `Value::Object` pointing to `Object::RustData` to `&T`.
    ///
    /// Used by generated `view::` struct accessors for `$rust_type` fields.
    pub fn as_rust_data<T: 'static>(&self, value: &Value) -> Result<&T, VmInternalError> {
        let ptr = match value {
            Value::Object(ptr) => *ptr,
            other => {
                return Err(VmInternalError::TypeError {
                    expected: Type::Object(ObjectType::RustData),
                    got: self.type_of(other),
                });
            }
        };
        let obj = self.get_object(ptr);
        match obj {
            Object::RustData(arc) => {
                arc.downcast_ref::<T>()
                    .ok_or_else(|| VmInternalError::RustTypeError {
                        expected: TypeId::of::<T>(),
                        got: arc.as_ref().type_id(),
                    })
            }
            _ => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::RustData),
                got: self.type_of(value),
            }),
        }
    }

    /// Extract an `&Instance` from a `Value::Object`.
    ///
    /// Used by generated glue code to construct `view::` structs.
    pub fn as_instance(&self, value: &Value) -> Result<&Instance, VmInternalError> {
        let ptr = match value {
            Value::Object(ptr) => *ptr,
            other => {
                return Err(VmInternalError::TypeError {
                    expected: Type::Object(ObjectType::Instance),
                    got: self.type_of(other),
                });
            }
        };
        let obj = self.get_object(ptr);
        match obj {
            Object::Instance(instance) => Ok(instance),
            _ => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::Instance),
                got: self.type_of(value),
            }),
        }
    }

    /// Look up a class by fully-qualified name and return its `HeapPtr`.
    ///
    /// Used by generated `copy::` struct `to_value()` methods.
    /// Panics if the class is not found (programming error — all builtin classes must exist).
    pub fn resolve_class(&self, name: &str) -> HeapPtr {
        *self
            .resolved_class_names
            .get(name)
            .unwrap_or_else(|| panic!("resolve_class: class {name:?} not found"))
    }

    /// Allocate a type descriptor object on the heap.
    pub fn alloc_type(&mut self, ty: baml_type::Ty) -> Value {
        Value::Object(self.tlab.alloc(Object::Type(Box::new(ty))))
    }

    /// Stops the execution of the current bytecode in favor of the given
    /// function
    ///
    /// When the new control flow ends (given functions pops from the stack)
    /// then the previosly running bytecode resumes execution.
    fn interrupt(&mut self, function_ptr: HeapPtr, args: &[Value]) -> Result<VmExecState, VmError> {
        let obj = self.get_object(function_ptr);
        if !matches!(obj, Object::Function(_)) {
            return Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::Function(FunctionType::Any)),
                got: Type::Object(ObjectType::of(obj)),
            }
            .into());
        }

        // Index of the frame that starts the interrupt code.
        self.interrupt_frame = Some(self.frames.len());

        let locals_offset = self.stack.len();

        // Params.
        self.stack.extend(args.iter().copied());

        // Push the new frame.
        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: function_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(locals_offset),
            faulting_pc: 0,
        }));
        self.allocate_real_locals_for_frame(function_ptr)?;

        // Execute the interrupt code and return the result.
        self.exec()
    }

    fn allocate_real_locals_for_frame(
        &mut self,
        function_ptr: HeapPtr,
    ) -> Result<(), VmInternalError> {
        let obj = self.get_object(function_ptr);
        let real_local_count = match obj {
            Object::Function(function) => function.real_local_count,
            Object::Closure(closure) => {
                // SAFETY: closure.function points to a Function object with
                // appropriate lifetime guarantees.
                let func_obj = unsafe { closure.function.get() };
                match func_obj {
                    Object::Function(f) => f.real_local_count,
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Object(ObjectType::Function(FunctionType::Any)),
                            got: Type::Object(ObjectType::of(func_obj)),
                        });
                    }
                }
            }
            Object::BoundMethod(bm) => {
                // SAFETY: bm.function points to a Function object with appropriate
                // lifetime guarantees.
                let func_obj = unsafe { bm.function.get() };
                match func_obj {
                    Object::Function(f) => f.real_local_count,
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Object(ObjectType::Function(FunctionType::Any)),
                            got: Type::Object(ObjectType::of(func_obj)),
                        });
                    }
                }
            }
            _ => {
                return Err(VmInternalError::TypeError {
                    expected: Type::Object(ObjectType::Any),
                    got: Type::Object(ObjectType::of(obj)),
                });
            }
        };

        let new_len = self.stack.len() + real_local_count;
        self.stack.resize(new_len, Value::Null);
        Ok(())
    }

    #[inline]
    fn local_slot_stack_index(locals_offset: StackIndex, slot: usize) -> StackIndex {
        assert!(
            slot > 0,
            "local slot 0 is reserved and should never be materialized on stack"
        );
        StackIndex::from_raw(locals_offset.raw() + slot - 1)
    }

    pub fn error_to_exception_value(&mut self, error: VmBamlError) -> Value {
        let (class, fields) = match error {
            VmBamlError::InvalidArgument { message } => (
                ErrorClass::InvalidArgument,
                vec![self.alloc_string(message)],
            ),
            VmBamlError::Io { message } => (ErrorClass::Io, vec![self.alloc_string(message)]),
            VmBamlError::Timeout {
                message,
                duration_ms,
            } => (
                ErrorClass::Timeout,
                vec![
                    self.alloc_string(message),
                    duration_ms.map_or(Value::Null, Value::Int),
                ],
            ),
            VmBamlError::Unsupported { message } => {
                (ErrorClass::Unsupported, vec![self.alloc_string(message)])
            }
            VmBamlError::AccessError { message } => {
                (ErrorClass::AccessError, vec![self.alloc_string(message)])
            }
            VmBamlError::RenderPrompt { message } => {
                (ErrorClass::RenderPrompt, vec![self.alloc_string(message)])
            }
            VmBamlError::NotImplemented { message } => {
                (ErrorClass::NotImplemented, vec![self.alloc_string(message)])
            }
            VmBamlError::LlmClient { message } => {
                (ErrorClass::LlmClient, vec![self.alloc_string(message)])
            }
            VmBamlError::DevOther { message } => {
                (ErrorClass::DevOther, vec![self.alloc_string(message)])
            }
            VmBamlError::HostPanic { message } => {
                (ErrorClass::HostPanic, vec![self.alloc_string(message)])
            }
        };
        self.alloc_error_value(class, fields)
    }

    pub(crate) fn alloc_error_value(&mut self, class: ErrorClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.error_class_ptrs[class as usize];
        let instance_ptr = self.tlab.alloc(Object::Instance(Instance {
            class: class_ptr,
            fields,
        }));
        Value::Object(instance_ptr)
    }

    /// Construct a `baml.errors.StackTrace` instance from captured error locations.
    ///
    /// Allocates one `baml.errors.StackFrame` per frame, an array to hold them,
    /// and the outer `StackTrace` wrapper. Only called when a catch handler binds
    /// a `stack_trace` parameter.
    pub(crate) fn alloc_stack_trace(&mut self, trace: &[StackFrame]) -> Value {
        // Build StackFrame instances (fields: file, line, function_name)
        let frames: Vec<Value> = trace
            .iter()
            .map(|loc| {
                let file = self.alloc_string(loc.file_path.clone());
                #[allow(clippy::cast_possible_wrap)]
                let line = Value::Int(loc.error_line as i64);
                let function_name = self.alloc_string(loc.function_name.clone());
                self.alloc_error_value(ErrorClass::StackFrame, vec![file, line, function_name])
            })
            .collect();

        let frames_array = Value::Object(self.tlab.alloc(Object::Array(frames)));
        self.alloc_error_value(ErrorClass::StackTrace, vec![frames_array])
    }

    pub(crate) fn panic_to_exception_value(&mut self, panic: VmPanic) -> Value {
        let (class, fields) = match panic {
            VmPanic::DivisionByZero { left, .. } => (PanicClass::DivisionByZero, vec![left]),
            VmPanic::IndexOutOfBounds { index, length } =>
            {
                #[allow(clippy::cast_possible_wrap)]
                (
                    PanicClass::IndexOutOfBounds,
                    vec![Value::Int(index), Value::Int(length as i64)],
                )
            }
            VmPanic::MapKeyNotFound => {
                let key = self.alloc_string("(unknown)".to_string());
                (PanicClass::MapKeyNotFound, vec![key])
            }
            VmPanic::StackOverflow => {
                let msg = self.alloc_string("stack overflow".to_string());
                (PanicClass::StackOverflow, vec![msg])
            }
            VmPanic::AssertionFailed => {
                let msg = self.alloc_string("assertion failed".to_string());
                (PanicClass::AssertionFailed, vec![msg])
            }
            VmPanic::Unreachable => {
                let msg = self.alloc_string("unreachable code executed".to_string());
                (PanicClass::Unreachable, vec![msg])
            }
            VmPanic::UserPanic { message } => {
                let msg = self.alloc_string(message);
                (PanicClass::UserPanic, vec![msg])
            }
            VmPanic::AllocFailure { message } => {
                let msg = self.alloc_string(message);
                (PanicClass::AllocFailure, vec![msg])
            }
        };
        self.alloc_panic_value(class, fields)
    }

    /// Allocate a `baml.panics.*` class instance using pre-resolved pointers.
    pub fn alloc_panic_value(&mut self, class: PanicClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.panic_class_ptrs[class as usize];
        let instance_ptr = self.tlab.alloc(Object::Instance(Instance {
            class: class_ptr,
            fields,
        }));
        Value::Object(instance_ptr)
    }

    /// Unwinds error values (both thrown and panics).
    fn capture_stack_trace(&self) -> Vec<StackFrame> {
        self.frames
            .iter()
            .filter_map(|frame| {
                let func = self.get_object(frame.function()).as_callable().ok()?;
                match frame {
                    Frame::Bytecode(frame) => {
                        let error_line = if let Some(compact) = &func.bytecode.compact {
                            compact.source_line_for_pc(frame.faulting_pc)
                        } else {
                            func.bytecode.source_line_for_pc(frame.faulting_pc)
                        };
                        Some(StackFrame {
                            function_name: func.name.clone(),
                            file_path: func.source_file.clone(),
                            function_span: func.span,
                            error_line,
                        })
                    }
                    Frame::Native(_) => Some(StackFrame {
                        function_name: func.name.clone(),
                        file_path: func.source_file.clone(),
                        function_span: func.span,
                        error_line: 0,
                    }),
                }
            })
            .collect()
    }

    fn try_unwind_exception(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        exception_value: Value,
    ) -> Result<(), VmError> {
        // Capture the stack trace before unwinding destroys frame information.
        let trace: Vec<StackFrame> = self.capture_stack_trace();

        // Walk the call stack from the current frame outward looking for an
        // exception table entry that covers the faulting PC.
        loop {
            debug_assert!(
                !self.frames.is_empty(),
                "try_unwind_exception called with no frames"
            );
            let depth = self.frames.len() - 1;
            let frame = &self.frames[depth];

            // Native continuation frames have no exception handlers and own
            // no eval stack region — just pop and continue unwinding.
            if matches!(frame, Frame::Native(_)) {
                if self.frames.len() <= 1 {
                    return Err(VmError::Thrown(exception_value));
                }
                self.frames.pop();
                // Clean up tracing / interrupt bookkeeping
                while self
                    .traced_frames
                    .last()
                    .is_some_and(|d| *d >= self.frames.len())
                {
                    self.traced_frames.pop();
                }
                if let Some(interrupt_depth) = self.interrupt_frame
                    && interrupt_depth >= self.frames.len()
                {
                    self.interrupt_frame = None;
                }
                continue; // try next outer frame
            }

            // From here, frame is guaranteed Bytecode.
            let Frame::Bytecode(frame) = frame else {
                unreachable!("non-Native frames already handled above");
            };

            // faulting_pc is kept up-to-date by both exec_inner (legacy) and
            // exec_compact before dispatching each instruction.
            let faulting_pc = frame.faulting_pc;

            // Load the function for this frame to access its exception table.
            // SAFETY: See `load_function` doc comment.
            let frame_function = unsafe { self.load_function(depth)? };

            // Find the first exception table entry covering this PC.
            // Use compact exception table when available (byte-offset PCs),
            // otherwise fall back to the legacy instruction-index table.
            let handler_entry = if let Some(compact) = &frame_function.bytecode.compact {
                compact
                    .exception_handlers_for_pc(faulting_pc)
                    .next()
                    .cloned()
            } else {
                frame_function
                    .bytecode
                    .exception_handlers_for_pc(faulting_pc)
                    .next()
                    .cloned()
            };
            if let Some(entry) = handler_entry {
                // Found a handler in this frame. Truncate the eval stack back
                // to just after the frame's locals region (removes stale
                // temporaries from interrupted expressions).
                let locals_offset = frame.locals_offset;
                let locals_end =
                    locals_offset.raw() + frame_function.arity + frame_function.real_local_count;
                self.stack.truncate(locals_end);

                // Store the exception value in the designated error slot.
                let error_stack_slot =
                    Self::local_slot_stack_index(locals_offset, entry.error_slot);
                self.stack[error_stack_slot] = exception_value;

                // Store stack trace in stack_trace_slot if the catch clause binds it.
                if entry.has_stack_trace_slot() {
                    let st_value = self.alloc_stack_trace(&trace);
                    let st_stack_slot =
                        Self::local_slot_stack_index(locals_offset, entry.stack_trace_slot);
                    self.stack[st_stack_slot] = st_value;
                }

                // Jump to the handler.
                let Frame::Bytecode(bf) = &mut self.frames[depth] else {
                    unreachable!("frame at depth is Bytecode");
                };
                bf.instruction_ptr = entry.handler_pc;

                // Update caller's frame_idx / function references.
                *frame_idx = depth;
                *function = frame_function;
                return Ok(());
            }

            // No handler in this frame -- pop it and try the caller.
            if self.frames.len() <= 1 {
                // No more frames to unwind through.
                return Err(VmError::ThrownUnhandled {
                    value: exception_value,
                    trace,
                });
            }

            let popped = self.frames.pop().expect("frame stack is not empty");
            match popped {
                Frame::Bytecode(bf) => {
                    self.stack.drain(bf.locals_offset..);
                }
                Frame::Native(_) => {} // native frames own no stack region
            }

            // Clean up tracing / interrupt bookkeeping for popped frames.
            while self
                .traced_frames
                .last()
                .is_some_and(|d| *d >= self.frames.len())
            {
                self.traced_frames.pop();
            }

            if let Some(interrupt_depth) = self.interrupt_frame
                && interrupt_depth >= self.frames.len()
            {
                self.interrupt_frame = None;
            }
        }
    }

    fn resolve_callable_target(
        &self,
        callee_value: Value,
    ) -> Result<(HeapPtr, usize), VmInternalError> {
        let expected_type = FunctionType::Callable;
        let callee_ptr = self.as_object_ptr(&callee_value, expected_type.into())?;
        let obj = self.get_object(callee_ptr);
        match obj {
            Object::Function(callee_fn) => Ok((callee_ptr, callee_fn.arity)),
            Object::Closure(closure) => {
                // SAFETY: closure.function points to a Function object with
                // appropriate lifetime guarantees.
                let func_obj = unsafe { closure.function.get() };
                match func_obj {
                    Object::Function(callee_fn) => Ok((callee_ptr, callee_fn.arity)),
                    _ => Err(VmInternalError::TypeError {
                        expected: expected_type.into(),
                        got: ObjectType::of(func_obj).into(),
                    }),
                }
            }
            Object::BoundMethod(bm) => {
                // BoundMethod: the arity reported here is the full arity (including
                // self). CallIndirect has a dedicated path for BoundMethod that
                // inserts the receiver and passes full_arity; this arm handles any
                // edge-case callers that go through resolve_callable_target.
                let func_obj = unsafe { bm.function.get() };
                match func_obj {
                    Object::Function(callee_fn) => Ok((callee_ptr, callee_fn.arity)),
                    _ => Err(VmInternalError::TypeError {
                        expected: expected_type.into(),
                        got: ObjectType::of(func_obj).into(),
                    }),
                }
            }
            _ => Err(VmInternalError::TypeError {
                expected: expected_type.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Prepare a `YieldToCall`-style invocation: if `callee` is a
    /// `BoundMethod`, insert the receiver at the front of `args` and return
    /// the inner `Function`'s `HeapPtr`; otherwise return `callee` unchanged.
    ///
    /// This ensures that native builtins (e.g. `Array.map`) that call user
    /// callbacks via `YieldToCall { callee, args }` work correctly with bound
    /// methods — the receiver is transparently prepended so the arity check in
    /// `execute_call_from_locals_offset` sees the full argument count.
    fn resolve_bound_method_callee(&self, callee: HeapPtr, args: &mut Vec<Value>) -> HeapPtr {
        let obj = self.get_object(callee);
        if let Object::BoundMethod(bm) = obj {
            let receiver = bm.receiver;
            let fn_ptr = bm.function;
            // Prepend receiver so the inner function sees [self, arg1, ..., argN].
            args.insert(0, receiver);
            fn_ptr
        } else {
            callee
        }
    }

    fn execute_call_from_locals_offset(
        &mut self,
        callee_ptr: HeapPtr,
        locals_offset: StackIndex,
        arg_count: usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        // Resolve the callee: either a plain Function, a Closure, or a BoundMethod wrapping one.
        let callee = match self.get_object(callee_ptr) {
            Object::Function(f) => f,
            Object::Closure(c) => {
                // SAFETY: closure.function is a compile-time or TLAB-allocated
                // Function object whose lifetime is at least as long as the closure.
                let func_obj: &'static Object = unsafe { c.function.get() };
                match func_obj {
                    Object::Function(f) => f,
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: FunctionType::Callable.into(),
                            got: ObjectType::of(func_obj).into(),
                        }
                        .into());
                    }
                }
            }
            Object::BoundMethod(bm) => {
                // SAFETY: bm.function points to a Function object allocated in the
                // compile-time object pool or TLAB, with lifetime at least as long
                // as the BoundMethod.
                let func_obj: &'static Object = unsafe { bm.function.get() };
                match func_obj {
                    Object::Function(f) => f,
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: FunctionType::Callable.into(),
                            got: ObjectType::of(func_obj).into(),
                        }
                        .into());
                    }
                }
            }
            other => {
                return Err(VmInternalError::TypeError {
                    expected: FunctionType::Callable.into(),
                    got: ObjectType::of(other).into(),
                }
                .into());
            }
        };

        // Compiler should have already checked this so we could
        // skip it but it's an easy and fast check.
        if arg_count != callee.arity {
            return Err(VmInternalError::InvalidArgumentCount {
                expected: callee.arity,
                got: arg_count,
            }
            .into());
        }

        // Check if we've reached the max call stack size.
        if self.frames.len() >= MAX_FRAMES {
            return Err(VmError::Thrown(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            ));
        }

        let is_traced = callee.trace;

        match callee.kind {
            FunctionKind::Native(func_ptr) => {
                // Cast the type-erased pointer back to NativeFunction.
                //
                // SAFETY: The pointer was created by casting a NativeFunction to *const ()
                // in attach_builtins, so it's safe to cast it back. We use transmute
                // because Rust doesn't allow `as` casts from *const () to fn pointers.
                // The explicit type parameters document exactly what we're doing.
                let func = unsafe { std::mem::transmute::<*const (), NativeFunction>(func_ptr) };

                // Native functions should manage their own gc roots (or never yield).
                // They have no data on the stack.
                let args: Vec<Value> = self.stack.drain(locals_offset..).collect();

                // Run Rust native function, converting NativeCallResult → VmError.
                match func(self, &args) {
                    NativeCallResult::Done(v) => {
                        self.stack.push(v);
                    }
                    NativeCallResult::Error(e) => {
                        return Err(self.native_error_to_vm_error(e));
                    }
                    NativeCallResult::YieldToCall {
                        callee,
                        args: mut callback_args,
                        continuation,
                    } => {
                        // Push a Native continuation frame, then dispatch the
                        // callback through ECFLO. The exec loop's continuation
                        // handler (at the top of the loop) will invoke the
                        // continuation when the callback completes.
                        self.frames.push(Frame::Native(NativeFrame {
                            function: callee_ptr,
                            continuation,
                        }));

                        // If callee is a BoundMethod, insert receiver into args.
                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args);

                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        self.stack.extend(callback_args);

                        return self.execute_call_from_locals_offset(
                            real_callee,
                            cb_locals,
                            arg_count,
                            frame_idx,
                            function,
                        );
                    }
                }
            }

            FunctionKind::Bytecode => {
                // For traced functions, snapshot args before pushing the frame.
                let trace_data = if is_traced {
                    let args: Vec<Value> = self.stack[locals_offset..].to_owned();
                    let callee_name = callee.name.clone();
                    Some((callee_name, args))
                } else {
                    None
                };

                // Push the new frame.
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function: callee_ptr,
                    instruction_ptr: 0,
                    locals_offset,
                    faulting_pc: 0,
                }));
                self.allocate_real_locals_for_frame(callee_ptr)?;

                // Update frame_idx to point to the new frame.
                *frame_idx = self.frames.len() - 1;

                // If traced, record the frame and yield a span notification.
                if let Some((callee_name, args)) = trace_data {
                    self.traced_frames.push(*frame_idx);

                    return Ok(Some(VmExecState::SpanNotify(
                        SpanNotification::FunctionEnter {
                            function_name: callee_name,
                            frame_depth: *frame_idx,
                            args,
                        },
                    )));
                }

                // SAFETY: See `load_function` doc comment.
                *function = unsafe { self.load_function(*frame_idx)? };
            }

            FunctionKind::SysOp(_) => {
                log::error!(
                    "[VM] tried to CALL SysOp function '{}' via bytecode — SysOps must go through the engine yield path",
                    callee.name
                );
                return Err(VmInternalError::TypeError {
                    expected: FunctionType::Callable.into(),
                    got: FunctionType::from(&callee.kind).into(),
                }
                .into());
            }

            FunctionKind::NativeUnresolved => {
                // This should never happen - native functions should be resolved
                // by attach_builtins() before the VM runs.
                panic!(
                    "Unresolved native function '{}' - did you forget to call attach_builtins()?",
                    callee.name
                );
            }
        }

        if self.early_yield.should_early_yield() {
            return Ok(Some(VmExecState::EarlyYield));
        }
        Ok(None)
    }

    /// Convert a [`VmRustFnError`] into the corresponding [`VmError`].
    fn native_error_to_vm_error(&mut self, err: VmRustFnError) -> VmError {
        match err {
            VmRustFnError::Panic(panic) => VmError::Thrown(self.panic_to_exception_value(panic)),
            VmRustFnError::BamlError(err) => VmError::Thrown(self.error_to_exception_value(err)),
            VmRustFnError::InternalError(err) => VmError::InternalError(err),
        }
    }

    // Runs filters and returns remaining notifications for the watched node.
    fn process_notifications(
        &mut self,
        watched_node: NodeId,
    ) -> Result<Vec<watch::NodeId>, VmError> {
        // Make a copy of all the roots that reach the watched node.
        let mut notifications = self.watch.copy_roots_reaching(watched_node);

        // Some notifications might be removed with filters,  we'll return this.
        let mut filtered_notifications = vec![];

        // Sort them by variables first. At the moment this is not really used
        // because we only have variables, at this point it's unlikely we will
        // implement notifications on objects (references), so we might be able
        // to get rid of this.
        notifications.sort_by(|a, b| match (a, b) {
            (NodeId::LocalVar(a), NodeId::LocalVar(b)) => a.cmp(b),
            (NodeId::LocalVar(_), NodeId::HeapObject(_)) => std::cmp::Ordering::Less,
            (NodeId::HeapObject(_), NodeId::LocalVar(_)) => std::cmp::Ordering::Greater,
            (NodeId::HeapObject(a), NodeId::HeapObject(b)) => a.cmp(b),
        });

        for notification in notifications {
            // The call to copy_roots_reaching() should always return valid
            // roots, so this should really be unreachable.
            let Some(state) = self.watch.root_state(notification) else {
                continue;
            };

            match state.filter {
                // Manual notify means skip this notification. If paused also skip
                WatchFilter::Manual | WatchFilter::Paused => continue,

                // Default filter is a basic diff. If the value has actually
                // changed, then notify.
                WatchFilter::Default => {
                    let Some(last_assigned) = state.last_assigned else {
                        filtered_notifications.push(notification);
                        continue;
                    };

                    if !crate::package_baml::PackageBamlImpl::deep_equals(
                        self,
                        &last_assigned,
                        &state.value,
                    ) {
                        filtered_notifications.push(notification);
                    }
                }

                // Run user function to decide if we should notify.
                WatchFilter::Function(filter_func) => {
                    match self.interrupt(filter_func, &[state.value]) {
                        Ok(VmExecState::Complete(Value::Bool(notify))) => {
                            if notify {
                                filtered_notifications.push(notification);
                            }
                        }
                        Ok(VmExecState::Complete(other)) => {
                            return Err(VmInternalError::TypeError {
                                expected: Type::Bool,
                                got: self.type_of(&other),
                            }
                            .into());
                        }
                        Ok(_) => {
                            return Err(VmInternalError::ExpectedCompletion.into());
                        }
                        Err(err) => return Err(err),
                    }
                }
            }
        }

        Ok(filtered_notifications)
    }

    /// When a watched node changes, we need to update the graph topology
    /// and copy the previous values of the affected roots.
    fn update_watched_node(
        &mut self,
        watched_node: NodeId,
        path: watch::Path,
        old_value: Value,
        new_value: Value,
    ) {
        if let Value::Object(old) = old_value {
            self.watch
                .unlink_edge(watched_node, path.clone(), NodeId::HeapObject(old));
        }

        if let Value::Object(new) = new_value {
            watch::track_watch_dependencies(&mut self.watch, watched_node, path, new);
        }

        // Deep-copy previous root values so the notification filter can diff
        // old vs new. Two-pass because `baml_deep_copy` needs `&mut self`,
        // which conflicts with borrowing `self.watch` for root_state.
        let roots = self.watch.copy_roots_reaching(watched_node);
        let mut old_roots_copies = Vec::with_capacity(roots.len());

        for &root in &roots {
            if let Some(val) = self.watch.root_state(root).map(|s| s.value) {
                let deep_copy = crate::package_baml::PackageBamlImpl::deep_copy(self, &val);
                old_roots_copies.push(deep_copy);
            }
        }

        for (&root, old_value) in roots.iter().zip(old_roots_copies) {
            if let Some(state) = self.watch.root_state_mut(root) {
                state.last_assigned = Some(old_value);
            }
        }
    }

    /// Load the function object for the given frame.
    ///
    /// # Safety
    ///
    /// Function objects are compiled ahead of time and live in the compile-time
    /// heap, which is never garbage collected. The returned `'static` reference
    /// is valid for the lifetime of the program.
    ///
    /// TODO: When we add lambdas that capture variables, their function objects
    /// will be allocated at runtime on the TLAB and *can* be garbage collected.
    /// At that point `'static` becomes unsound. Options:
    ///   1. Pin closures in a non-GC'd region so they remain `'static`.
    ///   2. Drop `'static` and re-deref after each GC safepoint (cheap re-deref,
    ///      still no clone).
    #[inline]
    unsafe fn load_function(&self, frame_idx: usize) -> Result<&'static Function, VmInternalError> {
        let ptr = self.frames[frame_idx].function();
        // SAFETY: See doc comment above.
        let obj: &'static Object = unsafe { ptr.get() };
        match obj {
            Object::Function(f) => Ok(f),
            Object::Closure(closure) => {
                // SAFETY: See doc comment — same lifetime guarantee applies to the
                // inner function referenced by the closure.
                let func_obj: &'static Object = unsafe { closure.function.get() };
                func_obj.as_function()
            }
            Object::BoundMethod(bm) => {
                // SAFETY: See doc comment — same lifetime guarantee applies to the
                // inner function referenced by the bound method.
                let func_obj: &'static Object = unsafe { bm.function.get() };
                func_obj.as_function()
            }
            _ => Err(VmInternalError::TypeError {
                expected: FunctionType::Callable.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    /// Wraps `exec_inner` to convert `InternalError` → `TracedInternalError`
    /// with a captured stack trace.
    pub fn exec(&mut self) -> Result<VmExecState, VmError> {
        match self.exec_inner() {
            Err(VmError::InternalError(err)) => {
                let trace = self.capture_stack_trace();
                Err(VmError::TracedInternalError { source: err, trace })
            }
            other => other,
        }
    }

    fn exec_inner(&mut self) -> Result<VmExecState, VmError> {
        if self.frames.is_empty() {
            return Ok(VmExecState::Complete(Value::Null));
        }

        let mut frame_idx = self.frames.len() - 1;
        let mut function = unsafe { self.load_function(frame_idx)? };

        // Delegate to compact dispatch if compact bytecode is available.
        if function.bytecode.compact.is_some() {
            return self.exec_compact();
        }

        loop {
            // ── CPS continuation handler ──────────────────────────────
            //
            // When a bytecode callback returns (Return instruction) and
            // the frame below is Frame::Native, the Return handler just
            // pops the bytecode frame and leaves the result on the stack.
            // We detect the Native frame here and drive the continuation.
            //
            // This also handles re-entry after exec() yielded a
            // FunctionExit span for a traced callback.
            while matches!(self.frames.last(), Some(Frame::Native(_))) {
                let v = self.stack.ensure_pop()?;
                let Some(Frame::Native(nf)) = self.frames.pop() else {
                    unreachable!("just matched Some(Frame::Native(_))");
                };
                let native_fn_ptr = nf.function;

                match nf.continuation.call(self, v) {
                    NativeCallResult::Done(val) => {
                        self.stack.push(val);
                        // Continue the while loop — there may be stacked
                        // Native frames below (e.g. nested continuations).
                    }
                    NativeCallResult::Error(e) => match self.native_error_to_vm_error(e) {
                        VmError::Thrown(exception_value) => {
                            self.try_unwind_exception(
                                &mut frame_idx,
                                &mut function,
                                exception_value,
                            )?;
                            break;
                        }
                        other => return Err(other),
                    },
                    NativeCallResult::YieldToCall {
                        callee,
                        args: mut callback_args,
                        continuation,
                    } => {
                        // The continuation wants to call another function.
                        // Push the new Native frame, then dispatch the
                        // callback through ECFLO.
                        self.frames.push(Frame::Native(NativeFrame {
                            function: native_fn_ptr,
                            continuation,
                        }));

                        // If callee is a BoundMethod, insert receiver into args.
                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args);

                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        self.stack.extend(callback_args);

                        let ecflo_result = match self.execute_call_from_locals_offset(
                            real_callee,
                            cb_locals,
                            arg_count,
                            &mut frame_idx,
                            &mut function,
                        ) {
                            Ok(result) => result,
                            Err(VmError::Thrown(exception_value)) => {
                                self.try_unwind_exception(
                                    &mut frame_idx,
                                    &mut function,
                                    exception_value,
                                )?;
                                break;
                            }
                            Err(other) => return Err(other),
                        };

                        if let Some(state) = ecflo_result {
                            return Ok(state);
                        }
                        // If ECFLO returned None: either a bytecode frame
                        // was pushed (top is Bytecode, while exits) or a
                        // native callback completed inline (top is Native,
                        // while continues).
                    }
                }
            }

            if self.frames.is_empty() {
                return self
                    .stack
                    .ensure_pop()
                    .map_err(VmError::InternalError)
                    .map(VmExecState::Complete);
            }

            // Sync frame state — may have changed due to continuation
            // handling above or the previous step()'s Return.
            frame_idx = self.frames.len() - 1;
            function = unsafe { self.load_function(frame_idx)? };

            // ── Instruction execution ─────────────────────────────────

            let Frame::Bytecode(bf) = &mut self.frames[frame_idx] else {
                unreachable!("exec loop frame is always Bytecode after continuation handler");
            };
            let instruction_ptr = bf.instruction_ptr;
            bf.instruction_ptr += 1;
            bf.faulting_pc = instruction_ptr;

            #[cfg(debug_assertions)]
            #[allow(clippy::print_stderr)] // intentional debug output
            if std::env::var("BEX_VM_DEBUG").is_ok() {
                let stack = self
                    .stack
                    .iter()
                    .map(crate::debug::display_value)
                    .collect::<Vec<_>>()
                    .join(", ");

                let (instruction, metadata) = crate::debug::display_instruction(
                    instruction_ptr,
                    function,
                    &self.globals,
                    None,
                    None,
                );

                eprintln!("[{stack}]");
                eprintln!("{instruction} {metadata}");
            }

            let step_result = self.step(&mut frame_idx, &mut function, instruction_ptr);

            match step_result {
                Ok(Some(state)) => return Ok(state),
                Ok(None) => {}
                Err(VmError::InternalError(err)) => return Err(VmError::InternalError(err)),
                Err(VmError::Thrown(exception_value)) => {
                    self.try_unwind_exception(&mut frame_idx, &mut function, exception_value)?;
                }
                Err(
                    e @ (VmError::ThrownUnhandled { .. } | VmError::TracedInternalError { .. }),
                ) => return Err(e),
            }
        }
    }

    /// Execute a single instruction.
    ///
    /// Returns `Ok(Some(state))` when the VM must yield control flow to the
    /// embedder (await, schedule, complete, notify). Returns `Ok(None)` when
    /// execution should continue to the next instruction.
    ///
    /// All control flow instructions should include
    /// ```rust,ignore
    /// if self.early_yield.should_early_yield() {
    ///     return Ok(Some(VmExecState::EarlyYield));
    /// }
    /// ```
    /// at the end to allow early yielding in the event of a long-running
    /// operation.
    fn step(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        instruction_ptr: usize,
    ) -> Result<Option<VmExecState>, VmError> {
        match function.bytecode.instructions[instruction_ptr] {
            Instruction::NotifyBlock(block_index) => {
                // Get the notification from the function's storage
                let notification = &function.block_notifications[block_index];

                // Create a copy with the function name populated
                let full_notification = bytecode::BlockNotification {
                    function_name: function.name.clone(),
                    block_name: notification.block_name.clone(),
                    level: notification.level,
                    block_type: notification.block_type,
                    is_enter: notification.is_enter,
                };

                return Ok(Some(VmExecState::Notify(WatchNotification::Block(
                    full_notification,
                ))));
            }

            Instruction::VizEnter(index) | Instruction::VizExit(index) => {
                let instruction = &function.bytecode.instructions[instruction_ptr];
                let delta = match instruction {
                    Instruction::VizEnter(_) => bytecode::VizExecDelta::Enter,
                    Instruction::VizExit(_) => bytecode::VizExecDelta::Exit,
                    _ => unreachable!("matched on viz instruction"),
                };

                #[allow(clippy::cast_possible_wrap)]
                let node = function.viz_nodes.get(index).ok_or({
                    VmError::Thrown(self.panic_to_exception_value(VmPanic::IndexOutOfBounds {
                        index: index as i64,
                        length: function.viz_nodes.len(),
                    }))
                })?;

                let event = bytecode::VizExecEvent {
                    delta,
                    node_id: node.node_id,
                    node_type: node.node_type,
                    label: node.label.clone(),
                    header_level: node.header_level,
                };

                return Ok(Some(VmExecState::Notify(WatchNotification::Viz {
                    function_name: function.name.clone(),
                    event,
                })));
            }

            Instruction::LoadConst(index) => {
                // Use pre-resolved constants (resolved at load time)
                let value = function.bytecode.resolved_constants[index];
                self.stack.push(value);
            }

            Instruction::LoadVar(index) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let slot = Self::local_slot_stack_index(bf.locals_offset, index);
                let value = self.stack[slot];
                self.stack.push(value);
            }

            Instruction::StoreVar(index) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                // Absolute index of the local variable.
                let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);

                // New value.
                let value = self.stack.ensure_pop()?;

                // Old value being replaced.
                let old_value = std::mem::replace(&mut self.stack[local_var_index], value);

                // If this local is watched, update the watch graph.
                //
                // A watched local is a root in the watch graph. When
                // reassigned (e.g. `v = new_val`), three things happen:
                //
                // 1. `update_watched_node` handles edge topology: unlinks
                //    the old binding (so mutations to the old object no
                //    longer trigger notifications), links the new one, and
                //    deep-copies the previous root state into
                //    `last_assigned` so the notification filter can diff
                //    old vs new.
                //
                // 2. `state.value` is updated to the new value. This is
                //    specific to `StoreVar` — for field/array/map stores
                //    the root's top-level binding hasn't changed, but here
                //    the root itself is being rebound.
                //
                // 3. `process_notifications` walks all roots reaching this
                //    node (just itself, since it IS a root) and applies
                //    the watch filter to decide whether to notify.
                if self.watched_vars.contains_key(&local_var_index) {
                    let watched_node = NodeId::LocalVar(local_var_index);

                    self.update_watched_node(watched_node, watch::Path::Binding, old_value, value);

                    if let Some(state) = self.watch.root_state_mut(watched_node) {
                        state.value = value;
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }
            }

            Instruction::LoadGlobal(index) => {
                let value = &self.globals[index];
                self.stack.push(*value);
            }

            Instruction::StoreGlobal(index) => {
                // Consume the value. Read impl of Instruction::StoreVar.
                let value = self.stack.ensure_pop()?;

                // No write barrier: globals is a VM-owned root structure, not a heap object.
                self.globals[index] = value;
            }

            Instruction::LoadField(index) => {
                let top = self.stack.ensure_pop()?;

                let reference = self.as_object_ptr(&top, ObjectType::Instance)?;

                // Extract the field value before pushing to stack
                let field_value = {
                    let Object::Instance(instance) = self.get_object(reference) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(self.get_object(reference)).into(),
                        }
                        .into());
                    };
                    instance.fields[index]
                };

                // Push the value on top of the stack.
                self.stack.push(field_value);
            }

            Instruction::StoreField(index) => {
                // Consume the new value to be set from the stack.
                let new_value = self.stack.ensure_pop()?;

                // Consume the instance value from the stack.
                let instance_value = self.stack.ensure_pop()?;
                let instance_index = self.as_object_ptr(&instance_value, ObjectType::Instance)?;

                // Read old value (and typecheck).
                let old_value = match self.get_object(instance_index) {
                    Object::Instance(instance) => instance.fields[index],

                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Change graph topology.
                let watched_node = NodeId::HeapObject(instance_index);

                self.update_watched_node(
                    watched_node,
                    watch::Path::InstanceField(index),
                    old_value,
                    new_value,
                );

                // Write barrier: mark card if instance is in an older generation than the value.
                self.write_barrier(instance_index, new_value);

                // Set the new value.
                if let Object::Instance(instance) = self.get_object_mut(instance_index) {
                    instance.fields[index] = new_value;
                }

                let notifications = self.process_notifications(watched_node)?;

                if !notifications.is_empty() {
                    return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    ))));
                }
            }

            Instruction::InitField(index) => {
                // Consume the new value to be set from the stack.
                let new_value = self.stack.ensure_pop()?;

                // Peek — do NOT pop the instance; it stays on the stack for the next field.
                let instance_slot = self.stack.ensure_slot_from_top(0)?;
                let instance_value = self.stack[instance_slot];
                let instance_index = self.as_object_ptr(&instance_value, ObjectType::Instance)?;

                // Read old value for watch graph update (and typecheck).
                let old_value = match self.get_object(instance_index) {
                    Object::Instance(instance) => instance.fields[index],

                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Change graph topology.
                let watched_node = NodeId::HeapObject(instance_index);

                self.update_watched_node(
                    watched_node,
                    watch::Path::InstanceField(index),
                    old_value,
                    new_value,
                );

                // Write barrier: mark card if instance is in an older generation than the value.
                self.write_barrier(instance_index, new_value);

                // Set the new value.
                if let Object::Instance(instance) = self.get_object_mut(instance_index) {
                    instance.fields[index] = new_value;
                }

                let notifications = self.process_notifications(watched_node)?;

                if !notifications.is_empty() {
                    return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    ))));
                }
            }

            Instruction::Pop(n) => {
                let drain_start = self.stack.len() - n;
                let drain_range = StackIndex::from_raw(drain_start)..;
                self.stack.drain(drain_range);
            }

            Instruction::Copy(offset) => {
                let index = self.stack.ensure_slot_from_top(offset)?;
                let value = self.stack[index];
                self.stack.push(value);
            }

            Instruction::Jump(offset) => {
                // Offset can be negative (backward jumps for loops).
                // NOTE: checked_add_signed has a branch on overflow. If this
                // becomes a bottleneck on hot loops, it can be replaced with
                // wrapping_add_signed — the array bounds check on the next
                // iteration will catch invalid pointers anyway.
                let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                bf.instruction_ptr = instruction_ptr
                    .checked_add_signed(offset)
                    .ok_or(VmInternalError::InvalidJump)?;
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::PopJumpIfFalse(offset) => {
                // Pop the condition from the stack (don't leave it there).
                let condition = self.stack.ensure_pop()?;

                match condition {
                    // Reassign only if the condition is false.
                    Value::Bool(value) => {
                        if !value {
                            let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                                unreachable!("exec loop frame is always Bytecode");
                            };
                            bf.instruction_ptr = instruction_ptr
                                .checked_add_signed(offset)
                                .ok_or(VmInternalError::InvalidJump)?;
                        }
                    }

                    // Type error, we don't have "falsey" values in the language
                    // so we should always check booleans.
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Bool,
                            got: self.type_of(&other),
                        }
                        .into());
                    }
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::JumpIfFalse(offset) => {
                let top = self.stack.ensure_stack_top()?;
                let condition = self.stack[top];

                let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };

                match condition {
                    Value::Bool(value) => {
                        if !value {
                            bf.instruction_ptr = instruction_ptr
                                .checked_add_signed(offset)
                                .ok_or(VmInternalError::InvalidJump)?;
                        }
                    }
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Bool,
                            got: self.type_of(&other),
                        }
                        .into());
                    }
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Throw => {
                let value = self.stack.ensure_pop()?;
                self.try_unwind_exception(frame_idx, function, value)?;
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::BinOp(op) => {
                let right = self.stack.ensure_pop()?;
                let left = self.stack.ensure_pop()?;

                let result = match (left, right) {
                    (Value::Int(left), Value::Int(right)) => Value::Int(match op {
                        BinOp::Div if right == 0 => {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::DivisionByZero {
                                    left: Value::Int(left),
                                    right: Value::Int(right),
                                },
                            )));
                        }

                        BinOp::Add => left + right,
                        BinOp::Sub => left - right,
                        BinOp::Mul => left * right,
                        BinOp::Div => left / right,
                        BinOp::Mod => left % right,

                        BinOp::BitAnd => left & right,
                        BinOp::BitOr => left | right,
                        BinOp::BitXor => left ^ right,
                        BinOp::Shl => left << right,
                        BinOp::Shr => left >> right,
                    }),

                    (Value::Float(left), Value::Float(right)) => {
                        Value::Float(match op {
                            BinOp::Div if right == 0.0 => {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    },
                                )));
                            }

                            BinOp::Add => left + right,
                            BinOp::Sub => left - right,
                            BinOp::Mul => left * right,
                            BinOp::Div => left / right,
                            BinOp::Mod => left % right,

                            // Bitwise ops not applicable to floats.
                            BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr => {
                                return Err(VmInternalError::CannotApplyBinOp {
                                    left: Type::Float,
                                    right: Type::Float,
                                    op,
                                }
                                .into());
                            }
                        })
                    }

                    // Mixed int/float: promote int to float.
                    #[allow(clippy::cast_precision_loss)]
                    (Value::Int(left), Value::Float(right)) => {
                        let left = left as f64;
                        Value::Float(match op {
                            BinOp::Div if right == 0.0 => {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    },
                                )));
                            }

                            BinOp::Add => left + right,
                            BinOp::Sub => left - right,
                            BinOp::Mul => left * right,
                            BinOp::Div => left / right,
                            BinOp::Mod => left % right,

                            BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr => {
                                return Err(VmInternalError::CannotApplyBinOp {
                                    left: Type::Int,
                                    right: Type::Float,
                                    op,
                                }
                                .into());
                            }
                        })
                    }

                    #[allow(clippy::cast_precision_loss)]
                    (Value::Float(left), Value::Int(right)) => {
                        let right = right as f64;
                        Value::Float(match op {
                            BinOp::Div if right == 0.0 => {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    },
                                )));
                            }

                            BinOp::Add => left + right,
                            BinOp::Sub => left - right,
                            BinOp::Mul => left * right,
                            BinOp::Div => left / right,
                            BinOp::Mod => left % right,

                            BinOp::BitAnd
                            | BinOp::BitOr
                            | BinOp::BitXor
                            | BinOp::Shl
                            | BinOp::Shr => {
                                return Err(VmInternalError::CannotApplyBinOp {
                                    left: Type::Float,
                                    right: Type::Int,
                                    op,
                                }
                                .into());
                            }
                        })
                    }

                    (Value::Object(_), Value::Object(_)) if op == BinOp::Add => {
                        let left = self.as_string(&left)?;
                        let right = self.as_string(&right)?;

                        let mut concat = left.clone();
                        concat.push_str(right);

                        self.alloc_string(concat)
                    }

                    _ => {
                        return Err(VmInternalError::CannotApplyBinOp {
                            left: self.type_of(&left),
                            right: self.type_of(&right),
                            op,
                        }
                        .into());
                    }
                };

                self.stack.push(result);
            }

            Instruction::CmpOp(op) => {
                let right = self.stack.ensure_pop()?;
                let left = self.stack.ensure_pop()?;

                let result = match (left, right) {
                    (Value::Int(left), Value::Int(right)) => Value::Bool(match op {
                        CmpOp::Eq => left == right,
                        CmpOp::NotEq => left != right,
                        CmpOp::Lt => left < right,
                        CmpOp::LtEq => left <= right,
                        CmpOp::Gt => left > right,
                        CmpOp::GtEq => left >= right,
                    }),

                    #[allow(clippy::float_cmp)]
                    // intentional exact comparison for equality operators
                    (Value::Float(left), Value::Float(right)) => Value::Bool(match op {
                        CmpOp::Eq => left == right,
                        CmpOp::NotEq => left != right,
                        CmpOp::Lt => left < right,
                        CmpOp::LtEq => left <= right,
                        CmpOp::Gt => left > right,
                        CmpOp::GtEq => left >= right,
                    }),

                    // Mixed int/float comparisons: promote int to float.
                    #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
                    (Value::Int(left), Value::Float(right)) => {
                        let left = left as f64;
                        Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,
                        })
                    }

                    #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
                    (Value::Float(left), Value::Int(right)) => {
                        let right = right as f64;
                        Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,
                        })
                    }

                    (Value::Object(left_index), Value::Object(right_index))
                        if matches!(self.get_object(left_index), Object::String(_))
                            && matches!(self.get_object(right_index), Object::String(_)) =>
                    {
                        let left = self.as_string(&left)?;
                        let right = self.as_string(&right)?;

                        Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,
                        })
                    }

                    // Uint8Array comparison: compare by content
                    (Value::Object(left_index), Value::Object(right_index))
                        if matches!(self.get_object(left_index), Object::Uint8Array(_))
                            && matches!(self.get_object(right_index), Object::Uint8Array(_)) =>
                    {
                        let left = self.as_uint8array(&left)?;
                        let right = self.as_uint8array(&right)?;

                        Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            _ => {
                                return Err(VmInternalError::CannotApplyCmpOp {
                                    left: Type::Object(ObjectType::Uint8Array),
                                    right: Type::Object(ObjectType::Uint8Array),
                                    op,
                                }
                                .into());
                            }
                        })
                    }

                    // Variant comparison: compare by enum type and variant index
                    (Value::Object(left_index), Value::Object(right_index))
                        if matches!(self.get_object(left_index), Object::Variant(_))
                            && matches!(self.get_object(right_index), Object::Variant(_)) =>
                    {
                        let Object::Variant(left_var) = self.get_object(left_index) else {
                            unreachable!()
                        };
                        let Object::Variant(right_var) = self.get_object(right_index) else {
                            unreachable!()
                        };

                        Value::Bool(match op {
                            CmpOp::Eq => {
                                left_var.enm == right_var.enm && left_var.index == right_var.index
                            }
                            CmpOp::NotEq => {
                                left_var.enm != right_var.enm || left_var.index != right_var.index
                            }
                            _ => {
                                return Err(VmInternalError::CannotApplyCmpOp {
                                    left: Type::Object(ObjectType::Variant),
                                    right: Type::Object(ObjectType::Variant),
                                    op,
                                }
                                .into());
                            }
                        })
                    }

                    _ => Value::Bool(match op {
                        CmpOp::Eq => left == right,
                        CmpOp::NotEq => left != right,

                        _ => {
                            return Err(VmInternalError::CannotApplyCmpOp {
                                left: self.type_of(&left),
                                right: self.type_of(&right),
                                op,
                            }
                            .into());
                        }
                    }),
                };

                self.stack.push(result);
            }

            Instruction::UnaryOp(op) => {
                let value = self.stack.ensure_pop()?;

                let result = match (op, value) {
                    (UnaryOp::Not, Value::Bool(value)) => Value::Bool(!value),
                    (UnaryOp::Neg, Value::Int(value)) => Value::Int(-value),
                    (UnaryOp::Neg, Value::Float(value)) => Value::Float(-value),
                    _ => {
                        return Err(VmInternalError::CannotApplyUnaryOp {
                            op,
                            value: self.type_of(&value),
                        }
                        .into());
                    }
                };

                self.stack.push(result);
            }

            Instruction::AllocArray(size) => {
                // Pop all the elements from the stack and create an array.
                let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                let array = self.stack.drain(drain_range).collect();

                // Allocate it on the heap.
                let array_index = self.tlab.alloc(Object::Array(array));

                // Push the array object on top of the stack.
                self.stack.push(Value::Object(array_index));
            }

            Instruction::LoadArrayElement => {
                // Stack should contain [array, index]
                // Pop the index first, then the array
                let index_value = self.stack.ensure_pop()?;
                let array_value = self.stack.ensure_pop()?;

                let array_obj_index = self.as_object_ptr(&array_value, ObjectType::Array)?;

                // Get the array length for bounds checking.
                let array_len = match self.get_object(array_obj_index) {
                    Object::Array(arr) => arr.len(),
                    Object::Uint8Array(bytes) => bytes.len(),
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Get the index
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                // bounds checked below
                let index = match index_value {
                    Value::Int(i) => {
                        if i < 0 || i as usize >= array_len {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: i,
                                    length: array_len,
                                },
                            )));
                        }
                        i as usize
                    }
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Int,
                            got: self.type_of(&index_value),
                        }
                        .into());
                    }
                };

                // Extract the array element before pushing to stack
                #[allow(clippy::cast_possible_wrap)]
                let element = {
                    match self.get_object(array_obj_index) {
                        Object::Array(array) => {
                            if index >= array.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: array.len(),
                                    },
                                )));
                            }
                            array[index]
                        }
                        Object::Uint8Array(bytes) => {
                            if index >= bytes.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: bytes.len(),
                                    },
                                )));
                            }
                            Value::Int(i64::from(bytes[index]))
                        }
                        _ => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(self.get_object(array_obj_index)).into(),
                            }
                            .into());
                        }
                    }
                };

                // Push the element onto the stack
                self.stack.push(element);
            }

            Instruction::LoadMapElement => {
                // LoadMapElement Instruction
                //
                // Stack before: [map, key]
                // Stack after: [value]
                //
                // Interpretation steps:
                // 1. Pop key from stack (top element)
                // 2. Pop map reference from stack (bottom element)
                // 3. Validate that the popped map reference is indeed a map object
                // 4. Get the key as a string from the objects pool (maps use string keys)
                //    - Validate key_value is an object reference to a String
                //    - Get the string reference from the objects pool
                // 5. Look up the value at map[key]
                // 6. Handle the case where key doesn't exist in the map
                //    - Return a runtime error NoSuchKeyInMap if key not found
                // 7. Push the found value onto the stack

                let key_value = self.stack.ensure_pop()?;
                let map_value = self.stack.ensure_pop()?;

                let map_index = self.as_object_ptr(&map_value, ObjectType::Map)?;

                let Object::Map(map) = self.get_object(map_index) else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Map.into(),
                        got: ObjectType::of(self.get_object(map_index)).into(),
                    }
                    .into());
                };

                // Get the string key from the objects pool
                let key_index = self.as_object_ptr(&key_value, ObjectType::String)?;
                let key = self.get_object(key_index).as_string()?;

                // Look up the value in the map
                let value = map.get(key).copied().ok_or(VmError::Thrown(
                    self.panic_to_exception_value(VmPanic::MapKeyNotFound),
                ))?;

                // Push the value onto the stack
                self.stack.push(value);
            }

            Instruction::StoreArrayElement => {
                // Instruction args.
                let new_value = self.stack.ensure_pop()?;
                let index_value = self.stack.ensure_pop()?;
                let array_value = self.stack.ensure_pop()?;
                let array_object_index = self.as_object_ptr(&array_value, ObjectType::Array)?;

                // Get the array length for bounds checking.
                let array_len = match self.get_object(array_object_index) {
                    Object::Array(arr) => arr.len(),
                    Object::Uint8Array(bytes) => bytes.len(),
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Verify index.
                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                // bounds checked below
                let index = match index_value {
                    Value::Int(i) => {
                        if i < 0 || i as usize >= array_len {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: i,
                                    length: array_len,
                                },
                            )));
                        }
                        i as usize
                    }
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Int,
                            got: self.type_of(&other),
                        }
                        .into());
                    }
                };

                // Read old value (and typecheck).
                #[allow(clippy::cast_possible_wrap)]
                let old_value = match self.get_object(array_object_index) {
                    Object::Array(array) => {
                        if index >= array.len() {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: index as i64,
                                    length: array.len(),
                                },
                            )));
                        }
                        array[index]
                    }
                    Object::Uint8Array(bytes) => {
                        if index >= bytes.len() {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: index as i64,
                                    length: bytes.len(),
                                },
                            )));
                        }
                        Value::Int(i64::from(bytes[index]))
                    }
                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Change graph topology
                let watched_node = NodeId::HeapObject(array_object_index);
                self.update_watched_node(
                    watched_node,
                    watch::Path::ArrayIndex(index),
                    old_value,
                    new_value,
                );

                // Write barrier: mark card if array is in an older generation than the value.
                self.write_barrier(array_object_index, new_value);

                // Set the new value.
                match self.get_object_mut(array_object_index) {
                    Object::Array(array) => {
                        array[index] = new_value;
                    }
                    Object::Uint8Array(bytes) => {
                        let Value::Int(i) = new_value else {
                            return Err(VmInternalError::TypeError {
                                expected: Type::Int,
                                got: self.type_of(&new_value),
                            }
                            .into());
                        };
                        // following JS, we truncate the value to 8-bit unsigned integer
                        bytes[index] = (i.cast_unsigned() & 0xFF) as u8;
                    }
                    _ => {
                        unreachable!(
                            "We already checked earlier that we are operating on an array-like type"
                        );
                    }
                }

                let notifications = self.process_notifications(watched_node)?;

                if !notifications.is_empty() {
                    return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    ))));
                }
            }

            Instruction::StoreMapElement => {
                // Instruction args.
                let new_value = self.stack.ensure_pop()?;
                let key_value = self.stack.ensure_pop()?;
                let map_value = self.stack.ensure_pop()?;

                // Get the string key from the objects pool.
                let key_index = self.as_object_ptr(&key_value, ObjectType::String)?;
                let key = self.get_object(key_index).as_string()?.clone();

                let map_index = self.as_object_ptr(&map_value, ObjectType::Map)?;

                // Read old value (and typecheck).
                //
                // If the map didn't contain any value we'll use null so
                // there's not watch graph edge to update.
                let old_value = match self.get_object(map_index) {
                    Object::Map(map) => map.get(&key).copied().unwrap_or(Value::Null),

                    other => {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(other).into(),
                        }
                        .into());
                    }
                };

                // Change graph topology
                let watched_node = NodeId::HeapObject(map_index);

                self.update_watched_node(
                    watched_node,
                    watch::Path::MapKey(key.clone()),
                    old_value,
                    new_value,
                );

                // Write barrier: mark card if map is in an older generation than the value.
                self.write_barrier(map_index, new_value);

                // Set the new value.
                if let Object::Map(map) = self.get_object_mut(map_index) {
                    map.insert(key, new_value);
                }

                let notifications = self.process_notifications(watched_node)?;

                if !notifications.is_empty() {
                    return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    ))));
                }
            }

            Instruction::AllocInstance(index) => {
                // Convert compile-time ObjectIndex to HeapPtr
                let class_ptr = self.idx_to_ptr(index);
                let Object::Class(class) = self.get_object(class_ptr) else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Class.into(),
                        got: ObjectType::of(self.get_object(class_ptr)).into(),
                    }
                    .into());
                };

                // Allocate the fields.
                let mut fields = Vec::with_capacity(class.fields.len());
                fields.resize(class.fields.len(), Value::Null);

                // Allocate an instance of the class.
                let instance_ptr = self.tlab.alloc(Object::Instance(Instance {
                    class: class_ptr,
                    fields,
                }));

                // Push the instance object on top of the stack.
                self.stack.push(Value::Object(instance_ptr));
            }

            // TODO: Contains a lot of typechecking, we know at compile time
            // that all this stuff is right. Should do something about it.
            Instruction::AllocVariant(enum_index) => {
                // Convert compile-time ObjectIndex to HeapPtr
                let enum_ptr = self.idx_to_ptr(enum_index);
                // Extract the variant count before popping from stack to avoid borrow conflicts
                let variant_count = {
                    let Object::Enum(enm) = self.get_object(enum_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Enum.into(),
                            got: ObjectType::of(self.get_object(enum_ptr)).into(),
                        }
                        .into());
                    };
                    enm.variants.len()
                };

                let variant = self.stack.ensure_pop()?;

                let Value::Int(variant_index) = variant else {
                    return Err(VmInternalError::TypeError {
                        expected: Type::Int,
                        got: self.type_of(&variant),
                    }
                    .into());
                };

                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                // Safe: we check variant_index < 0 first, so the cast
                // only executes for non-negative values.
                if variant_index < 0 || variant_index as usize >= variant_count {
                    return Err(VmError::Thrown(self.panic_to_exception_value(
                        VmPanic::IndexOutOfBounds {
                            index: variant_index,
                            length: variant_count,
                        },
                    )));
                }

                #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                // checked non-negative above
                let variant_usize = variant_index as usize;

                let variant_ptr = self.tlab.alloc(Object::Variant(Variant {
                    enm: enum_ptr,
                    index: variant_usize,
                }));

                // Push the variant object on top of the stack.
                self.stack.push(Value::Object(variant_ptr));
            }

            Instruction::DispatchFuture(callee) => {
                let callee_value = self.globals[callee];
                let expected_type = FunctionType::SysOp;
                let callee_ptr = self.as_object_ptr(&callee_value, expected_type.into())?;

                // Can't dispatch if it's not a function
                let Object::Function(callable_future) = self.get_object(callee_ptr) else {
                    return Err(VmInternalError::TypeError {
                        expected: expected_type.into(),
                        got: ObjectType::of(self.get_object(callee_ptr)).into(),
                    }
                    .into());
                };

                // Must be a sys_op - extract the SysOp.
                let FunctionKind::SysOp(sys_op) = callable_future.kind else {
                    return Err(VmInternalError::TypeError {
                        expected: FunctionType::SysOp.into(),
                        got: FunctionType::from(&callable_future.kind).into(),
                    }
                    .into());
                };

                let args_offset = self.stack.len().checked_sub(callable_future.arity).ok_or(
                    VmInternalError::NotEnoughItemsOnStack(callable_future.arity),
                )?;
                let args_offset = StackIndex::from_raw(args_offset);

                // Collect function call args and cleanup consumed stack.
                let future_args: Vec<Value> = self.stack.drain(args_offset..).collect();

                // Create the pending future with the SysOp enum.
                let pending_future = PendingFuture {
                    operation: sys_op,
                    args: future_args,
                };

                // Allocate the future.
                let future_value = self.alloc_future(Future::Pending(pending_future));

                // Extract the index
                let Value::Object(object_index) = future_value else {
                    unreachable!("alloc_future returns Value::Object")
                };

                // Now leave the future on top of the stack.
                self.stack.push(future_value);

                // Yield control flow back to the embedder.
                return Ok(Some(VmExecState::ScheduleFuture(object_index)));
            }

            Instruction::Await => {
                let value = self.stack.ensure_stack_top()?;

                let wanted_type = FutureType::Any;

                let index = self.as_object_ptr(&self.stack[value], wanted_type.into())?;

                // Check if future is ready and extract value if so
                let ready_value = {
                    let Object::Future(awaiting) = self.get_object(index) else {
                        return Err(VmInternalError::TypeError {
                            expected: wanted_type.into(),
                            got: ObjectType::of(self.get_object(index)).into(),
                        }
                        .into());
                    };

                    match awaiting {
                        // Can't do nothing, handle control flow back to embedder.
                        Future::Pending(_) => {
                            return Ok(Some(VmExecState::Await(index)));
                        }

                        // Return the ready value
                        Future::Ready(value) => *value,
                    }
                };

                // Replace the future on the eval stack with the ready value
                self.stack.pop();
                self.stack.push(ready_value);
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Watch(index) => {
                // Stack contains: [channel, filter]

                // Consume filter.
                let filter = match self.stack.ensure_pop()? {
                    Value::Null => WatchFilter::Default,
                    Value::Object(object_index) => match self.get_object(object_index) {
                        Object::Function(_) => WatchFilter::Function(object_index),
                        Object::String(mode) if mode == "manual" => WatchFilter::Manual,
                        Object::String(mode) if mode == "never" => WatchFilter::Paused,
                        _ => {
                            return Err(VmInternalError::InvalidFilter.into());
                        }
                    },
                    _ => {
                        return Err(VmInternalError::InvalidFilter.into());
                    }
                };

                // Consume channel.
                let channel_value = self.stack.ensure_pop()?;
                let channel = self.as_string(&channel_value)?.to_owned();

                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                let value = self.stack[local_var_index];

                // The variable index should be the same as where the value is stored
                let var_node = NodeId::LocalVar(local_var_index);

                // Register this variable as an emittable root.
                self.watch.register_root(
                    var_node,
                    RootState {
                        channel,
                        value,
                        filter,
                        last_notified: None,
                        last_assigned: None,
                    },
                );

                let watched_var_name = &function.local_names[index];
                // Track this so we can unregister on scope exit
                self.watched_vars.insert(
                    local_var_index,
                    (watched_var_name.clone(), function.name.clone()),
                );

                // If it's an object, build the entire dependency graph
                if let Value::Object(object_index) = value {
                    watch::track_watch_dependencies(
                        &mut self.watch,
                        var_node,
                        watch::Path::Binding,
                        object_index,
                    );
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Unwatch(index) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);

                // Remove from watched_vars tracking
                if self.watched_vars.remove(&local_var_index).is_some() {
                    let var_node = NodeId::LocalVar(local_var_index);
                    // Unregister this variable as a root
                    self.watch.unregister_root(var_node);

                    // If it was linked to an object, unlink it
                    let value = self.stack[local_var_index];
                    if let Value::Object(object_index) = value {
                        self.watch.unlink_edge(
                            var_node,
                            watch::Path::Binding,
                            NodeId::HeapObject(object_index),
                        );
                    }
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Notify(index) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                let var_node = NodeId::LocalVar(local_var_index);

                let notifications = self.watch.copy_roots_reaching(var_node);

                if notifications.len() != 1 && notifications.first() != Some(&var_node) {
                    return Err(VmInternalError::InvalidManualNotify.into());
                }

                return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                    notifications,
                ))));
            }

            Instruction::Call(callee) => {
                let callee_value = self.globals[callee];
                let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;
                let args_offset = self
                    .stack
                    .len()
                    .checked_sub(arg_count)
                    .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count))?;
                let locals_offset = StackIndex::from_raw(args_offset);

                return self.execute_call_from_locals_offset(
                    callee_ptr,
                    locals_offset,
                    arg_count,
                    frame_idx,
                    function,
                );
            }

            Instruction::CallIndirect => {
                // Stack layout: [arg1, arg2, ..., argN, callee]
                let callee_slot = self.stack.ensure_stack_top()?;
                let callee_value = self.stack[callee_slot];
                let callee_ptr =
                    self.as_object_ptr(&callee_value, FunctionType::Callable.into())?;
                let obj = self.get_object(callee_ptr);

                if let Object::BoundMethod(bm) = obj {
                    // BoundMethod: insert receiver as `self` before the visible args.
                    //
                    // Stack before: [arg1, ..., argN, bound_method]
                    //   where N = visible_arity = full_arity - 1
                    // Stack after:  [receiver, arg1, ..., argN]
                    //   which the bytecode sees as locals 1..full_arity
                    let func_obj = unsafe { bm.function.get() };
                    let full_arity = match func_obj {
                        Object::Function(f) => f.arity,
                        _ => {
                            return Err(VmInternalError::TypeError {
                                expected: FunctionType::Callable.into(),
                                got: ObjectType::of(func_obj).into(),
                            }
                            .into());
                        }
                    };
                    debug_assert!(
                        full_arity >= 1,
                        "BoundMethod's inner function must have self parameter (arity >= 1), got {full_arity}"
                    );
                    let visible_arity = full_arity.saturating_sub(1);
                    let receiver = bm.receiver;
                    let fn_ptr = bm.function;

                    // Pop the callee (bound_method) off the top.
                    let _popped = self.stack.ensure_pop()?;

                    // Stack: [arg1, ..., argN] where N = visible_arity.
                    let args_offset = self
                        .stack
                        .len()
                        .checked_sub(visible_arity)
                        .ok_or(VmInternalError::NotEnoughItemsOnStack(visible_arity))?;

                    // Insert receiver at args_offset, shifting args right by one slot.
                    self.stack.insert(args_offset, receiver);

                    // Stack: [receiver, arg1, ..., argN] — full_arity items at args_offset.
                    let locals_offset = StackIndex::from_raw(args_offset);

                    if let Some(state) = self.execute_call_from_locals_offset(
                        fn_ptr,
                        locals_offset,
                        full_arity,
                        frame_idx,
                        function,
                    )? {
                        return Ok(Some(state));
                    }
                } else {
                    // Plain Function or Closure: existing path.
                    let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;
                    let args_offset = self
                        .stack
                        .len()
                        .checked_sub(arg_count + 1)
                        .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count + 1))?;
                    let _popped_callee = self.stack.ensure_pop()?;
                    let locals_offset = StackIndex::from_raw(args_offset);

                    if let Some(state) = self.execute_call_from_locals_offset(
                        callee_ptr,
                        locals_offset,
                        arg_count,
                        frame_idx,
                        function,
                    )? {
                        return Ok(Some(state));
                    }
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Return => {
                // Pop the result from the eval stack.
                let result = self.stack.ensure_pop()?;

                // Check if this frame was traced.
                // Capture function name before popping the frame.
                let span_exit = if self.traced_frames.last() == Some(frame_idx) {
                    let func_name = self
                        .get_object(self.frames[*frame_idx].function())
                        .as_callable()
                        .map(|f| f.name.clone())
                        .ok();
                    self.traced_frames.pop();
                    func_name
                } else {
                    None
                };

                // Restore the eval stack to the state before the function
                // was called and leave the result on top.
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                self.stack.drain(bf.locals_offset..);
                self.stack.push(result);

                // Pop from the call stack.
                self.frames.pop();

                // Return from interrupt.
                if Some(self.frames.len()) == self.interrupt_frame {
                    self.interrupt_frame = None;
                    return self
                        .stack
                        .ensure_pop()
                        .map_err(VmError::InternalError)
                        .map(VmExecState::Complete)
                        .map(Some);
                }

                // If there are no more frames, we're done.
                if self.frames.is_empty() {
                    return self
                        .stack
                        .ensure_pop()
                        .map_err(VmError::InternalError)
                        .map(VmExecState::Complete)
                        .map(Some);
                }

                // Yield FunctionExit for traced frames (with result value).
                if let Some(name) = span_exit {
                    return Ok(Some(VmExecState::SpanNotify(
                        SpanNotification::FunctionExit {
                            function_name: name,
                            result,
                        },
                    )));
                }

                // Native continuation frames (from CPS callbacks like
                // array.map) are handled at the top of the exec loop.
                // Just update frame_idx so the loop detects them.
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::AllocMap(n) => {
                let map = if n > 0 {
                    let end_of_values = self.stack.ensure_slot_from_top(2 * n - 1)?;
                    let end_of_keys = self.stack.ensure_slot_from_top(n - 1)?;
                    let idx_of_last_key = self.stack.ensure_slot_from_top(n - 1)?;

                    // We can safely copy the objects that act as values so there's no problem
                    // with not draining them.
                    let values = self.stack[end_of_values..end_of_keys].iter().copied();

                    // We cannot copy key references since we aren't interning yet, so we
                    // must clone the strings.
                    // Here we'll also double-check that the keys are strings. This adds `n`
                    // branches which is not ideal for performance. Might want to consider this
                    // in map accesses.
                    let keys = self.stack[idx_of_last_key..].iter().map(|k| {
                        let obj_index = self.as_object_ptr(k, ObjectType::String)?;

                        self.get_object(obj_index).as_string().cloned()
                    });

                    let pairs = values
                        .zip(keys)
                        .map(|(val, key_res)| key_res.map(|k| (k, val)));

                    let map = pairs.collect::<Result<IndexMap<_, _>, _>>()?;

                    // drain & drop the drain so that vec is empty.
                    self.stack.drain(end_of_values..);

                    map
                } else {
                    // nothing to pop.
                    IndexMap::new()
                };

                let obj_index = self.tlab.alloc(Object::Map(map));

                self.stack.push(Value::Object(obj_index));
            }

            // ============================================================
            // Jump Table Instructions
            // ============================================================
            Instruction::JumpTable { table_idx, default } => {
                // Pop discriminant from stack
                let discriminant = self.stack.ensure_pop()?;

                // Must be an integer
                let Value::Int(value) = discriminant else {
                    return Err(VmInternalError::TypeError {
                        expected: Type::Int,
                        got: self.type_of(&discriminant),
                    }
                    .into());
                };

                // Lookup in jump table
                let table = &function.bytecode.jump_tables[table_idx];
                let offset = table.lookup(value).unwrap_or(default);

                // Jump
                let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                bf.instruction_ptr = instruction_ptr
                    .checked_add_signed(offset)
                    .ok_or(VmInternalError::InvalidJump)?;

                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Discriminant => {
                // Pop value from stack
                let value = self.stack.ensure_pop()?;

                // Must be an object (variants are heap-allocated)
                let Value::Object(object_idx) = value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Variant.into(),
                        got: self.type_of(&value),
                    }
                    .into());
                };

                // Must be a Variant object
                let variant_index = {
                    let Object::Variant(variant) = self.get_object(object_idx) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Variant.into(),
                            got: ObjectType::of(self.get_object(object_idx)).into(),
                        }
                        .into());
                    };
                    variant.index
                };

                // Variant.index is the discriminant we need
                #[allow(clippy::cast_possible_wrap)]
                self.stack.push(Value::Int(variant_index as i64));
            }

            Instruction::TypeTag => {
                let value = self.stack.ensure_pop()?;
                let tag = value_type_tag(&value);
                self.stack.push(Value::Int(tag));
            }

            Instruction::IsType(const_idx) => {
                let value = self.stack.ensure_pop()?;
                let expected = &function.bytecode.resolved_constants[const_idx];
                let result = match expected {
                    Value::Object(class_ptr) => {
                        // Class identity check: is value an instance of this class?
                        match value {
                            Value::Object(val_ptr) => match self.get_object(val_ptr) {
                                Object::Instance(instance) => instance.class == *class_ptr,
                                _ => false,
                            },
                            _ => false,
                        }
                    }
                    Value::Int(tag) => {
                        // Type tag check: does value's type tag match?
                        value_type_tag(&value) == *tag
                    }
                    _ => false,
                };
                self.stack.push(Value::Bool(result));
            }

            #[allow(
                clippy::cast_sign_loss,
                clippy::cast_lossless,
                clippy::cast_possible_truncation
            )]
            Instruction::DenseTag(table_idx) => {
                let tag = match self.stack.ensure_pop()? {
                    Value::Int(t) => t,
                    other => {
                        // TypeTag always pushes Int, so this shouldn't happen.
                        return Err(VmInternalError::TypeError {
                            expected: Type::Int,
                            got: self.type_of(&other),
                        }
                        .into());
                    }
                };
                let table = &function.bytecode.match_hash_tables[table_idx];
                let h =
                    ((tag as u64).wrapping_mul(table.multiply) >> table.shift) & table.mask as u64;
                let entry = &table.entries[h as usize];
                if entry.expected_tag == tag {
                    self.stack.push(Value::Int(i64::from(entry.dense_index)));
                } else {
                    // Tag not in this match — sentinel for jump_table default
                    self.stack.push(Value::Int(-1));
                }
            }

            Instruction::ThrowIfPanic => {
                let value = self.stack.ensure_pop()?;
                let is_panic = match value {
                    Value::Object(ptr) => match self.get_object(ptr) {
                        Object::Instance(instance) => {
                            self.panic_class_ptrs.contains(&instance.class)
                        }
                        _ => false,
                    },
                    _ => false,
                };
                if is_panic {
                    self.try_unwind_exception(frame_idx, function, value)?;
                }
                if self.early_yield.should_early_yield() {
                    return Ok(Some(VmExecState::EarlyYield));
                }
            }

            Instruction::Unreachable => {
                // This instruction should never be executed. If we reach it,
                // there's a bug in the compiler or type system.
                return Err(VmError::Thrown(
                    self.panic_to_exception_value(VmPanic::Unreachable),
                ));
            }

            Instruction::MakeCell => {
                let value = self.stack.ensure_pop()?;
                let cell = Object::Cell(Cell { value });
                let ptr = self.tlab.alloc(cell);
                self.stack.push(Value::Object(ptr));
            }

            Instruction::MakeClosure(obj_idx, capture_count) => {
                let mut captures = Vec::with_capacity(capture_count);
                for _ in 0..capture_count {
                    captures.push(self.stack.ensure_pop()?);
                }
                // Captures were pushed left-to-right, popped right-to-left.
                captures.reverse();
                let function_ptr = self.idx_to_ptr(obj_idx);
                let closure = Object::Closure(Closure {
                    function: function_ptr,
                    captures,
                });
                let ptr = self.tlab.alloc(closure);
                self.stack.push(Value::Object(ptr));
            }

            Instruction::MakeBoundMethod(global_idx) => {
                let receiver = self.stack.ensure_pop()?;
                let callee_value = self.globals[global_idx];
                let function_ptr =
                    self.as_object_ptr(&callee_value, FunctionType::Callable.into())?;
                debug_assert!(
                    matches!(self.get_object(function_ptr), Object::Function(_)),
                    "MakeBoundMethod expects a Function global, got {:?}",
                    ObjectType::of(self.get_object(function_ptr)),
                );
                let bound = Object::BoundMethod(BoundMethod {
                    function: function_ptr,
                    receiver,
                });
                let ptr = self.tlab.alloc(bound);
                self.stack.push(Value::Object(ptr));
            }

            Instruction::LoadDeref(slot) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let cell_value = self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                let Value::Object(cell_ptr) = cell_value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: self.type_of(&cell_value),
                    }
                    .into());
                };
                // SAFETY: cell_ptr is a VM-owned Cell object; single-threaded.
                let obj = unsafe { cell_ptr.get() };
                let Object::Cell(cell) = obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: ObjectType::of(obj).into(),
                    }
                    .into());
                };
                self.stack.push(cell.value);
            }

            Instruction::StoreDeref(slot) => {
                let value = self.stack.ensure_pop()?;
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let cell_value = self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                let Value::Object(cell_ptr) = cell_value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: self.type_of(&cell_value),
                    }
                    .into());
                };
                // Write barrier before the mutable borrow so we hold only &self.heap.
                self.write_barrier(cell_ptr, value);
                // SAFETY: cell_ptr is a VM-owned Cell object; single-threaded.
                let obj = unsafe { cell_ptr.get_mut() };
                let Object::Cell(cell) = obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: ObjectType::of(obj).into(),
                    }
                    .into());
                };
                cell.value = value;
            }

            Instruction::LoadCapture(idx) => {
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let closure_ptr = bf.function;
                // SAFETY: closure_ptr is the frame's function, valid for
                // the duration of this frame.
                let obj = unsafe { closure_ptr.get() };
                let Object::Closure(closure) = obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Closure.into(),
                        got: ObjectType::of(obj).into(),
                    }
                    .into());
                };
                let cell_value = closure.captures[idx];
                let Value::Object(cell_ptr) = cell_value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: self.type_of(&cell_value),
                    }
                    .into());
                };
                // SAFETY: cell_ptr is a VM-owned Cell object; single-threaded.
                let cell_obj = unsafe { cell_ptr.get() };
                let Object::Cell(cell) = cell_obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: ObjectType::of(cell_obj).into(),
                    }
                    .into());
                };
                self.stack.push(cell.value);
            }

            Instruction::StoreCapture(idx) => {
                let value = self.stack.ensure_pop()?;
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let closure_ptr = bf.function;
                // SAFETY: closure_ptr is the frame's function, valid for
                // the duration of this frame.
                let obj = unsafe { closure_ptr.get() };
                let Object::Closure(closure) = obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Closure.into(),
                        got: ObjectType::of(obj).into(),
                    }
                    .into());
                };
                let cell_value = closure.captures[idx];
                // `closure` (and `obj`) are no longer used past this point.
                let Value::Object(cell_ptr) = cell_value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: self.type_of(&cell_value),
                    }
                    .into());
                };
                // Write barrier: mark card if cell is in an older generation than the value.
                // The shared borrows of `obj`/`closure` have ended, so `write_barrier`
                // (which borrows `self` immutably via `self.heap`) is safe here.
                self.write_barrier(cell_ptr, value);
                // SAFETY: cell_ptr is a VM-owned Cell object; single-threaded.
                let cell_obj = unsafe { cell_ptr.get_mut() };
                let Object::Cell(cell) = cell_obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: ObjectType::of(cell_obj).into(),
                    }
                    .into());
                };
                cell.value = value;
            }

            Instruction::CaptureRef(idx) => {
                // Push the raw cell pointer from captures[idx] without
                // reading through the cell.  Used by nested closures to
                // forward a shared cell to an inner closure.
                let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                    unreachable!("exec loop frame is always Bytecode");
                };
                let closure_ptr = bf.function;
                // SAFETY: closure_ptr is the frame's function, valid for
                // the duration of this frame.
                let obj = unsafe { closure_ptr.get() };
                let Object::Closure(closure) = obj else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Closure.into(),
                        got: ObjectType::of(obj).into(),
                    }
                    .into());
                };
                self.stack.push(closure.captures[idx]);
            }

            Instruction::SendEvent => {
                // Stack layout (top-to-bottom): [data, event_name]
                // Pop data first (it's on top), then event_name.
                let data = self.stack.ensure_pop()?;
                let name_value = self.stack.ensure_pop()?;

                // Extract the event name string from the heap.
                let event_name = self.as_string(&name_value)?.clone();

                // Capture source location from the current frame's line table.
                let source_location = if let Frame::Bytecode(bf) = &self.frames[*frame_idx] {
                    let pc = bf.faulting_pc;
                    let func_obj = self.get_object(bf.function).as_callable().ok();
                    func_obj
                        .and_then(|func| {
                            if let Some(compact) = &func.bytecode.compact {
                                compact.line_entry_for_pc(pc)
                            } else {
                                func.bytecode.line_entry_for_pc(pc)
                            }
                        })
                        .map(|entry| {
                            (
                                entry.span.file_id.as_u32(),
                                u32::try_from(entry.line).unwrap_or(u32::MAX),
                                entry.span.range.start().into(),
                                u32::from(entry.span.range.start()),
                                u32::from(entry.span.range.end()),
                            )
                        })
                } else {
                    None
                };

                return Ok(Some(VmExecState::Event {
                    event_name,
                    data,
                    source_location,
                }));
            }
        }

        Ok(None)
    }

    // ── Shared helpers for compact dispatch ──────────────────────────────────

    /// Execute a comparison operation. Pops two values, pushes a Bool.
    /// Shared between the legacy `step()` `CmpOp` arm and the expanded compact opcodes.
    fn exec_cmpop(&mut self, op: CmpOp) -> Result<(), VmError> {
        let right = self.stack.ensure_pop()?;
        let left = self.stack.ensure_pop()?;

        let result = match (left, right) {
            (Value::Int(l), Value::Int(r)) => Value::Bool(match op {
                CmpOp::Eq => l == r,
                CmpOp::NotEq => l != r,
                CmpOp::Lt => l < r,
                CmpOp::LtEq => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::GtEq => l >= r,
            }),

            #[allow(clippy::float_cmp)]
            (Value::Float(l), Value::Float(r)) => Value::Bool(match op {
                CmpOp::Eq => l == r,
                CmpOp::NotEq => l != r,
                CmpOp::Lt => l < r,
                CmpOp::LtEq => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::GtEq => l >= r,
            }),

            #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
            (Value::Int(l), Value::Float(r)) => {
                let l = l as f64;
                Value::Bool(match op {
                    CmpOp::Eq => l == r,
                    CmpOp::NotEq => l != r,
                    CmpOp::Lt => l < r,
                    CmpOp::LtEq => l <= r,
                    CmpOp::Gt => l > r,
                    CmpOp::GtEq => l >= r,
                })
            }

            #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
            (Value::Float(l), Value::Int(r)) => {
                let r = r as f64;
                Value::Bool(match op {
                    CmpOp::Eq => l == r,
                    CmpOp::NotEq => l != r,
                    CmpOp::Lt => l < r,
                    CmpOp::LtEq => l <= r,
                    CmpOp::Gt => l > r,
                    CmpOp::GtEq => l >= r,
                })
            }

            (Value::Object(li), Value::Object(ri))
                if matches!(self.get_object(li), Object::String(_))
                    && matches!(self.get_object(ri), Object::String(_)) =>
            {
                let ls = self.as_string(&left)?;
                let rs = self.as_string(&right)?;
                Value::Bool(match op {
                    CmpOp::Eq => ls == rs,
                    CmpOp::NotEq => ls != rs,
                    CmpOp::Lt => ls < rs,
                    CmpOp::LtEq => ls <= rs,
                    CmpOp::Gt => ls > rs,
                    CmpOp::GtEq => ls >= rs,
                })
            }

            (Value::Object(li), Value::Object(ri))
                if matches!(self.get_object(li), Object::Uint8Array(_))
                    && matches!(self.get_object(ri), Object::Uint8Array(_)) =>
            {
                let la = self.as_uint8array(&left)?;
                let ra = self.as_uint8array(&right)?;
                Value::Bool(match op {
                    CmpOp::Eq => la == ra,
                    CmpOp::NotEq => la != ra,
                    _ => {
                        return Err(VmInternalError::CannotApplyCmpOp {
                            left: bex_vm_types::types::Type::Object(ObjectType::Uint8Array),
                            right: bex_vm_types::types::Type::Object(ObjectType::Uint8Array),
                            op,
                        }
                        .into());
                    }
                })
            }

            (Value::Object(li), Value::Object(ri))
                if matches!(self.get_object(li), Object::Variant(_))
                    && matches!(self.get_object(ri), Object::Variant(_)) =>
            {
                let Object::Variant(lv) = self.get_object(li) else {
                    unreachable!()
                };
                let Object::Variant(rv) = self.get_object(ri) else {
                    unreachable!()
                };
                Value::Bool(match op {
                    CmpOp::Eq => lv.enm == rv.enm && lv.index == rv.index,
                    CmpOp::NotEq => lv.enm != rv.enm || lv.index != rv.index,
                    _ => {
                        return Err(VmInternalError::CannotApplyCmpOp {
                            left: bex_vm_types::types::Type::Object(ObjectType::Variant),
                            right: bex_vm_types::types::Type::Object(ObjectType::Variant),
                            op,
                        }
                        .into());
                    }
                })
            }

            _ => Value::Bool(match op {
                CmpOp::Eq => left == right,
                CmpOp::NotEq => left != right,
                _ => {
                    return Err(VmInternalError::CannotApplyCmpOp {
                        left: self.type_of(&left),
                        right: self.type_of(&right),
                        op,
                    }
                    .into());
                }
            }),
        };

        self.stack.push(result);
        Ok(())
    }

    /// Execute a binary arithmetic operation. Pops two values, pushes the result.
    /// Shared between the legacy `step()` `BinOp` arm and the compact `Add` opcode
    /// (which needs string concatenation in addition to numeric dispatch).
    fn exec_binop(&mut self, op: BinOp) -> Result<(), VmError> {
        let right = self.stack.ensure_pop()?;
        let left = self.stack.ensure_pop()?;

        let result = match (left, right) {
            (Value::Int(l), Value::Int(r)) => Value::Int(match op {
                BinOp::Div if r == 0 => {
                    return Err(VmError::Thrown(self.panic_to_exception_value(
                        VmPanic::DivisionByZero {
                            left: Value::Int(l),
                            right: Value::Int(r),
                        },
                    )));
                }
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => l / r,
                BinOp::Mod => l % r,
                BinOp::BitAnd => l & r,
                BinOp::BitOr => l | r,
                BinOp::BitXor => l ^ r,
                BinOp::Shl => l << r,
                BinOp::Shr => l >> r,
            }),

            (Value::Float(l), Value::Float(r)) => Value::Float(match op {
                BinOp::Div if r == 0.0 => {
                    return Err(VmError::Thrown(self.panic_to_exception_value(
                        VmPanic::DivisionByZero {
                            left: Value::Float(l),
                            right: Value::Float(r),
                        },
                    )));
                }
                BinOp::Add => l + r,
                BinOp::Sub => l - r,
                BinOp::Mul => l * r,
                BinOp::Div => l / r,
                BinOp::Mod => l % r,
                BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                    return Err(VmInternalError::CannotApplyBinOp {
                        left: bex_vm_types::types::Type::Float,
                        right: bex_vm_types::types::Type::Float,
                        op,
                    }
                    .into());
                }
            }),

            #[allow(clippy::cast_precision_loss)]
            (Value::Int(l), Value::Float(r)) => {
                let l = l as f64;
                Value::Float(match op {
                    BinOp::Div if r == 0.0 => {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::Float(l),
                                right: Value::Float(r),
                            },
                        )));
                    }
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                    BinOp::Mod => l % r,
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                        return Err(VmInternalError::CannotApplyBinOp {
                            left: bex_vm_types::types::Type::Int,
                            right: bex_vm_types::types::Type::Float,
                            op,
                        }
                        .into());
                    }
                })
            }

            #[allow(clippy::cast_precision_loss)]
            (Value::Float(l), Value::Int(r)) => {
                let r = r as f64;
                Value::Float(match op {
                    BinOp::Div if r == 0.0 => {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::Float(l),
                                right: Value::Float(r),
                            },
                        )));
                    }
                    BinOp::Add => l + r,
                    BinOp::Sub => l - r,
                    BinOp::Mul => l * r,
                    BinOp::Div => l / r,
                    BinOp::Mod => l % r,
                    BinOp::BitAnd | BinOp::BitOr | BinOp::BitXor | BinOp::Shl | BinOp::Shr => {
                        return Err(VmInternalError::CannotApplyBinOp {
                            left: bex_vm_types::types::Type::Float,
                            right: bex_vm_types::types::Type::Int,
                            op,
                        }
                        .into());
                    }
                })
            }

            (Value::Object(_), Value::Object(_)) if op == BinOp::Add => {
                let ls = self.as_string(&left)?;
                let rs = self.as_string(&right)?;
                let mut concat = ls.clone();
                concat.push_str(rs);
                self.alloc_string(concat)
            }

            _ => {
                return Err(VmInternalError::CannotApplyBinOp {
                    left: self.type_of(&left),
                    right: self.type_of(&right),
                    op,
                }
                .into());
            }
        };

        self.stack.push(result);
        Ok(())
    }

    /// Get a helper for reading from compact bytecode that borrows the current
    /// bytecode frame's `instruction_ptr` mutably.
    fn current_bytecode_frame_mut(&mut self, frame_idx: usize) -> &mut BytecodeFrame {
        let Frame::Bytecode(bf) = &mut self.frames[frame_idx] else {
            unreachable!("exec_compact frame is always Bytecode");
        };
        bf
    }

    /// Compact bytecode dispatch loop.
    ///
    /// Reads opcodes from `CompactCode.code` instead of indexing `Vec<Instruction>`.
    /// `instruction_ptr` is a byte offset into the code array.
    fn exec_compact(&mut self) -> Result<VmExecState, VmError> {
        if self.frames.is_empty() {
            return Ok(VmExecState::Complete(Value::Null));
        }

        let mut frame_idx = self.frames.len() - 1;
        let mut function = unsafe { self.load_function(frame_idx)? };

        loop {
            // ── CPS continuation handler (identical to exec_inner) ────────
            while matches!(self.frames.last(), Some(Frame::Native(_))) {
                let v = self.stack.ensure_pop()?;
                let Some(Frame::Native(nf)) = self.frames.pop() else {
                    unreachable!("just matched Some(Frame::Native(_))");
                };
                let native_fn_ptr = nf.function;

                match nf.continuation.call(self, v) {
                    NativeCallResult::Done(val) => {
                        self.stack.push(val);
                    }
                    NativeCallResult::Error(e) => match self.native_error_to_vm_error(e) {
                        VmError::Thrown(exception_value) => {
                            self.try_unwind_exception(
                                &mut frame_idx,
                                &mut function,
                                exception_value,
                            )?;
                            break;
                        }
                        other => return Err(other),
                    },
                    NativeCallResult::YieldToCall {
                        callee,
                        args: mut callback_args,
                        continuation,
                    } => {
                        self.frames.push(Frame::Native(NativeFrame {
                            function: native_fn_ptr,
                            continuation,
                        }));

                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args);
                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        self.stack.extend(callback_args);

                        let ecflo_result = match self.execute_call_from_locals_offset(
                            real_callee,
                            cb_locals,
                            arg_count,
                            &mut frame_idx,
                            &mut function,
                        ) {
                            Ok(result) => result,
                            Err(VmError::Thrown(exception_value)) => {
                                self.try_unwind_exception(
                                    &mut frame_idx,
                                    &mut function,
                                    exception_value,
                                )?;
                                break;
                            }
                            Err(other) => return Err(other),
                        };

                        if let Some(state) = ecflo_result {
                            return Ok(state);
                        }
                    }
                }
            }

            if self.frames.is_empty() {
                return self
                    .stack
                    .ensure_pop()
                    .map_err(VmError::InternalError)
                    .map(VmExecState::Complete);
            }

            frame_idx = self.frames.len() - 1;
            function = unsafe { self.load_function(frame_idx)? };

            // ── Instruction execution ─────────────────────────────────────
            let Frame::Bytecode(bf) = &mut self.frames[frame_idx] else {
                unreachable!(
                    "exec_compact loop frame is always Bytecode after continuation handler"
                );
            };

            let compact = function.bytecode.compact.as_ref().unwrap();
            let saved_pc = bf.instruction_ptr;
            bf.faulting_pc = saved_pc;

            // Read opcode byte and advance PC past it.
            let op_byte = compact.code[saved_pc];
            bf.instruction_ptr = saved_pc + 1;

            let step_result = self.step_compact(&mut frame_idx, &mut function, op_byte);

            match step_result {
                Ok(Some(state)) => return Ok(state),
                Ok(None) => {}
                Err(VmError::InternalError(err)) => return Err(VmError::InternalError(err)),
                Err(VmError::Thrown(exception_value)) => {
                    self.try_unwind_exception(&mut frame_idx, &mut function, exception_value)?;
                }
                Err(
                    e @ (VmError::ThrownUnhandled { .. } | VmError::TracedInternalError { .. }),
                ) => return Err(e),
            }
        }
    }

    /// Execute a single compact-encoded instruction.
    ///
    /// Reads operands from the compact code stream (advancing `bf.instruction_ptr`),
    /// then executes the same logic as the corresponding arm in `step()`.
    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_lossless,
        clippy::useless_conversion
    )]
    fn step_compact(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        op_byte: u8,
    ) -> Result<Option<VmExecState>, VmError> {
        use bex_vm_types::bytecode::OpCode;

        // Unchecked byte-stream readers. SAFETY: the compact bytecode is produced
        // by our own encoder which guarantees correct sizes (verified by
        // debug_assert_eq!(code.len(), byte_offset) in lower_to_compact pass 2).
        // The PC always stays in bounds during well-formed execution.
        #[inline]
        unsafe fn read_u32_unchecked(code: &[u8], pc: &mut usize) -> u32 {
            unsafe {
                let p = *pc;
                let bytes = [
                    *code.get_unchecked(p),
                    *code.get_unchecked(p + 1),
                    *code.get_unchecked(p + 2),
                    *code.get_unchecked(p + 3),
                ];
                *pc = p + 4;
                u32::from_le_bytes(bytes)
            }
        }

        #[inline]
        unsafe fn read_i32_unchecked(code: &[u8], pc: &mut usize) -> i32 {
            unsafe {
                let p = *pc;
                let bytes = [
                    *code.get_unchecked(p),
                    *code.get_unchecked(p + 1),
                    *code.get_unchecked(p + 2),
                    *code.get_unchecked(p + 3),
                ];
                *pc = p + 4;
                i32::from_le_bytes(bytes)
            }
        }

        #[inline]
        unsafe fn read_i8_unchecked(code: &[u8], pc: &mut usize) -> i8 {
            unsafe {
                let val = *code.get_unchecked(*pc) as i8;
                *pc += 1;
                val
            }
        }

        let op = OpCode::try_from(op_byte).map_err(VmInternalError::InvalidOpcode)?;

        // SAFETY: `function` is `&'static Function`, so `code` has lifetime 'static.
        // This does NOT borrow from `self`, so there is no conflict with `&mut self`.
        // All unchecked reads are safe because the compact bytecode is produced by our
        // own encoder which guarantees correct byte layout (verified by debug_assert in
        // lower_to_compact). The PC always stays in bounds during well-formed execution.
        let code: &'static [u8] = &function.bytecode.compact.as_ref().unwrap().code;

        // SAFETY: see above — bytecode invariants guarantee all reads are in bounds.
        #[allow(unused_unsafe)]
        unsafe {
            match op {
                // ── Common constants ──────────────────────────────────────────
                OpCode::LoadNull => {
                    self.stack.push(Value::Null);
                }
                OpCode::LoadTrue => {
                    self.stack.push(Value::Bool(true));
                }
                OpCode::LoadFalse => {
                    self.stack.push(Value::Bool(false));
                }
                OpCode::LoadIntSmall => {
                    let val = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_i8_unchecked(code, &mut bf.instruction_ptr)
                    };
                    self.stack.push(Value::Int(i64::from(val)));
                }

                // ── LoadConst ─────────────────────────────────────────────────
                OpCode::LoadConst => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let val = function.bytecode.resolved_constants[idx];
                    self.stack.push(val);
                }

                // ── LoadVar / StoreVar ────────────────────────────────────────
                OpCode::LoadVar => {
                    let slot = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let stack_slot = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack[stack_slot];
                    self.stack.push(value);
                }

                OpCode::StoreVar => {
                    let slot = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack.ensure_pop()?;
                    let old_value = std::mem::replace(&mut self.stack[local_var_index], value);

                    if self.watched_vars.contains_key(&local_var_index) {
                        let watched_node = NodeId::LocalVar(local_var_index);
                        self.update_watched_node(
                            watched_node,
                            watch::Path::Binding,
                            old_value,
                            value,
                        );
                        if let Some(state) = self.watch.root_state_mut(watched_node) {
                            state.value = value;
                        }
                        let notifications = self.process_notifications(watched_node)?;
                        if !notifications.is_empty() {
                            return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                                notifications,
                            ))));
                        }
                    }
                }

                // ── LoadGlobal / StoreGlobal ──────────────────────────────────
                OpCode::LoadGlobal => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.globals[global_idx];
                    self.stack.push(value);
                }

                OpCode::StoreGlobal => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.stack.ensure_pop()?;
                    self.globals[global_idx] = value;
                }

                // ── LoadField / StoreField / InitField ────────────────────────
                OpCode::LoadField => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let top = self.stack.ensure_pop()?;
                    let obj_ptr = self.as_object_ptr(&top, ObjectType::Instance)?;
                    let Object::Instance(instance) = self.get_object(obj_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(self.get_object(obj_ptr)).into(),
                        }
                        .into());
                    };
                    let value = instance.fields[idx];
                    self.stack.push(value);
                }

                OpCode::StoreField => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let new_value = self.stack.ensure_pop()?;
                    let instance_value = self.stack.ensure_pop()?;
                    let obj_ptr = self.as_object_ptr(&instance_value, ObjectType::Instance)?;

                    let old_value = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        instance.fields[idx]
                    };

                    let watched_node = NodeId::HeapObject(obj_ptr);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::InstanceField(idx),
                        old_value,
                        new_value,
                    );
                    self.write_barrier(obj_ptr, new_value);
                    let Object::Instance(instance) = self.get_object_mut(obj_ptr) else {
                        unreachable!("already type-checked above");
                    };
                    instance.fields[idx] = new_value;

                    let notifications = self.process_notifications(watched_node)?;
                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }

                OpCode::InitField => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let new_value = self.stack.ensure_pop()?;
                    let instance_value = self.stack.ensure_pop()?;
                    let obj_ptr = self.as_object_ptr(&instance_value, ObjectType::Instance)?;
                    self.write_barrier(obj_ptr, new_value);
                    let Object::Instance(instance) = self.get_object_mut(obj_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(self.get_object(obj_ptr)).into(),
                        }
                        .into());
                    };
                    instance.fields[idx] = new_value;
                    self.stack.push(instance_value);
                }

                // ── Pop / Copy ────────────────────────────────────────────────
                OpCode::Pop => {
                    let n = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let drain_start = self.stack.len() - n;
                    let drain_range = StackIndex::from_raw(drain_start)..;
                    self.stack.drain(drain_range);
                }

                OpCode::Copy => {
                    let offset = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let index = self.stack.ensure_slot_from_top(offset)?;
                    let value = self.stack[index];
                    self.stack.push(value);
                }

                // ── Allocation opcodes ────────────────────────────────────────
                OpCode::AllocArray => {
                    let size = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                    let array = self.stack.drain(drain_range).collect();
                    let array_index = self.tlab.alloc(Object::Array(array));
                    self.stack.push(Value::Object(array_index));
                }

                OpCode::AllocMap => {
                    let n = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let map = if n > 0 {
                        let end_of_values = self.stack.ensure_slot_from_top(2 * n - 1)?;
                        let end_of_keys = self.stack.ensure_slot_from_top(n - 1)?;
                        let idx_of_last_key = self.stack.ensure_slot_from_top(n - 1)?;
                        let values = self.stack[end_of_values..end_of_keys].iter().copied();
                        let keys = self.stack[idx_of_last_key..].iter().map(|k| {
                            let obj_index = self.as_object_ptr(k, ObjectType::String)?;
                            self.get_object(obj_index).as_string().cloned()
                        });
                        let pairs = values
                            .zip(keys)
                            .map(|(val, key_res)| key_res.map(|k| (k, val)));
                        let map = pairs.collect::<Result<IndexMap<_, _>, _>>()?;
                        self.stack.drain(end_of_values..);
                        map
                    } else {
                        IndexMap::new()
                    };
                    let obj_index = self.tlab.alloc(Object::Map(map));
                    self.stack.push(Value::Object(obj_index));
                }

                OpCode::AllocInstance => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let class_ptr = self.idx_to_ptr(ObjectIndex::from_raw(raw as usize));
                    let Object::Class(class) = self.get_object(class_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Class.into(),
                            got: ObjectType::of(self.get_object(class_ptr)).into(),
                        }
                        .into());
                    };
                    let mut fields = Vec::with_capacity(class.fields.len());
                    fields.resize(class.fields.len(), Value::Null);
                    let instance_ptr =
                        self.tlab
                            .alloc(Object::Instance(bex_vm_types::types::Instance {
                                class: class_ptr,
                                fields,
                            }));
                    self.stack.push(Value::Object(instance_ptr));
                }

                OpCode::AllocVariant => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let enum_ptr = self.idx_to_ptr(ObjectIndex::from_raw(raw as usize));
                    let variant_count = {
                        let Object::Enum(enm) = self.get_object(enum_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Enum.into(),
                                got: ObjectType::of(self.get_object(enum_ptr)).into(),
                            }
                            .into());
                        };
                        enm.variants.len()
                    };
                    let variant = self.stack.ensure_pop()?;
                    let Value::Int(variant_index) = variant else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&variant),
                        }
                        .into());
                    };
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    if variant_index < 0 || variant_index as usize >= variant_count {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::IndexOutOfBounds {
                                index: variant_index,
                                length: variant_count,
                            },
                        )));
                    }
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let variant_usize = variant_index as usize;
                    let variant_ptr = self.tlab.alloc(Object::Variant(Variant {
                        enm: enum_ptr,
                        index: variant_usize,
                    }));
                    self.stack.push(Value::Object(variant_ptr));
                }

                // ── DispatchFuture ────────────────────────────────────────────
                OpCode::DispatchFuture => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let callee = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.globals[callee];
                    let expected_type = FunctionType::SysOp;
                    let callee_ptr = self.as_object_ptr(&callee_value, expected_type.into())?;
                    let Object::Function(callable_future) = self.get_object(callee_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: expected_type.into(),
                            got: ObjectType::of(self.get_object(callee_ptr)).into(),
                        }
                        .into());
                    };
                    let FunctionKind::SysOp(sys_op) = callable_future.kind else {
                        return Err(VmInternalError::TypeError {
                            expected: FunctionType::SysOp.into(),
                            got: FunctionType::from(&callable_future.kind).into(),
                        }
                        .into());
                    };
                    let args_offset = self.stack.len().checked_sub(callable_future.arity).ok_or(
                        VmInternalError::NotEnoughItemsOnStack(callable_future.arity),
                    )?;
                    let args_offset = StackIndex::from_raw(args_offset);
                    let future_args: Vec<Value> = self.stack.drain(args_offset..).collect();
                    let pending_future = bex_vm_types::types::PendingFuture {
                        operation: sys_op,
                        args: future_args,
                    };
                    let future_value =
                        self.alloc_future(bex_vm_types::types::Future::Pending(pending_future));
                    let Value::Object(object_index) = future_value else {
                        unreachable!("alloc_future returns Value::Object")
                    };
                    self.stack.push(future_value);
                    return Ok(Some(VmExecState::ScheduleFuture(object_index)));
                }

                // ── Watch / Unwatch / Notify / NotifyBlock ────────────────────
                OpCode::Watch => {
                    let index = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let filter = match self.stack.ensure_pop()? {
                        Value::Null => WatchFilter::Default,
                        Value::Object(object_index) => match self.get_object(object_index) {
                            Object::Function(_) => WatchFilter::Function(object_index),
                            Object::String(mode) if mode == "manual" => WatchFilter::Manual,
                            Object::String(mode) if mode == "never" => WatchFilter::Paused,
                            _ => return Err(VmInternalError::InvalidFilter.into()),
                        },
                        _ => return Err(VmInternalError::InvalidFilter.into()),
                    };
                    let channel_value = self.stack.ensure_pop()?;
                    let channel = self.as_string(&channel_value)?.to_owned();
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                    let value = self.stack[local_var_index];
                    let var_node = NodeId::LocalVar(local_var_index);
                    self.watch.register_root(
                        var_node,
                        RootState {
                            channel,
                            value,
                            filter,
                            last_notified: None,
                            last_assigned: None,
                        },
                    );
                    let watched_var_name = &function.local_names[index];
                    self.watched_vars.insert(
                        local_var_index,
                        (watched_var_name.clone(), function.name.clone()),
                    );
                    if let Value::Object(object_index) = value {
                        watch::track_watch_dependencies(
                            &mut self.watch,
                            var_node,
                            watch::Path::Binding,
                            object_index,
                        );
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::Unwatch => {
                    let index = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                    if self.watched_vars.remove(&local_var_index).is_some() {
                        let var_node = NodeId::LocalVar(local_var_index);
                        self.watch.unregister_root(var_node);
                        let value = self.stack[local_var_index];
                        if let Value::Object(object_index) = value {
                            self.watch.unlink_edge(
                                var_node,
                                watch::Path::Binding,
                                NodeId::HeapObject(object_index),
                            );
                        }
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::Notify => {
                    let index = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                    let var_node = NodeId::LocalVar(local_var_index);
                    let notifications = self.watch.copy_roots_reaching(var_node);
                    if notifications.len() != 1 && notifications.first() != Some(&var_node) {
                        return Err(VmInternalError::InvalidManualNotify.into());
                    }
                    return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    ))));
                }

                OpCode::NotifyBlock => {
                    let block_index = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let notification = &function.block_notifications[block_index];
                    let full_notification = bytecode::BlockNotification {
                        function_name: function.name.clone(),
                        block_name: notification.block_name.clone(),
                        level: notification.level,
                        block_type: notification.block_type,
                        is_enter: notification.is_enter,
                    };
                    return Ok(Some(VmExecState::Notify(WatchNotification::Block(
                        full_notification,
                    ))));
                }

                // ── VizEnter / VizExit ────────────────────────────────────────
                OpCode::VizEnter | OpCode::VizExit => {
                    let index = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let delta = if op == OpCode::VizEnter {
                        bytecode::VizExecDelta::Enter
                    } else {
                        bytecode::VizExecDelta::Exit
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    let node = function.viz_nodes.get(index).ok_or({
                        VmError::Thrown(self.panic_to_exception_value(VmPanic::IndexOutOfBounds {
                            index: index as i64,
                            length: function.viz_nodes.len(),
                        }))
                    })?;
                    let event = bytecode::VizExecEvent {
                        delta,
                        node_id: node.node_id,
                        node_type: node.node_type,
                        label: node.label.clone(),
                        header_level: node.header_level,
                    };
                    return Ok(Some(VmExecState::Notify(WatchNotification::Viz {
                        function_name: function.name.clone(),
                        event,
                    })));
                }

                // ── Call ──────────────────────────────────────────────────────
                OpCode::Call => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.globals[callee_global];
                    let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;
                    let args_offset = self
                        .stack
                        .len()
                        .checked_sub(arg_count)
                        .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count))?;
                    let locals_offset = StackIndex::from_raw(args_offset);
                    return self.execute_call_from_locals_offset(
                        callee_ptr,
                        locals_offset,
                        arg_count,
                        frame_idx,
                        function,
                    );
                }

                // ── CallIndirect ──────────────────────────────────────────────
                OpCode::CallIndirect => {
                    let callee_slot = self.stack.ensure_stack_top()?;
                    let callee_value = self.stack[callee_slot];
                    let callee_ptr =
                        self.as_object_ptr(&callee_value, FunctionType::Callable.into())?;
                    let obj = self.get_object(callee_ptr);

                    if let Object::BoundMethod(bm) = obj {
                        let func_obj = unsafe { bm.function.get() };
                        let full_arity = match func_obj {
                            Object::Function(f) => f.arity,
                            _ => {
                                return Err(VmInternalError::TypeError {
                                    expected: FunctionType::Callable.into(),
                                    got: ObjectType::of(func_obj).into(),
                                }
                                .into());
                            }
                        };
                        debug_assert!(
                            full_arity >= 1,
                            "BoundMethod's inner function must have self parameter"
                        );
                        let visible_arity = full_arity.saturating_sub(1);
                        let receiver = bm.receiver;
                        let fn_ptr = bm.function;
                        let _popped = self.stack.ensure_pop()?;
                        let args_offset = self
                            .stack
                            .len()
                            .checked_sub(visible_arity)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(visible_arity))?;
                        self.stack.insert(args_offset, receiver);
                        let locals_offset = StackIndex::from_raw(args_offset);
                        if let Some(state) = self.execute_call_from_locals_offset(
                            fn_ptr,
                            locals_offset,
                            full_arity,
                            frame_idx,
                            function,
                        )? {
                            return Ok(Some(state));
                        }
                    } else {
                        let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;
                        let args_offset = self
                            .stack
                            .len()
                            .checked_sub(arg_count + 1)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count + 1))?;
                        let _popped_callee = self.stack.ensure_pop()?;
                        let locals_offset = StackIndex::from_raw(args_offset);
                        if let Some(state) = self.execute_call_from_locals_offset(
                            callee_ptr,
                            locals_offset,
                            arg_count,
                            frame_idx,
                            function,
                        )? {
                            return Ok(Some(state));
                        }
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Return ────────────────────────────────────────────────────
                OpCode::Return => {
                    let result = self.stack.ensure_pop()?;
                    let span_exit = if self.traced_frames.last() == Some(frame_idx) {
                        let func_name = self
                            .get_object(self.frames[*frame_idx].function())
                            .as_callable()
                            .map(|f| f.name.clone())
                            .ok();
                        self.traced_frames.pop();
                        func_name
                    } else {
                        None
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    self.stack.drain(bf.locals_offset..);
                    self.stack.push(result);
                    self.frames.pop();
                    if Some(self.frames.len()) == self.interrupt_frame {
                        self.interrupt_frame = None;
                        return self
                            .stack
                            .ensure_pop()
                            .map_err(VmError::InternalError)
                            .map(VmExecState::Complete)
                            .map(Some);
                    }
                    if self.frames.is_empty() {
                        return self
                            .stack
                            .ensure_pop()
                            .map_err(VmError::InternalError)
                            .map(VmExecState::Complete)
                            .map(Some);
                    }
                    if let Some(name) = span_exit {
                        return Ok(Some(VmExecState::SpanNotify(
                            SpanNotification::FunctionExit {
                                function_name: name,
                                result,
                            },
                        )));
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Await ─────────────────────────────────────────────────────
                OpCode::Await => {
                    let value = self.stack.ensure_stack_top()?;
                    let wanted_type = bex_vm_types::types::FutureType::Any;
                    let index = self.as_object_ptr(&self.stack[value], wanted_type.into())?;
                    let ready_value = {
                        let Object::Future(awaiting) = self.get_object(index) else {
                            return Err(VmInternalError::TypeError {
                                expected: wanted_type.into(),
                                got: ObjectType::of(self.get_object(index)).into(),
                            }
                            .into());
                        };
                        match awaiting {
                            bex_vm_types::types::Future::Pending(_) => {
                                return Ok(Some(VmExecState::Await(index)));
                            }
                            bex_vm_types::types::Future::Ready(v) => *v,
                        }
                    };
                    self.stack.pop();
                    self.stack.push(ready_value);
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Throw ─────────────────────────────────────────────────────
                OpCode::Throw => {
                    let value = self.stack.ensure_pop()?;
                    self.try_unwind_exception(frame_idx, function, value)?;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Jump opcodes ──────────────────────────────────────────────
                OpCode::Jump => {
                    let offset = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_i32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let bf = self.current_bytecode_frame_mut(*frame_idx);
                    // offset is relative to instruction end (current bf.instruction_ptr)
                    bf.instruction_ptr = (bf.instruction_ptr as i64 + offset as i64) as usize;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::PopJumpIfFalse => {
                    let offset = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_i32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let cond = self.stack.ensure_pop()?;
                    if cond == Value::Bool(false) {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        bf.instruction_ptr = (bf.instruction_ptr as i64 + offset as i64) as usize;
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::JumpIfFalse => {
                    let offset = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_i32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let top_slot = self.stack.ensure_stack_top()?;
                    let cond = self.stack[top_slot];
                    if cond == Value::Bool(false) {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        bf.instruction_ptr = (bf.instruction_ptr as i64 + offset as i64) as usize;
                    }
                }

                // ── JumpTable ─────────────────────────────────────────────────
                OpCode::JumpTable => {
                    let (table_idx, default_offset) = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        let tidx = read_u32_unchecked(code, &mut bf.instruction_ptr) as usize;
                        let def = read_i32_unchecked(code, &mut bf.instruction_ptr);
                        (tidx, def)
                    };
                    let discriminant = self.stack.ensure_pop()?;
                    let Value::Int(value) = discriminant else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&discriminant),
                        }
                        .into());
                    };
                    // Use pre-translated compact jump table (byte-offset-relative).
                    let compact = function.bytecode.compact.as_ref().unwrap();
                    let compact_table = &compact.jump_tables[table_idx];
                    let bf_ip = {
                        let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                            unreachable!()
                        };
                        bf.instruction_ptr
                    };
                    let offset = compact_table.lookup(value).unwrap_or(default_offset);
                    let bf = self.current_bytecode_frame_mut(*frame_idx);
                    bf.instruction_ptr = (bf_ip as i64 + offset as i64) as usize;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Discriminant ──────────────────────────────────────────────
                OpCode::Discriminant => {
                    let value = self.stack.ensure_pop()?;
                    let Value::Object(object_idx) = value else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Variant.into(),
                            got: self.type_of(&value),
                        }
                        .into());
                    };
                    let variant_index = {
                        let Object::Variant(variant) = self.get_object(object_idx) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Variant.into(),
                                got: ObjectType::of(self.get_object(object_idx)).into(),
                            }
                            .into());
                        };
                        variant.index
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    self.stack.push(Value::Int(variant_index as i64));
                }

                // ── TypeTag ───────────────────────────────────────────────────
                OpCode::TypeTag => {
                    let value = self.stack.ensure_pop()?;
                    let tag = value_type_tag(&value);
                    self.stack.push(Value::Int(tag));
                }

                // ── IsType ────────────────────────────────────────────────────
                OpCode::IsType => {
                    let const_idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let value = self.stack.ensure_pop()?;
                    let expected = &function.bytecode.resolved_constants[const_idx];
                    let result = match expected {
                        Value::Object(class_ptr) => match value {
                            Value::Object(val_ptr) => match self.get_object(val_ptr) {
                                Object::Instance(instance) => instance.class == *class_ptr,
                                _ => false,
                            },
                            _ => false,
                        },
                        Value::Int(tag) => value_type_tag(&value) == *tag,
                        _ => false,
                    };
                    self.stack.push(Value::Bool(result));
                }

                // ── DenseTag ──────────────────────────────────────────────────
                #[allow(
                    clippy::cast_sign_loss,
                    clippy::cast_lossless,
                    clippy::cast_possible_truncation
                )]
                OpCode::DenseTag => {
                    let table_idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let tag = match self.stack.ensure_pop()? {
                        Value::Int(t) => t,
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: bex_vm_types::types::Type::Int,
                                got: self.type_of(&other),
                            }
                            .into());
                        }
                    };
                    let table = &function.bytecode.match_hash_tables[table_idx];
                    let h = ((tag as u64).wrapping_mul(table.multiply) >> table.shift)
                        & table.mask as u64;
                    let entry = &table.entries[h as usize];
                    if entry.expected_tag == tag {
                        self.stack.push(Value::Int(i64::from(entry.dense_index)));
                    } else {
                        self.stack.push(Value::Int(-1));
                    }
                }

                // ── ThrowIfPanic ──────────────────────────────────────────────
                OpCode::ThrowIfPanic => {
                    let value = self.stack.ensure_pop()?;
                    let is_panic = match value {
                        Value::Object(ptr) => match self.get_object(ptr) {
                            Object::Instance(instance) => {
                                self.panic_class_ptrs.contains(&instance.class)
                            }
                            _ => false,
                        },
                        _ => false,
                    };
                    if is_panic {
                        self.try_unwind_exception(frame_idx, function, value)?;
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Unreachable ───────────────────────────────────────────────
                OpCode::Unreachable => {
                    return Err(VmError::Thrown(
                        self.panic_to_exception_value(VmPanic::Unreachable),
                    ));
                }

                // ── MakeCell ──────────────────────────────────────────────────
                OpCode::MakeCell => {
                    let value = self.stack.ensure_pop()?;
                    let cell = Object::Cell(bex_vm_types::types::Cell { value });
                    let ptr = self.tlab.alloc(cell);
                    self.stack.push(Value::Object(ptr));
                }

                // ── MakeClosure ───────────────────────────────────────────────
                OpCode::MakeClosure => {
                    let (obj_idx_raw, capture_count) = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        let o = read_u32_unchecked(code, &mut bf.instruction_ptr);
                        let c = read_u32_unchecked(code, &mut bf.instruction_ptr);
                        (o as usize, c as usize)
                    };
                    let mut captures = Vec::with_capacity(capture_count);
                    for _ in 0..capture_count {
                        captures.push(self.stack.ensure_pop()?);
                    }
                    captures.reverse();
                    let function_ptr = self.idx_to_ptr(ObjectIndex::from_raw(obj_idx_raw));
                    let closure = Object::Closure(Closure {
                        function: function_ptr,
                        captures,
                    });
                    let ptr = self.tlab.alloc(closure);
                    self.stack.push(Value::Object(ptr));
                }

                // ── MakeBoundMethod ───────────────────────────────────────────
                OpCode::MakeBoundMethod => {
                    let raw = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr)
                    };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let receiver = self.stack.ensure_pop()?;
                    let callee_value = self.globals[global_idx];
                    let function_ptr =
                        self.as_object_ptr(&callee_value, FunctionType::Callable.into())?;
                    let bound = Object::BoundMethod(BoundMethod {
                        function: function_ptr,
                        receiver,
                    });
                    let ptr = self.tlab.alloc(bound);
                    self.stack.push(Value::Object(ptr));
                }

                // ── LoadDeref / StoreDeref ────────────────────────────────────
                OpCode::LoadDeref => {
                    let slot = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let cell_value =
                        self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                    let Value::Object(cell_ptr) = cell_value else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    let obj = unsafe { cell_ptr.get() };
                    let Object::Cell(cell) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    self.stack.push(cell.value);
                }

                OpCode::StoreDeref => {
                    let slot = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let value = self.stack.ensure_pop()?;
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let cell_value =
                        self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                    let Value::Object(cell_ptr) = cell_value else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    self.write_barrier(cell_ptr, value);
                    let obj = unsafe { cell_ptr.get_mut() };
                    let Object::Cell(cell) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    cell.value = value;
                }

                // ── LoadCapture / StoreCapture / CaptureRef ───────────────────
                OpCode::LoadCapture => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let closure_ptr = bf.function;
                    let obj = unsafe { closure_ptr.get() };
                    let Object::Closure(closure) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Closure.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    let cell_value = closure.captures[idx];
                    let Value::Object(cell_ptr) = cell_value else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    let cell_obj = unsafe { cell_ptr.get() };
                    let Object::Cell(cell) = cell_obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(cell_obj).into(),
                        }
                        .into());
                    };
                    self.stack.push(cell.value);
                }

                OpCode::StoreCapture => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let value = self.stack.ensure_pop()?;
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let closure_ptr = bf.function;
                    let obj = unsafe { closure_ptr.get() };
                    let Object::Closure(closure) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Closure.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    let cell_value = closure.captures[idx];
                    let Value::Object(cell_ptr) = cell_value else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    self.write_barrier(cell_ptr, value);
                    let cell_obj = unsafe { cell_ptr.get_mut() };
                    let Object::Cell(cell) = cell_obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(cell_obj).into(),
                        }
                        .into());
                    };
                    cell.value = value;
                }

                OpCode::CaptureRef => {
                    let idx = {
                        let bf = self.current_bytecode_frame_mut(*frame_idx);
                        read_u32_unchecked(code, &mut bf.instruction_ptr) as usize
                    };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let closure_ptr = bf.function;
                    let obj = unsafe { closure_ptr.get() };
                    let Object::Closure(closure) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Closure.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    self.stack.push(closure.captures[idx]);
                }

                // ── Array / Map element ops ───────────────────────────────────
                OpCode::LoadArrayElement => {
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;
                    let array_obj_index = self.as_object_ptr(&array_value, ObjectType::Array)?;
                    let array_len = match self.get_object(array_obj_index) {
                        Object::Array(arr) => arr.len(),
                        Object::Uint8Array(bytes) => bytes.len(),
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }
                            .into());
                        }
                    };
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 || i as usize >= array_len {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: i,
                                        length: array_len,
                                    },
                                )));
                            }
                            i as usize
                        }
                        _ => {
                            return Err(VmInternalError::TypeError {
                                expected: bex_vm_types::types::Type::Int,
                                got: self.type_of(&index_value),
                            }
                            .into());
                        }
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    let element = match self.get_object(array_obj_index) {
                        Object::Array(array) => {
                            if index >= array.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: array.len(),
                                    },
                                )));
                            }
                            array[index]
                        }
                        Object::Uint8Array(bytes) => {
                            if index >= bytes.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: bytes.len(),
                                    },
                                )));
                            }
                            Value::Int(i64::from(bytes[index]))
                        }
                        _ => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(self.get_object(array_obj_index)).into(),
                            }
                            .into());
                        }
                    };
                    self.stack.push(element);
                }

                OpCode::LoadMapElement => {
                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;
                    let map_index = self.as_object_ptr(&map_value, ObjectType::Map)?;
                    let Object::Map(map) = self.get_object(map_index) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(self.get_object(map_index)).into(),
                        }
                        .into());
                    };
                    let key_index = self.as_object_ptr(&key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?;
                    let value = map.get(key).copied().ok_or(VmError::Thrown(
                        self.panic_to_exception_value(VmPanic::MapKeyNotFound),
                    ))?;
                    self.stack.push(value);
                }

                OpCode::StoreArrayElement => {
                    let new_value = self.stack.ensure_pop()?;
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;
                    let array_object_index = self.as_object_ptr(&array_value, ObjectType::Array)?;
                    let array_len = match self.get_object(array_object_index) {
                        Object::Array(arr) => arr.len(),
                        Object::Uint8Array(bytes) => bytes.len(),
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }
                            .into());
                        }
                    };
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 || i as usize >= array_len {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: i,
                                        length: array_len,
                                    },
                                )));
                            }
                            i as usize
                        }
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: bex_vm_types::types::Type::Int,
                                got: self.type_of(&other),
                            }
                            .into());
                        }
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    let old_value = match self.get_object(array_object_index) {
                        Object::Array(array) => {
                            if index >= array.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: array.len(),
                                    },
                                )));
                            }
                            array[index]
                        }
                        Object::Uint8Array(bytes) => {
                            if index >= bytes.len() {
                                return Err(VmError::Thrown(self.panic_to_exception_value(
                                    VmPanic::IndexOutOfBounds {
                                        index: index as i64,
                                        length: bytes.len(),
                                    },
                                )));
                            }
                            Value::Int(i64::from(bytes[index]))
                        }
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }
                            .into());
                        }
                    };
                    let watched_node = NodeId::HeapObject(array_object_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::ArrayIndex(index),
                        old_value,
                        new_value,
                    );
                    self.write_barrier(array_object_index, new_value);
                    match self.get_object_mut(array_object_index) {
                        Object::Array(array) => {
                            array[index] = new_value;
                        }
                        Object::Uint8Array(bytes) => {
                            let Value::Int(i) = new_value else {
                                return Err(VmInternalError::TypeError {
                                    expected: bex_vm_types::types::Type::Int,
                                    got: self.type_of(&new_value),
                                }
                                .into());
                            };
                            bytes[index] = (i.cast_unsigned() & 0xFF) as u8;
                        }
                        _ => unreachable!("already typechecked"),
                    }
                    let notifications = self.process_notifications(watched_node)?;
                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }

                OpCode::StoreMapElement => {
                    let new_value = self.stack.ensure_pop()?;
                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;
                    let key_index = self.as_object_ptr(&key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?.clone();
                    let map_index = self.as_object_ptr(&map_value, ObjectType::Map)?;
                    let old_value = match self.get_object(map_index) {
                        Object::Map(map) => map.get(&key).copied().unwrap_or(Value::Null),
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: ObjectType::of(other).into(),
                            }
                            .into());
                        }
                    };
                    let watched_node = NodeId::HeapObject(map_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::MapKey(key.clone()),
                        old_value,
                        new_value,
                    );
                    self.write_barrier(map_index, new_value);
                    if let Object::Map(map) = self.get_object_mut(map_index) {
                        map.insert(key, new_value);
                    }
                    let notifications = self.process_notifications(watched_node)?;
                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }

                // ── Expanded arithmetic ───────────────────────────────────────
                OpCode::Add => self.exec_binop(BinOp::Add)?,
                OpCode::Sub => self.exec_binop(BinOp::Sub)?,
                OpCode::Mul => self.exec_binop(BinOp::Mul)?,
                OpCode::Div => self.exec_binop(BinOp::Div)?,
                OpCode::Mod => self.exec_binop(BinOp::Mod)?,
                OpCode::BitAnd => self.exec_binop(BinOp::BitAnd)?,
                OpCode::BitOr => self.exec_binop(BinOp::BitOr)?,
                OpCode::BitXor => self.exec_binop(BinOp::BitXor)?,
                OpCode::Shl => self.exec_binop(BinOp::Shl)?,
                OpCode::Shr => self.exec_binop(BinOp::Shr)?,

                // ── Expanded comparison ───────────────────────────────────────
                OpCode::Eq => self.exec_cmpop(CmpOp::Eq)?,
                OpCode::NotEq => self.exec_cmpop(CmpOp::NotEq)?,
                OpCode::Lt => self.exec_cmpop(CmpOp::Lt)?,
                OpCode::LtEq => self.exec_cmpop(CmpOp::LtEq)?,
                OpCode::Gt => self.exec_cmpop(CmpOp::Gt)?,
                OpCode::GtEq => self.exec_cmpop(CmpOp::GtEq)?,

                // ── Expanded unary ────────────────────────────────────────────
                OpCode::Not => {
                    let val = self.stack.ensure_pop()?;
                    match val {
                        Value::Bool(b) => self.stack.push(Value::Bool(!b)),
                        _ => {
                            return Err(VmInternalError::CannotApplyUnaryOp {
                                op: UnaryOp::Not,
                                value: self.type_of(&val),
                            }
                            .into());
                        }
                    }
                }
                OpCode::Neg => {
                    let val = self.stack.ensure_pop()?;
                    match val {
                        Value::Int(n) => self.stack.push(Value::Int(-n)),
                        Value::Float(n) => self.stack.push(Value::Float(-n)),
                        _ => {
                            return Err(VmInternalError::CannotApplyUnaryOp {
                                op: UnaryOp::Neg,
                                value: self.type_of(&val),
                            }
                            .into());
                        }
                    }
                }

                // ── SendEvent ─────────────────────────────────────────────────
                OpCode::SendEvent => {
                    let data = self.stack.ensure_pop()?;
                    let name_value = self.stack.ensure_pop()?;
                    let event_name = self.as_string(&name_value)?.clone();
                    let source_location = if let Frame::Bytecode(bf) = &self.frames[*frame_idx] {
                        let pc = bf.faulting_pc;
                        let func_obj = self.get_object(bf.function).as_callable().ok();
                        func_obj
                            .and_then(|func| {
                                if let Some(compact) = &func.bytecode.compact {
                                    compact.line_entry_for_pc(pc)
                                } else {
                                    func.bytecode.line_entry_for_pc(pc)
                                }
                            })
                            .map(|entry| {
                                (
                                    entry.span.file_id.as_u32(),
                                    u32::try_from(entry.line).unwrap_or(u32::MAX),
                                    entry.span.range.start().into(),
                                    u32::from(entry.span.range.start()),
                                    u32::from(entry.span.range.end()),
                                )
                            })
                    } else {
                        None
                    };
                    return Ok(Some(VmExecState::Event {
                        event_name,
                        data,
                        source_location,
                    }));
                }
            }
        } // end unsafe block

        Ok(None)
    }
}

impl ::bex_vm_types::RootHaver for BexVm {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        // Stack values
        roots.extend(self.stack.iter().filter_map(|v| match v {
            Value::Object(ptr) => Some(*ptr),
            _ => None,
        }));

        // Watch state (last_assigned/last_notified values that aren't on the stack)
        self.watch.collect_roots(roots);

        // Frame function pointers (needed once closures are heap-allocated)
        roots.extend(self.collect_frame_roots());

        // Note: Frame locals are stored in the stack at the locals_offset position,
        // so they're already included in the stack iteration above.
    }

    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        // The GC has reset the heap's TLAB cursor (`gen0_next_chunk`) and
        // swapped semispaces, so this VM's cached `alloc_ptr`/`alloc_limit`
        // now point into a region the heap will hand out to other VMs as a
        // fresh chunk. Drop them so the next allocation refills from the
        // post-GC cursor.
        self.tlab.invalidate();

        // Stack values
        for value in &mut self.stack {
            if let Value::Object(ptr) = value {
                if let Some(&new_ptr) = roots.get(ptr) {
                    *ptr = new_ptr;
                }
            }
        }

        // Watch state (last_assigned/last_notified values that aren't on the stack)
        self.watch.forward_roots(roots);

        // Frame function pointers (needed once closures are heap-allocated)
        for frame in &mut self.frames {
            frame.forward_roots(roots);
        }
    }
}
