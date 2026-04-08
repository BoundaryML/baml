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

use ::bex_vm_types::types::ErrorClass;
use ::core::any::TypeId;
use bex_heap::{BexHeap, Tlab};
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, GlobalPool, HeapPtr, Instruction, Object, ObjectIndex, ObjectPool,
    ObjectType, PanicClass, StackIndex, UnaryOp, Value, Variant,
    bytecode::{self, BlockNotification},
    types::{
        Cell, Closure, Function, FunctionType, Future, FutureType, Instance, PendingFuture, Type,
    },
};
use indexmap::IndexMap;

use crate::{
    StackTrace,
    errors::{ErrorLocation, VmBamlError, VmError, VmInternalError, VmPanic, VmRustFnError},
    indexable::{EvalStack, EvalStackTrait},
    package_baml::{BamlPackageBaml, NativeFunction},
    types::ObjectTrait,
    watch::{self, NodeId, RootState, Watch, WatchFilter},
};

/// Max call stack size.
pub const MAX_FRAMES: usize = 256;

/// Call frame.
///
/// This is what gets pushed onto the call stack every time we call a function.
///
/// As with [`Value`], this struct should not own allocated objects (like
/// functions) but instead use references to index into [`BexVm::heap`]. Should
/// be [`Copy`].
#[derive(Clone, Copy, Debug)]
pub(crate) struct Frame {
    /// Pointer to the running function object.
    pub(crate) function: HeapPtr,

    /// Instruction pointer (IP) or program counter (PC).
    ///
    /// Points to the next instruction that the VM will execute.
    pub(crate) instruction_ptr: usize,

    /// Local variables offset in the eval stack.
    pub(crate) locals_offset: StackIndex,
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

        Self {
            frames: Vec::new(),
            stack: EvalStack::new(),
            heap,
            tlab,
            globals,
            resolved_class_names,
            error_class_ptrs,
            panic_class_ptrs,
            watch: Watch::new(),
            watched_vars: HashMap::new(),
            interrupt_frame: None,
            traced_frames: Vec::new(),
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

    /// Collect all `HeapPtr`s stored in call frames (frame function pointers).
    ///
    /// Used by `bex_engine` to include frame roots in GC root sets.
    pub fn collect_frame_roots(&self) -> Vec<HeapPtr> {
        self.frames.iter().map(|f| f.function).collect()
    }

    /// Update frame function pointers according to a GC forwarding map.
    ///
    /// Must be called after a GC cycle to keep frame pointers valid.
    pub fn apply_frame_forwarding(&mut self, forwarding: &HashMap<HeapPtr, HeapPtr>) {
        for frame in &mut self.frames {
            if let Some(&new_ptr) = forwarding.get(&frame.function) {
                frame.function = new_ptr;
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
    /// manages the heap across multiple VM instances.
    pub fn from_program(program: bex_vm_types::Program) -> Result<Self, VmInternalError> {
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

        Ok(Self::new(heap, globals, resolved_class_names))
    }

    /// Bootstraps the VM preparing the given function to run.
    pub fn set_entry_point(&mut self, function: HeapPtr, args: &[Value]) {
        debug_assert!(
            matches!(self.get_object(function), Object::Function(_)),
            "expect function as entry point, got {:?}",
            self.get_object(function)
        );

        self.stack.extend(args.iter().copied());

        self.frames.push(Frame {
            function,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        });

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
        Value::Object(self.tlab.alloc(Object::Type(ty)))
    }

    /// Builds a stack trace for the given error.
    ///
    /// The error is assumed to have happened wherever the instruction pointer
    /// was left at.
    ///
    /// TODO: Not a clean API for the caller, VM should ideally return some kind
    /// of error struct that contains the error and trace and this would not
    /// be needed. That requires some refactoring though.
    pub fn stack_trace(&self, error: VmError) -> StackTrace {
        let trace = self
            .frames
            .iter()
            .map(|frame| {
                let function = self.get_object(frame.function).as_function()?;

                // VM increments instruction pointer as soon as it reads the
                // instruction. So in reality the error ocurred on the previous
                // instruction. The saturating sub is just in case the code has
                // a bug somewhere.
                let last_executed_instruction = frame.instruction_ptr.saturating_sub(1);

                Ok(ErrorLocation {
                    function_name: function.name.clone(),
                    function_span: function.span,
                    error_line: function
                        .bytecode
                        .source_line_for_pc(last_executed_instruction),
                })
            })
            .collect::<Result<Vec<_>, VmError>>()
            .unwrap_or_default();

        StackTrace { error, trace }
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
        self.frames.push(Frame {
            function: function_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(locals_offset),
        });
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
    fn try_unwind_exception(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        exception_value: Value,
    ) -> Result<(), VmError> {
        // Walk the call stack from the current frame outward looking for an
        // exception table entry that covers the faulting PC.
        loop {
            debug_assert!(
                !self.frames.is_empty(),
                "try_unwind_exception called with no frames"
            );
            let depth = self.frames.len() - 1;
            let frame = &self.frames[depth];

            // The frame's instruction_ptr already points to the NEXT instruction
            // (it was incremented before the instruction executed), so the
            // faulting PC is one less.
            debug_assert!(
                frame.instruction_ptr > 0,
                "instruction_ptr should be > 0 after execution"
            );
            let faulting_pc = frame.instruction_ptr - 1;

            // Load the function for this frame to access its exception table.
            // SAFETY: See `load_function` doc comment.
            let frame_function = unsafe { self.load_function(depth)? };

            // Find the first exception table entry covering this PC.
            if let Some(entry) = frame_function
                .bytecode
                .exception_handlers_for_pc(faulting_pc)
                .next()
            {
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

                // Jump to the handler.
                self.frames[depth].instruction_ptr = entry.handler_pc;

                // Update caller's frame_idx / function references.
                *frame_idx = depth;
                *function = frame_function;
                return Ok(());
            }

            // No handler in this frame -- pop it and try the caller.
            if self.frames.len() <= 1 {
                // No more frames to unwind through.
                return Err(VmError::Thrown(exception_value));
            }

            let popped = self.frames.pop().expect("frame stack is not empty");
            self.stack.drain(popped.locals_offset..);

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
            _ => Err(VmInternalError::TypeError {
                expected: expected_type.into(),
                got: ObjectType::of(obj).into(),
            }),
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
        // Resolve the callee: either a plain Function or a Closure wrapping one.
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

                // NOTE: (perf) could use drain(..) instead, or even maintain the arguments
                // reference in the stack, using `swap` to insert the result.
                let args = self.stack[locals_offset..].to_owned();

                // Run Rust native function, converting VmRustFnError → VmError.
                let result = match func(self, &args) {
                    Ok(v) => v,
                    Err(VmRustFnError::Panic(panic)) => {
                        return Err(VmError::Thrown(self.panic_to_exception_value(panic)));
                    }
                    Err(VmRustFnError::BamlError(err)) => {
                        return Err(VmError::Thrown(self.error_to_exception_value(err)));
                    }
                    Err(VmRustFnError::InternalError(err)) => {
                        return Err(VmError::InternalError(err));
                    }
                };

                // Drop function call and place result on top.
                self.stack.drain(locals_offset..);
                self.stack.push(result);
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
                self.frames.push(Frame {
                    function: callee_ptr,
                    instruction_ptr: 0,
                    locals_offset,
                });
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

        Ok(None)
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
        let ptr = self.frames[frame_idx].function;
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
            _ => Err(VmInternalError::TypeError {
                expected: FunctionType::Callable.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    pub fn exec(&mut self) -> Result<VmExecState, VmError> {
        // Grab the last frame from the call stack.
        //
        // Note that [`Frame`] is [`Copy`], so in case the borrow checker
        // complains too much and you can't circumvent it then you can make a
        // local copy of the frame, modify it as needed, and then when we're
        // done with this frame store it back in the vector to persist changes.
        // It's a similar trick to what we've implemented in the cycle detection
        // algorithm. Take a look at the `strong_connect` function in the
        // `tarjan.rs` file.
        // Check if we have frames to execute
        if self.frames.is_empty() {
            return Ok(VmExecState::Complete(Value::Null));
        }

        // Get the frame index (we'll use indexing instead of holding a mutable reference
        // to avoid borrow checker issues). This is mutable so we can update it when
        // pushing new frames during function calls.
        let mut frame_idx = self.frames.len() - 1;

        // SAFETY: See `load_function` doc comment.
        let mut function = unsafe { self.load_function(frame_idx)? };

        loop {
            // Current instruction pointer (read from frame).
            let instruction_ptr = self.frames[frame_idx].instruction_ptr;

            // Move the frame's IP to the next instruction. We'll deal with
            // jump offsets later.
            self.frames[frame_idx].instruction_ptr += 1;

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
            }
        }
    }

    /// Execute a single instruction.
    ///
    /// Returns `Ok(Some(state))` when the VM must yield control flow to the
    /// embedder (await, schedule, complete, notify). Returns `Ok(None)` when
    /// execution should continue to the next instruction.
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
                let slot =
                    Self::local_slot_stack_index(self.frames[*frame_idx].locals_offset, index);
                let value = self.stack[slot];
                self.stack.push(value);
            }

            Instruction::StoreVar(index) => {
                // Absolute index of the local variable.
                let local_var_index =
                    Self::local_slot_stack_index(self.frames[*frame_idx].locals_offset, index);

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
                self.frames[*frame_idx].instruction_ptr = instruction_ptr
                    .checked_add_signed(offset)
                    .ok_or(VmInternalError::InvalidJump)?;
            }

            Instruction::PopJumpIfFalse(offset) => {
                // Pop the condition from the stack (don't leave it there).
                let condition = self.stack.ensure_pop()?;

                match condition {
                    // Reassign only if the condition is false.
                    Value::Bool(value) => {
                        if !value {
                            self.frames[*frame_idx].instruction_ptr = instruction_ptr
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
            }

            Instruction::Throw => {
                let value = self.stack.ensure_pop()?;
                self.try_unwind_exception(frame_idx, function, value)?;
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

                        CmpOp::InstanceOf => {
                            return Err(VmInternalError::CannotApplyCmpOp {
                                left: Type::Int,
                                right: Type::Int,
                                op,
                            }
                            .into());
                        }
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

                        CmpOp::InstanceOf => {
                            return Err(VmInternalError::CannotApplyCmpOp {
                                left: Type::Float,
                                right: Type::Float,
                                op,
                            }
                            .into());
                        }
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

                            CmpOp::InstanceOf => {
                                return Err(VmInternalError::CannotApplyCmpOp {
                                    left: Type::Int,
                                    right: Type::Float,
                                    op,
                                }
                                .into());
                            }
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

                            CmpOp::InstanceOf => {
                                return Err(VmInternalError::CannotApplyCmpOp {
                                    left: Type::Float,
                                    right: Type::Int,
                                    op,
                                }
                                .into());
                            }
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
                            CmpOp::InstanceOf => {
                                return Err(VmInternalError::CannotApplyCmpOp {
                                    left: Type::Object(ObjectType::String),
                                    right: Type::Object(ObjectType::String),
                                    op,
                                }
                                .into());
                            }
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

                        CmpOp::InstanceOf => {
                            // null/non-object is never an instance of anything.
                            match left {
                                Value::Object(left_ptr) => match self.get_object(left_ptr) {
                                    Object::Instance(instance) => {
                                        let right_ptr =
                                            self.as_object_ptr(&right, ObjectType::Class)?;
                                        instance.class == right_ptr
                                    }
                                    _ => false,
                                },
                                _ => false,
                            }
                        }

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

                let local_var_index =
                    Self::local_slot_stack_index(self.frames[*frame_idx].locals_offset, index);
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
            }

            Instruction::Unwatch(index) => {
                let local_var_index =
                    Self::local_slot_stack_index(self.frames[*frame_idx].locals_offset, index);

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
            }

            Instruction::Notify(index) => {
                let local_var_index =
                    Self::local_slot_stack_index(self.frames[*frame_idx].locals_offset, index);
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

            Instruction::Return => {
                // Pop the result from the eval stack.
                let result = self.stack.ensure_pop()?;

                // Check if this frame was traced.
                // Capture function name before popping the frame.
                let span_exit = if self.traced_frames.last() == Some(frame_idx) {
                    let func_name = self
                        .get_object(self.frames[*frame_idx].function)
                        .as_function()
                        .map(|f| f.name.clone())
                        .ok();
                    self.traced_frames.pop();
                    func_name
                } else {
                    None
                };

                // Restore the eval stack to the state before the function
                // was called and leave the result on top.
                self.stack.drain(self.frames[*frame_idx].locals_offset..);
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

                // Resume previous frame execution.
                *frame_idx = self.frames.len() - 1;

                // SAFETY: See `load_function` doc comment.
                *function = unsafe { self.load_function(*frame_idx)? };
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
                self.frames[*frame_idx].instruction_ptr = instruction_ptr
                    .checked_add_signed(offset)
                    .ok_or(VmInternalError::InvalidJump)?;
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

            Instruction::LoadDeref(slot) => {
                let locals_offset = self.frames[*frame_idx].locals_offset;
                let cell_value = self.stack[Self::local_slot_stack_index(locals_offset, slot)];
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
                let locals_offset = self.frames[*frame_idx].locals_offset;
                let cell_value = self.stack[Self::local_slot_stack_index(locals_offset, slot)];
                let Value::Object(cell_ptr) = cell_value else {
                    return Err(VmInternalError::TypeError {
                        expected: ObjectType::Cell.into(),
                        got: self.type_of(&cell_value),
                    }
                    .into());
                };
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
                let closure_ptr = self.frames[*frame_idx].function;
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
                let closure_ptr = self.frames[*frame_idx].function;
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
                let closure_ptr = self.frames[*frame_idx].function;
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
        }

        Ok(None)
    }
}
