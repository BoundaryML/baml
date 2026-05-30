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

use smallvec::SmallVec;

/// Branch hint: tells the compiler this condition is almost never true.
/// Used on the cold side of `if unlikely(cond) { ... }` in the dispatch
/// loop's hot path — measurably faster than letting the compiler guess
/// for checks like "any watch installed?" that fire on every variable
/// store but are virtually always false outside of a debug session.
#[allow(clippy::inline_always)]
#[inline(always)]
#[cold]
fn cold() {}

#[allow(clippy::inline_always)]
#[inline(always)]
fn unlikely(b: bool) -> bool {
    if b {
        cold();
    }
    b
}

/// `unreachable_unchecked()` guarded by a debug-build check.
///
/// Specialized opcodes and frame dispatch rely on the bytecode verifier /
/// type-directed specialization guaranteeing the matched-on operand or frame
/// type, so these branches are dead. In debug/test builds this `unreachable!()`s
/// (surfacing a specialization or codegen bug instead of silent UB); in release
/// it elides the check via `unreachable_unchecked()`.
macro_rules! verifier_unreachable {
    () => {
        if cfg!(debug_assertions) {
            unreachable!(
                "VM verifier invariant violated — a specialization/codegen bug let an \
                 unexpected operand or frame type reach an unchecked branch"
            )
        } else {
            // SAFETY: guaranteed unreachable by the bytecode verifier / type-directed
            // opcode specialization (see the matched-on type at the call site).
            unsafe { ::std::hint::unreachable_unchecked() }
        }
    };
}

use ::bex_heap::TlabHolder;

pub type InterfaceImplementorEntry = (baml_type::TypeName, Vec<baml_type::Ty>);
pub type InterfaceImplementors = indexmap::IndexMap<baml_type::TypeName, Vec<InterfaceImplementorEntry>>;
use ::bex_vm_types::{
    EarlyYieldCheck, RootHaver,
    types::{ErrorClass, FutureId},
};
use ::core::any::TypeId;
#[cfg(not(target_arch = "wasm32"))]
use ::core::sync::atomic::AtomicBool;
use bex_heap::{BexHeap, Tlab};
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, FutureRead, GlobalIndex, HeapPtr, Object, ObjectIndex, ObjectPool,
    ObjectType, PanicClass, PermitProof, StackIndex, UnaryOp, Value, Variant, VmGlobals,
    bytecode::{self, BlockNotification, Instruction},
    types::{
        BoundMethod, Closure, ConstValue, Function, FunctionOrigin, FunctionType, Instance, Type,
        UnscheduledFuture,
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
#[derive(Clone, Debug)]
pub struct BytecodeFrame {
    /// Pointer to the running function (or closure) object.
    pub function: HeapPtr,
    /// Instruction pointer (IP). Points to the next instruction.
    pub instruction_ptr: usize,
    /// Local variables offset in the eval stack.
    pub(crate) locals_offset: StackIndex,
    /// Resolved type arguments for this call frame.
    ///
    /// Populated by the `Call { ntypeargs }` instruction when the callee is
    /// generic.  Empty for non-generic calls.  Used by the `LoadType`
    /// instruction to substitute `TypeArgRef(n)` leaves in a `TyTemplate`.
    pub type_args: Vec<baml_type::Ty>,
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
pub struct NativeFrame {
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
pub enum Frame {
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

#[cfg(test)]
mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::atomic::AtomicBool;
    use std::{collections::HashMap, sync::Arc};

    use baml_type::{Name, Ty, TyAttr, TyTemplate, TypeName};
    use bex_heap::{BexHeap, CollectionLevel, Tlab};
    use bex_vm_types::{
        EarlyYieldCheck, FunctionKind, GlobalPool, HeapPtr, Object, ObjectIndex, RootHaver, Value,
        ValueKind, VmGlobals,
        bytecode::{Bytecode, FieldCopy, FieldCopySet},
        types::{Class, ClassField, Function, FunctionOrigin, Instance, type_tags},
    };

    use super::{BexVm, Frame, VmExecState, WatchNotification, value_type_tag};
    use crate::{
        indexable::EvalStack,
        package_baml::{NativeCallResult, NativeFunction},
        watch::{NodeId, RootState, Watch, WatchFilter},
    };

    fn int_ty() -> Ty {
        Ty::Int {
            attr: TyAttr::default(),
        }
    }

    fn test_field(name: &str) -> ClassField {
        ClassField {
            name: name.to_string(),
            field_type: int_ty(),
            field_template: TyTemplate::Concrete(int_ty()),
            description: None,
            alias: None,
            skip: false,
        }
    }

    fn test_class(field_count: usize) -> Object {
        Object::Class(Box::new(Class {
            name: TypeName::local(Name::new("TestClass")),
            fields: (0..field_count)
                .map(|idx| test_field(&format!("field{idx}")))
                .collect(),
            description: None,
            alias: None,
            type_tag: 100,
            ty_attr: TyAttr::default(),
        }))
    }

    fn early_yield_for_test() -> EarlyYieldCheck {
        #[cfg(target_arch = "wasm32")]
        {
            EarlyYieldCheck::new()
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            EarlyYieldCheck::new(Arc::new(AtomicBool::new(false)))
        }
    }

    fn test_vm(compile_time_objects: Vec<Object>) -> BexVm {
        let heap = BexHeap::new(compile_time_objects);
        BexVm {
            frames: Vec::new(),
            stack: EvalStack::new(),
            heap: Arc::clone(&heap),
            early_yield: early_yield_for_test(),
            tlab: Tlab::new(heap),
            globals: VmGlobals::Owned(GlobalPool::new()),
            resolved_class_names: HashMap::new(),
            error_class_ptrs: Vec::new(),
            panic_class_ptrs: Vec::new(),
            watch: Watch::new(),
            watched_vars: HashMap::new(),
            interrupt_frame: None,
            traced_frames: Vec::new(),
            current_span_context: None,
            argv: Arc::from([]),
            pending_call_type_args: Vec::new(),
            interface_implementors: Arc::new(indexmap::IndexMap::new()),
        }
    }

    fn native_done(_vm: &mut BexVm, _args: &[Value]) -> NativeCallResult {
        NativeCallResult::Done(Value::int(42))
    }

    fn native_function_object() -> Object {
        let native: NativeFunction = native_done;
        Object::Function(Box::new(Function {
            name: "test_native".to_string(),
            source_file: String::new(),
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Native(native as *const ()),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            block_notifications: Vec::new(),
            viz_nodes: Vec::new(),
            return_type: int_ty(),
            stream_return_type: Ty::Null {
                attr: TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            throws_type: None,
            origin: FunctionOrigin::Internal,
            body_meta: None,
            trace: false,
        }))
    }

    fn vm_with_native_entry() -> (BexVm, HeapPtr) {
        let mut vm = test_vm(vec![native_function_object()]);
        let native_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.globals = VmGlobals::Owned(GlobalPool::from_vec(vec![Value::object(native_ptr)]));
        (vm, native_ptr)
    }

    fn trampoline_ptr(vm: &BexVm) -> HeapPtr {
        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.function
    }

    #[test]
    fn omitted_arg_uses_unknown_type_tag() {
        let value = Value::OMITTED_ARG;

        assert_eq!(value_type_tag(value), type_tags::UNKNOWN);
        assert!(matches!(
            Value::int(type_tags::UNKNOWN).kind(),
            ValueKind::Int(tag) if value_type_tag(value) == tag
        ));
        assert!(!matches!(
            Value::int(type_tags::INT).kind(),
            ValueKind::Int(tag) if value_type_tag(value) == tag
        ));
    }

    #[test]
    fn init_spread_preserves_pre_spread_watch_baseline() {
        let mut vm = test_vm(vec![test_class(2)]);
        let class_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        let source_ptr = vm.tlab.alloc(Object::Instance(Instance::new(
            class_ptr,
            Vec::new(),
            vec![Value::int(10), Value::int(2)],
        )));
        let dest_ptr = vm.tlab.alloc(Object::Instance(Instance::new(
            class_ptr,
            Vec::new(),
            vec![Value::int(1), Value::int(2)],
        )));
        let root = NodeId::HeapObject(dest_ptr);
        vm.watch.register_root(
            root,
            RootState {
                value: Value::object(dest_ptr),
                last_assigned: None,
                last_notified: None,
                channel: "test".to_string(),
                filter: WatchFilter::Default,
            },
        );

        let result = vm
            .init_spread(
                Value::object(dest_ptr),
                Value::object(source_ptr),
                &FieldCopySet {
                    fields: vec![
                        FieldCopy { source: 0, dest: 0 },
                        FieldCopy { source: 1, dest: 1 },
                    ],
                },
            )
            .expect("spread should succeed");

        let Some(VmExecState::Notify(WatchNotification::Variables(notifications))) = result else {
            panic!("expected watched spread notification");
        };
        assert_eq!(notifications, vec![root]);

        let Object::Instance(dest) = vm.get_object(dest_ptr) else {
            panic!("destination should remain an instance");
        };
        assert_eq!(
            dest.field_values().collect::<Vec<_>>(),
            vec![Value::int(10), Value::int(2)]
        );
    }

    #[test]
    fn trampoline_function_survives_gc_while_frame_is_active() {
        let (mut vm, native_ptr) = vm_with_native_entry();

        vm.set_entry_point(native_ptr, &[]);
        let trampoline = trampoline_ptr(&vm);

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&trampoline),
            "active trampoline frame must root its synthetic function"
        );

        let (stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };

        assert_eq!(stats.live_count, 1);
        assert!(
            forwarding.contains_key(&trampoline),
            "active trampoline function must be forwarded by GC"
        );

        vm.forward_roots(&forwarding);

        let moved_trampoline = trampoline_ptr(&vm);
        assert!(
            matches!(vm.get_object(moved_trampoline), Object::Function(f) if f.name == "$entry::test_native"),
            "frame should point at the moved trampoline function after forwarding"
        );

        let result = vm.exec().expect("native trampoline should execute");
        assert!(
            matches!(result, VmExecState::Complete(value) if value == Value::int(42)),
            "native trampoline should return the native result"
        );
    }

    #[test]
    fn trampoline_function_is_collected_after_return() {
        let (mut vm, native_ptr) = vm_with_native_entry();

        vm.set_entry_point(native_ptr, &[]);
        let trampoline = trampoline_ptr(&vm);

        let result = vm.exec().expect("native trampoline should execute");
        assert!(
            matches!(result, VmExecState::Complete(value) if value == Value::int(42)),
            "native trampoline should return the native result"
        );
        assert!(
            vm.frames.is_empty(),
            "trampoline frame should be popped after return"
        );

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            !roots.contains(&trampoline),
            "returned trampoline function must not remain rooted"
        );

        let (stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };

        assert_eq!(stats.live_count, 0);
        assert!(
            stats.collected_count >= 1,
            "GC should collect the unrooted trampoline function"
        );
        assert!(
            !forwarding.contains_key(&trampoline),
            "unrooted trampoline function must not be forwarded after return"
        );
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
    pub frames: Vec<Frame>,

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
    ///
    /// During `$init`, this is `VmGlobals::Owned` so `StoreGlobal` can
    /// populate top-level let bindings. After `$init` completes, the engine
    /// freezes the pool into a shared `Arc<[Value]>` and every subsequent VM
    /// is constructed with `VmGlobals::Shared`; `StoreGlobal` against the
    /// shared view is a `VmInternalError`.
    pub globals: VmGlobals,

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

    /// Type-args of the *currently dispatching* call, populated by the
    /// `Instruction::Call` handler from the leading `ntypeargs` `Object::Type`
    /// stack slots before invoking the callee.
    ///
    /// For bytecode callees this is redundant (the type-args are written into
    /// the new frame's `type_args` field by the Call writeback), but for native
    /// callees it is the only channel — the native dispatch path does not push
    /// a bytecode frame, so without this slot any leading type-args would be
    /// silently dropped.
    ///
    /// Saved/restored across nested `Call` instructions; native handlers that
    /// re-enter the VM (via `YieldToCall`) therefore see their own type-args
    /// even if the inner callback uses different ones.
    pending_call_type_args: Vec<baml_type::Ty>,

    /// Per-program interface implementation registry (BEP-044). Used by the
    /// `type.implements()` / `type.implementors()` / `type.implemented_by()`
    /// reflection methods. Shared `Arc` so spawned VMs (lambdas, futures)
    /// don't duplicate the map.
    pub interface_implementors: Arc<InterfaceImplementors>,
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
    /// Awaiting a pending future.
    ///
    /// - Input: a `FutureId` corresponding to a (probably) pending future
    /// - Output (success): the future's result on top of the stack
    /// - Output (failure): an exception/panic passed to the VM
    /// - Output (internal error): engine error
    Await(FutureId),

    /// BEP-034: VM yields a `spawn { body }` to the engine.
    ///
    /// - Input: a `HeapPtr` to the `UnscheduledFuture` object the VM
    ///   allocated. The struct carries the body closure and the
    ///   optional spawn name.
    /// - Output: the engine builds a fresh `Future::Pending(id)` heap
    ///   object, dispatches the body on a new `BexThread`, and pushes
    ///   the future pointer onto the VM stack. Terminal transitions
    ///   (`Ready`/`Error`/`Cancelled`/`InternalError`) happen later
    ///   via the `FutureManager`; the VM only ever sees `Pending`
    ///   directly after this yield.
    ///
    /// BEP-034 phase D′: this used to be `ScheduleFuture` and covered
    /// both sys-ops and spawns; sys-ops have moved to the dedicated
    /// single-yield `SysOp` variant below.
    Spawn(HeapPtr),

    /// BEP-034 phase D′: VM is invoking a sys-op and wants its return
    /// value pushed back on the stack.
    ///
    /// Replaces the two-yield `ScheduleFuture` → `Await` dance the old
    /// MIR emitted for every sys-op call. The engine runs the op
    /// inline (synchronously if `Ready`, or by awaiting the `Async`
    /// future while releasing the heap permit), races it against the
    /// active cancel token, and pushes the resulting `Value` on the
    /// VM stack before resuming. Errors propagate as
    /// `EngineError::UnhandledThrow` exactly like today's sys-op
    /// fulfillment path.
    ///
    /// - Input: the `SysOp` to run plus its `args`, popped from the
    ///   eval stack by the `OpCode::SysOp` handler.
    /// - Output: a single `Value` on the VM stack.
    /// - No `Object::Future` is allocated; no `FutureManager` entry
    ///   is created.
    SysOp {
        operation: bex_vm_types::SysOp,
        args: Vec<Value>,
    },

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
    /// Interface → implementors registry (BEP-044) for runtime reflection.
    pub interface_implementors: InterfaceImplementors,
}

/// Convert a compiled `Program` to a `BytecodeProgram` with native functions attached.
///
/// This is the bridge between compilation output and VM execution. It:
/// 1. Attaches native function implementations to builtin functions
/// 2. Builds resolved name lookups for functions, classes, and enums
pub fn convert_program(program: bex_vm_types::Program) -> Result<BytecodeProgram, VmInternalError> {
    // Convert objects, attaching native functions
    let mut objects: Vec<Object> = program
        .objects
        .into_iter()
        .map(crate::package_baml::attach_builtins)
        .collect::<Result<Vec<_>, _>>()?;

    // Lower every Function's bytecode to compact form. exec_compact requires
    // this — callers that bypass BexEngine (e.g. tests using BexVm::from_program
    // directly) would otherwise hit Option::unwrap on compact.as_ref().
    for obj in &mut objects {
        if let Object::Function(func) = obj {
            if func.bytecode.compact.is_none() {
                func.bytecode.compact = Some(func.bytecode.lower_to_compact());
            }
        }
    }

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
        interface_implementors: program.interface_implementors,
    })
}

/// Extract an `f64` if `value` carries a heap-boxed float.
///
/// Floats are no longer inline in `Value`; they live as `Object::Float(f64)`
/// behind a `HeapPtr`. Returns `None` for any other variant (including ints —
/// callers that want int→float promotion must combine this with `as_int`).
#[inline]
fn value_as_float(value: Value) -> Option<f64> {
    let ptr = value.as_object_ptr()?;
    // SAFETY: HeapPtr from a live Value is valid for read.
    match unsafe { ptr.get() } {
        Object::Float(f) => Some(*f),
        _ => None,
    }
}

/// Get the type tag for any runtime value.
///
/// This is a free function to avoid borrow checker issues when called
/// from within the instruction dispatch loop.
fn value_type_tag(value: Value) -> i64 {
    use bex_vm_types::{ValueKind, types::type_tags};

    match value.kind() {
        ValueKind::OmittedArg => type_tags::UNKNOWN,
        ValueKind::Int(_) => type_tags::INT,
        ValueKind::Bool(_) => type_tags::BOOL,
        ValueKind::Null => type_tags::NULL,
        ValueKind::Object(ptr) => {
            // SAFETY: Reading type information from objects via HeapPtr.
            let obj = unsafe { ptr.get() };
            match obj {
                Object::Float(_) => type_tags::FLOAT,
                Object::String(_) => type_tags::STRING,
                Object::Bigint(_) => type_tags::BIGINT,
                Object::Uint8Array(_) => type_tags::UINT8ARRAY,
                Object::Variant(_) => type_tags::ENUM,
                Object::Array(_) => type_tags::LIST,
                Object::Map(_) => type_tags::MAP,
                Object::Function(_) => type_tags::FUNCTION,
                Object::Closure(_) => type_tags::FUNCTION,
                Object::BoundMethod(_) => type_tags::FUNCTION,
                Object::HostClosure(_) => type_tags::FUNCTION,
                Object::Cell(_) => type_tags::UNKNOWN,
                Object::Future(_) => type_tags::FUTURE,
                Object::UnscheduledFuture(_) => type_tags::FUTURE,
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

/// A popped operand for a specialized bigint opcode.
///
/// The specialized `*Bigint` / `CmpBigint*` opcodes accept one `int` operand
/// (so mixed `bigint`/`int` operators don't need a MIR coercion). Rather than
/// allocate a BEX-heap `Object::Bigint` for that `int`, it stays a small `i64`
/// here and is widened to a local `BigInt` (a `Cow`) only at the point of use.
#[derive(Clone, Copy)]
enum BigintOperand {
    /// A heap-resident `Object::Bigint`.
    Heap(HeapPtr),
    /// An `int` operand, to be widened to a local `BigInt` on demand.
    Int(i64),
}

impl BexVm {
    /// Construct a [`PermitProof`] tied to `&self`'s borrow.
    ///
    /// # Why this is safe
    ///
    /// A `&BexVm` is only ever obtained inside an
    /// `bex_heap::ActiveHeapPermit<BexVm>` deref context — every
    /// caller of any `&BexVm` method is therefore already inside an
    /// active heap permit. The returned `PermitProof<'_>`'s lifetime is
    /// bounded by `&self`, which is bounded by the wrapping permit's,
    /// so the proof witnesses a genuinely-held permit.
    ///
    /// The internal `PermitProof::new()` is `unsafe` because in general
    /// minting a proof from nothing breaks the type-level
    /// permit-exclusion guarantee. Here the borrow of `&self` *is* the
    /// runtime witness; we just need to repackage it as the canonical
    /// proof token. This is the VM-internal mirror of
    /// `bex_heap::ActiveHeapPermit::proof`.
    #[inline]
    #[must_use]
    #[allow(
        clippy::unused_self,
        reason = "the `&self` borrow is load-bearing — it ties the returned \
                  proof's lifetime to a valid `&BexVm` borrow, which only \
                  exists inside an active permit deref"
    )]
    pub(crate) fn proof(&self) -> PermitProof<'_> {
        // SAFETY: `&self` is only obtainable inside an active permit
        // deref; see the method-level "Why this is safe" argument.
        #[allow(unsafe_code, reason = "PermitProof::new is the unsafe boundary")]
        unsafe {
            PermitProof::new()
        }
    }

    /// Create a new VM with a shared heap.
    ///
    /// The heap is shared across all VMs. Each VM gets its own TLAB
    /// for contention-free allocation.
    pub fn new(
        heap: Arc<BexHeap>,
        globals: VmGlobals,
        resolved_class_names: HashMap<String, HeapPtr>,
        #[cfg(not(target_arch = "wasm32"))] park_requested: Arc<AtomicBool>,
        argv: Arc<[String]>,
        interface_implementors: Arc<InterfaceImplementors>,
    ) -> Self {
        // Defer the first TLAB chunk reservation until the first `tlab.alloc`,
        // which the engine reaches only after the VM has been registered as a
        // permit holder via `HeapPermitManager::new_permit` and a permit is
        // active. Eagerly calling `Tlab::new` here would reserve a chunk
        // *before* registration, leaving the cursor stale across any GC that
        // fires in the engine's pre-permit window.
        let tlab = Tlab::new_empty(Arc::clone(&heap));

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
            pending_call_type_args: Vec::new(),
            interface_implementors,
        }
    }

    /// Type-args of the currently dispatching call.
    ///
    /// Populated by `Instruction::Call` when the call carries `ntypeargs > 0`.
    /// For BAML→BAML calls these are also written into the callee's frame; for
    /// BAML→native calls this slot is the **only** channel, since native
    /// dispatch does not push a bytecode frame.
    ///
    /// Returns an empty slice for calls with `ntypeargs == 0` and from outside
    /// any call dispatch context.
    pub fn current_call_type_args(&self) -> &[baml_type::Ty] {
        &self.pending_call_type_args
    }

    /// Read an object from the heap via `HeapPtr`.
    ///
    /// # Safety
    ///
    /// The returned `&Object` may be aliased by other spawned VMs. It is safe
    /// to inspect immutable object metadata through it, and to reach mutable
    /// internals only when that field has its own synchronization (for example
    /// `LockedContainer` data, atomic instance fields, cells, or futures).
    #[inline]
    pub fn get_object(&self, ptr: HeapPtr) -> &Object {
        // SAFETY: `HeapPtr` points into stable heap storage. Shared mutable
        // state behind the object must provide its own synchronization.
        unsafe { ptr.get() }
    }

    /// Get mutable access to an object via `HeapPtr`.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to the heap object itself. `&mut
    /// self` only proves exclusive access to this VM, not to objects shared with
    /// spawned VMs. Mutator paths for shared state should use [`Self::get_object`]
    /// plus the object's interior synchronization instead.
    #[inline]
    pub fn get_object_mut(&mut self, ptr: HeapPtr) -> &mut Object {
        debug_assert!(
            !self.heap.is_compile_time_ptr(ptr),
            "Cannot mutate compile-time object"
        );
        // SAFETY: caller upholds the object-exclusivity contract documented above.
        unsafe { ptr.get_mut() }
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
        value: Value,
        object_type: ObjectType,
    ) -> Result<HeapPtr, VmInternalError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: object_type.into(),
                got: self.type_of(&value),
            });
        };
        Ok(ptr)
    }

    /// Get string from a Value.
    pub fn as_string(&self, value: &Value) -> Result<&bex_vm_types::BexStr, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::String)?;
        self.get_object(ptr).as_string()
    }

    /// Get a reference to the `Arc<BigInt>` from a bigint Value.
    pub fn as_bigint(
        &self,
        value: &Value,
    ) -> Result<&std::sync::Arc<num_bigint::BigInt>, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Bigint)?;
        match self.get_object(ptr) {
            Object::Bigint(arc) => Ok(arc),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Bigint.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    /// Get uint8array from a Value. Acquires the container's mutex.
    pub fn as_uint8array(
        &self,
        value: &Value,
    ) -> Result<bex_vm_types::Uint8ArrayReadGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Uint8Array)?;
        let obj = self.get_object(ptr);
        match obj {
            Object::Uint8Array(bytes) => Ok(bytes.lock()),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Uint8Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable uint8array from a Value. Acquires the container's mutex.
    pub fn as_uint8array_mut(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::Uint8ArrayWriteGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Uint8Array)?;
        match self.get_object(ptr) {
            Object::Uint8Array(bytes) => Ok(bytes.lock_mut()),
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
    ///
    /// Strings are immutable at the current BAML language surface. Do not use
    /// this for spawned user-code mutation unless strings gain the same
    /// object-level synchronization as containers.
    pub fn as_string_mut(
        &mut self,
        value: &Value,
    ) -> Result<&mut bex_vm_types::BexStr, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::String)?;
        self.get_object_mut(ptr).as_string_mut()
    }

    /// Get array from a Value. Acquires the container's mutex; the
    /// returned guard derefs to `&[Value]` and releases the lock on drop.
    pub fn as_array(
        &self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayReadGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Array)?;
        let obj = self.get_object(ptr);
        match obj {
            Object::Array(arr) => Ok(arr.lock()),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable array from a Value. Acquires the container's mutex;
    /// the returned guard derefs to `&mut Vec<Value>` and releases the
    /// lock on drop, serializing concurrent mutators under `spawn`.
    pub fn as_array_mut(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayWriteGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(*value, ObjectType::Array)?;
        // Conservative write barrier: any mutable access to an older-generation
        // array may introduce cross-generation references. Used by builtin dispatch
        // (Array.push, Array.pop, etc.) where the actual written values are not
        // visible at this call site.
        self.heap.conservative_write_barrier(ptr);
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(ptr), Object::Array(_)) {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(self.get_object(ptr)).into(),
            });
        }
        match self.get_object(ptr) {
            Object::Array(arr) => Ok(arr.lock_mut()),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get map from a Value. Acquires the container's mutex.
    pub fn as_map(&self, value: &Value) -> Result<bex_vm_types::MapReadGuard<'_>, VmInternalError> {
        let index = self.as_object_ptr(*value, ObjectType::Map)?;
        let obj = self.get_object(index);
        match obj {
            Object::Map(map) => Ok(map.lock()),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Get mutable map from a Value. Acquires the container's mutex.
    pub fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::MapWriteGuard<'_>, VmInternalError> {
        let index = self.as_object_ptr(*value, ObjectType::Map)?;
        // Conservative write barrier: any mutable access to an older-generation
        // map may introduce cross-generation references. Used by builtin dispatch
        // (Map.set, etc.) where the actual written values are not visible here.
        self.heap.conservative_write_barrier(index);
        // Check type first to avoid borrow issues
        if !matches!(self.get_object(index), Object::Map(_)) {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(self.get_object(index)).into(),
            });
        }
        match self.get_object(index) {
            Object::Map(map) => Ok(map.lock_mut()),
            _ => unreachable!("type was just checked"),
        }
    }

    /// Get Value reference (for generic types).
    #[allow(dead_code)]
    pub fn as_value_mut(&mut self, value: &Value) -> Result<&mut Value, VmInternalError> {
        // This is used by macro-generated code for generic type parameters.
        // For now, we don't support mutable access to generic values.
        let Some(ptr) = value.as_object_ptr() else {
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
        let mut compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Box every reachable `ConstValue::Float` into a compile-time
        // `Object::Float` (floats can no longer live inline in `Value`).
        let float_indices = bex_vm_types::types::box_compile_time_floats(
            &mut compile_time_objects,
            &bytecode.globals,
        );

        // Create heap with compile-time objects
        let heap = BexHeap::new(compile_time_objects);

        // Convert compile-time globals (ConstValue) to runtime globals (Value).
        // The `from_program` constructor is test-only — we hand the VM an
        // `Owned` view so that any `$init` bytecode the test happens to drive
        // can write to globals.
        let globals_vec: Vec<Value> = bytecode
            .globals
            .into_iter()
            .map(|cv| match cv {
                bex_vm_types::ConstValue::Float(f) => {
                    let idx = float_indices[&f.to_bits()];
                    Value::object(heap.compile_time_ptr(idx))
                }
                other => other.to_value(|idx| heap.compile_time_ptr(idx.into_raw())),
            })
            .collect();
        let globals = VmGlobals::Owned(bex_vm_types::GlobalPool::from_vec(globals_vec));

        // Build resolved_class_names: convert ObjectIndex -> HeapPtr.
        //
        // Enum HeapPtrs are folded into the same map: BAML's type namespace is
        // shared across classes and enums (a class and an enum cannot share an
        // FQN), so callers that need name-based runtime lookup (e.g.
        // `baml.json.from_string<Color>(...)`) can dispatch on the resulting
        // `Object` kind.
        let mut resolved_class_names: HashMap<String, HeapPtr> = bytecode
            .resolved_class_names
            .into_iter()
            .map(|(name, idx)| (name, heap.compile_time_ptr(idx.into_raw())))
            .collect();
        resolved_class_names.extend(
            bytecode
                .resolved_enums_names
                .into_iter()
                .map(|(name, idx)| (name, heap.compile_time_ptr(idx.into_raw()))),
        );

        let interface_implementors = Arc::new(bytecode.interface_implementors);

        Ok(Self::new(
            heap,
            globals,
            resolved_class_names,
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            Arc::from(Vec::<String>::new()),
            interface_implementors,
        ))
    }

    /// Bootstraps the VM preparing the given callable to run.
    ///
    /// `function` may point to either an [`Object::Function`] or an
    /// [`Object::Closure`]. Closure entry points are used by BEP-034
    /// `spawn { ... }`: the compiler lowers the body to a lambda, wraps it
    /// in a closure that carries the captured environment, then hands the
    /// closure pointer to a fresh `BexThread` which calls `set_entry_point`.
    pub fn set_entry_point(&mut self, function: HeapPtr, args: &[Value]) {
        // BEP-034 spawn entry points can pass a `Closure` (carrying
        // captured type args from the surrounding scope); host calls
        // typically pass a bare `Function` and want no type args.
        // Either way, fan out to `set_entry_point_with_type_args` so
        // the type-arg-aware host call path
        // (`BexEngine::call_function_bound_args`) shares one
        // implementation with the spawn path.
        let type_args = match self.get_object(function) {
            Object::Function(_) => vec![],
            Object::Closure(closure) => closure.captured_type_args.clone(),
            other => panic!("expect function or closure as entry point, got {other:?}"),
        };
        self.set_entry_point_with_type_args(function, args, type_args);
    }

    /// Like [`Self::set_entry_point`], but seeds the entry frame's
    /// `type_args` slot. Use when the host invokes a generic function
    /// (e.g. a user function with `<T>`) and needs to thread `T` through.
    ///
    /// Bytecode entry points are pushed directly. Native and sysop entries are
    /// wrapped in a synthetic bytecode caller that executes either
    /// `CALL <native>; RETURN` or `SYS_OP <sysop>; RETURN`, giving the normal VM
    /// machinery a bytecode frame to resume into.
    pub fn set_entry_point_with_type_args(
        &mut self,
        function: HeapPtr,
        args: &[Value],
        type_args: Vec<baml_type::Ty>,
    ) {
        debug_assert!(
            matches!(
                self.get_object(function),
                Object::Function(_) | Object::Closure(_)
            ),
            "expect function or closure as entry point, got {:?}",
            self.get_object(function)
        );

        let callable_kind = match self.get_object(function) {
            Object::Function(f) => f.kind,
            Object::Closure(closure) => {
                let func_obj = unsafe { closure.function.get() };
                match func_obj {
                    Object::Function(f) => f.kind,
                    other => unreachable!("expect closure function, got {other:?}"),
                }
            }
            other => unreachable!("expect function or closure as entry point, got {other:?}"),
        };

        match callable_kind {
            FunctionKind::Bytecode => {
                self.pending_call_type_args.clone_from(&type_args);
                self.stack.extend(args.iter().copied());
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function,
                    instruction_ptr: 0,
                    locals_offset: StackIndex::from_raw(0),
                    type_args,
                    faulting_pc: 0,
                }));

                // Entry functions need the same frame-local pre-allocation as normal
                // bytecode calls now that INIT_LOCALS is gone from bytecode.
                self.allocate_real_locals_for_frame(function)
                    .expect("entry point must be a valid function frame");
            }
            FunctionKind::Native(_) | FunctionKind::SysOp(_) => {
                self.push_trampoline_frame(function, args, type_args, callable_kind);
            }
            FunctionKind::NativeUnresolved => {
                unreachable!("entry point kind is not directly invokable: {callable_kind:?}");
            }
        }
    }

    fn global_index_for_function_ptr(&self, function: HeapPtr) -> Option<GlobalIndex> {
        self.globals
            .as_slice(self.proof())
            .iter()
            .position(|value| value.as_object_ptr() == Some(function))
            .map(GlobalIndex::from_raw)
    }

    fn push_trampoline_frame(
        &mut self,
        function: HeapPtr,
        args: &[Value],
        type_args: Vec<baml_type::Ty>,
        callable_kind: FunctionKind,
    ) {
        let callee_global = self
            .global_index_for_function_ptr(function)
            .expect("entry point must be present in globals");
        let ntypeargs = u16::try_from(type_args.len()).expect("entry type args fit in u16");

        let (callee_name, return_type, throws_type) = match self.get_object(function) {
            Object::Function(f) => (f.name.clone(), f.return_type.clone(), f.throws_type.clone()),
            other => unreachable!("expect function as entry point, got {other:?}"),
        };

        self.pending_call_type_args.clear();
        match callable_kind {
            FunctionKind::Native(_) => {
                for ty in type_args {
                    let ty_ptr = self.tlab.alloc(Object::Type(Box::new(ty)));
                    self.stack.push(Value::object(ty_ptr));
                }
                self.stack.extend(args.iter().copied());
            }
            FunctionKind::SysOp(_) => {
                self.stack.extend(args.iter().copied());
            }
            FunctionKind::Bytecode | FunctionKind::NativeUnresolved => {
                unreachable!("trampoline frame requires Native or SysOp")
            }
        }

        let instructions = match callable_kind {
            FunctionKind::Native(_) => vec![
                Instruction::Call {
                    callee: callee_global,
                    ntypeargs,
                },
                Instruction::Return,
            ],
            FunctionKind::SysOp(_) => vec![Instruction::SysOp(callee_global), Instruction::Return],
            FunctionKind::Bytecode | FunctionKind::NativeUnresolved => {
                unreachable!("trampoline frame requires Native or SysOp")
            }
        };
        let mut bytecode = bytecode::Bytecode {
            instructions,
            ..bytecode::Bytecode::default()
        };
        bytecode.compact = Some(bytecode.lower_to_compact());

        let entry_function = Function {
            name: format!("$entry::{callee_name}"),
            source_file: String::new(),
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            block_notifications: Vec::new(),
            viz_nodes: Vec::new(),
            return_type,
            stream_return_type: baml_type::Ty::Null {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            throws_type,
            origin: FunctionOrigin::Internal,
            body_meta: None,
            trace: false,
        };
        let entry_ptr = self.tlab.alloc(Object::Function(Box::new(entry_function)));

        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            faulting_pc: 0,
        }));
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

    /// Returns a reference to the unscheduled future at `future_ptr`.
    ///
    /// Returns [`VmInternalError::TypeError`] if the heap object is not an
    /// `Object::UnscheduledFuture`.
    pub fn unscheduled_future(
        &self,
        future_ptr: HeapPtr,
    ) -> Result<&UnscheduledFuture, VmInternalError> {
        match self.get_object(future_ptr) {
            Object::UnscheduledFuture(future) => Ok(future),
            other => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::UnscheduledFuture),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    /// Allocates an array on the heap and returns it to the caller.
    pub fn alloc_array(&mut self, values: Vec<Value>) -> Value {
        Value::object(self.tlab.alloc(Object::Array(values.into())))
    }

    pub fn alloc_map(&mut self, values: IndexMap<bex_vm_types::BexStr, Value>) -> Value {
        Value::object(self.tlab.alloc(Object::Map(values.into())))
    }

    pub fn alloc_string(&mut self, s: impl Into<bex_vm_types::BexStr>) -> Value {
        Value::object(self.tlab.alloc(Object::String(s.into())))
    }

    /// Allocate a heap-boxed float and return it as a `Value`.
    pub fn alloc_float(&mut self, f: f64) -> Value {
        Value::object(self.tlab.alloc(Object::Float(f)))
    }

    /// Allocate a bigint on the heap. Takes an `Arc<BigInt>` to allow sharing.
    ///
    /// Refuses values exceeding `MAX_BIGINT_BITS` so a single arithmetic op
    /// (add/sub/mul/shl) can't materialise an arbitrarily large bigint and
    /// blow out memory. Callers whose input is bounded — e.g. a bounded-length
    /// literal parse, or a small operand promoted in a `bigint × int` operator
    /// — can never trip the check in practice, but routing through this path
    /// keeps the guard central.
    ///
    /// On failure returns `VmError::Thrown` with an `AllocFailure` exception
    /// already constructed so instruction handlers can use `?` directly.
    /// Codegenned glue functions return `VmRustFnError`; they call
    /// `try_alloc_bigint` instead, which returns the raw `VmPanic`
    /// (auto-converts to `VmRustFnError::Panic` via `#[from]`).
    pub fn alloc_bigint(
        &mut self,
        arc: std::sync::Arc<num_bigint::BigInt>,
    ) -> Result<Value, VmError> {
        match self.try_alloc_bigint(arc) {
            Ok(v) => Ok(v),
            Err(panic) => Err(VmError::Thrown(self.panic_to_exception_value(panic))),
        }
    }

    /// Pop a bigint operand without allocating.
    ///
    /// The specialized `*Bigint` opcodes accept one `int` operand (mirroring
    /// the way `int`/`float` mix): `try_specialize_binary_op` routes
    /// `bigint`/`int` pairs to these opcodes, so the matched operand is
    /// guaranteed to be either an `Object::Bigint` or a `Value::Int`. The `int`
    /// operand is resolved to a small *local* `BigInt` (a `Cow`) at the point of
    /// use — the operator handling itself, rather than relying on a MIR-inserted
    /// coercion, and crucially without allocating a BEX-heap `Object::Bigint`
    /// for the operand.
    fn pop_bigint_operand(&mut self) -> BigintOperand {
        let v = self.stack.ensure_pop();
        if let Some(ptr) = v.as_object_ptr() {
            debug_assert!(
                matches!(self.get_object(ptr), Object::Bigint(_)),
                "pop_bigint_operand: object operand is not a Bigint (specialization bug)"
            );
            BigintOperand::Heap(ptr)
        } else if let Some(n) = v.as_int() {
            BigintOperand::Int(n)
        } else {
            verifier_unreachable!()
        }
    }

    /// Resolve a [`BigintOperand`] to a borrowable `BigInt`.
    ///
    /// A heap operand borrows the heap-resident `BigInt` directly; an `int`
    /// operand is widened to a small *local* `BigInt` owned by the returned
    /// `Cow` (no BEX-heap allocation).
    fn bigint_operand(&self, op: BigintOperand) -> std::borrow::Cow<'_, num_bigint::BigInt> {
        match op {
            BigintOperand::Heap(ptr) => match self.get_object(ptr) {
                Object::Bigint(arc) => std::borrow::Cow::Borrowed(arc.as_ref()),
                _ => verifier_unreachable!(),
            },
            BigintOperand::Int(n) => std::borrow::Cow::Owned(num_bigint::BigInt::from(n)),
        }
    }

    /// View a `Value` as a `BigInt` for comparison, if it is numerically a
    /// bigint or an `int`: an `int` is widened to a small *local* `BigInt`
    /// (owned `Cow`, no heap alloc), a heap `Object::Bigint` is borrowed.
    /// Returns `None` for anything else (float, string, …).
    ///
    /// Used by the generic comparison path (`exec_cmpop`) so a `bigint` vs
    /// `int` mix compares by value — matching the specialized `CmpBigint*`
    /// opcodes (`bigint_cmp`) — when the static types were erased (e.g. a
    /// union/`any` operand) and the generic `CmpOp` was emitted instead.
    fn value_as_bigint_cow(&self, v: Value) -> Option<std::borrow::Cow<'_, num_bigint::BigInt>> {
        if let Some(n) = v.as_int() {
            Some(std::borrow::Cow::Owned(num_bigint::BigInt::from(n)))
        } else if let Some(ptr) = v.as_object_ptr() {
            match self.get_object(ptr) {
                Object::Bigint(arc) => Some(std::borrow::Cow::Borrowed(arc.as_ref())),
                _ => None,
            }
        } else {
            None
        }
    }

    /// Reconstruct the original `Value` for a [`BigintOperand`].
    ///
    /// Used to populate panic payloads (e.g. `DivisionByZero`) without
    /// allocating: the `int` operand stays an `int`, the heap operand stays a
    /// pointer.
    fn bigint_operand_value(op: BigintOperand) -> Value {
        match op {
            BigintOperand::Heap(ptr) => Value::object(ptr),
            BigintOperand::Int(n) => Value::int(n),
        }
    }

    /// Evaluate a specialized bigint arithmetic / bitwise / shift op.
    ///
    /// Concentrates all the caps and panics that the per-opcode handlers used
    /// to copy-paste. The borrows of the two operands are scoped to a block so
    /// they drop before any heap allocation; only the resulting `BigInt` (or a
    /// `VmPanic`) escapes.
    fn bigint_binop(
        &mut self,
        op: BinOp,
        l: BigintOperand,
        r: BigintOperand,
    ) -> Result<Value, VmError> {
        let outcome: Result<num_bigint::BigInt, VmPanic> = {
            match op {
                BinOp::Add => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    Ok(&*lb + &*rb)
                }
                BinOp::Sub => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    Ok(&*lb - &*rb)
                }
                BinOp::Mul => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    // Pre-flight bit-length check: `bits(lb * rb) ≤ bits(lb) +
                    // bits(rb)` exactly. Reject before materializing the product
                    // so two operands near `MAX_BIGINT_BITS` can't blow memory
                    // with an intermediate twice the limit. Matches `Shl`.
                    let estimated_bits = lb.bits().saturating_add(rb.bits());
                    if estimated_bits > crate::package_baml::bigint::MAX_BIGINT_BITS {
                        Err(VmPanic::AllocFailure {
                            message: format!(
                                "bigint mul: result of bigint multiplication would require ~{estimated_bits} bits (limit: {})",
                                crate::package_baml::bigint::MAX_BIGINT_BITS
                            ),
                        })
                    } else {
                        Ok(&*lb * &*rb)
                    }
                }
                BinOp::Div => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    if *rb == num_bigint::BigInt::ZERO {
                        Err(VmPanic::DivisionByZero {
                            left: Self::bigint_operand_value(l),
                            right: Self::bigint_operand_value(r),
                        })
                    } else {
                        Ok(&*lb / &*rb)
                    }
                }
                BinOp::Mod => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    if *rb == num_bigint::BigInt::ZERO {
                        Err(VmPanic::DivisionByZero {
                            left: Self::bigint_operand_value(l),
                            right: Self::bigint_operand_value(r),
                        })
                    } else {
                        Ok(&*lb % &*rb)
                    }
                }
                BinOp::BitAnd => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    Ok(&*lb & &*rb)
                }
                BinOp::BitOr => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    Ok(&*lb | &*rb)
                }
                BinOp::BitXor => {
                    let lb = self.bigint_operand(l);
                    let rb = self.bigint_operand(r);
                    Ok(&*lb ^ &*rb)
                }
                BinOp::Shl => {
                    // Two failure modes with distinct categories:
                    // - Negative count: `baml.panics.NegativeBitShift` (caller bug).
                    // - Count exceeds `usize`: `baml.panics.AllocFailure`
                    //   (the would-be result is unrepresentable in memory).
                    //
                    // Resolve the count directly from `r`: an `int` count avoids
                    // even a local `BigInt`.
                    let shift: Result<usize, VmPanic> = match r {
                        BigintOperand::Int(n) => {
                            if n < 0 {
                                Err(VmPanic::NegativeBitShift {
                                    message: format!("bigint shl: negative shift count ({n})"),
                                })
                            } else {
                                // On 64-bit targets a non-negative `i64` always
                                // fits in `usize`; on 32-bit targets (wasm32) a
                                // huge count overflows `usize`, so the would-be
                                // result is unrepresentable — `AllocFailure`,
                                // matching the `Heap` path and the old `ShlBigint`.
                                usize::try_from(n).map_err(|_| VmPanic::AllocFailure {
                                    message: format!(
                                        "bigint shl: shift count ({n}) does not fit in usize"
                                    ),
                                })
                            }
                        }
                        BigintOperand::Heap(_) => {
                            let rb = self.bigint_operand(r);
                            if rb.sign() == num_bigint::Sign::Minus {
                                Err(VmPanic::NegativeBitShift {
                                    message: format!("bigint shl: negative shift count ({rb})"),
                                })
                            } else {
                                usize::try_from(rb.as_ref()).map_err(|_| VmPanic::AllocFailure {
                                    message: format!(
                                        "bigint shl: shift count ({rb}) does not fit in usize"
                                    ),
                                })
                            }
                        }
                    };
                    match shift {
                        Err(panic) => Err(panic),
                        Ok(shift) => {
                            let lb = self.bigint_operand(l);
                            let estimated_bits = lb.bits().saturating_add(shift as u64);
                            if estimated_bits > crate::package_baml::bigint::MAX_BIGINT_BITS {
                                Err(VmPanic::AllocFailure {
                                    message: format!(
                                        "bigint shl: result of {lb} << {shift} would require ~{estimated_bits} bits (limit: {})",
                                        crate::package_baml::bigint::MAX_BIGINT_BITS
                                    ),
                                })
                            } else {
                                Ok(&*lb << shift)
                            }
                        }
                    }
                }
                BinOp::Shr => {
                    // Reject negative shift counts as `baml.panics.NegativeBitShift`
                    // (mirrors `Shl`). Non-negative counts that don't fit in a
                    // `usize` saturate to `0n`/`-1n` below.
                    let shift_opt: Result<Option<usize>, VmPanic> = match r {
                        BigintOperand::Int(n) => {
                            if n < 0 {
                                Err(VmPanic::NegativeBitShift {
                                    message: format!("bigint shr: negative shift count ({n})"),
                                })
                            } else {
                                Ok(usize::try_from(n).ok())
                            }
                        }
                        BigintOperand::Heap(_) => {
                            let rb = self.bigint_operand(r);
                            if rb.sign() == num_bigint::Sign::Minus {
                                Err(VmPanic::NegativeBitShift {
                                    message: format!("bigint shr: negative shift count ({rb})"),
                                })
                            } else {
                                Ok(usize::try_from(rb.as_ref()).ok())
                            }
                        }
                    };
                    match shift_opt {
                        Err(panic) => Err(panic),
                        Ok(shift_opt) => {
                            // Right shift never grows the value, so a non-negative
                            // shift count too large for `usize` is treated as
                            // "shift past every bit". `num-bigint`'s `Shr` is an
                            // arithmetic right shift (rounds toward -∞), so
                            // positives saturate to 0 and negatives to -1 —
                            // matching `i*::shr`.
                            let lb = self.bigint_operand(l);
                            Ok(match shift_opt {
                                Some(shift) => &*lb >> shift,
                                None if lb.sign() == num_bigint::Sign::Minus => {
                                    num_bigint::BigInt::from(-1)
                                }
                                None => num_bigint::BigInt::ZERO,
                            })
                        }
                    }
                }
            }
        };
        match outcome {
            Ok(result) => self.alloc_bigint(std::sync::Arc::new(result)),
            Err(panic) => Err(VmError::Thrown(self.panic_to_exception_value(panic))),
        }
    }

    /// Evaluate a specialized bigint comparison.
    fn bigint_cmp(&self, op: CmpOp, l: BigintOperand, r: BigintOperand) -> bool {
        let lb = self.bigint_operand(l);
        let rb = self.bigint_operand(r);
        match op {
            CmpOp::Eq => *lb == *rb,
            CmpOp::NotEq => *lb != *rb,
            CmpOp::Lt => *lb < *rb,
            CmpOp::LtEq => *lb <= *rb,
            CmpOp::Gt => *lb > *rb,
            CmpOp::GtEq => *lb >= *rb,
        }
    }

    /// Like `alloc_bigint` but returns the raw `VmPanic` so codegen glue
    /// (which returns `VmRustFnError`) can use `?` to propagate.
    pub fn try_alloc_bigint(
        &mut self,
        arc: std::sync::Arc<num_bigint::BigInt>,
    ) -> Result<Value, VmPanic> {
        let bits = arc.bits();
        if bits > crate::package_baml::bigint::MAX_BIGINT_BITS {
            return Err(VmPanic::AllocFailure {
                message: format!(
                    "bigint allocation requires {bits} bits (limit: {})",
                    crate::package_baml::bigint::MAX_BIGINT_BITS
                ),
            });
        }
        Ok(Value::object(self.tlab.alloc(Object::Bigint(arc))))
    }

    pub fn alloc_uint8array(&mut self, data: Vec<u8>) -> Value {
        Value::object(self.tlab.alloc_uint8array(data))
    }

    /// TODO: Seems to low level for an embedder, provide an API that takes
    /// class name and mapping of field name => value instead.
    pub fn alloc_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::object(
            self.tlab
                .alloc(Object::Instance(Instance::new(class, vec![], fields))),
        )
    }

    // TODO: Same problem as above. Ideally takes (&str, &str) instead.
    pub fn alloc_variant(&mut self, enm: HeapPtr, index: usize) -> Value {
        Value::object(self.tlab.alloc(Object::Variant(Variant { enm, index })))
    }

    /// Allocate a collector object on the heap.
    pub fn alloc_collector(&mut self, collector: bex_vm_types::CollectorRef) -> Value {
        Value::object(self.tlab.alloc_collector(collector))
    }

    /// Get collector ref from a Value.
    pub fn as_collector(
        &self,
        value: &Value,
    ) -> Result<&bex_vm_types::CollectorRef, VmInternalError> {
        let index = self.as_object_ptr(*value, ObjectType::Collector)?;
        let obj = self.get_object(index);
        match obj {
            Object::Collector(c) => Ok(c),
            _ => Err(VmInternalError::TypeError {
                expected: ObjectType::Collector.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// Allocate opaque Rust data on the heap, returning a `Value::object(HeapPtr)`.
    ///
    /// Used by generated `copy::` structs for `$rust_type` fields.
    pub fn alloc_rust_data(&mut self, data: Arc<dyn std::any::Any + Send + Sync>) -> Value {
        Value::object(self.tlab.alloc_rust_data(data))
    }

    /// Downcast a `Value` carrying a heap pointer to `Object::RustData` to `&T`.
    ///
    /// Used by generated `view::` struct accessors for `$rust_type` fields.
    pub fn as_rust_data<T: 'static>(&self, value: &Value) -> Result<&T, VmInternalError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::RustData),
                got: self.type_of(value),
            });
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

    /// Extract an `&Instance` from a `Value` carrying a heap-object pointer.
    ///
    /// Used by generated glue code to construct `view::` structs.
    pub fn as_instance(&self, value: &Value) -> Result<&Instance, VmInternalError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::Instance),
                got: self.type_of(value),
            });
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

    /// Look up a function by its fully-qualified name by scanning `vm.globals`.
    ///
    /// Returns `Some(ptr)` for the first `Object::Function` whose `name` matches,
    /// or `None` if no such function exists in the global pool.
    ///
    /// This is O(globals) and intended for use in native methods that need to
    /// dispatch to a dynamically resolved method (e.g. `Map.to_json`). Not
    /// suitable for hot paths; callers that need repeated lookups should cache
    /// the result.
    pub fn find_function_by_name(&self, name: &str) -> Option<HeapPtr> {
        for v in self.globals.as_slice(self.proof()) {
            if let Some(ptr) = v.as_object_ptr() {
                if let Object::Function(f) = self.get_object(ptr) {
                    if f.name == name {
                        return Some(ptr);
                    }
                }
            }
        }
        None
    }

    /// Allocate a `BoundMethod` on the heap, binding `function` (a `HeapPtr`
    /// pointing to an `Object::Function`) to `receiver`.
    ///
    /// When the bound method is called via `YieldToCall`, the VM automatically
    /// inserts `receiver` as the first argument (`self`).
    pub fn alloc_bound_method(&mut self, function: HeapPtr, receiver: Value) -> Value {
        Value::object(
            self.tlab
                .alloc(Object::BoundMethod(BoundMethod { function, receiver })),
        )
    }

    /// Allocate a type descriptor object on the heap.
    pub fn alloc_type(&mut self, ty: baml_type::Ty) -> Value {
        Value::object(self.tlab.alloc_type(ty))
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
            type_args: vec![],
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
        self.stack.resize(new_len, Value::NULL);
        Ok(())
    }

    #[inline]
    fn local_slot_stack_index(locals_offset: StackIndex, slot: usize) -> StackIndex {
        debug_assert!(
            slot > 0,
            "local slot 0 is reserved and should never be materialized on stack"
        );
        StackIndex::from_raw(locals_offset.raw() + slot - 1)
    }

    fn store_local_value(
        &mut self,
        local_var_index: StackIndex,
        value: Value,
    ) -> Result<Option<VmExecState>, VmError> {
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
        //    specific to local stores — for field/array/map stores
        //    the root's top-level binding hasn't changed, but here
        //    the root itself is being rebound.
        //
        // 3. `process_notifications` walks all roots reaching this
        //    node (just itself, since it IS a root) and applies
        //    the watch filter to decide whether to notify.
        if unlikely(!self.watched_vars.is_empty())
            && self.watched_vars.contains_key(&local_var_index)
        {
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

        Ok(None)
    }

    pub fn error_to_exception_value(&mut self, error: VmBamlError) -> Value {
        let (class, fields) = match error {
            VmBamlError::InvalidArgument { message } => (
                ErrorClass::InvalidArgument,
                vec![self.alloc_string(message)],
            ),
            VmBamlError::ParseError { message } => {
                (ErrorClass::ParseError, vec![self.alloc_string(message)])
            }
            VmBamlError::Io { message } => (ErrorClass::Io, vec![self.alloc_string(message)]),
            VmBamlError::Timeout {
                message,
                duration_ms,
            } => (
                ErrorClass::Timeout,
                vec![
                    self.alloc_string(message),
                    duration_ms.map_or(Value::NULL, Value::int),
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
        let instance_ptr =
            self.tlab
                .alloc(Object::Instance(Instance::new(class_ptr, vec![], fields)));
        Value::object(instance_ptr)
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
                let line = Value::int(loc.error_line as i64);
                let function_name = self.alloc_string(loc.function_name.clone());
                self.alloc_error_value(ErrorClass::StackFrame, vec![file, line, function_name])
            })
            .collect();

        let frames_array = Value::object(self.tlab.alloc(Object::Array(frames.into())));
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
                    vec![Value::int(index), Value::int(length as i64)],
                )
            }
            VmPanic::InvalidFieldAccess {
                field_index,
                field_count,
            } =>
            {
                #[allow(clippy::cast_possible_wrap)]
                (
                    PanicClass::InvalidFieldAccess,
                    vec![
                        Value::int(field_index as i64),
                        Value::int(field_count as i64),
                    ],
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
            VmPanic::Cancelled => {
                let msg = self.alloc_string("operation cancelled".to_string());
                (PanicClass::Cancelled, vec![msg])
            }
            VmPanic::UserPanic { message } => {
                let msg = self.alloc_string(message);
                (PanicClass::UserPanic, vec![msg])
            }
            VmPanic::Exit { code } => (PanicClass::Exit, vec![Value::int(code)]),
            VmPanic::AllocFailure { message } => {
                let msg = self.alloc_string(message);
                (PanicClass::AllocFailure, vec![msg])
            }
            VmPanic::HostUnavailable { resource, message } => {
                let resource = self.alloc_string(resource);
                let message = self.alloc_string(message);
                (PanicClass::HostUnavailable, vec![resource, message])
            }
            VmPanic::NegativeBitShift { message } => {
                let msg = self.alloc_string(message);
                (PanicClass::NegativeBitShift, vec![msg])
            }
        };
        self.alloc_panic_value(class, fields)
    }

    fn invalid_field_access_error(&mut self, field_index: usize, field_count: usize) -> VmError {
        VmError::Thrown(self.panic_to_exception_value(VmPanic::InvalidFieldAccess {
            field_index,
            field_count,
        }))
    }

    /// Allocate a `baml.panics.*` class instance using pre-resolved pointers.
    pub fn alloc_panic_value(&mut self, class: PanicClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.panic_class_ptrs[class as usize];
        let instance_ptr =
            self.tlab
                .alloc(Object::Instance(Instance::new(class_ptr, vec![], fields)));
        Value::object(instance_ptr)
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
        let callee_ptr = self.as_object_ptr(callee_value, expected_type.into())?;
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
    /// `BoundMethod`, insert the receiver at the front of `args`. The returned
    /// `HeapPtr` is `callee` unchanged — keeping the `BoundMethod` identity so
    /// that `execute_call_from_locals_offset` can extract the receiver's
    /// `class_type_args` to seed `frame.type_args` (needed for
    /// `reflect.type_of<T>()` inside generic methods invoked indirectly).
    /// `execute_call_from_locals_offset` and `load_function` both unwrap the
    /// `BoundMethod` to its inner `Function` for dispatch.
    fn resolve_bound_method_callee(&self, callee: HeapPtr, args: &mut Vec<Value>) -> HeapPtr {
        let obj = self.get_object(callee);
        if let Object::BoundMethod(bm) = obj {
            let receiver = bm.receiver;
            // Prepend receiver so the inner function sees [self, arg1, ..., argN].
            args.insert(0, receiver);
        }
        callee
    }

    /// Build the single-yield `SysOp::BamlHostCallHostValue` dispatch for
    /// invoking a host closure. `closure_ptr` is the `Object::HostClosure` heap
    /// pointer (passed straight through as the sys-op handle); `user_args` are
    /// the call arguments in positional order, already drained off the stack by
    /// the caller.
    ///
    /// The engine's `VmExecState::SysOp` handler runs the op (firing the
    /// bridge's `HostDispatchFn`, awaiting the host's response, racing
    /// cancellation) and pushes the converted result back onto the VM stack, so
    /// a host-closure call resolves to a value with no `Future` surfaced to
    /// BAML. Shared by the direct `CallIndirect` opcodes and the indirect
    /// native higher-order-builtin callback path
    /// (`execute_call_from_locals_offset`).
    ///
    /// Sys-op arg layout (mirrors the codegen-generated glue for
    /// `baml.host.call_host_value` in `sys_ops/.../io_generated.rs`):
    ///   args\[0\] = `handle`     (`Object::HostClosure` → `BexExternalValue::HostValue`)
    ///   args\[1\] = `args_array` (`Object::Array<Value>`)
    ///   args\[2\] = `ret_ty`     (`Object::Type<Ty>`)
    fn host_closure_call_sysop(
        &mut self,
        closure_ptr: HeapPtr,
        user_args: Vec<Value>,
    ) -> VmExecState {
        // Read arity + return type out of the closure, then drop the borrow
        // before allocating (a TLAB allocation may move/collect heap objects).
        let (arity, ret_ty) = match self.get_object(closure_ptr) {
            Object::HostClosure(hc) => (hc.arity, hc.ret_ty.as_ref().clone()),
            // Every caller gates on `Object::HostClosure` before calling.
            _ => unreachable!("host_closure_call_sysop requires an Object::HostClosure"),
        };
        debug_assert_eq!(
            user_args.len(),
            arity,
            "HostClosure call: drained {} args but declared arity is {arity}",
            user_args.len(),
        );
        let args_array_ptr = self.tlab.alloc(Object::Array(user_args.into()));
        let ret_ty_ptr = self.tlab.alloc(Object::Type(Box::new(ret_ty)));
        VmExecState::SysOp {
            operation: bex_vm_types::SysOp::BamlHostCallHostValue,
            args: vec![
                Value::object(closure_ptr),
                Value::object(args_array_ptr),
                Value::object(ret_ty_ptr),
            ],
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
        // A host closure isn't a Function/Closure/BoundMethod and dispatches via
        // a single-yield sys-op rather than a pushed frame. This path is reached
        // when a host callable is invoked *indirectly* — e.g. handed to a native
        // higher-order builtin like `array.map(f)`, whose `YieldToCall` funnels
        // its callback through here (a direct `f(x)` is handled inline by the
        // `CallIndirect` opcodes). The call args are already on the stack at
        // `locals_offset`; drain them and yield. The Native continuation frame
        // the caller pushed resumes — with the host result on the stack — once
        // the engine completes the op, exactly as for a bytecode callback's
        // return value.
        if matches!(self.get_object(callee_ptr), Object::HostClosure(_)) {
            let user_args: Vec<Value> = self.stack.drain(locals_offset..).collect();
            return Ok(Some(self.host_closure_call_sysop(callee_ptr, user_args)));
        }

        // Extract captured_type_args from a Closure callee before we discard
        // the concrete Closure type in favour of the inner Function.
        // These are injected into the new BytecodeFrame after it is created.
        let closure_type_args: Vec<baml_type::Ty> = match self.get_object(callee_ptr) {
            Object::Closure(c) => c.captured_type_args.clone(),
            _ => vec![],
        };

        // For BoundMethod callees, extract the receiver's class_type_args so
        // they can seed frame.type_args before call-site explicit type args are
        // appended.  This implements the De Bruijn ordering:
        //   frame.type_args = receiver.class_type_args ++ explicit_call_site_args
        // matching enclosing_generic_params() which puts class params first.
        let bound_method_class_type_args: Vec<baml_type::Ty> = match self.get_object(callee_ptr) {
            Object::BoundMethod(bm) => match bm.receiver.as_object_ptr() {
                Some(recv_ptr) => match self.get_object(recv_ptr) {
                    Object::Instance(inst) => inst.class_type_args.clone(),
                    _ => vec![],
                },
                None => vec![],
            },
            _ => vec![],
        };

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
                // SmallVec avoids heap allocation for calls with ≤4 args (the common case).
                let args: SmallVec<[Value; 4]> = self.stack.drain(locals_offset..).collect();

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
                        type_args: callback_type_args,
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

                        // Mirror the Call-instruction's type-arg plumbing
                        // (see vm.rs:3757) so a native helper that yields with
                        // explicit `type_args` (e.g. `baml.json.from_json`
                        // dispatching a generic class' `from_json`) seeds the
                        // callee's frame correctly.  Save/restore around the
                        // dispatch matches the Call-instruction handler.
                        let prev_pending = std::mem::replace(
                            &mut self.pending_call_type_args,
                            callback_type_args.clone(),
                        );
                        let frames_before = self.frames.len();

                        let result = self.execute_call_from_locals_offset(
                            real_callee,
                            cb_locals,
                            arg_count,
                            frame_idx,
                            function,
                        );

                        self.pending_call_type_args = prev_pending;

                        // Append explicit type-args to the newly-pushed
                        // bytecode frame's `type_args` (after class-args from
                        // BoundMethod / Closure seeding).
                        if !callback_type_args.is_empty() && self.frames.len() > frames_before {
                            if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                                bf.type_args.extend(callback_type_args);
                            }
                        }

                        // Update *frame_idx to point at the new topmost frame.
                        // Required so the caller's tight inner-dispatch loop
                        // detects the frame change (the pushed Native
                        // continuation frame for the outer native that
                        // yielded) and breaks out to let exec_compact's
                        // continuation handler run it.  Without this, when the
                        // recursive callback was itself a Native that returned
                        // Done synchronously (no frame_idx update from the
                        // recursion), the inner loop would continue stepping
                        // the caller's bytecode with a stale Native frame on
                        // top — eventually reading past the caller's code end.
                        if !self.frames.is_empty() {
                            *frame_idx = self.frames.len() - 1;
                        }

                        return result;
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
                // Seed frame.type_args from:
                //  1. BoundMethod callees: the receiver's class_type_args (De
                //     Bruijn slot 0..n_class_params).  The Call-instruction
                //     writeback at vm.rs:3619-3629 appends explicit call-site
                //     type args after these, preserving ordering
                //     [class_args, fn_args].
                //  2. Closure callees: captured_type_args (whole-frame snapshot
                //     taken at MakeClosure time; enclosing_generic_params()
                //     already widened to class+fn params so the ordering is
                //     consistent).
                //  3. Plain Function callees: vec![] (no-op).
                //
                // Note: BoundMethod takes priority over Closure; a method can
                // never be both simultaneously.
                let initial_type_args = if !bound_method_class_type_args.is_empty() {
                    bound_method_class_type_args
                } else {
                    closure_type_args
                };
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function: callee_ptr,
                    instruction_ptr: 0,
                    locals_offset,
                    type_args: initial_type_args,
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
            VmRustFnError::Thrown(value) => VmError::Thrown(value),
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
                    match self.interrupt(filter_func, &[state.value])? {
                        VmExecState::Complete(v) => match v.as_bool() {
                            Some(true) => filtered_notifications.push(notification),
                            Some(false) => {}
                            None => {
                                return Err(VmInternalError::TypeError {
                                    expected: Type::Bool,
                                    got: self.type_of(&v),
                                }
                                .into());
                            }
                        },
                        _ => return Err(VmInternalError::ExpectedCompletion.into()),
                    }
                }
            }
        }

        Ok(filtered_notifications)
    }

    fn init_spread(
        &mut self,
        dest_value: Value,
        source_value: Value,
        field_copy_set: &bytecode::FieldCopySet,
    ) -> Result<Option<VmExecState>, VmError> {
        let dest_ptr = self.as_object_ptr(dest_value, ObjectType::Instance)?;
        let source_ptr = self.as_object_ptr(source_value, ObjectType::Instance)?;

        let copied_fields = {
            let Object::Instance(source) = self.get_object(source_ptr) else {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Instance.into(),
                    got: ObjectType::of(self.get_object(source_ptr)).into(),
                }
                .into());
            };
            let Object::Instance(dest) = self.get_object(dest_ptr) else {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Instance.into(),
                    got: ObjectType::of(self.get_object(dest_ptr)).into(),
                }
                .into());
            };

            let mut copied_fields = Vec::with_capacity(field_copy_set.fields.len());
            let mut invalid_field_access = None;
            for copy in &field_copy_set.fields {
                let Some(old_value) = dest.try_load_field(copy.dest) else {
                    invalid_field_access = Some((copy.dest, dest.field_len()));
                    break;
                };
                let Some(new_value) = source.try_load_field(copy.source) else {
                    invalid_field_access = Some((copy.source, source.field_len()));
                    break;
                };
                copied_fields.push((copy.dest, old_value, new_value));
            }
            if let Some((index, field_count)) = invalid_field_access {
                return Err(self.invalid_field_access_error(index, field_count));
            }

            copied_fields
        };

        let watched_node = NodeId::HeapObject(dest_ptr);
        let roots = self.watch.copy_roots_reaching(watched_node);
        let mut old_roots_copies = Vec::with_capacity(roots.len());
        for &root in &roots {
            if let Some(val) = self.watch.root_state(root).map(|s| s.value) {
                old_roots_copies.push(crate::package_baml::PackageBamlImpl::deep_copy(self, &val));
            }
        }

        for (dest_field, old_value, new_value) in copied_fields {
            self.update_watched_node_dependencies(
                watched_node,
                watch::Path::InstanceField(dest_field),
                old_value,
                new_value,
            );
            let store_error = {
                let Object::Instance(dest) = self.get_object(dest_ptr) else {
                    unreachable!("destination instance already type-checked above");
                };
                (dest_field >= dest.field_len()).then_some(dest.field_len())
            };
            if let Some(length) = store_error {
                return Err(self.invalid_field_access_error(dest_field, length));
            }
            self.heap.write_barrier(dest_ptr, new_value);
            let Object::Instance(dest) = self.get_object(dest_ptr) else {
                unreachable!("destination instance already type-checked above");
            };
            dest.store_field(dest_field, new_value);
        }

        for (&root, old_value) in roots.iter().zip(old_roots_copies) {
            if let Some(state) = self.watch.root_state_mut(root) {
                state.last_assigned = Some(old_value);
            }
        }

        let notifications = self.process_notifications(watched_node)?;
        if !notifications.is_empty() {
            return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                notifications,
            ))));
        }

        Ok(None)
    }

    fn alloc_initialized_instance(
        &mut self,
        plan: &bytecode::ClassInitPlan,
    ) -> Result<Value, VmError> {
        let class_ptr = self.idx_to_ptr(plan.class_obj);
        let class_field_count = match self.get_object(class_ptr) {
            Object::Class(class) => class.fields.len(),
            other => {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Class.into(),
                    got: ObjectType::of(other).into(),
                }
                .into());
            }
        };
        let field_value_count = plan.fields.len();
        let ntypeargs = plan.ntypeargs as usize;
        let total_inputs = field_value_count + ntypeargs;
        let base = self
            .stack
            .len()
            .checked_sub(total_inputs)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(total_inputs))?;

        let mut class_type_args = Vec::with_capacity(ntypeargs);
        for offset in 0..ntypeargs {
            let slot = base + field_value_count + offset;
            let value = self.stack[StackIndex::from_raw(slot)];
            let ptr = self.as_object_ptr(value, ObjectType::Type)?;
            let Object::Type(ty) = self.get_object(ptr) else {
                unreachable!("as_object_ptr guarantees Type variant");
            };
            class_type_args.push(*ty.clone());
        }

        let fields = if field_value_count == class_field_count
            && plan
                .fields
                .iter()
                .copied()
                .enumerate()
                .all(|(idx, field_idx)| idx == field_idx)
        {
            let fields = self
                .stack
                .drain(StackIndex::from_raw(base)..StackIndex::from_raw(base + field_value_count))
                .collect();
            if ntypeargs > 0 {
                drop(self.stack.drain(StackIndex::from_raw(base)..));
            }
            fields
        } else {
            let mut fields = vec![Value::NULL; class_field_count];
            let mut inputs = self.stack.drain(StackIndex::from_raw(base)..);
            for (field_idx, value) in plan
                .fields
                .iter()
                .copied()
                .zip((&mut inputs).take(field_value_count))
            {
                fields[field_idx] = value;
            }
            drop(inputs);
            fields
        };

        Ok(Value::object(self.tlab.alloc(Object::Instance(
            Instance::new(class_ptr, class_type_args, fields),
        ))))
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

        self.update_watched_node_dependencies(watched_node, path, old_value, new_value);

        for (&root, old_value) in roots.iter().zip(old_roots_copies) {
            if let Some(state) = self.watch.root_state_mut(root) {
                state.last_assigned = Some(old_value);
            }
        }
    }

    fn update_watched_node_dependencies(
        &mut self,
        watched_node: NodeId,
        path: watch::Path,
        old_value: Value,
        new_value: Value,
    ) {
        if let Some(old) = old_value.as_object_ptr() {
            self.watch
                .unlink_edge(watched_node, path.clone(), NodeId::HeapObject(old));
        }

        if let Some(new) = new_value.as_object_ptr() {
            watch::track_watch_dependencies(&mut self.watch, watched_node, path, new);
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
        // Re-arm the long-running-loop detector at every yield boundary so
        // each `exec()` call starts with a fresh budget; a single `exec()`
        // call yields back to the embedder eventually (e.g. via `Await`,
        // `EarlyYield`, etc.), which is the right granularity for the
        // counter to reset at.
        self.early_yield.reset();

        match self.exec_inner() {
            Err(VmError::InternalError(err)) => {
                let trace = self.capture_stack_trace();
                Err(VmError::TracedInternalError { source: err, trace })
            }
            other => other,
        }
    }

    #[allow(clippy::inline_always)] // Measured: 20-40% speedup from inlining the dispatch loop
    #[inline(always)]
    fn exec_inner(&mut self) -> Result<VmExecState, VmError> {
        if self.frames.is_empty() {
            return Ok(VmExecState::Complete(Value::NULL));
        }
        self.exec_compact()
    }

    // ── Shared helpers for compact dispatch ──────────────────────────────────

    /// Execute a comparison operation. Pops two values, pushes a Bool.
    /// Shared between the legacy `step()` `CmpOp` arm and the expanded compact opcodes.
    fn exec_cmpop(&mut self, op: CmpOp) -> Result<(), VmError> {
        let right = self.stack.ensure_pop();
        let left = self.stack.ensure_pop();

        #[allow(clippy::cast_precision_loss, clippy::float_cmp)]
        let result = if let (Some(l), Some(r)) = (left.as_int(), right.as_int()) {
            Value::bool(match op {
                CmpOp::Eq => l == r,
                CmpOp::NotEq => l != r,
                CmpOp::Lt => l < r,
                CmpOp::LtEq => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::GtEq => l >= r,
            })
        } else if let (Some(l), Some(r)) = (
            left.as_int()
                .map(|i| i as f64)
                .or_else(|| value_as_float(left)),
            right
                .as_int()
                .map(|i| i as f64)
                .or_else(|| value_as_float(right)),
        ) {
            Value::bool(match op {
                CmpOp::Eq => l == r,
                CmpOp::NotEq => l != r,
                CmpOp::Lt => l < r,
                CmpOp::LtEq => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::GtEq => l >= r,
            })
        } else if let (Some(l), Some(r)) = (
            self.value_as_bigint_cow(left),
            self.value_as_bigint_cow(right),
        ) {
            // Mixed `bigint`/`int` comparison reached via the generic path: one
            // operand's static `bigint` type was erased (e.g. a union/`any`
            // operand), so emit produced a generic `CmpOp` rather than
            // `CmpBigint*`. The `int` operand is widened to a local `BigInt`;
            // comparison is by value, matching the specialized path. (Both
            // operands being `int` is already handled by the first arm above,
            // so at least one is a `bigint` here.)
            Value::bool(match op {
                CmpOp::Eq => l == r,
                CmpOp::NotEq => l != r,
                CmpOp::Lt => l < r,
                CmpOp::LtEq => l <= r,
                CmpOp::Gt => l > r,
                CmpOp::GtEq => l >= r,
            })
        } else if let (Some(li), Some(ri)) = (left.as_object_ptr(), right.as_object_ptr()) {
            let lobj = self.get_object(li);
            let robj = self.get_object(ri);
            match (lobj, robj) {
                (Object::String(_), Object::String(_)) => {
                    let ls = self.as_string(&left)?;
                    let rs = self.as_string(&right)?;
                    Value::bool(match op {
                        CmpOp::Eq => ls == rs,
                        CmpOp::NotEq => ls != rs,
                        CmpOp::Lt => ls < rs,
                        CmpOp::LtEq => ls <= rs,
                        CmpOp::Gt => ls > rs,
                        CmpOp::GtEq => ls >= rs,
                    })
                }
                (Object::Uint8Array(_), Object::Uint8Array(_)) => {
                    let la = self.as_uint8array(&left)?.to_vec();
                    let ra = self.as_uint8array(&right)?.to_vec();
                    Value::bool(match op {
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
                (Object::Variant(lv), Object::Variant(rv)) => Value::bool(match op {
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
                }),
                (Object::Type(lt), Object::Type(rt)) => Value::bool(match op {
                    CmpOp::Eq => lt == rt,
                    CmpOp::NotEq => lt != rt,
                    _ => {
                        return Err(VmInternalError::CannotApplyCmpOp {
                            left: bex_vm_types::types::Type::Object(ObjectType::Type),
                            right: bex_vm_types::types::Type::Object(ObjectType::Type),
                            op,
                        }
                        .into());
                    }
                }),
                // (Bigint, Bigint) — and any bigint/int mix — is handled by the
                // `value_as_bigint_cow` branch above, before this object match.
                _ => Value::bool(match op {
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
            }
        } else {
            Value::bool(match op {
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
            })
        };

        self.stack.push(result);
        Ok(())
    }

    /// Execute a binary arithmetic operation. Pops two values, pushes the result.
    /// Shared between the legacy `step()` `BinOp` arm and the compact `Add` opcode
    /// (which needs string concatenation in addition to numeric dispatch).
    fn exec_binop(&mut self, op: BinOp) -> Result<(), VmError> {
        let right = self.stack.ensure_pop();
        let left = self.stack.ensure_pop();

        #[allow(clippy::cast_precision_loss)]
        let result = if let (Some(l), Some(r)) = (left.as_int(), right.as_int()) {
            Value::int(match op {
                BinOp::Div if r == 0 => {
                    return Err(VmError::Thrown(self.panic_to_exception_value(
                        VmPanic::DivisionByZero {
                            left: Value::int(l),
                            right: Value::int(r),
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
            })
        } else if let (Some(l), Some(r)) = (
            left.as_int()
                .map(|i| i as f64)
                .or_else(|| value_as_float(left)),
            right
                .as_int()
                .map(|i| i as f64)
                .or_else(|| value_as_float(right)),
        ) {
            let f = match op {
                BinOp::Div if r == 0.0 => {
                    // Reuse the heap-boxed float operands directly; only
                    // allocate when a side was an Int promoted to f64
                    // (since the panic payload is conventionally a Float
                    // value here, matching the operation's result type).
                    let left_v = if left.is_object() {
                        left
                    } else {
                        self.alloc_float(l)
                    };
                    let right_v = if right.is_object() {
                        right
                    } else {
                        self.alloc_float(r)
                    };
                    return Err(VmError::Thrown(self.panic_to_exception_value(
                        VmPanic::DivisionByZero {
                            left: left_v,
                            right: right_v,
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
                        left: self.type_of(&left),
                        right: self.type_of(&right),
                        op,
                    }
                    .into());
                }
            };
            self.alloc_float(f)
        } else if left.is_object() && right.is_object() && op == BinOp::Add {
            let ls = self.as_string(&left)?;
            let rs = self.as_string(&right)?;
            let result = bex_str::BexStr::concat(ls.clone(), rs.clone());
            self.alloc_string(result)
        } else {
            return Err(VmInternalError::CannotApplyBinOp {
                left: self.type_of(&left),
                right: self.type_of(&right),
                op,
            }
            .into());
        };

        self.stack.push(result);
        Ok(())
    }

    /// Compact bytecode dispatch loop.
    ///
    /// Reads opcodes from `CompactCode.code` instead of indexing `Vec<Instruction>`.
    /// `instruction_ptr` is a byte offset into the code array.
    ///
    /// Key optimization: `pc` and `code` are kept as local variables in the hot
    /// loop, avoiding frame access on every instruction. They are only saved back
    /// to the frame when control flow changes (calls, returns, exceptions, yields).
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn exec_compact(&mut self) -> Result<VmExecState, VmError> {
        if self.frames.is_empty() {
            return Ok(VmExecState::Complete(Value::NULL));
        }

        let mut frame_idx = self.frames.len() - 1;
        let mut function = unsafe { self.load_function(frame_idx)? };

        // Outer loop handles CPS continuations and frame transitions.
        loop {
            // ── CPS continuation handler (identical to exec_inner) ────────
            while matches!(self.frames.last(), Some(Frame::Native(_))) {
                let v = self.stack.ensure_pop();
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
                        type_args: callback_type_args,
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

                        // Mirror the Call-instruction's type-arg plumbing for
                        // continuation-driven YieldToCall.
                        let prev_pending = std::mem::replace(
                            &mut self.pending_call_type_args,
                            callback_type_args.clone(),
                        );
                        let frames_before = self.frames.len();

                        let ecflo_outcome = self.execute_call_from_locals_offset(
                            real_callee,
                            cb_locals,
                            arg_count,
                            &mut frame_idx,
                            &mut function,
                        );

                        self.pending_call_type_args = prev_pending;

                        if !callback_type_args.is_empty() && self.frames.len() > frames_before {
                            if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(frame_idx) {
                                bf.type_args.extend(callback_type_args);
                            }
                        }

                        let ecflo_result = match ecflo_outcome {
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
                return Ok(VmExecState::Complete(self.stack.ensure_pop()));
            }

            frame_idx = self.frames.len() - 1;
            function = unsafe { self.load_function(frame_idx)? };

            // ── Extract locals for the tight inner dispatch loop ──────────
            // SAFETY: code is &'static because Function is &'static.
            let code: &'static [u8] = &function.bytecode.compact.as_ref().unwrap().code;
            let Frame::Bytecode(bf) = &mut self.frames[frame_idx] else {
                unreachable!(
                    "exec_compact loop frame is always Bytecode after continuation handler"
                );
            };
            let mut pc = bf.instruction_ptr;

            // ── Tight inner dispatch loop ─────────────────────────────────
            // pc/code/function/frame_idx are kept as locals across many
            // instructions — we only break out (re-extracting from the frame)
            // on actual control-flow changes (Call, Return, Throw).
            //
            // Critical perf: simple ops (arithmetic, load/store, jumps within
            // the same function) never touch the frame's instruction_ptr.
            loop {
                let orig_frame_idx = frame_idx;
                let step_result = self.step_compact(&mut pc, &mut frame_idx, &mut function, code);

                match step_result {
                    Ok(None) if frame_idx == orig_frame_idx => {
                        // Simple op, frame unchanged. pc is already advanced
                        // as a local — no frame write needed. Continue tight loop.
                        continue;
                    }
                    Ok(None) => {
                        // Frame changed (Call/Return) — Call saved pc before the
                        // call. Re-extract code/function for the new frame.
                        break;
                    }
                    Ok(Some(state)) => {
                        // Yielding — save pc to current frame so we can resume.
                        if frame_idx == orig_frame_idx {
                            if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(frame_idx) {
                                bf.instruction_ptr = pc;
                            }
                        }
                        return Ok(state);
                    }
                    Err(VmError::InternalError(err)) => {
                        return Err(VmError::InternalError(err));
                    }
                    Err(VmError::Thrown(exception_value)) => {
                        // Throw saves pc inside its handler before unwinding.
                        self.try_unwind_exception(&mut frame_idx, &mut function, exception_value)?;
                        break; // re-extract code/function after unwind
                    }
                    Err(
                        e @ (VmError::ThrownUnhandled { .. } | VmError::TracedInternalError { .. }),
                    ) => return Err(e),
                }
            }
        }
    }

    /// Execute a single compact-encoded instruction.
    ///
    /// `pc` is a local from the caller's tight loop — operand reads advance it
    /// directly without going through frame indirection. The caller saves `pc`
    /// back to the frame after this returns.
    #[allow(
        clippy::too_many_lines,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::cast_lossless,
        clippy::useless_conversion,
        clippy::inline_always
    )]
    #[inline(always)]
    fn step_compact(
        &mut self,
        pc: &mut usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        code: &'static [u8],
    ) -> Result<Option<VmExecState>, VmError> {
        use bex_vm_types::bytecode::OpCode;

        // Unchecked byte-stream readers. SAFETY: the compact bytecode is produced
        // by our own encoder which guarantees correct sizes (verified by
        // debug_assert_eq!(code.len(), byte_offset) in lower_to_compact pass 2).
        // The PC always stays in bounds during well-formed execution.
        #[inline(always)]
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

        #[inline(always)]
        unsafe fn read_u16_unchecked(code: &[u8], pc: &mut usize) -> u16 {
            unsafe {
                let p = *pc;
                let bytes = [*code.get_unchecked(p), *code.get_unchecked(p + 1)];
                *pc = p + 2;
                u16::from_le_bytes(bytes)
            }
        }

        #[inline(always)]
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

        #[inline(always)]
        unsafe fn read_i8_unchecked(code: &[u8], pc: &mut usize) -> i8 {
            unsafe {
                let val = *code.get_unchecked(*pc) as i8;
                *pc += 1;
                val
            }
        }

        // Read opcode byte and advance PC past it.
        // SAFETY: PC is always in bounds (bytecode invariant).
        // Read opcode byte and advance PC past it.
        // SAFETY: PC is always in bounds (bytecode invariant).
        #[allow(unsafe_code)]
        let op_byte = unsafe { *code.get_unchecked(*pc) };
        *pc += 1;

        // Save faulting PC for error reporting (points to the opcode byte).
        let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
            verifier_unreachable!()
        };
        bf.faulting_pc = *pc - 1;

        // SAFETY: OpCode is #[repr(u8)] and the compact bytecode is produced by our
        // own encoder which only emits valid opcode bytes.
        #[allow(unsafe_code)]
        let op: OpCode = unsafe { std::mem::transmute(op_byte) };

        // Tagged-int comparisons skip untagging by comparing bits directly
        // (see `Value::tagged_int_add` for the encoding rationale; the
        // shift-left-by-1 preserves signed ordering between operands that
        // share the same tag bit). Float comparisons unwrap two heap-boxed
        // floats and apply the operator; both pops are guaranteed Float by
        // the bytecode encoder, so the `else` arms are unreachable.
        macro_rules! cmp_int_op {
            ($op:tt) => {{
                let r = self.stack.ensure_pop();
                let l = self.stack.ensure_pop();
                self.stack
                    .push(Value::bool((l.bits() as i64) $op (r.bits() as i64)));
            }};
        }
        macro_rules! cmp_float_op {
            ($op:tt) => {{
                let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                    std::hint::unreachable_unchecked()
                };
                let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                    std::hint::unreachable_unchecked()
                };
                self.stack.push(Value::bool(l $op r));
            }};
        }

        // SAFETY: see above — bytecode invariants guarantee all reads are in bounds.
        #[allow(unused_unsafe)]
        unsafe {
            match op {
                // ── Common constants ──────────────────────────────────────────
                OpCode::LoadNull => {
                    self.stack.push(Value::NULL);
                }
                OpCode::LoadTrue => {
                    self.stack.push(Value::bool(true));
                }
                OpCode::LoadFalse => {
                    self.stack.push(Value::bool(false));
                }
                OpCode::LoadIntSmall => {
                    let val = { read_i8_unchecked(code, pc) };
                    self.stack.push(Value::int(i64::from(val)));
                }

                // ── LoadConst ─────────────────────────────────────────────────
                OpCode::LoadConst => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let val = function.bytecode.resolved_constants[idx];
                    self.stack.push(val);
                }

                // ── LoadVar / StoreVar ────────────────────────────────────────
                OpCode::LoadVar => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let stack_slot = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack[stack_slot];
                    self.stack.push(value);
                }

                OpCode::StoreVar => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack.ensure_pop();
                    if let Some(state) = self.store_local_value(local_var_index, value)? {
                        return Ok(Some(state));
                    }
                }

                OpCode::StoreVarLoadVar => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value_slot = self.stack.ensure_slot_from_top(0);
                    let value = self.stack[value_slot];
                    if let Some(state) = self.store_local_value(local_var_index, value)? {
                        return Ok(Some(state));
                    }
                }

                // ── LoadGlobal / StoreGlobal ──────────────────────────────────
                OpCode::LoadGlobal => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.globals.get(self.proof(), global_idx);
                    self.stack.push(value);
                }

                OpCode::StoreGlobal => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.stack.ensure_pop();
                    // Only valid during `$init`; post-init globals are frozen in `Arc<[Value]>`
                    // and a write here is a VM internal error.
                    self.globals
                        .set(global_idx, value, VmInternalError::StoreGlobalAfterInit)?;
                }

                // ── LoadField / StoreField / InitField ────────────────────────
                OpCode::LoadField => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let top = self.stack.ensure_pop();
                    let obj_ptr = self.as_object_ptr(top, ObjectType::Instance)?;
                    let load_result = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        instance
                            .try_load_field(idx)
                            .ok_or_else(|| instance.field_len())
                    };
                    let value = match load_result {
                        Ok(value) => value,
                        Err(length) => {
                            return Err(self.invalid_field_access_error(idx, length));
                        }
                    };
                    self.stack.push(value);
                }

                OpCode::StoreField => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let new_value = self.stack.ensure_pop();
                    let instance_value = self.stack.ensure_pop();
                    let obj_ptr = self.as_object_ptr(instance_value, ObjectType::Instance)?;

                    let old_value = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        instance
                            .try_load_field(idx)
                            .ok_or_else(|| instance.field_len())
                    };
                    let old_value = match old_value {
                        Ok(old_value) => old_value,
                        Err(length) => {
                            return Err(self.invalid_field_access_error(idx, length));
                        }
                    };

                    let watched_node = NodeId::HeapObject(obj_ptr);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::InstanceField(idx),
                        old_value,
                        new_value,
                    );
                    let store_error = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            unreachable!("already type-checked above");
                        };
                        (idx >= instance.field_len()).then_some(instance.field_len())
                    };
                    if let Some(length) = store_error {
                        return Err(self.invalid_field_access_error(idx, length));
                    }
                    self.heap.write_barrier(obj_ptr, new_value);
                    let Object::Instance(instance) = self.get_object(obj_ptr) else {
                        unreachable!("already type-checked above");
                    };
                    instance.store_field(idx, new_value);

                    let notifications = self.process_notifications(watched_node)?;
                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }

                OpCode::InitField => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let new_value = self.stack.ensure_pop();
                    let instance_value = self.stack.ensure_pop();
                    let obj_ptr = self.as_object_ptr(instance_value, ObjectType::Instance)?;
                    let store_error = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        (idx >= instance.field_len()).then_some(instance.field_len())
                    };
                    if let Some(length) = store_error {
                        return Err(self.invalid_field_access_error(idx, length));
                    }
                    self.heap.write_barrier(obj_ptr, new_value);
                    let Object::Instance(instance) = self.get_object(obj_ptr) else {
                        unreachable!("already type-checked above");
                    };
                    instance.store_field(idx, new_value);
                    self.stack.push(instance_value);
                }

                OpCode::InitSpread => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let source_value = self.stack.ensure_pop();
                    let dest_slot = self.stack.ensure_slot_from_top(0);
                    let dest_value = self.stack[dest_slot];
                    if let Some(state) = self.init_spread(
                        dest_value,
                        source_value,
                        &function.bytecode.field_copy_sets[idx],
                    )? {
                        return Ok(Some(state));
                    }
                }

                // ── Pop / Copy ────────────────────────────────────────────────
                OpCode::Pop => {
                    let n = { read_u32_unchecked(code, pc) as usize };
                    let drain_start = self.stack.len() - n;
                    let drain_range = StackIndex::from_raw(drain_start)..;
                    self.stack.drain(drain_range);
                }

                OpCode::Copy => {
                    let offset = { read_u32_unchecked(code, pc) as usize };
                    let index = self.stack.ensure_slot_from_top(offset);
                    let value = self.stack[index];
                    self.stack.push(value);
                }

                // ── Allocation opcodes ────────────────────────────────────────
                OpCode::AllocArray => {
                    let size = { read_u32_unchecked(code, pc) as usize };
                    let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                    let array: Vec<Value> = self.stack.drain(drain_range).collect();
                    let array_index = self.tlab.alloc(Object::Array(array.into()));
                    self.stack.push(Value::object(array_index));
                }

                OpCode::AllocMap => {
                    let n = { read_u32_unchecked(code, pc) as usize };
                    let map = if n > 0 {
                        let end_of_values = self.stack.ensure_slot_from_top(2 * n - 1);
                        let end_of_keys = self.stack.ensure_slot_from_top(n - 1);
                        let idx_of_last_key = self.stack.ensure_slot_from_top(n - 1);
                        let values = self.stack[end_of_values..end_of_keys].iter().copied();
                        let keys = self.stack[idx_of_last_key..].iter().map(|k| {
                            let obj_index = self.as_object_ptr(*k, ObjectType::String)?;
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
                    let obj_index = self.tlab.alloc(Object::Map(map.into()));
                    self.stack.push(Value::object(obj_index));
                }

                OpCode::AllocInstance => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let ntypeargs = { read_u16_unchecked(code, pc) } as usize;
                    let class_ptr = self.idx_to_ptr(ObjectIndex::from_raw(raw as usize));

                    // Pop class-level type args from the stack (sitting below any
                    // field init instructions that follow).
                    let class_type_args: Vec<baml_type::Ty> = if ntypeargs > 0 {
                        let base = self
                            .stack
                            .len()
                            .checked_sub(ntypeargs)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(ntypeargs))?;
                        let mut collected = Vec::with_capacity(ntypeargs);
                        for slot in base..(base + ntypeargs) {
                            let v = self.stack[StackIndex::from_raw(slot)];
                            let ptr = self.as_object_ptr(v, ObjectType::Type)?;
                            let Object::Type(ty) = self.get_object(ptr) else {
                                unreachable!("as_object_ptr guarantees Type variant");
                            };
                            collected.push(*ty.clone());
                        }
                        for _ in 0..ntypeargs {
                            self.stack.remove(base);
                        }
                        collected
                    } else {
                        vec![]
                    };

                    let Object::Class(class) = self.get_object(class_ptr) else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Class.into(),
                            got: ObjectType::of(self.get_object(class_ptr)).into(),
                        }
                        .into());
                    };
                    let mut fields = Vec::with_capacity(class.fields.len());
                    fields.resize(class.fields.len(), Value::NULL);
                    let instance_ptr =
                        self.tlab
                            .alloc(Object::Instance(bex_vm_types::types::Instance::new(
                                class_ptr,
                                class_type_args,
                                fields,
                            )));
                    self.stack.push(Value::object(instance_ptr));
                }

                OpCode::InitInstance => {
                    let plan_idx = { read_u32_unchecked(code, pc) as usize };
                    let instance = self.alloc_initialized_instance(
                        &function.bytecode.class_init_plans[plan_idx],
                    )?;
                    self.stack.push(instance);
                }

                OpCode::AllocVariant => {
                    let raw = { read_u32_unchecked(code, pc) };
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
                    let variant = self.stack.ensure_pop();
                    let Some(variant_index) = variant.as_int() else {
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
                    self.stack.push(Value::object(variant_ptr));
                }

                // ── SysOp (BEP-034 phase D′) ──────────────────────────────────
                OpCode::SysOp => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let callee = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.globals.get(self.proof(), callee);
                    let expected_type = FunctionType::SysOp;
                    let callee_ptr = self.as_object_ptr(callee_value, expected_type.into())?;
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
                    let call_args: Vec<Value> = self.stack.drain(args_offset..).collect();
                    return Ok(Some(VmExecState::SysOp {
                        operation: sys_op,
                        args: call_args,
                    }));
                }

                // ── Spawn (BEP-034) ────────────────────────────────────────────
                OpCode::Spawn => {
                    let name_value = self.stack.ensure_pop();
                    let closure_value = self.stack.ensure_pop();
                    let closure_ptr =
                        self.as_object_ptr(closure_value, ObjectType::Function(FunctionType::Any))?;
                    let name_ptr = if name_value.is_null() {
                        None
                    } else if let Some(ptr) = name_value.as_object_ptr() {
                        Some(ptr)
                    } else {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Object(ObjectType::String),
                            got: self.type_of(&name_value),
                        }
                        .into());
                    };
                    let pending_future = bex_vm_types::types::UnscheduledFuture {
                        closure: closure_ptr,
                        name: name_ptr,
                    };
                    let object_index = self.tlab.alloc(Object::UnscheduledFuture(pending_future));
                    return Ok(Some(VmExecState::Spawn(object_index)));
                }

                // ── Watch / Unwatch / Notify / NotifyBlock ────────────────────
                OpCode::Watch => {
                    let index = { read_u32_unchecked(code, pc) as usize };
                    let popped = self.stack.ensure_pop();
                    let filter = if popped.is_null() {
                        WatchFilter::Default
                    } else if let Some(object_index) = popped.as_object_ptr() {
                        match self.get_object(object_index) {
                            Object::Function(_) => WatchFilter::Function(object_index),
                            Object::String(mode) if mode == "manual" => WatchFilter::Manual,
                            Object::String(mode) if mode == "never" => WatchFilter::Paused,
                            _ => return Err(VmInternalError::InvalidFilter.into()),
                        }
                    } else {
                        return Err(VmInternalError::InvalidFilter.into());
                    };
                    let channel_value = self.stack.ensure_pop();
                    let channel = self.as_string(&channel_value)?.to_string();
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
                    if let Some(object_index) = value.as_object_ptr() {
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
                    let index = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, index);
                    if self.watched_vars.remove(&local_var_index).is_some() {
                        let var_node = NodeId::LocalVar(local_var_index);
                        self.watch.unregister_root(var_node);
                        let value = self.stack[local_var_index];
                        if let Some(object_index) = value.as_object_ptr() {
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
                    let index = { read_u32_unchecked(code, pc) as usize };
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
                    let block_index = { read_u32_unchecked(code, pc) as usize };
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
                    let index = { read_u32_unchecked(code, pc) as usize };
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
                    let raw = read_u32_unchecked(code, pc);
                    let ntypeargs = read_u16_unchecked(code, pc) as usize;
                    let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.globals.get(self.proof(), callee_global);
                    let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;

                    // Pop `ntypeargs` Object::Type values from the stack into a Vec<Ty>.
                    // These sit below the regular value args on the stack.
                    let type_args: Vec<baml_type::Ty> = if ntypeargs > 0 {
                        let total_needed = arg_count + ntypeargs;
                        let base = self
                            .stack
                            .len()
                            .checked_sub(total_needed)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(total_needed))?;
                        let mut collected = Vec::with_capacity(ntypeargs);
                        for slot in base..(base + ntypeargs) {
                            let v = self.stack[StackIndex::from_raw(slot)];
                            let ptr = self.as_object_ptr(v, ObjectType::Type)?;
                            let Object::Type(ty) = self.get_object(ptr) else {
                                unreachable!("as_object_ptr guarantees Type variant");
                            };
                            collected.push(*ty.clone());
                        }
                        for _ in 0..ntypeargs {
                            self.stack.remove(base);
                        }
                        collected
                    } else {
                        vec![]
                    };

                    let args_offset = self
                        .stack
                        .len()
                        .checked_sub(arg_count)
                        .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count))?;
                    let locals_offset = StackIndex::from_raw(args_offset);
                    // Save pc as return address before pushing new frame.
                    let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                        verifier_unreachable!()
                    };
                    bf.instruction_ptr = *pc;

                    let frames_before = self.frames.len();
                    // Mirror the native YieldToCall plumbing: stash type_args
                    // into `pending_call_type_args` so a native callee can
                    // read them via `current_call_type_args()` (e.g.
                    // `baml.json.from_json<T>` reads its `T` from there).
                    let prev_pending =
                        std::mem::replace(&mut self.pending_call_type_args, type_args.clone());
                    let result = self.execute_call_from_locals_offset(
                        callee_ptr,
                        locals_offset,
                        arg_count,
                        frame_idx,
                        function,
                    );
                    self.pending_call_type_args = prev_pending;
                    if !type_args.is_empty() && self.frames.len() > frames_before {
                        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                            bf.type_args.extend(type_args);
                        }
                    }
                    return result;
                }

                // ── CallIndirect ──────────────────────────────────────────────
                OpCode::CallIndirect => {
                    // Save pc as return address before any call.
                    let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                        verifier_unreachable!()
                    };
                    bf.instruction_ptr = *pc;

                    let callee_slot = self.stack.ensure_stack_top();
                    let callee_value = self.stack[callee_slot];
                    let callee_ptr =
                        self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                    let obj = self.get_object(callee_ptr);

                    if let Object::HostClosure(host_closure) = obj {
                        // Host-callable dispatch via a single-yield sys-op. See
                        // `Instruction::CallIndirect` / `host_closure_call_sysop`
                        // for the args-layout rationale.
                        let arity = host_closure.arity;
                        let _popped_callee = self.stack.ensure_pop();
                        // Defense-in-depth: see `Instruction::CallIndirect`
                        // above — a `HostClosure` has no inner `Object::Function`
                        // to cross-check `arity` against, so assert the operand
                        // stack actually holds the declared args before draining.
                        // Debug-only; the `checked_sub` below still guards
                        // underflow in release builds.
                        debug_assert!(
                            self.stack.len() >= arity,
                            "HostClosure CallIndirect: operand stack holds {} slots but declared arity is {arity}",
                            self.stack.len(),
                        );
                        let args_offset = self
                            .stack
                            .len()
                            .checked_sub(arity)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(arity))?;
                        let user_args: Vec<Value> = self
                            .stack
                            .drain(StackIndex::from_raw(args_offset)..)
                            .collect();
                        return Ok(Some(self.host_closure_call_sysop(callee_ptr, user_args)));
                    } else if let Object::BoundMethod(bm) = obj {
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
                        let _popped = self.stack.ensure_pop();
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
                        let _popped_callee = self.stack.ensure_pop();
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
                    let result = self.stack.ensure_pop();
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
                    // Update frame_idx so the outer loop detects the frame change
                    // and re-extracts code/pc/function for the parent frame.
                    if !self.frames.is_empty() {
                        *frame_idx = self.frames.len() - 1;
                    }
                    if Some(self.frames.len()) == self.interrupt_frame {
                        self.interrupt_frame = None;
                        return Ok(Some(VmExecState::Complete(self.stack.ensure_pop())));
                    }
                    if self.frames.is_empty() {
                        return Ok(Some(VmExecState::Complete(self.stack.ensure_pop())));
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
                    // Compact opcodes are 1 byte; rewinding by `AWAIT_OPCODE_LEN`
                    // puts `pc` back at the `OpCode::Await` byte so the outer
                    // exec loop re-executes the same Await on resume. The
                    // regular (non-compact) path expresses the same intent by
                    // explicitly setting `bf.instruction_ptr = instruction_ptr`.
                    const AWAIT_OPCODE_LEN: usize = 1;
                    let value = self.stack.ensure_stack_top();
                    let wanted_type = bex_vm_types::types::FutureType::Any;
                    let index = self.as_object_ptr(self.stack[value], wanted_type.into())?;
                    let ready_value = {
                        let Object::Future(awaiting) = self.get_object(index) else {
                            return Err(VmInternalError::TypeError {
                                expected: wanted_type.into(),
                                got: ObjectType::of(self.get_object(index)).into(),
                            }
                            .into());
                        };
                        match awaiting.read() {
                            FutureRead::Pending(future_id) => {
                                // Rewind pc to the Await opcode so the outer loop
                                // saves a position that re-executes Await once the
                                // future completes.
                                *pc -= AWAIT_OPCODE_LEN;
                                return Ok(Some(VmExecState::Await(future_id)));
                            }
                            FutureRead::Ready(v) => v,
                            // Reserved for future user-callable async functions
                            // that throw BAML values; the engine today routes all
                            // sys-op errors through `internal_error_future`.
                            FutureRead::Error(value) => return Err(VmError::Thrown(value)),
                            FutureRead::Cancelled => {
                                return Err(VmError::Thrown(
                                    self.panic_to_exception_value(VmPanic::Cancelled),
                                ));
                            }
                            FutureRead::InternalError(future_id) => {
                                // Yield back to the engine; it will surface the original
                                // error from the FutureManager's `SetOnce` (the entry is
                                // leaked by design for InternalError).
                                *pc -= AWAIT_OPCODE_LEN;
                                return Ok(Some(VmExecState::Await(future_id)));
                            }
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
                    let value = self.stack.ensure_pop();
                    // Save pc before unwinding (handler lookup needs it).
                    if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                        bf.instruction_ptr = *pc;
                    }
                    self.try_unwind_exception(frame_idx, function, value)?;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Jump opcodes ──────────────────────────────────────────────
                OpCode::Jump => {
                    let offset = read_i32_unchecked(code, pc);
                    // offset is relative to instruction end (current pc)
                    *pc = (*pc as i64 + offset as i64) as usize;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::PopJumpIfFalse => {
                    let offset = read_i32_unchecked(code, pc);
                    let cond = self.stack.ensure_pop();
                    if cond == Value::bool(false) {
                        *pc = (*pc as i64 + offset as i64) as usize;
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                OpCode::JumpIfFalse => {
                    let offset = read_i32_unchecked(code, pc);
                    let top_slot = self.stack.ensure_stack_top();
                    let cond = self.stack[top_slot];
                    if cond == Value::bool(false) {
                        *pc = (*pc as i64 + offset as i64) as usize;
                    }
                }

                // ── JumpTable ─────────────────────────────────────────────────
                OpCode::JumpTable => {
                    let table_idx = read_u32_unchecked(code, pc) as usize;
                    let default_offset = read_i32_unchecked(code, pc);
                    let discriminant = self.stack.ensure_pop();
                    let Some(value) = discriminant.as_int() else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&discriminant),
                        }
                        .into());
                    };
                    // Use pre-translated compact jump table (byte-offset-relative).
                    let compact = function.bytecode.compact.as_ref().unwrap();
                    let compact_table = &compact.jump_tables[table_idx];
                    let offset = compact_table.lookup(value).unwrap_or(default_offset);
                    *pc = (*pc as i64 + offset as i64) as usize;
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Discriminant ──────────────────────────────────────────────
                OpCode::Discriminant => {
                    let value = self.stack.ensure_pop();
                    let Some(object_idx) = value.as_object_ptr() else {
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
                    self.stack.push(Value::int(variant_index as i64));
                }

                // ── TypeTag ───────────────────────────────────────────────────
                OpCode::TypeTag => {
                    let value = self.stack.ensure_pop();
                    let tag = value_type_tag(value);
                    self.stack.push(Value::int(tag));
                }

                // ── IsType ────────────────────────────────────────────────────
                OpCode::IsType => {
                    let const_idx = { read_u32_unchecked(code, pc) as usize };
                    let value = self.stack.ensure_pop();
                    // Inspect the raw constant first to handle ClassWithTypeArgs
                    // (parametric class identity check). Other kinds fall through
                    // to the pre-resolved Object/Int path.
                    let raw_const = &function.bytecode.constants[const_idx];
                    let result = match raw_const {
                        ConstValue::ClassWithTypeArgs {
                            class_obj,
                            type_args_templates,
                        } => {
                            let class_ptr = self.idx_to_ptr(*class_obj);
                            match value.as_object_ptr() {
                                Some(val_ptr) => match self.get_object(val_ptr) {
                                    Object::Instance(inst) if inst.class == class_ptr => {
                                        let frame_type_args =
                                            if let Frame::Bytecode(bf) = &self.frames[*frame_idx] {
                                                bf.type_args.clone()
                                            } else {
                                                vec![]
                                            };
                                        // Position-wise match so a `Wildcard`
                                        // template arg (BEP-044 partial guard)
                                        // matches any concrete arg, while
                                        // pinned positions must compare equal.
                                        type_args_templates.len() == inst.class_type_args.len()
                                            && type_args_templates.iter().zip(&inst.class_type_args).all(
                                                |(template, actual)| {
                                                    matches!(
                                                        template,
                                                        baml_type::TyTemplate::Wildcard
                                                    ) || template.substitute(&frame_type_args)
                                                        == *actual
                                                },
                                            )
                                    }
                                    _ => false,
                                },
                                None => false,
                            }
                        }
                        _ => {
                            let expected = &function.bytecode.resolved_constants[const_idx];
                            if let Some(class_ptr) = expected.as_object_ptr() {
                                match value.as_object_ptr() {
                                    Some(val_ptr) => match self.get_object(val_ptr) {
                                        Object::Instance(instance) => instance.class == class_ptr,
                                        _ => false,
                                    },
                                    None => false,
                                }
                            } else if let Some(tag) = expected.as_int() {
                                value_type_tag(value) == tag
                            } else {
                                false
                            }
                        }
                    };
                    self.stack.push(Value::bool(result));
                }

                // ── DenseTag ──────────────────────────────────────────────────
                #[allow(
                    clippy::cast_sign_loss,
                    clippy::cast_lossless,
                    clippy::cast_possible_truncation
                )]
                OpCode::DenseTag => {
                    let table_idx = { read_u32_unchecked(code, pc) as usize };
                    let popped = self.stack.ensure_pop();
                    let Some(tag) = popped.as_int() else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&popped),
                        }
                        .into());
                    };
                    let table = &function.bytecode.match_hash_tables[table_idx];
                    let h = ((tag as u64).wrapping_mul(table.multiply) >> table.shift)
                        & table.mask as u64;
                    let entry = &table.entries[h as usize];
                    if entry.expected_tag == tag {
                        self.stack.push(Value::int(i64::from(entry.dense_index)));
                    } else {
                        self.stack.push(Value::int(-1));
                    }
                }

                // ── ThrowIfPanic ──────────────────────────────────────────────
                OpCode::ThrowIfPanic => {
                    let value = self.stack.ensure_pop();
                    let is_panic = match value.as_object_ptr() {
                        Some(ptr) => match self.get_object(ptr) {
                            Object::Instance(instance) => {
                                self.panic_class_ptrs.contains(&instance.class)
                            }
                            _ => false,
                        },
                        None => false,
                    };
                    if is_panic {
                        // Save pc before unwinding (handler lookup needs it).
                        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                            bf.instruction_ptr = *pc;
                        }
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
                    let value = self.stack.ensure_pop();
                    let cell = Object::Cell(bex_vm_types::types::Cell::new(value));
                    let ptr = self.tlab.alloc(cell);
                    self.stack.push(Value::object(ptr));
                }

                // ── MakeClosure ───────────────────────────────────────────────
                OpCode::MakeClosure => {
                    let obj_idx_raw = { read_u32_unchecked(code, pc) as usize };
                    let capture_count = { read_u16_unchecked(code, pc) as usize };
                    let ntypeargs = { read_u16_unchecked(code, pc) as usize };
                    let mut captures = Vec::with_capacity(capture_count);
                    for _ in 0..capture_count {
                        captures.push(self.stack.ensure_pop());
                    }
                    captures.reverse();

                    // Pop type args (pushed before the captures).
                    let captured_type_args: Vec<baml_type::Ty> = if ntypeargs > 0 {
                        let mut type_args = Vec::with_capacity(ntypeargs);
                        for _ in 0..ntypeargs {
                            let v = self.stack.ensure_pop();
                            let ptr = self.as_object_ptr(v, ObjectType::Type)?;
                            let Object::Type(ty) = self.get_object(ptr) else {
                                unreachable!("as_object_ptr guarantees Type variant");
                            };
                            type_args.push(*ty.clone());
                        }
                        type_args.reverse();
                        type_args
                    } else {
                        vec![]
                    };

                    let function_ptr = self.idx_to_ptr(ObjectIndex::from_raw(obj_idx_raw));
                    let closure = Object::Closure(Closure {
                        function: function_ptr,
                        captures,
                        captured_type_args,
                    });
                    let ptr = self.tlab.alloc(closure);
                    self.stack.push(Value::object(ptr));
                }

                // ── LoadType ──────────────────────────────────────────────────
                OpCode::LoadType => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let template = match &function.bytecode.constants[idx] {
                        ConstValue::Type(t) => t.clone(),
                        _ => {
                            return Err(VmInternalError::UnexpectedConstantKind.into());
                        }
                    };

                    let ty = {
                        let frame_type_args = if let Frame::Bytecode(bf) = &self.frames[*frame_idx]
                        {
                            bf.type_args.clone()
                        } else {
                            vec![]
                        };
                        if template.is_fully_concrete() {
                            if let baml_type::TyTemplate::Concrete(t) = &template {
                                t.clone()
                            } else {
                                template.substitute(&frame_type_args)
                            }
                        } else {
                            template.substitute(&frame_type_args)
                        }
                    };

                    let value = self.alloc_type(ty);
                    self.stack.push(value);
                }

                // ── MakeBoundMethod ───────────────────────────────────────────
                OpCode::MakeBoundMethod => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let receiver = self.stack.ensure_pop();
                    let callee_value = self.globals.get(self.proof(), global_idx);
                    let function_ptr =
                        self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                    let bound = Object::BoundMethod(BoundMethod {
                        function: function_ptr,
                        receiver,
                    });
                    let ptr = self.tlab.alloc(bound);
                    self.stack.push(Value::object(ptr));
                }

                // ── LoadDeref / StoreDeref ────────────────────────────────────
                OpCode::LoadDeref => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let cell_value =
                        self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                    let Some(cell_ptr) = cell_value.as_object_ptr() else {
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
                    self.stack.push(cell.load());
                }

                OpCode::StoreDeref => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let value = self.stack.ensure_pop();
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let cell_value =
                        self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)];
                    let Some(cell_ptr) = cell_value.as_object_ptr() else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    self.heap.write_barrier(cell_ptr, value);
                    let obj = unsafe { cell_ptr.get() };
                    let Object::Cell(cell) = obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(obj).into(),
                        }
                        .into());
                    };
                    cell.store(value);
                }

                // ── LoadCapture / StoreCapture / CaptureRef ───────────────────
                OpCode::LoadCapture => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
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
                    let Some(cell_ptr) = cell_value.as_object_ptr() else {
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
                    self.stack.push(cell.load());
                }

                OpCode::StoreCapture => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let value = self.stack.ensure_pop();
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
                    let Some(cell_ptr) = cell_value.as_object_ptr() else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: self.type_of(&cell_value),
                        }
                        .into());
                    };
                    self.heap.write_barrier(cell_ptr, value);
                    let cell_obj = unsafe { cell_ptr.get() };
                    let Object::Cell(cell) = cell_obj else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Cell.into(),
                            got: ObjectType::of(cell_obj).into(),
                        }
                        .into());
                    };
                    cell.store(value);
                }

                OpCode::CaptureRef => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
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
                OpCode::ContainerLen => {
                    let container = self.stack.ensure_pop();
                    let Some(ptr) = container.as_object_ptr() else {
                        return Err(VmInternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: self.type_of(&container),
                        }
                        .into());
                    };
                    #[allow(clippy::cast_possible_wrap)]
                    let len = match self.get_object(ptr) {
                        Object::Array(arr) => arr.len() as i64,
                        Object::Uint8Array(bytes) => bytes.len() as i64,
                        Object::Map(map) => map.len() as i64,
                        Object::String(s) => s.len() as i64,
                        other => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Array.into(),
                                got: ObjectType::of(other).into(),
                            }
                            .into());
                        }
                    };
                    self.stack.push(Value::int(len));
                }

                OpCode::LoadArrayElement => {
                    let index_value = self.stack.ensure_pop();
                    let array_value = self.stack.ensure_pop();
                    let array_obj_index = self.as_object_ptr(array_value, ObjectType::Array)?;
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let Some(i) = index_value.as_int() else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&index_value),
                        }
                        .into());
                    };
                    // Acquire the array's read lock for the duration of the
                    // bounds-check + element load so it stays atomic against
                    // a racing `push`/grow. Guard drops at the end of the
                    // inner scope before any `&mut self` call.
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let load_result: Result<Value, (i64, usize)> = {
                        match self.get_object(array_obj_index) {
                            Object::Array(arr) => {
                                let guard = arr.lock();
                                let len = guard.len();
                                if i < 0 || (i as usize) >= len {
                                    Err((i, len))
                                } else {
                                    Ok(guard[i as usize])
                                }
                            }
                            Object::Uint8Array(bytes) => {
                                let guard = bytes.lock();
                                let len = guard.len();
                                if i < 0 || (i as usize) >= len {
                                    Err((i, len))
                                } else {
                                    Ok(Value::int(i64::from(guard[i as usize])))
                                }
                            }
                            other => {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Array.into(),
                                    got: ObjectType::of(other).into(),
                                }
                                .into());
                            }
                        }
                    };
                    let element = match load_result {
                        Ok(v) => v,
                        Err((idx, len)) => {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: idx,
                                    length: len,
                                },
                            )));
                        }
                    };
                    self.stack.push(element);
                }

                OpCode::LoadMapElement => {
                    let key_value = self.stack.ensure_pop();
                    let map_value = self.stack.ensure_pop();
                    let map_index = self.as_object_ptr(map_value, ObjectType::Map)?;
                    let key_index = self.as_object_ptr(key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?.clone();
                    // Take the map's read lock and copy out the value so the
                    // guard releases before any `&mut self` call.
                    let lookup_result: Result<Option<Value>, ObjectType> =
                        match self.get_object(map_index) {
                            Object::Map(map) => {
                                let guard = map.lock();
                                Ok(guard.get(&key).copied())
                            }
                            other => Err(ObjectType::of(other)),
                        };
                    let value = match lookup_result {
                        Ok(Some(v)) => v,
                        Ok(None) => {
                            return Err(VmError::Thrown(
                                self.panic_to_exception_value(VmPanic::MapKeyNotFound),
                            ));
                        }
                        Err(got) => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: got.into(),
                            }
                            .into());
                        }
                    };
                    self.stack.push(value);
                }

                OpCode::StoreArrayElement => {
                    let new_value = self.stack.ensure_pop();
                    let index_value = self.stack.ensure_pop();
                    let array_value = self.stack.ensure_pop();
                    let array_object_index = self.as_object_ptr(array_value, ObjectType::Array)?;
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let Some(i) = index_value.as_int() else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&index_value),
                        }
                        .into());
                    };
                    let new_value_u8: Option<u8> =
                        new_value.as_int().map(|v| (v.cast_unsigned() & 0xFF) as u8);
                    // Acquire the array's write lock for bounds-check + old
                    // read + new write atomically. Guard drops at end of
                    // inner scope before any `&mut self` ops.
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let store_result: Result<Value, (i64, usize)> = {
                        match self.get_object(array_object_index) {
                            Object::Array(arr) => {
                                let mut guard = arr.lock_mut();
                                let len = guard.len();
                                if i < 0 || (i as usize) >= len {
                                    Err((i, len))
                                } else {
                                    let old = guard[i as usize];
                                    guard[i as usize] = new_value;
                                    Ok(old)
                                }
                            }
                            Object::Uint8Array(bytes) => {
                                let Some(byte_v) = new_value_u8 else {
                                    return Err(VmInternalError::TypeError {
                                        expected: bex_vm_types::types::Type::Int,
                                        got: self.type_of(&new_value),
                                    }
                                    .into());
                                };
                                let mut guard = bytes.lock_mut();
                                let len = guard.len();
                                if i < 0 || (i as usize) >= len {
                                    Err((i, len))
                                } else {
                                    let old = Value::int(i64::from(guard[i as usize]));
                                    guard[i as usize] = byte_v;
                                    Ok(old)
                                }
                            }
                            other => {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Array.into(),
                                    got: ObjectType::of(other).into(),
                                }
                                .into());
                            }
                        }
                    };
                    let old_value = match store_result {
                        Ok(v) => v,
                        Err((idx, len)) => {
                            return Err(VmError::Thrown(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: idx,
                                    length: len,
                                },
                            )));
                        }
                    };
                    #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                    let index = i as usize;
                    let watched_node = NodeId::HeapObject(array_object_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::ArrayIndex(index),
                        old_value,
                        new_value,
                    );
                    self.heap.write_barrier(array_object_index, new_value);
                    let notifications = self.process_notifications(watched_node)?;
                    if !notifications.is_empty() {
                        return Ok(Some(VmExecState::Notify(WatchNotification::Variables(
                            notifications,
                        ))));
                    }
                }

                OpCode::StoreMapElement => {
                    let new_value = self.stack.ensure_pop();
                    let key_value = self.stack.ensure_pop();
                    let map_value = self.stack.ensure_pop();
                    let key_index = self.as_object_ptr(key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?.clone();
                    let map_index = self.as_object_ptr(map_value, ObjectType::Map)?;
                    // Take the map's write lock for capture-old + insert-new
                    // atomically. Guard drops before any `&mut self` ops.
                    let store_result: Result<Value, ObjectType> = {
                        match self.get_object(map_index) {
                            Object::Map(map) => {
                                let mut guard = map.lock_mut();
                                let old = guard.get(&key).copied().unwrap_or(Value::NULL);
                                guard.insert(key.clone(), new_value);
                                Ok(old)
                            }
                            other => Err(ObjectType::of(other)),
                        }
                    };
                    let old_value = match store_result {
                        Ok(v) => v,
                        Err(got) => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: got.into(),
                            }
                            .into());
                        }
                    };
                    let watched_node = NodeId::HeapObject(map_index);
                    self.update_watched_node(
                        watched_node,
                        watch::Path::MapKey(key.to_string()),
                        old_value,
                        new_value,
                    );
                    self.heap.write_barrier(map_index, new_value);
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

                // ── Specialized int arithmetic (skip type dispatch) ───────────
                //
                // Add / Sub use [`Value::tagged_int_add`] / `_sub` which
                // operates directly on the tagged bit pattern, skipping
                // the shift-right / shift-left round-trip through
                // `as_int` + `Value::int`. Saves ~3 instructions per op
                // on the hot loop (`i += 1` in `loop_50m` etc.).
                OpCode::AddInt => {
                    let r = self.stack.ensure_pop();
                    let l = self.stack.ensure_pop();
                    self.stack.push(Value::tagged_int_add(l, r));
                }
                OpCode::SubInt => {
                    let r = self.stack.ensure_pop();
                    let l = self.stack.ensure_pop();
                    self.stack.push(Value::tagged_int_sub(l, r));
                }
                OpCode::MulInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    self.stack.push(Value::int(l * r));
                }
                OpCode::DivInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    if r == 0 {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::int(l),
                                right: Value::int(r),
                            },
                        )));
                    }
                    self.stack.push(Value::int(l / r));
                }
                OpCode::ModInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    if r == 0 {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::int(l),
                                right: Value::int(r),
                            },
                        )));
                    }
                    self.stack.push(Value::int(l % r));
                }

                // ── Specialized float arithmetic (skip type dispatch) ─────────
                OpCode::AddFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = self.alloc_float(l + r);
                    self.stack.push(v);
                }
                OpCode::SubFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = self.alloc_float(l - r);
                    self.stack.push(v);
                }
                OpCode::MulFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = self.alloc_float(l * r);
                    self.stack.push(v);
                }
                OpCode::DivFloat => {
                    // Keep the Value handles around so the DivisionByZero
                    // panic can reuse them instead of allocating two more
                    // `Object::Float` boxes on the TLAB just to error.
                    let right_v = self.stack.ensure_pop();
                    let left_v = self.stack.ensure_pop();
                    let Some(r) = value_as_float(right_v) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(left_v) else {
                        std::hint::unreachable_unchecked()
                    };
                    if r == 0.0 {
                        return Err(VmError::Thrown(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: left_v,
                                right: right_v,
                            },
                        )));
                    }
                    let v = self.alloc_float(l / r);
                    self.stack.push(v);
                }

                // ── Specialized int comparison (skip type dispatch) ───────────
                OpCode::CmpIntEq => cmp_int_op!(==),
                OpCode::CmpIntNotEq => cmp_int_op!(!=),
                OpCode::CmpIntLt => cmp_int_op!(<),
                OpCode::CmpIntLtEq => cmp_int_op!(<=),
                OpCode::CmpIntGt => cmp_int_op!(>),
                OpCode::CmpIntGtEq => cmp_int_op!(>=),

                // ── Specialized float comparison (skip type dispatch) ─────────
                #[allow(clippy::float_cmp)]
                OpCode::CmpFloatEq => cmp_float_op!(==),
                #[allow(clippy::float_cmp)]
                OpCode::CmpFloatNotEq => cmp_float_op!(!=),
                OpCode::CmpFloatLt => cmp_float_op!(<),
                OpCode::CmpFloatLtEq => cmp_float_op!(<=),
                OpCode::CmpFloatGt => cmp_float_op!(>),
                OpCode::CmpFloatGtEq => cmp_float_op!(>=),

                // ── Specialized bigint comparison (skip type dispatch) ────────
                OpCode::CmpBigintEq => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::Eq, l, r);
                    self.stack.push(Value::bool(result));
                }
                OpCode::CmpBigintNotEq => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::NotEq, l, r);
                    self.stack.push(Value::bool(result));
                }
                OpCode::CmpBigintLt => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::Lt, l, r);
                    self.stack.push(Value::bool(result));
                }
                OpCode::CmpBigintLtEq => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::LtEq, l, r);
                    self.stack.push(Value::bool(result));
                }
                OpCode::CmpBigintGt => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::Gt, l, r);
                    self.stack.push(Value::bool(result));
                }
                OpCode::CmpBigintGtEq => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let result = self.bigint_cmp(CmpOp::GtEq, l, r);
                    self.stack.push(Value::bool(result));
                }

                // ── Specialized bigint arithmetic (skip type dispatch) ────────
                OpCode::AddBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Add, l, r)?;
                    self.stack.push(value);
                }
                OpCode::SubBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Sub, l, r)?;
                    self.stack.push(value);
                }
                OpCode::MulBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Mul, l, r)?;
                    self.stack.push(value);
                }
                OpCode::DivBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Div, l, r)?;
                    self.stack.push(value);
                }
                OpCode::ModBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Mod, l, r)?;
                    self.stack.push(value);
                }
                OpCode::BitAndBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::BitAnd, l, r)?;
                    self.stack.push(value);
                }
                OpCode::BitOrBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::BitOr, l, r)?;
                    self.stack.push(value);
                }
                OpCode::BitXorBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::BitXor, l, r)?;
                    self.stack.push(value);
                }
                OpCode::ShlBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Shl, l, r)?;
                    self.stack.push(value);
                }
                OpCode::ShrBigint => {
                    let r = self.pop_bigint_operand();
                    let l = self.pop_bigint_operand();
                    let value = self.bigint_binop(BinOp::Shr, l, r)?;
                    self.stack.push(value);
                }

                // ── Expanded unary ────────────────────────────────────────────
                OpCode::Not => {
                    let val = self.stack.ensure_pop();
                    match val.as_bool() {
                        Some(b) => self.stack.push(Value::bool(!b)),
                        None => {
                            return Err(VmInternalError::CannotApplyUnaryOp {
                                op: UnaryOp::Not,
                                value: self.type_of(&val),
                            }
                            .into());
                        }
                    }
                }
                OpCode::Neg => {
                    let val = self.stack.ensure_pop();
                    if let Some(n) = val.as_int() {
                        self.stack.push(Value::int(-n));
                    } else if let Some(n) = value_as_float(val) {
                        let v = self.alloc_float(-n);
                        self.stack.push(v);
                    } else if let Some(ptr) = val.as_object_ptr() {
                        // Bigint negation. Compute the negated value into an
                        // owned `Arc` first so the immutable `get_object` borrow
                        // is released before the `&mut self` `alloc_bigint`.
                        let negated = match self.get_object(ptr) {
                            Object::Bigint(bi) => Some(std::sync::Arc::new(-bi.as_ref().clone())),
                            _ => None,
                        };
                        match negated {
                            Some(arc) => {
                                let result = self.alloc_bigint(arc)?;
                                self.stack.push(result);
                            }
                            None => {
                                return Err(VmInternalError::CannotApplyUnaryOp {
                                    op: UnaryOp::Neg,
                                    value: self.type_of(&val),
                                }
                                .into());
                            }
                        }
                    } else {
                        return Err(VmInternalError::CannotApplyUnaryOp {
                            op: UnaryOp::Neg,
                            value: self.type_of(&val),
                        }
                        .into());
                    }
                }

                // ── SendEvent ─────────────────────────────────────────────────
                OpCode::SendEvent => {
                    let data = self.stack.ensure_pop();
                    let name_value = self.stack.ensure_pop();
                    let event_name = self.as_string(&name_value)?.to_string();
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
        roots.extend(self.stack.iter().filter_map(Value::as_object_ptr));

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
            if let Some(ptr) = value.as_object_ptr() {
                if let Some(&new_ptr) = roots.get(&ptr) {
                    *value = Value::object(new_ptr);
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

impl TlabHolder for BexVm {
    fn tlab(&self) -> &Tlab {
        &self.tlab
    }
    fn tlab_mut(&mut self) -> &mut Tlab {
        &mut self.tlab
    }
}
