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

use bex_heap::{BexHeap, Tlab};
use bex_resource_types::ResourceHandle;
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, GlobalPool, HeapPtr, Instruction, Object, ObjectIndex, ObjectPool,
    ObjectType, StackIndex, UnaryOp, Value, Variant,
    bytecode::{self, BlockNotification},
    types::{FunctionType, Future, FutureType, Instance, PendingFuture, Type},
};
use indexmap::IndexMap;

use crate::{
    NativeFunction, StackTrace,
    errors::{ErrorLocation, InternalError, RuntimeError, VmError},
    indexable::{EvalStack, EvalStackTrait},
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
pub struct Frame {
    /// Pointer to the running function object.
    pub function: HeapPtr,

    /// Instruction pointer (IP) or program counter (PC).
    ///
    /// Points to the next instruction that the VM will execute. It is of type
    /// [`isize`] because some jumps can create negative offsets (for loops)
    /// and it's easier to operate on an [`isize`] and cast it to [`usize`]
    /// only once (when we index into [`bex_vm_types::Bytecode::instructions`]). However,
    /// this number should never be negative, otherwise indexing into the
    /// instruction vec will throw [`InternalError::NegativeInstructionPtr`].
    pub instruction_ptr: isize,

    /// Local variables offset in the eval stack.
    pub locals_offset: StackIndex,
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
    pub frames: Vec<Frame>,

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

    /// Offset of the first runtime allocated object.
    ///
    /// Used by GC to know which objects are compile-time vs runtime.
    pub runtime_allocs_offset: ObjectIndex,

    /// Environment variables available during execution.
    pub env_vars: HashMap<String, String>,

    /// Emit dependency graph.
    pub watch: Watch,

    /// Tracks which local variables are watched (have @watch).
    pub watched_vars: HashMap<StackIndex, (String, String)>,

    pub interrupt_frame: Option<usize>,
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
}

/// Convert a compiled `Program` to a `BytecodeProgram` with native functions attached.
///
/// This is the bridge between compilation output and VM execution. It:
/// 1. Attaches native function implementations to builtin functions
/// 2. Builds resolved name lookups for functions, classes, and enums
pub fn convert_program(program: bex_vm_types::Program) -> Result<BytecodeProgram, VmError> {
    // Convert objects, attaching native functions
    let objects: Vec<Object> = program
        .objects
        .into_iter()
        .map(crate::native::attach_builtins)
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
                resolved_class_names.insert(class.name.clone(), obj_idx);
            }
            Object::Enum(enum_def) => {
                resolved_enums_names.insert(enum_def.name.clone(), obj_idx);
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
                Object::Variant(_) => type_tags::ENUM,
                Object::Array(_) => type_tags::LIST,
                Object::Map(_) => type_tags::MAP,
                Object::Function(_) => type_tags::FUNCTION,
                Object::Future(_) => type_tags::FUTURE,
                Object::Enum(_) => type_tags::ENUM,
                Object::Media(_) => type_tags::MEDIA,
                Object::Resource(_) => type_tags::RESOURCE,
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
    pub fn new(heap: Arc<BexHeap>, globals: GlobalPool, env_vars: HashMap<String, String>) -> Self {
        let runtime_allocs_offset = ObjectIndex::from_raw(heap.compile_time_boundary());
        let tlab = Tlab::new(Arc::clone(&heap));

        Self {
            frames: Vec::new(),
            stack: EvalStack::new(),
            heap,
            tlab,
            globals,
            runtime_allocs_offset,
            env_vars,
            watch: Watch::new(),
            watched_vars: HashMap::new(),
            interrupt_frame: None,
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
    ) -> Result<HeapPtr, InternalError> {
        let Value::Object(ptr) = value else {
            return Err(InternalError::TypeError {
                expected: object_type.into(),
                got: self.type_of(value),
            });
        };
        Ok(*ptr)
    }

    /// Get string from a Value.
    pub fn as_string(&self, value: &Value) -> Result<&String, InternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::String)?;
        self.get_object(ptr).as_string()
    }

    /// Get type of a value.
    pub fn type_of(&self, value: &Value) -> Type {
        Type::of(value, |ptr| ObjectType::of(self.get_object(ptr)))
    }

    /// Get mutable string from a Value.
    pub fn as_string_mut(&mut self, value: &Value) -> Result<&mut String, InternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::String)?;
        self.get_object_mut(ptr).as_string_mut()
    }

    /// Get array from a Value.
    pub fn as_array(&self, value: &Value) -> Result<&[Value], InternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        let obj = self.get_object(ptr);
        match obj {
            Object::Array(arr) => Ok(arr.as_slice()),
            _ => Err(InternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable array from a Value.
    pub fn as_array_mut(&mut self, value: &Value) -> Result<&mut Vec<Value>, InternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(ptr), Object::Array(_)) {
            return Err(InternalError::TypeError {
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
    pub fn as_map(&self, value: &Value) -> Result<&IndexMap<String, Value>, InternalError> {
        let index = self.as_object_ptr(value, ObjectType::Map)?;
        let obj = self.get_object(index);
        match obj {
            Object::Map(map) => Ok(map),
            _ => Err(InternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable map from a Value.
    pub fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<&mut IndexMap<String, Value>, InternalError> {
        let index = self.as_object_ptr(value, ObjectType::Map)?;
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(index), Object::Map(_)) {
            return Err(InternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(self.get_object(index)).into(),
            });
        }
        match self.get_object_mut(index) {
            Object::Map(map) => Ok(map),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get media from a Value.
    pub fn as_media(
        &self,
        value: &Value,
        media_kind: baml_base::MediaKind,
    ) -> Result<&bex_vm_types::types::MediaValue, InternalError> {
        let index = self.as_object_ptr(value, ObjectType::Media(media_kind))?;
        let obj = self.get_object(index);
        match obj {
            Object::Media(media) => Ok(media),
            _ => Err(InternalError::TypeError {
                expected: ObjectType::Media(media_kind).into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable media from a Value (not currently used but required by macro).
    #[allow(dead_code)]
    pub fn as_media_mut(
        &mut self,
        value: &Value,
        media_kind: baml_base::MediaKind,
    ) -> Result<&mut bex_vm_types::types::MediaValue, InternalError> {
        let index = self.as_object_ptr(value, ObjectType::Media(media_kind))?;
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(index), Object::Media(_)) {
            return Err(InternalError::TypeError {
                expected: ObjectType::Media(media_kind).into(),
                got: ObjectType::of(self.get_object(index)).into(),
            });
        }
        match self.get_object_mut(index) {
            Object::Media(media) => Ok(media),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get Value reference (for generic types).
    #[allow(dead_code)]
    pub fn as_value_mut(&mut self, value: &Value) -> Result<&mut Value, InternalError> {
        // This is used by macro-generated code for generic type parameters.
        // For now, we don't support mutable access to generic values.
        let Value::Object(ptr) = value else {
            return Err(InternalError::InvalidObjectRef(0));
        };
        Err(InternalError::InvalidObjectRef(ptr.as_ptr() as usize))
    }

    /// TODO: We should remove this API in favor of using `bex_engine` only (vbv)
    /// Creates a VM from a compiled [`bex_vm_types::Program`].
    ///
    /// This is primarily for testing. In production, use `BexEngine` which
    /// manages the heap across multiple VM instances.
    pub fn from_program(program: bex_vm_types::Program) -> Result<Self, VmError> {
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

        Ok(Self::new(heap, globals, HashMap::new()))
    }

    /// Bootstraps the VM preparing the given function to run.
    #[allow(clippy::print_stderr)] // intentional debug warning for developer feedback
    pub fn set_entry_point(&mut self, function: HeapPtr, args: &[Value]) {
        debug_assert!(
            matches!(self.get_object(function), Object::Function(_)),
            "expect function as entry point, got {:?}",
            self.get_object(function)
        );

        // TODO: Run collect_garbage in codegen after each function call.
        if self.heap.len() != self.runtime_allocs_offset.into_raw() {
            eprintln!("WARNING: garbage collection did not run before setting a new entry point");
        }

        self.stack.push(Value::Object(function));
        self.stack.extend(args.iter().copied());

        self.frames.push(Frame {
            function,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
        });
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
    /// Returns [`InternalError::TypeError`] if the future is not pending, or not a future.
    pub fn pending_future(&self, future_ptr: HeapPtr) -> Result<&PendingFuture, InternalError> {
        match self.get_object(future_ptr) {
            Object::Future(Future::Pending(future)) => Ok(future),
            other => Err(InternalError::TypeError {
                expected: FutureType::Pending.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    pub fn fulfil_future(
        &mut self,
        future_ptr: HeapPtr,
        value: Value,
    ) -> Result<(), InternalError> {
        let Object::Future(future) = self.get_object_mut(future_ptr) else {
            return Err(InternalError::TypeError {
                expected: FutureType::Any.into(),
                got: ObjectType::of(self.get_object(future_ptr)).into(),
            });
        };

        *future = Future::Ready(value);

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

    /// Allocate a resource object on the heap.
    pub fn alloc_resource(&mut self, resource: ResourceHandle) -> Value {
        Value::Object(self.tlab.alloc(Object::Resource(resource)))
    }

    // pub fn alloc_media(&mut self, media: BamlMedia) -> Value {
    //     Value::Object(self.objects.insert(Object::Media(media)))
    // }

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
                let function = self.get_object(frame.function).as_function()?.clone();

                // VM increments instruction pointer as soon as it reads the
                // instruction. So in reality the error ocurred on the previous
                // instruction. The saturating sub is just in case the code has
                // a bug somewhere.
                let last_executed_instruction = frame.instruction_ptr.saturating_sub(1);

                #[allow(clippy::cast_sign_loss)] // instruction_ptr is always non-negative
                Ok(ErrorLocation {
                    function_name: function.name.clone(),
                    function_span: function.span,
                    error_line: function.bytecode.source_lines[last_executed_instruction as usize],
                })
            })
            .collect::<Result<Vec<_>, VmError>>()
            .map_err(|e| {
                RuntimeError::Other(format!(
                    "internal error: Vm::stack_trace() failed to build stack trace: {e}\n\noriginal error: {error}"
                ))
            })
            .unwrap_or_default();

        StackTrace { error, trace }
    }

    /// Stops the execution of the current bytecode in favor of the given
    /// function
    ///
    /// When the new control flow ends (given functions pops from the stack)
    /// then the previosly running bytecode resumes execution.
    fn interrupt(&mut self, function_ptr: HeapPtr, args: &[Value]) -> Result<VmExecState, VmError> {
        if !matches!(self.get_object(function_ptr), Object::Function(_)) {
            return Err(RuntimeError::Other("Invalid interrupt function".to_string()).into());
        }

        // Index of the frame that starts the interrupt code.
        self.interrupt_frame = Some(self.frames.len());

        let locals_offset = self.stack.len();

        // Params.
        self.stack.push(Value::Object(function_ptr));
        self.stack.extend(args.iter().copied());

        // Push the new frame.
        self.frames.push(Frame {
            function: function_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(locals_offset),
        });

        // Execute the interrupt code and return the result.
        self.exec()
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

                    match crate::native::baml_deep_equals(self, &[last_assigned, state.value]) {
                        Ok(Value::Bool(b)) => {
                            if !b {
                                filtered_notifications.push(notification);
                            }
                        }

                        other => {
                            return Err(RuntimeError::Other(format!(
                                "Invalid deep equals result during watch: {other:?}"
                            ))
                            .into());
                        }
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

                        other => {
                            return Err(RuntimeError::Other(format!(
                                "Invalid filter function return: {other:?}"
                            ))
                            .into());
                        }
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
    ) -> Result<(), VmError> {
        if let Value::Object(old) = old_value {
            self.watch
                .unlink_edge(watched_node, path.clone(), NodeId::HeapObject(old));
        }

        if let Value::Object(new) = new_value {
            // Track dependencies first (using closure to avoid borrow conflicts)
            watch::track_watch_dependencies(&mut self.watch, Value::Object(new));
            // Then link the edge
            self.watch
                .link_edge(watched_node, path, NodeId::HeapObject(new));
        }

        // Copy previous values.
        let mut old_roots_copies = vec![];

        for root in self.watch.copy_roots_reaching(watched_node) {
            if let Some(state) = self.watch.root_state(root) {
                let deep_copy = crate::native::baml_deep_copy(self, &[state.value])?;
                old_roots_copies.push(deep_copy);
            }
        }

        for (root, old_value) in self
            .watch
            .copy_roots_reaching(watched_node)
            .iter()
            .zip(old_roots_copies)
        {
            if let Some(state) = self.watch.root_state_mut(*root) {
                state.last_assigned = Some(old_value);
                // current value has not really changes, top level object is the same.
            }
        }

        Ok(())
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

        // Clone the function object to avoid borrow checker issues with the new heap architecture.
        // We clone because we need to access the function's bytecode throughout the loop
        // while also mutating self (stack, frames, etc.). This is a trade-off for the
        // unified heap architecture - the cost of cloning is acceptable for now.
        let function_obj_idx = self.frames[frame_idx].function;
        let mut function = self.get_object(function_obj_idx).as_function()?.clone();

        loop {
            // Current instruction pointer (read from frame).
            let instruction_ptr = self.frames[frame_idx].instruction_ptr;

            // Move the frame's IP to the next instruction. We'll deal with
            // jump offsets later.
            self.frames[frame_idx].instruction_ptr += 1;

            // NOTE: `core::intrinsics::unlikely` is only available on nightly.
            // This branch is a big annoyance for small functions (like pushing the frame)
            // and gets smaller the bigger the function due to branch (mis)prediction.
            if instruction_ptr < 0 {
                return Err(InternalError::NegativeInstructionPtr(instruction_ptr).into());
            }

            // Runtime debugging information.
            // #[cfg(debug_assertions)]
            // {
            //     let stack = self
            //         .stack
            //         .iter()
            //         .map(|v| crate::debug::display_value(v, &self.objects))
            //         .collect::<Vec<_>>()
            //         .join(", ");

            //     eprintln!("[{stack}]");

            //     let (instruction, metadata) = crate::debug::display_instruction(
            //         instruction_ptr,
            //         function,
            //         &self.stack,
            //         &self.objects,
            //         &self.globals,
            //     );

            //     eprintln!("{instruction} {metadata}");
            // }

            #[allow(clippy::cast_sign_loss)] // instruction_ptr is validated non-negative above
            match function.bytecode.instructions[instruction_ptr as usize] {
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

                    return Ok(VmExecState::Notify(WatchNotification::Block(
                        full_notification,
                    )));
                }
                Instruction::VizEnter(index) | Instruction::VizExit(index) => {
                    let instruction = &function.bytecode.instructions[instruction_ptr as usize];
                    let delta = match instruction {
                        Instruction::VizEnter(_) => bytecode::VizExecDelta::Enter,
                        Instruction::VizExit(_) => bytecode::VizExecDelta::Exit,
                        _ => unreachable!("matched on viz instruction"),
                    };

                    let node = function.viz_nodes.get(index).ok_or({
                        InternalError::ArrayIndexOutOfBounds {
                            index,
                            length: function.viz_nodes.len(),
                        }
                    })?;

                    let event = bytecode::VizExecEvent {
                        delta,
                        node_id: node.node_id,
                        node_type: node.node_type,
                        label: node.label.clone(),
                        header_level: node.header_level,
                    };

                    return Ok(VmExecState::Notify(WatchNotification::Viz {
                        function_name: function.name.clone(),
                        event,
                    }));
                }
                Instruction::LoadConst(index) => {
                    // Use pre-resolved constants (resolved at load time)
                    let value = function.bytecode.resolved_constants[index];
                    self.stack.push(value);
                }

                Instruction::LoadVar(index) => {
                    let value = self.stack[self.frames[frame_idx].locals_offset + index];
                    self.stack.push(value);
                }

                Instruction::StoreVar(index) => {
                    // Absolute index of the local variable.
                    let local_var_index = self.frames[frame_idx].locals_offset + index;

                    // New value.
                    let value = self.stack.ensure_pop()?;

                    // Old value being replaced.
                    let old_value = std::mem::replace(&mut self.stack[local_var_index], value);

                    // Check if this binding is emittable.
                    if self.watched_vars.contains_key(&local_var_index) {
                        // Node ID of the local variable in the emit graph.
                        let watched_node = NodeId::LocalVar(local_var_index);

                        // If we had a previous binding to an object, unlink it
                        // so it doesn't emit anymore.
                        if let Value::Object(old_node) = old_value {
                            self.watch.unlink_edge(
                                watched_node,
                                watch::Path::Binding,
                                NodeId::HeapObject(old_node),
                            );
                        }

                        // If we have a new binding, link it so it emits.
                        if let Value::Object(new_node) = value {
                            // Track dependencies first (using closure to avoid borrow conflicts)
                            watch::track_watch_dependencies(&mut self.watch, value);
                            // Then link the edge
                            self.watch.link_edge(
                                watched_node,
                                watch::Path::Binding,
                                NodeId::HeapObject(new_node),
                            );
                        }

                        let old_value_deep_copy =
                            crate::native::baml_deep_copy(self, &[old_value])?;

                        if let Some(state) = self.watch.root_state_mut(watched_node) {
                            state.last_assigned = Some(old_value_deep_copy);
                            state.value = value;
                        }

                        let notifications = self.process_notifications(watched_node)?;

                        // borrow checker - update frame index and function.
                        function = self
                            .get_object(self.frames[frame_idx].function)
                            .as_function()?
                            .clone();

                        if !notifications.is_empty() {
                            return Ok(VmExecState::Notify(WatchNotification::Variables(
                                notifications,
                            )));
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
                            return Err(InternalError::TypeError {
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
                    let instance_index =
                        self.as_object_ptr(&instance_value, ObjectType::Instance)?;

                    // Read old value (and typecheck).
                    let old_value = match self.get_object(instance_index) {
                        Object::Instance(instance) => instance.fields[index],

                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology.
                    let watched_node = NodeId::HeapObject(instance_index);

                    self.update_watched_node(
                        watched_node,
                        watch::Path::InstanceField(index),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Instance(instance) = self.get_object_mut(instance_index) {
                        instance.fields[index] = new_value;
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // Borrow checker - update function.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
                    }
                }

                Instruction::Pop(n) => {
                    let drain_start = self.stack.len() - n;
                    let drain_range = StackIndex::from_raw(drain_start)..;

                    // Check if any of the popped variables are emittable and
                    // unregister them
                    for i in drain_start..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going
                            // out of scope.
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable.
                            if let Value::Object(obj) = self.stack[index] {
                                self.watch.unlink_edge(
                                    var_node,
                                    watch::Path::Binding,
                                    NodeId::HeapObject(obj),
                                );
                            }
                        }
                    }

                    self.stack.drain(drain_range);
                }

                Instruction::Copy(offset) => {
                    let index = self.stack.ensure_slot_from_top(offset)?;
                    let value = self.stack[index];
                    self.stack.push(value);
                }

                Instruction::PopReplace(n) => {
                    let value = self.stack.ensure_pop()?;

                    // Pop the last `n` locals from the stack.
                    let drain_start = self.stack.len() - n;

                    // Clean up any emittable variables in the range being
                    // popped.
                    for i in drain_start..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going out of scope
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable
                            if let Value::Object(obj) = self.stack[index] {
                                self.watch.unlink_edge(
                                    var_node,
                                    watch::Path::Binding,
                                    NodeId::HeapObject(obj),
                                );
                            }
                        }
                    }

                    let drain_range = StackIndex::from_raw(drain_start)..;
                    self.stack.drain(drain_range);

                    // Push the value back on top of the stack.
                    self.stack.push(value);
                }

                Instruction::Jump(offset) => {
                    // Reassign the frame's IP to the new instruction.
                    // Remember that offset can be negative here, so even though
                    // we're adding it can still jump backwards.
                    self.frames[frame_idx].instruction_ptr = instruction_ptr + offset;
                }

                Instruction::PopJumpIfFalse(offset) => {
                    // Pop the condition from the stack (don't leave it there).
                    let condition = self.stack.ensure_pop()?;

                    match condition {
                        // Reassign only if the condition is false.
                        Value::Bool(value) => {
                            if !value {
                                self.frames[frame_idx].instruction_ptr = instruction_ptr + offset;
                            }
                        }

                        // Type error, we don't have "falsey" values in the language
                        // so we should always check booleans.
                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Bool,
                                got: self.type_of(&other),
                            }));
                        }
                    }
                }

                Instruction::BinOp(op) => {
                    let right = self.stack.ensure_pop()?;
                    let left = self.stack.ensure_pop()?;

                    let result = match (left, right) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(match op {
                            BinOp::Div if right == 0 => {
                                return Err(RuntimeError::DivisionByZero {
                                    left: Value::Int(left),
                                    right: Value::Int(right),
                                }
                                .into());
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
                                    return Err(RuntimeError::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    }
                                    .into());
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
                                    return Err(VmError::from(InternalError::CannotApplyBinOp {
                                        left: Type::Float,
                                        right: Type::Float,
                                        op,
                                    }));
                                }
                            })
                        }

                        (Value::Object(_), Value::Object(_)) if op == BinOp::Add => {
                            let left = self.as_string(&left)?;
                            let right = self.as_string(&right)?;

                            let mut concat = left.clone();
                            concat.push_str(right);

                            let concat_str_object = self.alloc_string(concat);

                            // Borrow check.
                            function = self
                                .get_object(self.frames[frame_idx].function)
                                .as_function()?
                                .clone();

                            concat_str_object
                        }

                        _ => {
                            return Err(VmError::from(InternalError::CannotApplyBinOp {
                                left: self.type_of(&left),
                                right: self.type_of(&right),
                                op,
                            }));
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
                                return Err(InternalError::CannotApplyCmpOp {
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
                                return Err(InternalError::CannotApplyCmpOp {
                                    left: Type::Float,
                                    right: Type::Float,
                                    op,
                                }
                                .into());
                            }
                        }),

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
                                    return Err(InternalError::CannotApplyCmpOp {
                                        left: Type::Object(ObjectType::String),
                                        right: Type::Object(ObjectType::String),
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
                                    left_var.enm == right_var.enm
                                        && left_var.index == right_var.index
                                }
                                CmpOp::NotEq => {
                                    left_var.enm != right_var.enm
                                        || left_var.index != right_var.index
                                }
                                _ => {
                                    return Err(InternalError::CannotApplyCmpOp {
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
                                let left = self.as_object_ptr(&left, ObjectType::Instance)?;

                                let Object::Instance(instance) = self.get_object(left) else {
                                    return Err(InternalError::TypeError {
                                        expected: ObjectType::Instance.into(),
                                        got: ObjectType::of(self.get_object(left)).into(),
                                    }
                                    .into());
                                };

                                let right = self.as_object_ptr(&right, ObjectType::Class)?;

                                instance.class == right
                            }

                            _ => {
                                return Err(VmError::from(InternalError::CannotApplyCmpOp {
                                    left: self.type_of(&left),
                                    right: self.type_of(&right),
                                    op,
                                }));
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
                            return Err(VmError::from(InternalError::CannotApplyUnaryOp {
                                op,
                                value: self.type_of(&value),
                            }));
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

                    // objects.push() above might've reallocated the vector so
                    // borrow checker complains. Restore the reference.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();
                }

                Instruction::LoadArrayElement => {
                    // Stack should contain [array, index]
                    // Pop the index first, then the array
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;

                    let array_obj_index = self.as_object_ptr(&array_value, ObjectType::Array)?;

                    // Get the index
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    // bounds checked below
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        _ => {
                            return Err(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.type_of(&index_value),
                            }
                            .into());
                        }
                    };

                    // Extract the array element before pushing to stack
                    let element = {
                        let Object::Array(array) = self.get_object(array_obj_index) else {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(self.get_object(array_obj_index)).into(),
                            }));
                        };

                        // Check bounds
                        if index >= array.len() {
                            return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                                index,
                                length: array.len(),
                            }));
                        }

                        array[index]
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
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(self.get_object(map_index)).into(),
                        }));
                    };

                    // Get the string key from the objects pool
                    let key_index = self.as_object_ptr(&key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?;

                    // Look up the value in the map
                    let value = map.get(key).copied().ok_or(RuntimeError::NoSuchKeyInMap)?;

                    // Push the value onto the stack
                    self.stack.push(value);
                }

                Instruction::StoreArrayElement => {
                    // Instruction args.
                    let new_value = self.stack.ensure_pop()?;
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;
                    let array_object_index = self.as_object_ptr(&array_value, ObjectType::Array)?;

                    // Verify index.
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    // bounds checked below
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.type_of(&other),
                            }));
                        }
                    };

                    // Read old value (and typecheck).
                    let old_value = match self.get_object(array_object_index) {
                        Object::Array(array) => {
                            // Check bounds.
                            if index >= array.len() {
                                return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                                    index,
                                    length: array.len(),
                                }));
                            }

                            array[index]
                        }

                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology
                    let watched_node = NodeId::HeapObject(array_object_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::ArrayIndex(index),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Array(array) = self.get_object_mut(array_object_index) {
                        array[index] = new_value;
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // borrow checker.
                    // No need to reassign frame since we're using frame_idx
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
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
                            return Err(VmError::from(InternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: ObjectType::of(other).into(),
                            }));
                        }
                    };

                    // Change graph topology
                    let watched_node = NodeId::HeapObject(map_index);

                    self.update_watched_node(
                        watched_node,
                        watch::Path::MapKey(key.clone()),
                        old_value,
                        new_value,
                    )?;

                    // Set the new value.
                    if let Object::Map(map) = self.get_object_mut(map_index) {
                        map.insert(key, new_value);
                    }

                    let notifications = self.process_notifications(watched_node)?;

                    // borrow checker.
                    // No need to reassign frame since we're using frame_idx
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();

                    if !notifications.is_empty() {
                        return Ok(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        )));
                    }
                }

                Instruction::AllocInstance(index) => {
                    // Convert compile-time ObjectIndex to HeapPtr
                    let class_ptr = self.idx_to_ptr(index);
                    let Object::Class(class) = self.get_object(class_ptr) else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Class.into(),
                            got: ObjectType::of(self.get_object(class_ptr)).into(),
                        }
                        .into());
                    };

                    // Allocate the fields.
                    let mut fields = Vec::with_capacity(class.field_names.len());
                    fields.resize(class.field_names.len(), Value::Null);

                    // Allocate an instance of the class.
                    let instance_ptr = self.tlab.alloc(Object::Instance(Instance {
                        class: class_ptr,
                        fields,
                    }));

                    // Push the instance object on top of the stack.
                    self.stack.push(Value::Object(instance_ptr));

                    // borrow check.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();
                }

                // TODO: Contains a lot of typechecking, we know at compile time
                // that all this stuff is right. Should do something about it.
                Instruction::AllocVariant(enum_index) => {
                    // Convert compile-time ObjectIndex to HeapPtr
                    let enum_ptr = self.idx_to_ptr(enum_index);
                    // Extract the variant count before popping from stack to avoid borrow conflicts
                    let variant_count = {
                        let Object::Enum(enm) = self.get_object(enum_ptr) else {
                            return Err(InternalError::TypeError {
                                expected: ObjectType::Enum.into(),
                                got: ObjectType::of(self.get_object(enum_ptr)).into(),
                            }
                            .into());
                        };
                        enm.variant_names.len()
                    };

                    let variant = self.stack.ensure_pop()?;

                    let Value::Int(variant_index) = variant else {
                        return Err(InternalError::TypeError {
                            expected: Type::Int,
                            got: self.type_of(&variant),
                        }
                        .into());
                    };

                    if variant_index < 0 {
                        return Err(InternalError::ArrayIndexIsNegative(variant_index).into());
                    }

                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    // checked non-negative above
                    let variant_usize = variant_index as usize;

                    if variant_usize >= variant_count {
                        return Err(InternalError::ArrayIndexOutOfBounds {
                            index: variant_usize,
                            length: variant_count,
                        }
                        .into());
                    }

                    let variant_ptr = self.tlab.alloc(Object::Variant(Variant {
                        enm: enum_ptr,
                        index: variant_usize,
                    }));

                    // Push the variant object on top of the stack.
                    self.stack.push(Value::Object(variant_ptr));

                    // borrow check.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();
                }

                Instruction::DispatchFuture(arg_count) => {
                    let args_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    let expected_type = FunctionType::External;

                    let index =
                        self.as_object_ptr(&self.stack[args_offset], expected_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callable_future) = self.get_object(index) else {
                        return Err(InternalError::TypeError {
                            expected: expected_type.into(),
                            got: ObjectType::of(self.get_object(index)).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != callable_future.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: callable_future.arity,
                            got: arg_count,
                        }));
                    }

                    // Must be an external operation - extract the ExternalOp.
                    let FunctionKind::External(external_op) = callable_future.kind else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: FunctionType::External.into(),
                            got: FunctionType::from(&callable_future.kind).into(),
                        }));
                    };

                    // Collect the function call args and cleanup the call.
                    let future_args: Vec<Value> = self.stack.drain(args_offset..).skip(1).collect();

                    // Create the pending future with the ExternalOp enum.
                    let pending_future = PendingFuture {
                        operation: external_op,
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
                    return Ok(VmExecState::ScheduleFuture(object_index));
                }

                Instruction::Await => {
                    let value = self.stack.ensure_stack_top()?;

                    let wanted_type = FutureType::Any;

                    let index = self.as_object_ptr(&self.stack[value], wanted_type.into())?;

                    // Check if future is ready and extract value if so
                    let ready_value = {
                        let Object::Future(awaiting) = self.get_object(index) else {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: wanted_type.into(),
                                got: ObjectType::of(self.get_object(index)).into(),
                            }));
                        };

                        match awaiting {
                            // Can't do nothing, handle control flow back to embedder.
                            Future::Pending(_) => {
                                return Ok(VmExecState::Await(index));
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
                                return Err(
                                    RuntimeError::Other("Invalid filter".to_string()).into()
                                );
                            }
                        },
                        _ => return Err(RuntimeError::Other("Invalid filter".to_string()).into()),
                    };

                    // Consume channel.
                    let channel_value = self.stack.ensure_pop()?;
                    let channel = self.as_string(&channel_value)?.to_owned();

                    let local_var_index =
                        StackIndex::from_raw(self.frames[frame_idx].locals_offset.raw() + index);
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

                    #[allow(clippy::cast_sign_loss)] // instruction_ptr is validated non-negative
                    let watched_var_name = &function.locals_in_scope
                        [function.bytecode.scopes[instruction_ptr as usize]][index];
                    // Track this so we can unregister on scope exit
                    self.watched_vars.insert(
                        local_var_index,
                        (watched_var_name.clone(), function.name.clone()),
                    );

                    // If it's an object, build the entire dependency graph
                    if let Value::Object(object_index) = value {
                        // Build the graph.
                        // Track dependencies first (using closure to avoid borrow conflicts)
                        watch::track_watch_dependencies(&mut self.watch, value);

                        // Link the root emittable variable to the object
                        self.watch.link_edge(
                            var_node,
                            watch::Path::Binding,
                            NodeId::HeapObject(object_index),
                        );
                    }
                }

                Instruction::Unwatch(index) => {
                    let local_var_index =
                        StackIndex::from_raw(self.frames[frame_idx].locals_offset.raw() + index);

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
                        StackIndex::from_raw(self.frames[frame_idx].locals_offset.raw() + index);
                    let var_node = NodeId::LocalVar(local_var_index);

                    let notifications = self.watch.copy_roots_reaching(var_node);

                    if notifications.len() != 1 && notifications.first() != Some(&var_node) {
                        return Err(RuntimeError::Other("Invalid manual notify".to_string()).into());
                    }

                    return Ok(VmExecState::Notify(WatchNotification::Variables(
                        notifications,
                    )));
                }

                Instruction::Call(arg_count) => {
                    // Function calls are pushed onto the stack like this:
                    //
                    // [callee, arg1, arg2, ..., argN]
                    //
                    // The callee is a ref to the function object, and after
                    // that we have all the function arguments. `arg_count` is
                    // the number of arguments pushed after the callee.
                    //
                    // That's how we compute the relative offset of the callee
                    // and it's local args in the stack.
                    let locals_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    // Get the function object from the stack.
                    let local = &self.stack[locals_offset];

                    let function_type = FunctionType::Callable;

                    let index = self.as_object_ptr(local, function_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callee) = self.get_object(index) else {
                        return Err(InternalError::TypeError {
                            expected: function_type.into(),
                            got: ObjectType::of(self.get_object(index)).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != callee.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: callee.arity,
                            got: arg_count,
                        }));
                    }

                    // Check if we've reached the max call stack size.
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
                    }

                    match callee.kind {
                        FunctionKind::Native(func_ptr) => {
                            // Cast the type-erased pointer back to NativeFunction.
                            //
                            // SAFETY: The pointer was created by casting a NativeFunction to *const ()
                            // in attach_builtins, so it's safe to cast it back. We use transmute
                            // because Rust doesn't allow `as` casts from *const () to fn pointers.
                            // The explicit type parameters document exactly what we're doing.
                            let func = unsafe {
                                std::mem::transmute::<*const (), NativeFunction>(func_ptr)
                            };

                            // NOTE: (perf) could use drain(..) instead, or even maintain the arguments
                            // reference in the stack, using `swap` to insert the result.
                            let args = self.stack
                                [StackIndex::from_raw(locals_offset.into_raw() + 1)..]
                                .to_owned();

                            // Run Rust native function.
                            let result = func(self, &args)?;

                            // Drop function call and place result on top.
                            self.stack.drain(locals_offset..);
                            self.stack.push(result);

                            // No need to update frame_idx since we didn't push a new frame.
                            // Just update the function reference.
                            function = self
                                .get_object(self.frames[frame_idx].function)
                                .as_function()?
                                .clone();
                        }

                        FunctionKind::Bytecode => {
                            // Push the new frame.
                            self.frames.push(Frame {
                                function: index,
                                instruction_ptr: 0,
                                locals_offset,
                            });

                            // Update frame_idx to point to the new frame.
                            frame_idx = self.frames.len() - 1;

                            // Grab function ref. We do this to avoid running this
                            // code at the beginning of each iteration since it's
                            // totally unnecessary. The function only changes when the
                            // frame changes.
                            function = self
                                .get_object(self.frames[frame_idx].function)
                                .as_function()?
                                .clone();
                        }

                        FunctionKind::External(_) => {
                            return Err(InternalError::TypeError {
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
                }

                Instruction::Return => {
                    // Pop the result from the eval stack.
                    let result = self.stack.ensure_pop()?;

                    // Clean up any emittable variables in the function's scope
                    for i in self.frames[frame_idx].locals_offset.into_raw()..self.stack.len() {
                        let index = StackIndex::from_raw(i);
                        if self.watched_vars.remove(&index).is_some() {
                            let var_node = NodeId::LocalVar(index);

                            // Unregister the root since the variable is going out of scope
                            self.watch.unregister_root(var_node);

                            // Also unlink any edge from this variable
                            if i < self.stack.len() {
                                if let Value::Object(obj) = self.stack[index] {
                                    self.watch.unlink_edge(
                                        var_node,
                                        watch::Path::Binding,
                                        NodeId::HeapObject(obj),
                                    );
                                }
                            }
                        }
                    }

                    // Restore the eval stack to the state before the function
                    // was called and leave the result on top.
                    self.stack.drain(self.frames[frame_idx].locals_offset..);
                    self.stack.push(result);

                    // Pop from the call stack.
                    self.frames.pop();

                    // Return from interrupt.
                    if Some(self.frames.len()) == self.interrupt_frame {
                        self.interrupt_frame = None;
                        return self
                            .stack
                            .ensure_pop()
                            .map(VmExecState::Complete)
                            .map_err(Into::into);
                    }

                    // If there are no more frames, we're done.
                    if self.frames.is_empty() {
                        return self
                            .stack
                            .ensure_pop()
                            .map(VmExecState::Complete)
                            .map_err(Into::into);
                    }

                    // Resume previous frame execution.
                    frame_idx = self.frames.len() - 1;

                    // Point to the previous frame's function. Read the
                    // implementation of `Instruction::Call` above this one for
                    // more information about this piece.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();
                }

                Instruction::Assert => {
                    let value = self.stack.pop().ok_or(RuntimeError::AssertionError)?;

                    let Value::Bool(condition_result) = value else {
                        return Err(InternalError::TypeError {
                            expected: Type::Bool,
                            got: self.type_of(&value),
                        }
                        .into());
                    };

                    if !condition_result {
                        return Err(RuntimeError::AssertionError.into());
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

                    // borrow check.
                    function = self
                        .get_object(self.frames[frame_idx].function)
                        .as_function()?
                        .clone();
                }

                // ============================================================
                // Jump Table Instructions
                // ============================================================
                Instruction::JumpTable { table_idx, default } => {
                    // Pop discriminant from stack
                    let discriminant = self.stack.ensure_pop()?;

                    // Must be an integer
                    let Value::Int(value) = discriminant else {
                        return Err(InternalError::TypeError {
                            expected: Type::Int,
                            got: self.type_of(&discriminant),
                        }
                        .into());
                    };

                    // Lookup in jump table
                    let table = &function.bytecode.jump_tables[table_idx];
                    let offset = table.lookup(value).unwrap_or(default);

                    // Jump
                    self.frames[frame_idx].instruction_ptr = instruction_ptr + offset;
                }

                Instruction::Discriminant => {
                    // Pop value from stack
                    let value = self.stack.ensure_pop()?;

                    // Must be an object (variants are heap-allocated)
                    let Value::Object(object_idx) = value else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Variant.into(),
                            got: self.type_of(&value),
                        }
                        .into());
                    };

                    // Must be a Variant object
                    let variant_index = {
                        let Object::Variant(variant) = self.get_object(object_idx) else {
                            return Err(InternalError::TypeError {
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

                Instruction::Unreachable => {
                    // This instruction should never be executed. If we reach it,
                    // there's a bug in the compiler or type system.
                    return Err(RuntimeError::Unreachable.into());
                }
            }
        }
    }
}
