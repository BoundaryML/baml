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

use baml_compiler_diagnostics::runtime_type;
use baml_type::{Int63, IntShiftError, Name, normalize::TypeContext};
use smallvec::SmallVec;

/// Lower named host `TypeVar` bindings to the positional De Bruijn `type_args`
/// vec the VM frame consumes. Each `(name, ty)` is placed at the index of the
/// matching name in `param_names` (the callee's De Bruijn-ordered generic
/// params); unbound slots default to the unknown/top type, and names not in
/// `param_names` are dropped — both rollout-safe.
///
/// When `param_names` is empty (no generic params recoverable for this callee,
/// e.g. a native generic builtin), the bindings are emitted in wire order as a
/// fallback, matching the host's De Bruijn send order.
fn lower_named_type_args(
    param_names: &[String],
    type_args: IndexMap<String, bex_vm_types::RealizedTy>,
) -> Vec<bex_vm_types::RealizedTy> {
    if param_names.is_empty() {
        return type_args.into_iter().map(|(_, ty)| ty).collect();
    }
    let mut positional = vec![bex_vm_types::RealizedTy::unknown(); param_names.len()];
    for (name, ty) in type_args {
        if let Some(idx) = param_names.iter().position(|p| *p == name) {
            positional[idx] = ty;
        }
    }
    positional
}

fn lower_named_type_values(
    param_names: &[String],
    type_values: IndexMap<String, TypeValue>,
) -> Vec<Option<TypeValue>> {
    if param_names.is_empty() {
        return type_values.into_values().map(Some).collect();
    }
    let mut positional = vec![None; param_names.len()];
    for (name, value) in type_values {
        if let Some(idx) = param_names.iter().position(|param| *param == name) {
            positional[idx] = Some(value);
        }
    }
    positional
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
use ::bex_vm_types::{
    EarlyYieldCheck, RootHaver,
    types::{ErrorClass, FutureId},
};
use ::core::any::TypeId;
#[cfg(not(target_arch = "wasm32"))]
use ::core::sync::atomic::AtomicBool;
use bex_events::{
    ids::{BexCallId, BexThreadId, FunctionId as ProfFunctionId},
    prof::{
        backend::{
            CapturePlan, ErrorCaptureAttempt, ErrorCaptureId, ErrorCaptureLossReason, ErrorSource,
            ErrorUnwindKind, ExecutionHandle, FunctionCaptureClass, InactiveReason,
            LocalIdOverrides, Owner, ProfilerMemoryGovernor, Reservation, ReservationClass,
            RootProfiler, TerminalErrorTarget, ThrowSite, ValueLossReason, ValueState,
        },
        record::CallSiteSourceSpan,
    },
    run::TraceCallKey,
};
use bex_heap::{BexHeap, Tlab};
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, FunctionMeta, FutureRead, GlobalIndex, HeapPtr, Object,
    ObjectIndex, ObjectPool, ObjectType, PanicClass, PermitProof, StackIndex, UnaryOp, Value,
    Variant, VmGlobals,
    bytecode::{self, Instruction},
    types::{
        BoundMethod, Closure, ConstValue, Function, FunctionOrigin, FunctionType, Instance, Type,
        TypeValue, UnscheduledFuture,
    },
};
use indexmap::IndexMap;

use crate::{
    errors::{
        ProfilerErrorKind, StackFrame, VmBamlError, VmError, VmInternalError, VmPanic,
        VmRustFnError, VmThrowSite, VmThrown, VmUnwindOrigin, VmUnwindSource,
    },
    indexable::{EvalStack, EvalStackTrait},
    package_baml::{NativeCallResult, NativeFunction},
    types::ObjectTrait,
};

/// Max call stack size.
pub const MAX_FRAMES: usize = 256;

#[derive(Clone, Copy)]
struct CallOptions<'a> {
    runtime_id: Option<Value>,
    type_args: &'a [bex_vm_types::RealizedTy],
    type_values: &'a [Option<TypeValue>],
    runtime_type_check: bool,
}

#[derive(Clone, Debug, Default)]
struct TakenTypeArgs {
    tys: Vec<bex_vm_types::RealizedTy>,
    values: Vec<Option<TypeValue>>,
}

fn append_virtual_method_type_args(
    frame_type_args: &mut Vec<bex_vm_types::RealizedTy>,
    method_type_args: &TakenTypeArgs,
) -> Vec<Option<TypeValue>> {
    let mut values = Vec::new();
    if !method_type_args.values.is_empty() {
        // The resolver-provided owner/impl slots precede method-level slots in
        // the callee frame. Preserve that sparse alignment for exact values.
        values.resize(frame_type_args.len(), None);
        values.extend_from_slice(&method_type_args.values);
    }
    frame_type_args.extend_from_slice(&method_type_args.tys);
    values
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
struct StaticVirtualCallKey {
    caller_function_addr: usize,
    call_pc: usize,
    receiver: StaticVirtualReceiverKey,
    /// The interface's nominal identity — which interface, independent of how
    /// it was instantiated. Hashes and compares as its tag, so this costs an
    /// integer rather than a name clone on every dispatch.
    interface_head: bex_vm_types::TypeHead,
    /// The instantiation. Impl selection compares these under
    /// `StructuralEquivCtx`, which is deliberately fact-poor, so they are
    /// keyed as spelled rather than canonicalized: two spellings the resolver
    /// distinguishes (for example `I<Animal>` and `I<Animal | Dog>`) must not
    /// share a cache entry. Empty for the overwhelmingly common non-generic
    /// interface case.
    interface_args: Box<[bex_vm_types::RealizedTy]>,
}

#[derive(Clone, Debug, PartialEq, Eq, Hash)]
enum StaticVirtualReceiverKey {
    /// Static program classes have immutable, program-unique tags. Generic
    /// arguments remain part of identity; the overwhelmingly common
    /// non-generic case clones an empty box without allocating.
    Class {
        type_tag: i64,
        type_args: Box<[bex_vm_types::RealizedTy]>,
    },
    /// Primitive/container receivers do not have a static class object to key
    /// through, so retain their realized type as the uncommon fallback.
    Other(bex_vm_types::RealizedTy),
}

#[derive(Clone, Debug)]
struct StaticVirtualCallTarget {
    callee: HeapPtr,
    frame_type_args: Vec<bex_vm_types::RealizedTy>,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
// These independent wire/runtime policy bits are deliberately not mutually
// exclusive states.
#[allow(clippy::struct_excessive_bools)]
pub struct VmCaptureMask {
    pub inputs: bool,
    pub output: bool,
    pub error: bool,
    pub manual: bool,
    pub selected: bool,
}

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct AwaitAccumulatorHealth {
    pub intervals_started: u64,
    pub clock_invalid: u64,
    pub memory_exceeded: u64,
    pub counter_saturated: bool,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
struct AwaitAccumulatorEntry {
    call_id: u64,
    await_ns: u64,
    await_count: u32,
}

#[derive(Debug)]
struct AwaitAccumulator {
    entries: Vec<AwaitAccumulatorEntry>,
    reservation: Option<Reservation>,
    health: AwaitAccumulatorHealth,
}

impl AwaitAccumulator {
    fn new() -> Self {
        Self {
            entries: Vec::new(),
            reservation: None,
            health: AwaitAccumulatorHealth::default(),
        }
    }
}

pub struct VmCallInputCapture<'a> {
    pub call: TraceCallKey,
    pub entries: &'a [(String, Value)],
    pub heap: &'a BexHeap,
    pub permit: PermitProof<'a>,
    pub manual: bool,
}

pub trait VmCallInputCaptureHook: Send + Sync {
    fn capture_call_input(&self, capture: VmCallInputCapture<'_>);
}

impl VmCaptureMask {
    #[must_use]
    pub const fn disabled() -> Self {
        Self {
            inputs: false,
            output: false,
            error: false,
            manual: false,
            selected: false,
        }
    }

    #[must_use]
    pub const fn from_capture_plan(plan: CapturePlan) -> Self {
        Self {
            inputs: plan.roles.inputs(),
            output: plan.roles.output(),
            error: plan.roles.error(),
            manual: plan.reasons.manual(),
            selected: plan.selected,
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum VmCallCaptureKind {
    Output,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmCallCaptureEvent {
    pub thread_id: u64,
    pub call_id: u64,
    pub kind: VmCallCaptureKind,
    pub value: Value,
    pub manual: bool,
}

#[derive(Debug)]
pub struct VmErrorCaptureEvent {
    pub id: ErrorCaptureId,
    pub value: Value,
    pub manual_eligible: bool,
    pub reservation: Reservation,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum VmProfilerUnwindState {
    NotApplicable,
    Capturing(ErrorCaptureId),
    Lost,
}

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
    ///
    /// Realized by construction: a generic call binds each parameter to a
    /// concrete type, seeded from the callee's realized `Object` type args
    /// (`GenericFunction`/`BoundMethod`/`Closure`/`Instance`), so a `LoadType`
    /// substitutes them into a fully realized type.
    pub type_args: Vec<bex_vm_types::RealizedTy>,
    /// Exact BEP-066 runtime-type metadata. Ordinary calls have no dynamic
    /// definitions or exact values, so keep the comparatively large pair of
    /// side lanes boxed and absent from their hot call frames.
    type_metadata: Option<Box<FrameTypeMetadata>>,
    /// Byte offset of the most recently dispatched opcode (compact path).
    /// In the legacy path this mirrors `instruction_ptr - 1` and is kept
    /// up-to-date before each `step()` call.
    /// Used by `capture_stack_trace`, `try_unwind_exception`, and event
    /// source location capture.
    pub(crate) faulting_pc: usize,
    /// This call's profiling id (BEX event stream; also `$id` semantics —
    /// minted unconditionally, M1 reads it). Frames live in a `Vec`, so this
    /// is Vec-element cost, not `Object` cost.
    pub(crate) call_id: u64,
    /// The caller's `call_id` (`0` = thread-root call), restored into
    /// `BexVm::current_call_id` when this frame pops. Stored here (rather
    /// than recomputed from the frame below) so frameless native calls in
    /// progress can't be skipped over.
    pub(crate) parent_call_id: u64,
    /// Resolved output/error capture behavior for this bytecode call.
    pub(crate) capture_mask: VmCaptureMask,
    pub(crate) function_id: u32,
}

#[derive(Clone, Debug, Default)]
struct FrameTypeMetadata {
    /// Exact runtime type values for explicitly supplied slots. Wrapper-
    /// supplied static slots use `None` and are reconstructed normally. The
    /// frame's realized `type_args` remain the execution authority and root
    /// their own heads; these values preserve the additional reflection
    /// identity needed to reconstruct an explicitly supplied `type` value.
    values: Vec<Option<TypeValue>>,
}

impl FrameTypeMetadata {
    /// Whether any type bound into this frame names a runtime declaration.
    ///
    /// A frame that binds only compiled declarations cannot change what a
    /// `LoadType` constant means, so such frames share the program-wide cache.
    /// One that binds a runtime declaration must not: the same constant
    /// resolves to a different type per binding.
    fn binds_runtime_declaration(&self, heap: &BexHeap) -> bool {
        self.values.iter().flatten().any(|value| {
            let mut runtime = false;
            value.ty.visit_heads(&mut |head| {
                runtime |= head.is_resolved() && !heap.is_compile_time_ptr(head.ptr());
            });
            runtime
        })
    }
}

impl RootHaver for BytecodeFrame {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        roots.push(self.function);
        // A generic slot may acquire a runtime declaration directly from a
        // trusted receiver (FunctionSpec/Stream) without a parallel reflected
        // TypeValue. The realized argument is the semantic type used by
        // LoadType, so its heads must survive and move with the frame even
        // when the optional metadata lane is absent.
        for ty in &self.type_args {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
        let Some(metadata) = &self.type_metadata else {
            return;
        };
        for value in metadata.values.iter().flatten() {
            value.ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
    }
    fn forward_roots(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
        self.function = roots.get(&self.function).copied().unwrap_or(self.function);
        for ty in &mut self.type_args {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        }
        let Some(metadata) = &mut self.type_metadata else {
            return;
        };
        for value in metadata.values.iter_mut().flatten() {
            value.ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        }
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
#[allow(
    clippy::large_enum_variant,
    reason = "bytecode frames are the hot path and remain inline; native continuations are rare"
)]
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
pub(crate) mod tests {
    #[cfg(not(target_arch = "wasm32"))]
    use std::sync::atomic::AtomicBool;
    use std::{collections::HashMap, sync::Arc};

    use bex_heap::{BexHeap, CollectionLevel, Tlab};
    use bex_vm_types::{
        EarlyYieldCheck, FunctionCaptureProps, FunctionKind, GlobalPool, HeapPtr, Object,
        ObjectIndex, RootHaver, Value, ValueKind, VmGlobals,
        bytecode::Bytecode,
        types::{BoundMethod, Closure, Function, FunctionOrigin, TypeValue, type_tags},
    };

    use super::{
        AwaitAccumulatorHealth, BexVm, Frame, FrameTypeMetadata, InactiveReason, RootProfiler,
        TakenTypeArgs, VmCaptureMask, VmExecState, append_virtual_method_type_args, value_type_tag,
    };
    use crate::{
        indexable::EvalStack,
        package_baml::{NativeCallResult, NativeFunction},
    };

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

    pub(crate) fn test_vm(compile_time_objects: Vec<Object>) -> BexVm {
        let heap = BexHeap::new(compile_time_objects);
        BexVm {
            frames: Vec::new(),
            stack: EvalStack::new(),
            op_count: 0,
            cur_pc: 0,
            heap: Arc::clone(&heap),
            early_yield: early_yield_for_test(),
            tlab: Tlab::new(heap),
            globals: VmGlobals::Owned(GlobalPool::new()),
            error_class_ptrs: Arc::from(Vec::new()),
            panic_class_ptrs: Arc::from(Vec::new()),
            prof_ring: None,
            prof_suppressed: false,
            root_profiler: RootProfiler::Inactive(InactiveReason::Disabled),
            profiler_session: None,
            prof_boundary_handle: None,
            prof_boundary_root_pending: false,
            prof_thread_id: 0,
            call_id_counter: 0,
            current_call_id: 0,
            pending_sysop_call_id: None,
            pending_sysop_function_id: None,
            prof_await: None,
            pending_sysop_capture_mask: VmCaptureMask::disabled(),
            pending_call_captures: Vec::new(),
            pending_error_captures: Vec::new(),
            prof_unwind_ordinal: 0,
            call_input_capture_hook: None,
            thrown_value_causes: Vec::new(),
            thrown_value_contexts: Vec::new(),
            preserved_throw_contexts: Vec::new(),
            bex_ref_seed: None,
            id_overrides: Vec::new(),
            argv: Arc::from([]),
            pending_call_type_args: Vec::new(),
            pending_call_type_values: Vec::new(),
            static_load_type_cache: HashMap::new(),
            static_virtual_call_cache: HashMap::new(),
            packages: Arc::new(crate::package_load::PackageIndex::default()),
            dynamic_dispatch: Arc::new(crate::package_load::DynDispatchTables::default()),
        }
    }

    fn await_test_governor() -> bex_events::prof::backend::ProfilerMemoryGovernor {
        use bex_events::prof::backend::{
            MeasuredLayouts, ProfilerMemoryGovernor, ProfilerSizingPolicy,
        };
        let sizing = ProfilerSizingPolicy::derive(32 * 1024 * 1024, MeasuredLayouts::V1).unwrap();
        ProfilerMemoryGovernor::new(sizing, MeasuredLayouts::V1)
    }

    #[test]
    fn sparse_await_accumulator_allocates_only_after_first_valid_wait() {
        let mut vm = test_vm(Vec::new());
        let governor = await_test_governor();
        assert_eq!(vm.prof_await_health(), AwaitAccumulatorHealth::default());
        assert!(vm.prof_await.is_none());

        vm.prof_enable_await_accumulator();
        assert!(vm.prof_await.as_ref().unwrap().entries.is_empty());
        assert!(vm.prof_await.as_ref().unwrap().reservation.is_none());
        vm.prof_record_await(7, Some(11), &governor);
        vm.prof_record_await(7, Some(13), &governor);

        assert_eq!(vm.prof_take_await(7), Some((24, 2)));
        assert!(vm.prof_await.as_ref().unwrap().entries.is_empty());
        assert!(vm.prof_await.as_ref().unwrap().reservation.is_some());
        assert_eq!(vm.prof_await_health().intervals_started, 2);
    }

    #[test]
    fn invalid_await_clock_records_loss_without_sparse_entry() {
        let mut vm = test_vm(Vec::new());
        let governor = await_test_governor();
        vm.prof_enable_await_accumulator();
        vm.prof_record_await(7, None, &governor);

        assert_eq!(vm.prof_take_await(7), None);
        assert_eq!(vm.prof_await_health().intervals_started, 1);
        assert_eq!(vm.prof_await_health().clock_invalid, 1);
        assert!(vm.prof_await.as_ref().unwrap().reservation.is_none());
    }

    fn native_done(_vm: &mut BexVm, _args: &[Value]) -> NativeCallResult {
        NativeCallResult::Done(Value::int(42))
    }

    fn native_function_object() -> Object {
        let native: NativeFunction = native_done;
        Object::Function(Box::new(Function {
            name: "test_native".to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Native(native as *const ()),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type: bex_vm_types::TyTemplate::Int {
                attr: baml_type::TyAttr::default(),
            },
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "int".to_string(),
            throws_type: bex_vm_types::TyTemplate::Never {
                attr: baml_type::TyAttr::default(),
            },
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,
            capture: FunctionCaptureProps::disabled(),
            function_id: 0,
            runtime_package: HeapPtr::null(),
        }))
    }

    fn vm_with_native_entry() -> (BexVm, HeapPtr) {
        let mut vm = test_vm(vec![native_function_object()]);
        let native_ptr = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.globals = VmGlobals::Owned(GlobalPool::from_vec(vec![Value::object(native_ptr)]));
        (vm, native_ptr)
    }

    #[test]
    fn runtime_cache_identity_unwraps_callable_allocations() {
        let (mut vm, function) = vm_with_native_entry();
        let closure_a = vm.tlab.alloc(Object::Closure(Closure {
            function,
            captures: Box::default(),
            captured_type_args: Box::default(),
        }));
        let closure_b = vm.tlab.alloc(Object::Closure(Closure {
            function,
            captures: Box::default(),
            captured_type_args: Box::default(),
        }));
        let method = vm.tlab.alloc(Object::BoundMethod(BoundMethod {
            function,
            receiver: Value::int(1),
            type_args: Box::default(),
        }));

        assert_ne!(
            closure_a, closure_b,
            "closures must be distinct allocations"
        );
        let function_addr = vm
            .runtime_cache_function_addr(function)
            .expect("function cache identity");
        assert_eq!(
            vm.runtime_cache_function_addr(closure_a),
            Some(function_addr)
        );
        assert_eq!(
            vm.runtime_cache_function_addr(closure_b),
            Some(function_addr)
        );
        assert_eq!(vm.runtime_cache_function_addr(method), Some(function_addr));
        assert_eq!(
            function_addr & 1,
            1,
            "runtime cache keys use the GC tag bit"
        );
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

    /// A declaration is reachable from a frame's bound types through the type's
    /// own head, so it is rooted and repointed with no table to keep in step.
    #[test]
    fn frame_type_metadata_is_included_in_vm_gc_roots() {
        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            name: bex_vm_types::DeclarationName::Anonymous(baml_type::Name::new("Metadata")),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            type_tag: tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: HeapPtr::null(),
        })));
        let bound = TypeValue::new(bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
            baml_type::TyAttr::default(),
        ));
        let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.type_metadata = Some(Box::new(FrameTypeMetadata {
            values: vec![Some(bound)],
        }));

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&class_ptr),
            "active frame metadata must root the declarations its types name"
        );

        let (_stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved = forwarding
            .get(&class_ptr)
            .copied()
            .expect("frame metadata definition must survive collection");
        vm.forward_roots(&forwarding);

        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            panic!("expected trampoline bytecode frame");
        };
        let mut repointed = Vec::new();
        for value in frame
            .type_metadata
            .as_ref()
            .expect("frame keeps its metadata")
            .values
            .iter()
            .flatten()
        {
            value.ty.visit_heads(&mut |head| repointed.push(head.ptr()));
        }
        assert_eq!(repointed, vec![moved]);
    }

    #[test]
    fn frame_type_args_root_and_forward_runtime_heads_without_metadata() {
        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            name: bex_vm_types::DeclarationName::Anonymous(baml_type::Name::new("LiveArg")),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            type_tag: tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: HeapPtr::null(),
        })));
        let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.type_args = vec![bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
            baml_type::TyAttr::default(),
        )];
        assert!(frame.type_metadata.is_none());

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&class_ptr),
            "a live realized frame argument must root its declaration without metadata",
        );

        let (_stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved = forwarding
            .get(&class_ptr)
            .copied()
            .expect("the frame type argument's declaration must survive collection");
        vm.forward_roots(&forwarding);

        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            panic!("expected trampoline bytecode frame");
        };
        let bex_vm_types::RealizedTy::Class(head, ..) = &frame.type_args[0] else {
            panic!("the frame argument must remain the dynamic class type")
        };
        assert_eq!(head.ptr(), moved);
    }

    #[test]
    fn pending_native_call_type_args_root_and_forward_runtime_heads() {
        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            name: bex_vm_types::DeclarationName::Anonymous(baml_type::Name::new(
                "PendingNativeArg",
            )),
            fields: Vec::new(),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            type_tag: tag,
            ty_attr: baml_type::TyAttr::default(),
            has_cleanup: false,
            generic_param_count: 0,
            owner: HeapPtr::null(),
        })));
        vm.pending_call_type_args = vec![bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
            baml_type::TyAttr::default(),
        )];

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&class_ptr),
            "a native call's realized argument must root its runtime declaration",
        );

        let (_stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved = forwarding
            .get(&class_ptr)
            .copied()
            .expect("the pending native type argument must survive collection");
        vm.forward_roots(&forwarding);

        let bex_vm_types::RealizedTy::Class(head, ..) = &vm.pending_call_type_args[0] else {
            panic!("the pending native argument must remain the dynamic class type")
        };
        assert_eq!(head.ptr(), moved);
    }

    /// The sibling above covers a frame's *definition overlay*. A frame's
    /// exact type **values** carry their own edges — a type's heads point at
    /// the declarations that give it meaning — and every one has to be rooted
    /// and forwarded, or the next collection leaves a head pointing at a moved
    /// object.
    #[test]
    fn frame_exact_type_values_root_and_forward_every_edge() {
        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let definition_ptr = vm.tlab.alloc(native_function_object());
        let head = bex_vm_types::TypeHead::new(
            definition_ptr,
            baml_type::typetag::TypeTag::fresh_dynamic(),
        );
        let exact = TypeValue::new(bex_vm_types::RealizedTy::Class(
            head,
            Box::new([]),
            baml_type::TyAttr::default(),
        ));

        let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.type_metadata = Some(Box::new(FrameTypeMetadata {
            values: vec![Some(exact)],
        }));

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&definition_ptr),
            "an exact type value's declarations must be rooted"
        );

        let (_stats, _remapped_roots, forwarding) = unsafe {
            vm.heap
                .collect_garbage_generational(&roots, CollectionLevel::Major)
        };
        let moved_definition = forwarding
            .get(&definition_ptr)
            .copied()
            .expect("definition must survive collection");
        vm.forward_roots(&forwarding);

        let Some(Frame::Bytecode(frame)) = vm.frames.last() else {
            panic!("expected trampoline bytecode frame");
        };
        let value = frame
            .type_metadata
            .as_ref()
            .and_then(|metadata| metadata.values.first())
            .and_then(Option::as_ref)
            .expect("exact value must still be there");
        let bex_vm_types::RealizedTy::Class(head, ..) = &value.ty else {
            panic!("the exact value must still be the class type it was built as")
        };
        assert_eq!(head.ptr(), moved_definition);
    }

    #[test]
    fn virtual_method_exact_type_values_follow_owner_slots() {
        let exact = TypeValue::new(bex_vm_types::RealizedTy::string());
        let method = TakenTypeArgs {
            tys: vec![bex_vm_types::RealizedTy::string()],
            values: vec![Some(exact.clone())],
        };
        let mut frame_type_args = vec![bex_vm_types::RealizedTy::int()];

        let values = append_virtual_method_type_args(&mut frame_type_args, &method);

        assert_eq!(
            frame_type_args,
            vec![
                bex_vm_types::RealizedTy::int(),
                bex_vm_types::RealizedTy::string()
            ]
        );
        assert_eq!(values.len(), 2);
        assert!(
            values[0].is_none(),
            "an owner slot with no recovered identity stays reconstructed"
        );
        assert_eq!(
            values[1].as_ref().map(|value| &value.ty),
            Some(&exact.ty),
            "the method slot carries the exact type value it was given"
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
#[derive(Clone)]
struct ThrowContext {
    trace: Arc<[StackFrame]>,
    cause: Value,
}

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

    /// Total bytecode ops dispatched (for the `kperf` profiler only; only
    /// incremented when the `kperf` feature is enabled).
    pub op_count: u64,

    /// Start-PC of the instruction currently executing in the innermost
    /// bytecode frame. Updated cheaply once per op (a flat field store) instead
    /// of writing the frame's `faulting_pc` every op. Outer frames record their
    /// call-site PC into `faulting_pc` at call time; the innermost frame's live
    /// PC is read from here. Used for exception-handler lookup and stack traces.
    pub cur_pc: usize,

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

    /// All loaded packages plus the program-wide interface → impl-rules index
    /// derived from them. Shared (`Arc`) across spawned VMs so workers resolve
    /// types, interfaces, and impls against the same index without rebuilding it.
    pub packages: Arc<crate::package_load::PackageIndex>,

    /// Runtime-created classes and interface implementations, shared by every
    /// VM spawned from the engine.
    pub dynamic_dispatch: Arc<crate::package_load::DynDispatchTables>,

    /// Pre-resolved heap pointers for `baml.errors.*` classes, indexed by
    /// `ErrorClass` discriminant. Shared (`Arc`) across spawned VMs — resolved
    /// once from `packages` rather than per construction.
    error_class_ptrs: Arc<[HeapPtr]>,

    /// Pre-resolved heap pointers for `baml.panics.*` classes, indexed by
    /// `PanicClass` discriminant. Shared (`Arc`) across spawned VMs.
    panic_class_ptrs: Arc<[HeapPtr]>,

    /// D5a snapshot: the profiling ring this engine claimed on the current
    /// OS thread, refreshed by the engine at the top of every exec resume
    /// (`run_thread_event_loop`) and **never valid across an `.await`**.
    /// `None` = profiling off. Pushes go through `prof_push_record`.
    pub prof_ring: Option<&'static bex_events::prof::Ring>,

    /// Per-root execution suppression for project/catalog work that must not
    /// become visible run/profile state. `$id` call ids are still minted.
    pub prof_suppressed: bool,

    /// Root admission token shared by this root and every spawned descendant.
    /// Inactive variants are immutable no-op adapters.
    pub root_profiler: RootProfiler,

    /// Direct session capability for non-structural profiler producers. It is
    /// present only for an admitted active root and inherited by descendants,
    /// so producer hooks never consult the consumer's engine registry.
    pub profiler_session: Option<Arc<bex_events::prof::backend::ProfilerSession>>,

    /// Non-owning projection of the outer completion guard's live boundary
    /// lease. It is used only to acquire a new owned child lease before that
    /// child becomes runnable.
    pub prof_boundary_handle: Option<ExecutionHandle>,

    /// True only for the first structural call of a user boundary's root VM.
    /// Spawned VMs inherit the root token with this bit cleared.
    pub prof_boundary_root_pending: bool,

    /// Logical BEX thread id for the profiling event stream, minted by the
    /// engine per logical thread (root call or spawn) — not the OS thread.
    pub prof_thread_id: u64,

    /// Per-call id counter; ids start at 1 (`0` = none). Minted
    /// unconditionally — it is `$id` language semantics (M1 reads it) — and
    /// only the ring write is gated on `prof_ring`.
    pub(crate) call_id_counter: u64,

    /// The innermost live call's id (`0` = at thread root). Parent for the
    /// next `CallFunction`; restored from the popped frame's
    /// `parent_call_id` on every pop.
    pub(crate) current_call_id: u64,

    /// The profiling call id of the sys-op the VM just yielded (set at the
    /// `VmExecState::SysOp` yield sites). The engine takes it and emits the
    /// matching `EndFunction` once the op completes — possibly on a
    /// different OS thread, hence engine-side via its TLS ring lookup.
    pub pending_sysop_call_id: Option<u64>,
    pub pending_sysop_function_id: Option<u32>,

    /// Sparse timing entries only for open calls that actually suspended.
    /// `None` is profiler-off/suppressed and owns no vector or reservation.
    prof_await: Option<AwaitAccumulator>,

    /// Resolved output/error capture behavior for the sys-op call currently
    /// yielded to the engine.
    pub pending_sysop_capture_mask: VmCaptureMask,

    /// Values observed at call return/throw boundaries. The engine drains
    /// these into `TraceHeap` while holding a heap permit.
    pub pending_call_captures: Vec<VmCallCaptureEvent>,

    /// Cold-path unwind drafts retained only until the engine copies the one
    /// error value and submits structural links to the backend.
    pub pending_error_captures: Vec<VmErrorCaptureEvent>,
    prof_unwind_ordinal: u64,

    /// Optional engine-owned hook for approved bytecode/sys-op input snapshots.
    pub call_input_capture_hook: Option<Arc<dyn VmCallInputCaptureHook>>,

    /// Thrown values already observed at their origin call. Stored as values
    /// instead of raw bits so GC forwarding can preserve rethrow identity.

    /// BEP-042 cause chain across a transparent re-raise: the `cause` context
    /// computed at each error value's original (fresh) throw site, keyed by the
    /// thrown value. A later *rethrow* of the same value (a non-throwing
    /// `defer` pad re-raising the in-flight error, the no-match fall-through,
    /// or `throw <binding>`) reuses this instead of re-running the cause walk —
    /// which from inside a handler body would self-link — so the chain survives
    /// the re-raise. Stored as values (not raw bits) so GC forwarding preserves
    /// both key and cause identity; deduplicated by value and cleared, like
    /// the other throw-state stores, when a fresh entry point is set on an
    /// empty frame stack.
    thrown_value_causes: Vec<(Value, Value)>,

    /// Most recently observed context for each thrown value. `UnknownError`
    /// conversion uses this to transfer the original trace and cause to the
    /// normalized value.
    thrown_value_contexts: Vec<(Value, ThrowContext)>,

    /// One-shot context transfers registered by `UnknownError.from` and
    /// `UnknownError.with_message`. The next fresh throw of the target consumes
    /// the entry, preserving the source throw site instead of the conversion
    /// boundary.
    preserved_throw_contexts: Vec<(Value, ThrowContext)>,

    /// Constants for building BEX `CallRef`s on demand (the `$id` surface):
    /// `(process_euid, engine_id)`, set once by the engine when it attaches
    /// identity to this VM. Unconditional — `$id` works with profiling off.
    pub bex_ref_seed: Option<(bex_events::ids::ProcessEuid, bex_events::ids::EngineId)>,

    /// `baml.id.set()` override for the *current* call: `(call_id, encoded
    /// override string, override uuid)`. Self-invalidating — read only while
    /// `current_call_id` still matches, so it dies with the call.
    /// `$id` overrides set via `baml.id.set` / `$id = ...`, one entry per
    /// overriding call, innermost last. An entry is read only while its
    /// call id equals `current_call_id` (so it dies with the call) and is
    /// popped when its frame exits (`prof_exit_call`), restoring the
    /// caller's override underneath. Call ids are minted monotonically per
    /// thread, so entries are strictly increasing by call id.
    pub(crate) id_overrides: Vec<(u64, String)>,
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
    pending_call_type_args: Vec<bex_vm_types::RealizedTy>,
    pending_call_type_values: Vec<Option<TypeValue>>,

    /// Created-once immutable type objects for fully realized `LoadType`
    /// constants in static functions. The compact numeric key keeps the
    /// per-iteration lookup cheaper than re-hashing the full realized type.
    /// Entries are explicit VM GC roots.
    static_load_type_cache: HashMap<(usize, usize), HeapPtr>,
    /// Monomorphic static virtual-call results keyed by caller site and the
    /// receiver/interface instantiation observed there. Only immutable
    /// program-image rules enter this cache, so runtime package loading and
    /// dynamic builder registration remain live on every dynamic dispatch.
    static_virtual_call_cache: HashMap<StaticVirtualCallKey, StaticVirtualCallTarget>,
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
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct VmEventSourceLocation {
    pub file_id: u32,
    pub line: u32,
    /// Zero means the VM does not know a source column; byte offsets remain precise.
    pub column: u32,
    pub start_offset: u32,
    pub end_offset: u32,
}

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

    /// BEP-034 `baml.future.__await_any`: awaiting the *first* of several
    /// pending futures to settle.
    ///
    /// - Input: the `FutureId`s of the inputs that are still pending (the
    ///   `OpCode::AwaitAny` handler scans the array operand and only yields
    ///   the ones not yet settled; if any were already settled it resolves
    ///   inline without yielding).
    /// - Output (success): nothing pushed — the engine parks until the first
    ///   of these settles, then resumes the VM, which re-executes the
    ///   `AwaitAny` opcode, finds a settled future, and pushes its `int`
    ///   index in input order.
    /// - Output (cancel): the thread's own cancel token fired; handled like
    ///   `Await` (settle our future as cancelled, or surface `Cancelled`).
    AwaitAny(Vec<FutureId>),

    /// BEP-034: VM yields a `spawn { body }` to the engine.
    ///
    /// - Input: a `HeapPtr` to the `UnscheduledFuture` object the VM
    ///   allocated. The struct carries the body closure, the optional
    ///   spawn name, and the `Future<T, E>` type arguments this spawn
    ///   site was typed at.
    /// - Output: the engine builds a fresh `Future::Pending(id)` heap
    ///   object at those types, dispatches the body on a new `BexThread`, and pushes
    ///   the future pointer onto the VM stack. Terminal transitions
    ///   (`Ready`/`Error`/`Cancelled`/`InternalError`) happen later
    ///   via the `FutureManager`; the VM only ever sees `Pending`
    ///   directly after this yield.
    ///
    /// BEP-034 phase D′: this used to be `ScheduleFuture` and covered
    /// both sys-ops and spawns; sys-ops have moved to the dedicated
    /// single-yield `SysOp` variant below.
    Spawn {
        future: HeapPtr,
        source_span: Option<CallSiteSourceSpan>,
    },

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

    /// The VM is yielding a custom event to be emitted.
    ///
    /// The engine handles this by converting both values to `BexExternalValue`
    /// and emitting a `CustomEvent` with the current span context.
    Event {
        /// Name of the event (extracted from the String heap object).
        event_name: String,
        /// Event payload (raw VM value; engine converts to `BexExternalValue`).
        data: Value,
        /// Source location where the event was emitted.
        source_location: Option<VmEventSourceLocation>,
    },

    /// We are still executing, but we should yield to allow other threads or the GC to run.
    EarlyYield,
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
    /// Maps function names to their global indices.
    /// Used for dynamic function lookup at runtime.
    pub function_global_indices: HashMap<String, usize>,
    /// Maps top-level let names to their global indices.
    pub let_global_indices: HashMap<String, usize>,
    /// Client build metadata, passed through to `SysOpContext`.
    pub client_metadata: HashMap<String, bex_vm_types::ClientBuildMeta>,
    /// Per-package program structure (global-index-keyed). The loader allocates
    /// the heap `Object::Package` / `Object::ImplRule` objects and the
    /// `vm.packages` index from this, resolving each `ObjectIndex` to a
    /// compile-time `HeapPtr`.
    pub packages: indexmap::IndexMap<baml_type::Name, bex_vm_types::types::ProgramPackage>,
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

    // Build the function-name lookup by scanning objects. Classes and enums are
    // resolved through `packages` at runtime, so they need no separate index here.
    // Interface bodies are anonymous at runtime — their `name` is display-only —
    // so they never enter a name-resolution surface (entry points, suffix
    // matching, engine lookups).
    let mut resolved_function_names = HashMap::new();
    for (idx, obj) in objects.iter().enumerate() {
        if let Object::Function(func) = obj
            && !func.is_interface_body
        {
            resolved_function_names
                .insert(func.name.clone(), (ObjectIndex::from_raw(idx), func.kind));
        }
    }

    Ok(BytecodeProgram {
        objects: ObjectPool::from_vec(objects),
        globals: program.globals,
        resolved_function_names,
        function_global_indices: program.function_global_indices,
        let_global_indices: program.let_global_indices,
        client_metadata: program.client_metadata,
        packages: program.packages,
    })
}

/// Resolve the heap pointers for the builtin `baml.errors.*` classes, indexed by
/// [`ErrorClass`] discriminant. The result is identical for every VM sharing a
/// `packages` index, so it is resolved once and shared (`Arc`) across spawns
/// rather than re-resolved per [`BexVm::new`].
pub fn resolve_error_class_ptrs(packages: &crate::package_load::PackageIndex) -> Arc<[HeapPtr]> {
    ErrorClass::ALL
        .iter()
        .map(|ec| {
            crate::package_load::lookup_type_by_fqn(packages, ec.fqn())
                .unwrap_or_else(|| panic!("error class {:?} not in packages", ec.fqn()))
        })
        .collect()
}

/// Resolve the heap pointers for the builtin `baml.panics.*` classes, indexed by
/// [`PanicClass`] discriminant. Shared across spawns like
/// [`resolve_error_class_ptrs`].
pub fn resolve_panic_class_ptrs(packages: &crate::package_load::PackageIndex) -> Arc<[HeapPtr]> {
    PanicClass::ALL
        .iter()
        .map(|pc| {
            crate::package_load::lookup_type_by_fqn(packages, pc.fqn())
                .unwrap_or_else(|| panic!("panic class {:?} not in packages", pc.fqn()))
        })
        .collect()
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

/// The [`ConcreteRealizedTy::Function`] a callable *value* denotes: the
/// `Function`'s stored signature templates, materialized against the realized
/// type arguments that value carries.
///
/// Every callable value is fully realized — a `Closure` carries
/// `captured_type_args`, a `BoundMethod` and a `GenericFunction` their complete
/// curried `type_args` — and the stored signature is a template over exactly
/// those frame slots, so substitution always yields a realized type. A failure
/// means the frame does not supply a slot the signature references, i.e. a
/// compiler/VM frame-layout bug, and is surfaced as an internal error rather
/// than a silently coarse or absent type (the same contract
/// `TyTemplate::substitute` states for every other materialization site).
///
/// `drop_receiver` skips the leading `self` parameter: a bound method's type is
/// its function's type with the receiver already applied.
///
/// [`ConcreteRealizedTy::Function`]: baml_type::ConcreteRealizedTy::Function
fn function_object_ty<C: baml_type::normalize::TypeContext<bex_vm_types::TypeHead>>(
    ctx: &C,
    f: &bex_vm_types::types::Function,
    type_args: &[bex_vm_types::RealizedTy],
    drop_receiver: bool,
) -> Result<bex_vm_types::ConcreteRealizedTy, VmInternalError> {
    use baml_type::{FunctionParamMode, TyAttr};
    use bex_vm_types::{ConcreteRealizedTy, RealizedFunctionParamTy};

    // `type_args` may legitimately be SHORTER than the declared arity: an
    // unspecialized generic reaches this reconstruction exactly so the
    // `TypeArgRefOutOfRange` substitution error below can report it — that is
    // how `callable_signature` discovers there is no realized signature yet.
    let materialize =
        |t: &bex_vm_types::TyTemplate| -> Result<bex_vm_types::RealizedTy, VmInternalError> {
            t.substitute(type_args, ctx)
                .map_err(|e| VmInternalError::TypeSubstitution {
                    message: e.to_string(),
                })
        };
    let params = f
        .param_types
        .iter()
        .enumerate()
        .skip(usize::from(drop_receiver))
        .map(|(i, ty)| {
            Ok(RealizedFunctionParamTy {
                name: f
                    .param_names
                    .get(i)
                    .filter(|n| !n.is_empty())
                    .map(|n| Name::new(n.as_str())),
                ty: materialize(ty)?,
                mode: if f.param_has_default.get(i).copied().unwrap_or(false) {
                    FunctionParamMode::Optional
                } else {
                    FunctionParamMode::Required
                },
            })
        })
        .collect::<Result<Vec<_>, VmInternalError>>()?;
    Ok(ConcreteRealizedTy::Function {
        params: params.into(),
        ret: Box::new(materialize(&f.return_type)?),
        throws: Box::new(materialize(&f.throws_type)?),
        attr: TyAttr::default(),
    })
}

/// A callable value's reconstructed signature in the shape the `reflect`
/// natives consume (BEP-062): parameters in declaration order (a bound
/// method's receiver dropped), the return type, and the throws type.
/// A callable that cannot throw reports `never` — the empty error set, and the
/// same spelling the static type uses.
pub(crate) struct CallableSignature {
    /// The declaration's fully qualified name; `None` for host closures and
    /// compiler-synthesized callables (lambda names are `<lambda(...)>`).
    pub(crate) name: Option<String>,
    pub(crate) params: Box<[bex_vm_types::RealizedFunctionParamTy]>,
    pub(crate) ret: bex_vm_types::RealizedTy,
    /// The error type; `never` when the callable cannot throw — the same
    /// spelling a function *type* uses, so a value's reconstructed signature
    /// and its written type agree.
    pub(crate) throws: bex_vm_types::RealizedTy,
    /// The declaration's joined `///` doc-comment lines, if any.
    pub(crate) docstring: Option<String>,
}

/// Reconstruct a [`CallableSignature`] from a raw `Function` object,
/// materializing its signature templates against `type_args` — the realized
/// frame the callable value carries. `drop_receiver` skips the leading `self`
/// parameter for bound methods.
///
/// Shares [`function_object_ty`]'s contract: substitution realizes fully or the
/// frame layout is broken, so there is no coarse fallback. Reflection reports
/// the same type the matcher tests against.
fn function_callable_signature<C: baml_type::normalize::TypeContext<bex_vm_types::TypeHead>>(
    ctx: &C,
    f: &bex_vm_types::types::Function,
    type_args: &[bex_vm_types::RealizedTy],
    drop_receiver: bool,
) -> Result<CallableSignature, VmInternalError> {
    use bex_vm_types::ConcreteRealizedTy;
    let ConcreteRealizedTy::Function {
        params,
        ret,
        throws,
        ..
    } = function_object_ty(ctx, f, type_args, drop_receiver)?
    else {
        unreachable!("function_object_ty always builds a Function type")
    };
    Ok(CallableSignature {
        name: f.declared_name.clone(),
        params,
        ret: *ret,
        throws: *throws,
        docstring: f.docstring.clone(),
    })
}

/// Get the type tag for any runtime value.
///
/// This is a free function to avoid borrow checker issues when called
/// from within the instruction dispatch loop.
/// The declaration a type names, together with its instantiation — `None` for
/// types that name no declaration.
///
/// Nominal identity is the head: content-addressed from the qualified name
/// for compiled declarations, a counter mint for runtime-created ones.
fn nominal_identity(
    ty: &bex_vm_types::RealizedTy,
) -> Option<(bex_vm_types::TypeHead, &[bex_vm_types::RealizedTy])> {
    use bex_vm_types::RealizedTy as T;
    match ty {
        T::Class(head, args, _) => Some((*head, &**args)),
        T::Enum(head, _) => Some((*head, &[])),
        // Structural, abstract, and literal types name no declaration. An
        // enum *variant* names one but is a proper subset of it, so it is not
        // that declaration's identity.
        T::Unknown { .. }
        | T::Never { .. }
        | T::Null { .. }
        | T::Bool { .. }
        | T::Int { .. }
        | T::Bigint { .. }
        | T::Float { .. }
        | T::String { .. }
        | T::Uint8Array { .. }
        | T::Media(..)
        | T::Literal(..)
        | T::Interface(..)
        | T::EnumVariant(..)
        | T::List(..)
        | T::Map { .. }
        | T::Union(..)
        | T::Function { .. }
        | T::Future(..)
        | T::RustType { .. }
        | T::Type { .. }
        | T::Resource { .. }
        | T::PromptAst { .. }
        | T::Void { .. }
        | T::TypeAlias(..) => None,
    }
}

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
                Object::GenericFunction(_) => type_tags::FUNCTION,
                Object::HostClosure(_) => type_tags::FUNCTION,
                Object::Cell(_) => type_tags::UNKNOWN,
                Object::Future(_) => type_tags::FUTURE,
                Object::UnscheduledFuture(_) => type_tags::FUTURE,
                Object::Enum(_) => type_tags::ENUM,
                Object::RustData(_) => type_tags::UNKNOWN,
                Object::Collector(_) => type_tags::COLLECTOR,
                Object::Type(_) => type_tags::TYPE,
                Object::Class(_) => type_tags::UNKNOWN,
                Object::TypeAlias(_) => type_tags::UNKNOWN,
                Object::Interface(_) => type_tags::UNKNOWN,
                Object::Package(_) => type_tags::UNKNOWN,
                Object::ImplRule(_) => type_tags::UNKNOWN,
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => type_tags::UNKNOWN,
                Object::Instance(instance) => {
                    let class_obj = unsafe { instance.class.get() };
                    let Object::Class(class) = class_obj else {
                        unreachable!("Instance.class does not point to a Class object")
                    };
                    // An instance dispatches on its class's head identity; the
                    // primitive arms above are raw tags in the same space.
                    class.type_tag.as_i64()
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
    #[allow(
        clippy::too_many_arguments,
        reason = "VM construction explicitly wires the shared runtime registries and class tables"
    )]
    pub fn new(
        heap: Arc<BexHeap>,
        globals: VmGlobals,
        #[cfg(not(target_arch = "wasm32"))] park_requested: Arc<AtomicBool>,
        argv: Arc<[String]>,
        packages: Arc<crate::package_load::PackageIndex>,
        dynamic_dispatch: Arc<crate::package_load::DynDispatchTables>,
        error_class_ptrs: Arc<[HeapPtr]>,
        panic_class_ptrs: Arc<[HeapPtr]>,
    ) -> Self {
        // Defer the first TLAB chunk reservation until the first `tlab.alloc`,
        // which the engine reaches only after the VM has been registered as a
        // permit holder via `HeapPermitManager::new_permit` and a permit is
        // active. Eagerly calling `Tlab::new` here would reserve a chunk
        // *before* registration, leaving the cursor stale across any GC that
        // fires in the engine's pre-permit window.
        let tlab = Tlab::new_empty(Arc::clone(&heap));

        // `error_class_ptrs` / `panic_class_ptrs` are resolved once from `packages`
        // by the caller (`resolve_error_class_ptrs` / `resolve_panic_class_ptrs`)
        // and shared across spawned VMs.

        let early_yield = EarlyYieldCheck::new(
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
        );

        Self {
            frames: Vec::new(),
            stack: EvalStack::new(),
            op_count: 0,
            cur_pc: 0,
            heap,
            early_yield,
            tlab,
            globals,
            error_class_ptrs,
            panic_class_ptrs,
            prof_ring: None,
            prof_suppressed: false,
            root_profiler: RootProfiler::Inactive(InactiveReason::Disabled),
            profiler_session: None,
            prof_boundary_handle: None,
            prof_boundary_root_pending: false,
            prof_thread_id: 0,
            call_id_counter: 0,
            current_call_id: 0,
            pending_sysop_call_id: None,
            pending_sysop_function_id: None,
            prof_await: None,
            pending_sysop_capture_mask: VmCaptureMask::disabled(),
            pending_call_captures: Vec::new(),
            pending_error_captures: Vec::new(),
            prof_unwind_ordinal: 0,
            call_input_capture_hook: None,
            thrown_value_causes: Vec::new(),
            thrown_value_contexts: Vec::new(),
            preserved_throw_contexts: Vec::new(),
            bex_ref_seed: None,
            id_overrides: Vec::new(),
            argv,
            pending_call_type_args: Vec::new(),
            pending_call_type_values: Vec::new(),
            static_load_type_cache: HashMap::new(),
            static_virtual_call_cache: HashMap::new(),
            packages,
            dynamic_dispatch,
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
    pub fn current_call_type_args(&self) -> &[bex_vm_types::RealizedTy] {
        &self.pending_call_type_args
    }

    fn take_type_args(&mut self, start: usize, count: usize) -> Result<TakenTypeArgs, VmError> {
        let end = start
            .checked_add(count)
            .filter(|end| *end <= self.stack.len())
            .ok_or(VmInternalError::NotEnoughItemsOnStack(count))?;
        if count == 0 {
            return Ok(TakenTypeArgs::default());
        }
        // Every type-operand position's contract is
        // `reflect.Type | reflect.TypeView`; a kind view converts at this
        // boundary to the `type` value it wraps, so everything downstream
        // sees `Object::Type` only.
        for slot in start..end {
            let value = self.stack[StackIndex::from_raw(slot)];
            if let Some(ty_value) =
                crate::package_reflect::type_kinds::as_view_type_value(self, value)
            {
                self.stack[StackIndex::from_raw(slot)] = ty_value;
            }
        }
        // A type built only from compiled declarations needs no per-frame
        // carrier: every consumer can find those declarations from the program
        // index, and they never move. Anything naming a runtime declaration
        // must ride along as an exact value so the callee reaches it.
        let all_plain_static = (start..end).all(|slot| {
            let value = self.stack[StackIndex::from_raw(slot)];
            value.as_object_ptr().is_some_and(|ptr| {
                matches!(self.get_object(ptr), Object::Type(type_value)
                    if crate::reachable::is_statically_declared(&type_value.ty))
            })
        });
        if all_plain_static {
            let tys = (start..end)
                .map(|slot| {
                    let value = self.stack[StackIndex::from_raw(slot)];
                    let ptr = value
                        .as_object_ptr()
                        .expect("plain static type argument is an object");
                    let Object::Type(type_value) = self.get_object(ptr) else {
                        unreachable!("plain static type argument is Object::Type")
                    };
                    type_value.ty.clone()
                })
                .collect();
            drop(
                self.stack
                    .drain(StackIndex::from_raw(start)..StackIndex::from_raw(end)),
            );
            return Ok(TakenTypeArgs {
                tys,
                values: Vec::new(),
            });
        }
        let mut type_args = TakenTypeArgs {
            tys: Vec::with_capacity(count),
            values: Vec::with_capacity(count),
        };
        for slot in start..end {
            let value = self.stack[StackIndex::from_raw(slot)];
            let direct = value.as_object_ptr().and_then(|ptr| {
                let Object::Type(type_value) = self.get_object(ptr) else {
                    return None;
                };
                Some((**type_value).clone())
            });
            let type_value = if let Some(type_value) = direct {
                type_value
            } else if let Some(result) =
                crate::package_reflect::runtime_class_builder::coerce_pending_type_arg(self, value)
            {
                result.map_err(VmError::thrown_fresh)?
            } else {
                return Err(VmInternalError::TypeError {
                    expected: Type::Object(ObjectType::Type),
                    got: self.type_of(&value),
                }
                .into());
            };
            type_args.tys.push(type_value.ty.clone());
            type_args.values.push(Some(type_value));
        }
        drop(
            self.stack
                .drain(StackIndex::from_raw(start)..StackIndex::from_raw(end)),
        );
        Ok(type_args)
    }

    fn take_type_args_below_values(
        &mut self,
        type_arg_count: usize,
        value_count: usize,
    ) -> Result<TakenTypeArgs, VmError> {
        let input_count = type_arg_count
            .checked_add(value_count)
            .expect("VM operand count fits in usize");
        let start = self
            .stack
            .len()
            .checked_sub(input_count)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(input_count))?;
        self.take_type_args(start, type_arg_count)
    }

    fn pop_type_args(&mut self, count: usize) -> Result<TakenTypeArgs, VmError> {
        self.take_type_args_below_values(count, 0)
    }

    pub fn set_call_input_capture_hook(&mut self, hook: Option<Arc<dyn VmCallInputCaptureHook>>) {
        self.call_input_capture_hook = hook;
    }

    pub fn drain_call_capture_events(&mut self) -> Vec<VmCallCaptureEvent> {
        std::mem::take(&mut self.pending_call_captures)
    }

    pub fn drain_error_capture_events(&mut self) -> Vec<VmErrorCaptureEvent> {
        std::mem::take(&mut self.pending_error_captures)
    }

    pub fn queue_engine_call_output_capture(
        &mut self,
        call_id: u64,
        mask: VmCaptureMask,
        value: Value,
    ) {
        if !mask.output {
            return;
        }
        self.queue_call_capture(call_id, VmCallCaptureKind::Output, value, mask.manual);
    }

    fn queue_call_capture(
        &mut self,
        call_id: u64,
        kind: VmCallCaptureKind,
        value: Value,
        manual: bool,
    ) {
        if call_id == 0 {
            return;
        }
        self.pending_call_captures.push(VmCallCaptureEvent {
            thread_id: self.prof_thread_id,
            call_id,
            kind,
            value,
            manual,
        });
    }

    fn maybe_queue_call_output(
        &mut self,
        call_id: u64,
        parent_call_id: u64,
        mask: VmCaptureMask,
        value: Value,
    ) {
        if parent_call_id == 0 || !mask.output {
            return;
        }
        self.queue_call_capture(call_id, VmCallCaptureKind::Output, value, mask.manual);
    }

    /// Remember the `cause` context computed at `value`'s original (fresh)
    /// throw site so a later rethrow of the same value can recover it. The
    /// stored cause is the error `value` superseded (the enclosing handler's
    /// context), NEVER `value`'s own materialized context, so it can never form
    /// a self-link. Keyed by value identity.
    fn record_throw_cause(&mut self, value: Value, cause: Value) {
        if cause == Value::NULL {
            self.thrown_value_causes.retain(|(v, _)| *v != value);
            return;
        }
        if let Some((_, existing)) = self
            .thrown_value_causes
            .iter_mut()
            .find(|(v, _)| *v == value)
        {
            *existing = cause;
        } else {
            self.thrown_value_causes.push((value, cause));
        }
    }

    /// The `cause` context recorded for `value` at its fresh throw site, or
    /// `Value::NULL` if it never superseded an error (a genuine rethrow).
    fn recorded_throw_cause(&self, value: Value) -> Value {
        self.thrown_value_causes
            .iter()
            .find(|(v, _)| *v == value)
            .map_or(Value::NULL, |(_, cause)| *cause)
    }

    fn record_throw_context(&mut self, value: Value, trace: Arc<[StackFrame]>, cause: Value) {
        let context = ThrowContext { trace, cause };
        if let Some((_, existing)) = self
            .thrown_value_contexts
            .iter_mut()
            .find(|(candidate, _)| *candidate == value)
        {
            *existing = context;
        } else {
            self.thrown_value_contexts.push((value, context));
        }
    }

    fn recorded_throw_context(&self, value: Value) -> Option<ThrowContext> {
        self.thrown_value_contexts
            .iter()
            .find(|(candidate, _)| *candidate == value)
            .map(|(_, context)| context.clone())
    }

    /// Transfer the source value's most recently observed throw context to the
    /// next fresh throw of `target`.
    pub(crate) fn preserve_throw_context(&mut self, source: Value, target: Value) {
        let Some(context) = self
            .thrown_value_contexts
            .iter()
            .find(|(candidate, _)| *candidate == source)
            .map(|(_, context)| context.clone())
        else {
            return;
        };

        if let Some((_, existing)) = self
            .preserved_throw_contexts
            .iter_mut()
            .find(|(candidate, _)| *candidate == target)
        {
            *existing = context;
        } else {
            self.preserved_throw_contexts.push((target, context));
        }
    }

    fn take_preserved_throw_context(&mut self, value: Value) -> Option<ThrowContext> {
        let index = self
            .preserved_throw_contexts
            .iter()
            .position(|(candidate, _)| *candidate == value)?;
        Some(self.preserved_throw_contexts.swap_remove(index).1)
    }

    fn trace_call_key_for_call_id(&self, call_id: u64) -> Option<TraceCallKey> {
        let (process_euid, engine_id) = self.bex_ref_seed?;
        Some(TraceCallKey {
            process_euid,
            engine_id,
            thread_id: BexThreadId(self.prof_thread_id),
            call_id: BexCallId(call_id),
        })
    }

    fn install_consumed_local_id_for_call(
        &mut self,
        call_id: u64,
        local_id: &crate::package_boundary::id::ConsumedLocalId,
    ) {
        if call_id == 0 {
            return;
        }
        if let Some(top) = self.id_overrides.last_mut()
            && top.0 == call_id
        {
            top.1.clone_from(&local_id.encoded);
        } else {
            self.id_overrides.push((call_id, local_id.encoded.clone()));
        }
        self.prof_push_set_function_id(call_id, local_id.boundary_id.as_bytes());
    }

    fn install_consumed_local_id_for_sysop(
        &mut self,
        call_id: u64,
        local_id: &crate::package_boundary::id::ConsumedLocalId,
    ) {
        if call_id == 0 {
            return;
        }
        self.prof_push_set_function_id(call_id, local_id.boundary_id.as_bytes());
    }

    fn consume_local_id_value(
        &mut self,
        value: Value,
    ) -> Result<crate::package_boundary::id::ConsumedLocalId, VmError> {
        crate::package_boundary::id::consume_local_id(self, value)
            .map_err(|err| self.native_error_to_vm_error(err))
    }

    fn invalid_argument_vm_error(&mut self, message: impl Into<String>) -> VmError {
        self.native_error_to_vm_error(
            VmBamlError::InvalidArgument {
                message: message.into(),
            }
            .into(),
        )
    }

    fn maybe_capture_named_inputs(
        &self,
        call_id: u64,
        entries: &[(String, Value)],
        mask: VmCaptureMask,
    ) {
        if !mask.inputs {
            return;
        }
        let Some(hook) = self.call_input_capture_hook.as_ref() else {
            return;
        };
        let Some(call) = self.trace_call_key_for_call_id(call_id) else {
            return;
        };
        hook.capture_call_input(VmCallInputCapture {
            call,
            entries,
            heap: self.heap.as_ref(),
            permit: self.proof(),
            manual: mask.manual,
        });
    }

    fn maybe_capture_call_inputs(
        &self,
        param_names: &[String],
        call_id: u64,
        locals_offset: StackIndex,
        arg_count: usize,
        mask: VmCaptureMask,
    ) {
        if !mask.inputs {
            return;
        }
        let base = locals_offset.into_raw();
        let mut entries = Vec::with_capacity(arg_count);
        for index in 0..arg_count {
            let name = param_names
                .get(index)
                .cloned()
                .unwrap_or_else(|| format!("arg{index}"));
            let value = self.stack[StackIndex::from_raw(base + index)];
            entries.push((name, value));
        }
        self.maybe_capture_named_inputs(call_id, &entries, mask);
    }

    /// The declared element type of `value` when it is an `Object::Array`, else
    /// `unknown`. The generated array-receiver glue calls this to build the
    /// [`ArrayView`](crate::package_baml::ArrayView) it hands a builtin, so a
    /// type-preserving builtin (e.g. `filter`) can tag its result array.
    pub fn array_element_ty(&self, value: &Value) -> bex_vm_types::RealizedTy {
        value
            .as_object_ptr()
            .and_then(|ptr| match self.get_object(ptr) {
                Object::Array(arr) => Some((*arr.element_ty).clone()),
                _ => None,
            })
            .unwrap_or_else(bex_vm_types::RealizedTy::unknown)
    }

    /// The declared key type of `value` when it is an `Object::Map`, else
    /// `unknown`. The generated map-receiver glue calls this (with
    /// [`Self::map_value_ty`]) to build the
    /// [`MapView`](crate::package_baml::MapView) it hands a builtin, so a
    /// type-preserving builtin can tag its result map.
    pub fn map_key_ty(&self, value: &Value) -> bex_vm_types::RealizedTy {
        value
            .as_object_ptr()
            .and_then(|ptr| match self.get_object(ptr) {
                Object::Map(map) => Some((*map.key_ty).clone()),
                _ => None,
            })
            .unwrap_or_else(bex_vm_types::RealizedTy::unknown)
    }

    /// The declared value type of `value` when it is an `Object::Map`, else
    /// `unknown`. The map analogue of [`Self::array_element_ty`]; see
    /// [`Self::map_key_ty`].
    pub fn map_value_ty(&self, value: &Value) -> bex_vm_types::RealizedTy {
        value
            .as_object_ptr()
            .and_then(|ptr| match self.get_object(ptr) {
                Object::Map(map) => Some((*map.value_ty).clone()),
                _ => None,
            })
            .unwrap_or_else(bex_vm_types::RealizedTy::unknown)
    }

    /// Realize a class field's type template against an instance's realized class
    /// type args, reducing any associated projection through the impl registry.
    ///
    /// A field template references only the class's own generic params (each bound
    /// to a realized arg here) and the fields have concrete declared types, so this
    /// always realizes; a substitution failure is a broken compiler/VM invariant,
    /// surfaced as a panic rather than a silent `unknown`.
    pub(crate) fn realize_field_ty(
        &self,
        template: &bex_vm_types::TyTemplate,
        class_type_args: &[bex_vm_types::RealizedTy],
    ) -> bex_vm_types::RealizedTy {
        template
            .substitute(class_type_args, self)
            .unwrap_or_else(|e| {
                unreachable!(
                    "class field type template did not realize against realized class args: {e}"
                )
            })
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

    /// The `Object::Package` for `pkg`, if loaded.
    fn package(&self, pkg: &Name) -> Option<&bex_vm_types::types::Package> {
        self.get_object(self.packages.package_ptr(pkg)?)
            .as_package()
    }

    fn package_for_type(&self, qtn: &baml_type::TypeName) -> Option<&bex_vm_types::types::Package> {
        self.package_for_type_in(self.current_runtime_package(), qtn)
    }

    /// [`Self::package_for_type`] against an explicitly chosen runtime package
    /// rather than the executing frame's.
    ///
    /// Reflection resolves names that belong to a package it is *inspecting*,
    /// not the one it is running in: a bound declared inside a
    /// `Package.compile`d package is `Local` to that package, so resolving it
    /// against the caller's world finds nothing and every such bound would fail
    /// closed. This is the name-resolution half of what
    /// [`ImplResolver::for_package`](crate::package_baml::ImplResolver) already
    /// does for impl rules.
    fn package_for_type_in(
        &self,
        current_ptr: HeapPtr,
        qtn: &baml_type::TypeName,
    ) -> Option<&bex_vm_types::types::Package> {
        if !current_ptr.is_null() {
            let current = self.get_object(current_ptr).as_package()?;
            if qtn.is_local() {
                return Some(current);
            }
            // Runtime-compiled packages link stdlib symbols straight back to
            // the immutable host image rather than copying stdlib packages
            // into their dynamic dependency graph. Type/interface lookup must
            // follow the same edge so virtual dispatch reaches the host's
            // static, cacheable impl rules. Restrict the fallback to the
            // compiler-owned stdlib set; undeclared user packages remain
            // inaccessible.
            if baml_builtins2::stdlib_package_names().contains(&qtn.package().as_str()) {
                return self.package(qtn.package());
            }
            let runtime = current.runtime()?;
            if let Some(dependency) = runtime.dependency_names.get(qtn.package().as_str()) {
                return self.get_object(*dependency).as_package();
            }
            return None;
        }
        self.package(qtn.package())
    }

    /// Look up a class or enum object by its qualified type name. Classes and
    /// enums share one type namespace, so a name resolves to at most one object.
    ///
    /// Only declared types resolve here: a runtime-created declaration is
    /// anonymous, so there is no qualified name to look it up by — it is
    /// reached through heads and mount surfaces instead.
    pub fn lookup_type(&self, qtn: &baml_type::TypeName) -> Option<HeapPtr> {
        let package = self.package_for_type(qtn)?;
        let local = bex_vm_types::types::LocalName {
            namespace: qtn.namespace().clone(),
            name: qtn.name().clone(),
        };
        package
            .classes
            .get(&local)
            .or_else(|| package.enums.get(&local))
            .copied()
    }

    /// The head for the declaration `qtn` names — its tag and its pointer, both
    /// read off the declaration itself.
    ///
    /// The one name→head channel inside the VM, for the boundaries that
    /// genuinely start from a name (the algebra's `reflect.AnyFunction` case, host
    /// conversion, codegen FQN constants). Everything interior already holds a
    /// head and must deref it instead: content-addressing a name into a tag
    /// would fabricate an identity rather than resolve one, and cannot see a
    /// runtime-created declaration at all.
    pub fn declaration_head(&self, qtn: &baml_type::TypeName) -> Option<bex_vm_types::TypeHead> {
        let ptr = self
            .lookup_type(qtn)
            .or_else(|| self.lookup_interface(qtn))
            .or_else(|| self.lookup_type_alias_ptr(qtn))?;
        let tag = match self.get_object(ptr) {
            Object::Class(class) => class.type_tag,
            Object::Enum(enm) => enm.type_tag,
            Object::Interface(iface) => iface.type_tag,
            Object::TypeAlias(alias) => alias.type_tag,
            _ => return None,
        };
        Some(bex_vm_types::TypeHead::new(ptr, tag))
    }

    /// Look up an interface object by its qualified type name. The returned
    /// pointer is the canonical `Object::Interface` for the interface — the same
    /// pointer that keys every package's [`bex_vm_types::types::Package::impl_rules`], so it can be
    /// used to resolve an interface's impls in O(1).
    pub fn lookup_interface(&self, qtn: &baml_type::TypeName) -> Option<HeapPtr> {
        self.lookup_interface_in(self.current_runtime_package(), qtn)
    }

    /// [`Self::lookup_interface`] rooted at an explicitly chosen runtime
    /// package. See `package_for_type_in` (private) for why reflection needs
    /// a root other than the executing frame's.
    pub fn lookup_interface_in(
        &self,
        current_ptr: HeapPtr,
        qtn: &baml_type::TypeName,
    ) -> Option<HeapPtr> {
        let local = bex_vm_types::types::LocalName {
            namespace: qtn.namespace().clone(),
            name: qtn.name().clone(),
        };
        self.package_for_type_in(current_ptr, qtn)?
            .interfaces
            .get(&local)
            .copied()
    }

    /// Look up a class, enum, or interface object by its fully-qualified dotted
    /// name, with the package as the leading segment. For builtin
    /// (dependency-package) types referenced by constant FQN; not valid for
    /// `user`-package types, whose rendered name elides the package — use
    /// [`Self::lookup_type`] there.
    pub fn lookup_type_by_fqn(&self, fqn: &str) -> Option<HeapPtr> {
        crate::package_load::lookup_type_by_fqn(&self.packages, fqn)
    }

    /// The `Object::TypeAlias` pointer for `qtn`, if its package declares one.
    pub fn lookup_type_alias_ptr(&self, qtn: &baml_type::TypeName) -> Option<HeapPtr> {
        let local = bex_vm_types::types::LocalName {
            namespace: qtn.namespace().clone(),
            name: qtn.name().clone(),
        };
        self.package_for_type(qtn)?
            .type_aliases
            .get(&local)
            .copied()
    }

    /// aliases survive to runtime; non-recursive ones are expanded inline).
    ///
    /// Reads through the package's `Object::TypeAlias` rather than a side map —
    /// the indirection a nominal reference will eventually point at directly.
    pub fn recursive_type_alias(
        &self,
        qtn: &baml_type::TypeName,
    ) -> Option<&bex_vm_types::RealizedTy> {
        let local = bex_vm_types::types::LocalName {
            namespace: qtn.namespace().clone(),
            name: qtn.name().clone(),
        };
        let alias_ptr = *self.package_for_type(qtn)?.type_aliases.get(&local)?;
        // SAFETY: a package's alias map holds only compile-time
        // `Object::TypeAlias` pointers, valid for the heap's lifetime.
        #[expect(unsafe_code, reason = "deref a compile-time alias pointer")]
        let object = unsafe { alias_ptr.get() };
        match object {
            Object::TypeAlias(alias) => Some(&alias.definition),
            _ => None,
        }
    }

    /// The definition behind a type alias, given the declaration pointer its
    /// head carries.
    ///
    /// A deref rather than a lookup — and unlike a name-keyed one it works for
    /// a runtime-declared alias, which no package index can name.
    pub fn type_alias_definition(&self, alias: HeapPtr) -> Option<&bex_vm_types::RealizedTy> {
        match self.get_object(alias) {
            Object::TypeAlias(alias) => Some(&alias.definition),
            _ => None,
        }
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
            frame.collect_roots(&mut roots);
        }
        roots
    }

    pub(crate) fn current_runtime_package(&self) -> HeapPtr {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Bytecode(frame) => Some(match self.get_object(frame.function) {
                    Object::Function(function) => function.runtime_package,
                    Object::GenericFunction(function) => function.runtime_package,
                    Object::Closure(closure) => match unsafe { closure.function.get() } {
                        Object::Function(function) => function.runtime_package,
                        _ => HeapPtr::null(),
                    },
                    Object::BoundMethod(method) => match unsafe { method.function.get() } {
                        Object::Function(function) => function.runtime_package,
                        _ => HeapPtr::null(),
                    },
                    _ => HeapPtr::null(),
                }),
                Frame::Native(_) => None,
            })
            .unwrap_or_else(HeapPtr::null)
    }

    /// Return the created-once type value for a declaration owned by the
    /// currently executing runtime package.
    ///
    /// A runtime-compiled declaration is not in the program image, so a
    /// concrete `LoadType` in that package must reuse the value allocated at
    /// package load — that value carries the definition overlay and owner edge
    /// the type needs to stay meaningful. Imported and composite types
    /// deliberately miss this lookup and retain the static path.
    fn current_runtime_declaration_type(&self, ty: &bex_vm_types::RealizedTy) -> Option<HeapPtr> {
        let (bex_vm_types::RealizedTy::Class(head, ..)
        | bex_vm_types::RealizedTy::Enum(head, ..)
        | bex_vm_types::RealizedTy::Interface(head, ..)) = ty
        else {
            return None;
        };
        if !head.is_resolved() {
            return None;
        }

        let package_ptr = self.current_runtime_package();
        if package_ptr.is_null() {
            return None;
        }
        let Object::Package(package) = self.get_object(package_ptr) else {
            unreachable!("runtime function owner does not point to Object::Package")
        };
        // The head *is* the key: a declaration this package created has an
        // entry, and one it merely imported or composed does not — which is
        // exactly the set that must reuse the created-once value.
        let ptr = package.runtime()?.type_values.get(&head.ptr()).copied()?;
        match self.get_object(ptr) {
            Object::Type(value) if &value.ty == ty => Some(ptr),
            _ => None,
        }
    }

    fn load_global_in(&self, package: HeapPtr, index: GlobalIndex) -> Value {
        if package.as_ptr().is_null() {
            return self.globals.get(self.proof(), index);
        }
        let Object::Package(package) = self.get_object(package) else {
            unreachable!("runtime owner does not point to Object::Package")
        };
        package
            .runtime()
            .and_then(|runtime| runtime.load_global(index.raw()))
            .expect("runtime global index was validated by dynamic link")
    }

    fn store_global_in(
        &mut self,
        package_ptr: HeapPtr,
        index: GlobalIndex,
        value: Value,
    ) -> Result<(), VmInternalError> {
        if package_ptr.as_ptr().is_null() {
            return self
                .globals
                .set(index, value, VmInternalError::StoreGlobalAfterInit);
        }
        let Object::Package(package) = self.get_object(package_ptr) else {
            unreachable!("runtime owner does not point to Object::Package")
        };
        let runtime = package
            .runtime()
            .expect("runtime function owner has a runtime image");
        if runtime.initialized {
            return Err(VmInternalError::StoreGlobalAfterInit);
        }
        let slot = runtime
            .globals
            .get(index.raw())
            .ok_or(VmInternalError::StoreGlobalAfterInit)?;
        slot.store(value);
        self.heap.write_barrier(package_ptr, value);
        Ok(())
    }

    /// Convert an image-local `ObjectIndex` to a heap pointer. Runtime
    /// functions address their owning package table; static functions address
    /// the immutable compile-time image.
    #[inline]
    pub fn idx_to_ptr(&self, idx: ObjectIndex) -> HeapPtr {
        let package_ptr = self.current_runtime_package();
        if package_ptr.as_ptr().is_null() {
            return self.heap.compile_time_ptr(idx.into_raw());
        }
        let Object::Package(package) = self.get_object(package_ptr) else {
            unreachable!("runtime owner does not point to Object::Package")
        };
        package
            .runtime()
            .and_then(|runtime| runtime.objects.get(idx.raw()))
            .copied()
            .expect("runtime object index was validated by dynamic link")
    }

    /// Resolve the authored function behind an ordinary generic-function
    /// value. `GenericFunction` has no projection state: it always dispatches
    /// this function directly.
    pub fn generic_function_authored_ptr(
        &self,
        generic: &bex_vm_types::GenericFunction,
    ) -> Result<HeapPtr, VmInternalError> {
        let authored_value = self.load_global_in(generic.runtime_package, generic.function);
        let authored_ptr = self.as_object_ptr(authored_value, FunctionType::Callable.into())?;
        if !matches!(self.get_object(authored_ptr), Object::Function(_)) {
            return Err(VmInternalError::TypeError {
                expected: FunctionType::Callable.into(),
                got: ObjectType::of(self.get_object(authored_ptr)).into(),
            });
        }
        Ok(authored_ptr)
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

    /// Pop the `Object::Type` on top of the stack and clone out the type it
    /// wraps.
    ///
    /// The counterpart to a preceding `LoadType`, whose pushed value is already
    /// resolved against the frame's type args. Opcodes whose type operands ride
    /// the stack rather than the instruction stream (`AllocArray`, `AllocMap`,
    /// `Spawn`) consume them this way.
    fn ensure_pop_type(&mut self) -> Result<bex_vm_types::RealizedTy, VmInternalError> {
        let value = self.stack.ensure_pop();
        // A kind view converts to the `type` value it wraps at this boundary.
        let value =
            crate::package_reflect::type_kinds::as_view_type_value(self, value).unwrap_or(value);
        let ptr = self.as_object_ptr(value, ObjectType::Type)?;
        match self.get_object(ptr) {
            Object::Type(type_value) => Ok(type_value.ty.clone()),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Type.into(),
                got: ObjectType::of(other).into(),
            }),
        }
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

    /// Truthiness of a value (B-1563): `false`, `null`, an omitted
    /// argument, zero (`0`, `0n`, `0.0`), and empty string/list/map/bytes
    /// are falsy; every other
    /// value - including `NaN`, instances, variants, closures, and
    /// futures - is truthy. The scalar fast path never touches the heap;
    /// only strings, containers, and the boxed numerics dereference.
    pub fn is_truthy(&self, value: Value) -> bool {
        use bex_vm_types::ValueKind;
        match value.kind() {
            ValueKind::Null | ValueKind::OmittedArg => false,
            ValueKind::Bool(b) => b,
            ValueKind::Int(v) => v != 0,
            ValueKind::Object(ptr) => match self.get_object(ptr) {
                Object::String(s) => !s.is_empty(),
                Object::Array(arr) => !arr.is_empty(),
                Object::Map(map) => !map.is_empty(),
                Object::Uint8Array(bytes) => !bytes.is_empty(),
                Object::Float(f) => *f != 0.0,
                Object::Bigint(n) => n.sign() != num_bigint::Sign::NoSign,
                _ => true,
            },
        }
    }

    /// Get type of a value.
    pub fn type_of(&self, value: &Value) -> Type {
        Type::of(value, |ptr| ObjectType::of(self.get_object(ptr)))
    }

    fn value_matches_type_constant(
        &self,
        frame_idx: usize,
        value: Value,
        raw_const: &ConstValue,
        resolved_const: Value,
    ) -> Result<bool, VmInternalError> {
        let frame_type_args = match &self.frames[frame_idx] {
            Frame::Bytecode(frame) => frame.type_args.as_slice(),
            Frame::Native(_) => &[],
        };
        match raw_const {
            // Structural type test against a complete `TyTemplate` (a
            // container element type, a bare frame ref, …), matched with the
            // canonical type algebra — emitted for element-discriminating
            // containers, unions, and frame refs (see the emitter's
            // `is_type`).
            ConstValue::Type(template) => {
                crate::type_match::value_matches_template(self, value, template, frame_type_args)
            }
            // Membership in a singleton type. Decided against the value's own
            // identity rather than against the type it reconstructs, because
            // `value_concrete_ty`-style reconstruction is what the general
            // algebra needs and this is the one type whose inhabitants are
            // enumerated rather than described. Equality here is *exact* — same
            // representation, same contents — never the numeric-tower widening
            // `==` applies (B-1073): `1` and `1.0` are disjoint types, so no
            // float inhabits the int-literal singleton.
            ConstValue::Literal(literal) => Ok(self.value_is_literal(value, literal)),
            ConstValue::ClassWithTypeArgs {
                class_obj,
                type_args_templates,
            } => {
                let class_ptr = self.idx_to_ptr(*class_obj);
                match value.as_object_ptr() {
                    Some(val_ptr) => match self.get_object(val_ptr) {
                        // Class-pointer identity (above) fixes the class; each
                        // type arg is then related *invariantly* through the
                        // canonical algebra (BAML generics are invariant). No
                        // covariance is needed for a reified frame type-param
                        // that inference widened to a union (`T = Shape | Sq`
                        // vs a value's narrower `Shape`): the algebra knows
                        // `Sq <: Shape` and absorbs `Shape | Sq == Shape`, so
                        // the invariant relation already holds — the retired
                        // guard needed a covariant band-aid only because it
                        // could not see that membership.
                        Object::Instance(inst) if inst.class == class_ptr => {
                            debug_assert_eq!(
                                type_args_templates.len(),
                                inst.class_type_args.len(),
                                "Class should have consistent number of generic parameters",
                            );
                            if type_args_templates.len() != inst.class_type_args.len() {
                                return Ok(false);
                            }
                            for (template, actual) in
                                type_args_templates.iter().zip(&inst.class_type_args)
                            {
                                if !crate::type_match::class_type_arg_matches(
                                    self,
                                    template,
                                    frame_type_args,
                                    actual.as_ty(),
                                )? {
                                    return Ok(false);
                                }
                            }
                            Ok(true)
                        }
                        _ => Ok(false),
                    },
                    None => Ok(false),
                }
            }
            _ => {
                if let Some(expected_ptr) = resolved_const.as_object_ptr() {
                    // Class- or enum-pointer identity: `is Foo` checks the
                    // instance's class object; `is Color` checks the variant's
                    // enum object. Enum-type tests dispatch on enum identity
                    // because the shared `ENUM` type tag cannot tell `Color`
                    // from `Status`.
                    Ok(match value.as_object_ptr() {
                        Some(val_ptr) => match self.get_object(val_ptr) {
                            Object::Instance(instance) => instance.class == expected_ptr,
                            Object::Variant(variant) => variant.enm == expected_ptr,
                            _ => false,
                        },
                        None => false,
                    })
                } else if let Some(tag) = resolved_const.as_int() {
                    Ok(value_type_tag(value) == tag)
                } else {
                    Ok(false)
                }
            }
        }
    }

    /// A callable value's [`CallableSignature`] (BEP-062 `reflect`), or `None`
    /// for a non-callable value.
    ///
    /// Every function-pointer value is a wrapper object — a plain reference
    /// is a pooled empty-type-args `GenericFunction` (see the emit-side
    /// `emit_pooled_function_value`), so a raw `Object::Function` is never a
    /// data value and deliberately has no arm here.
    ///
    /// This reports the same types [`Self::value_concrete_ty`] reconstructs for
    /// the same value (a `BoundMethod`'s receiver drops in both), plus the
    /// reflection-only metadata a structural type does not carry: the
    /// function's name, docstring, and per-parameter modes.
    pub(crate) fn callable_signature(&self, value: Value) -> Option<CallableSignature> {
        match self.get_object(value.as_object_ptr()?) {
            Object::Closure(closure) => {
                // SAFETY: `closure.function` points to a live `Function`, the
                // same invariant `resolve_callable_target` relies on.
                match unsafe { closure.function.get() } {
                    Object::Function(f) => {
                        function_callable_signature(self, f, &closure.captured_type_args, false)
                            .ok()
                    }
                    _ => None,
                }
            }
            Object::GenericFunction(gf) => {
                let target = self.generic_function_authored_ptr(gf).ok()?;
                match self.get_object(target) {
                    Object::Function(f) => {
                        function_callable_signature(self, f, &gf.type_args, false).ok()
                    }
                    _ => None,
                }
            }
            Object::BoundMethod(bm) => {
                // SAFETY: `bm.function` points to a live `Function` (the bind
                // site stored it), as at `CallIndirect`'s BoundMethod arm.
                match unsafe { bm.function.get() } {
                    Object::Function(f) => {
                        function_callable_signature(self, f, &bm.type_args, true).ok()
                    }
                    _ => None,
                }
            }
            Object::HostClosure(hc) => Some(CallableSignature {
                // Host closures are FFI-constructed; they carry no name.
                name: None,
                params: (*hc.params).clone().into(),
                // A host callable's declared bottom/unit throws is normalized to
                // `unknown` when the closure is bound (see the engine's
                // conversion): foreign code may surface a native exception no
                // matter what it declares, so its error contract is opaque
                // rather than empty. Nothing here can be `void`.
                throws: (*hc.throws_ty).clone(),
                ret: (*hc.ret_ty).clone(),
                // Host closures are FFI-constructed; they carry no docs.
                docstring: None,
            }),
            _ => None,
        }
    }

    /// Whether `value` inhabits the singleton type denoted by `literal`.
    ///
    /// The runtime counterpart of `TyTemplate::Literal`: exact representation
    /// *and* exact contents. A heap `Float` is not the int literal `1` however
    /// close its magnitude, and a `bigint` is not an `int` — those are disjoint
    /// types (`TYPE_SYSTEM.md` "Concrete Types": no concrete type is a subtype
    /// of another), even though the `==` operator deliberately widens across
    /// all three.
    ///
    /// Float equality is exact here by construction, not by oversight: a
    /// singleton holds one value, so membership is identity and a tolerance
    /// would admit values the type excludes.
    #[allow(clippy::float_cmp)]
    pub(crate) fn value_is_literal(&self, value: Value, literal: &baml_type::Literal) -> bool {
        match literal {
            baml_type::Literal::Int(n) => value.as_int() == Some(*n),
            baml_type::Literal::Bool(b) => value.as_bool() == Some(*b),
            baml_type::Literal::String(s) => match value.as_object_ptr() {
                Some(ptr) => matches!(self.get_object(ptr), Object::String(v) if **v == **s),
                None => false,
            },
            baml_type::Literal::Bigint(n) => match value.as_object_ptr() {
                Some(ptr) => matches!(self.get_object(ptr), Object::Bigint(v) if **v == *n),
                None => false,
            },
            // No surface syntax produces a float literal *type* (the parser
            // rejects `1.0` in type position), so this arm is unreachable from
            // BAML source. It is still answered exactly rather than defaulted,
            // so a future float-literal type cannot inherit a silent `false`.
            baml_type::Literal::Float(repr) => match value.as_object_ptr() {
                Some(ptr) => match self.get_object(ptr) {
                    Object::Float(v) => repr.parse::<f64>().is_ok_and(|expected| *v == expected),
                    _ => false,
                },
                None => false,
            },
        }
    }

    /// The value's type at maximum precision — its *singleton* type where it
    /// has one, otherwise the same leaf [`Self::value_concrete_ty`] reports.
    ///
    /// `TYPE_SYSTEM.md` "Values and membership" defines membership as "`v` is a
    /// member of `T` iff `v`'s concrete type is a subtype of `T`". That only
    /// decides literal types if the reconstructed type is the value's *most
    /// precise* one:
    /// reporting the int `1` as `int` makes `is_subtype(int, 1)` false and no
    /// value would ever inhabit a literal type. Precision costs nothing at the
    /// other end — `Literal(Int, 1) <: int` holds — so every test the coarser
    /// reconstruction passed still passes.
    ///
    /// Deliberately separate from [`Self::value_concrete_ty`] rather than
    /// replacing it: impl-registry dispatch keys on that one, and `implement I
    /// for int` must resolve for every int, not for the singleton `1`.
    pub(crate) fn value_singleton_ty(&self, value: Value) -> Option<bex_vm_types::RealizedTy> {
        use baml_type::{Freshness, TyAttr};
        use bex_vm_types::RealizedTy;
        let literal = |lit| RealizedTy::Literal(lit, Freshness::Regular, TyAttr::default());
        if let Some(n) = value.as_int() {
            return Some(literal(baml_type::Literal::Int(n)));
        }
        if let Some(b) = value.as_bool() {
            return Some(literal(baml_type::Literal::Bool(b)));
        }
        if let Some(ptr) = value.as_object_ptr() {
            match self.get_object(ptr) {
                Object::String(s) => {
                    return Some(literal(baml_type::Literal::String((**s).to_owned())));
                }
                Object::Bigint(n) => {
                    return Some(literal(baml_type::Literal::Bigint((**n).clone())));
                }
                // A float has no literal type to be precise about, and every
                // other object's precise type is already its concrete one.
                _ => {}
            }
        }
        Some(self.value_concrete_ty(value)?.into())
    }

    /// The callable's own `Function` plus the type arguments it already carries.
    ///
    /// A callable value is one of three shapes, each currying its arguments in
    /// its own field; the two questions below both need the same pair, so they
    /// ask it here rather than each re-matching the three.
    pub(crate) fn callable_function_and_type_args(
        &self,
        value: Value,
    ) -> Option<(&Function, &[bex_vm_types::RealizedTy])> {
        match self.get_object(value.as_object_ptr()?) {
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(function) => Some((function, &closure.captured_type_args)),
                _ => None,
            },
            Object::GenericFunction(generic) => {
                let target = self.generic_function_authored_ptr(generic).ok()?;
                match self.get_object(target) {
                    Object::Function(function) => Some((function, &generic.type_args)),
                    _ => None,
                }
            }
            Object::BoundMethod(method) => match unsafe { method.function.get() } {
                Object::Function(function) => Some((function, &method.type_args)),
                _ => None,
            },
            _ => None,
        }
    }

    fn callable_display_name(function: &Function) -> String {
        function
            .declared_name
            .clone()
            .unwrap_or_else(|| function.name.clone())
    }

    /// Name an otherwise callable value whose generic frame is incomplete.
    /// Reflection uses this to distinguish an unspecialized generic from a
    /// genuinely non-callable value when signature reconstruction fails.
    pub(crate) fn unspecialized_generic_callable_name(&self, value: Value) -> Option<String> {
        let (function, type_args) = self.callable_function_and_type_args(value)?;
        (type_args.len() < function.generic_param_bounds.len())
            .then(|| Self::callable_display_name(function))
    }

    /// Name a callable whose *body* cannot be realized against the type-argument
    /// frame it carries.
    ///
    /// A generic function's declared signature can be free of its own type
    /// parameters — a companion like `GenericList@spec` can expose a callable
    /// signature whose body still materializes the parent's type arguments —
    /// so signature reconstruction succeeds and the value looks ordinary. Its body still
    /// materializes `T` (the output-format schema, for one), and entering it
    /// with an empty frame fails deep inside `LoadType` as a VM internal error
    /// that no `catch` can see. Reflection asks this question before handing
    /// such a value out or dispatching it, so the caller gets a diagnostic.
    ///
    /// The check runs the very substitution the body would run, so detection and
    /// failure cannot drift apart; it is gated on the callable being a generic
    /// missing arguments, which keeps it off every ordinary call.
    pub(crate) fn generic_callable_body_needs_type_args(&self, value: Value) -> Option<String> {
        let (function, type_args) = self.callable_function_and_type_args(value)?;
        if type_args.len() >= function.generic_param_bounds.len() {
            return None;
        }
        let unrealizable = |template: &bex_vm_types::TyTemplate| {
            matches!(
                template.substitute(type_args, self),
                Err(baml_type::SubstituteError::TypeArgRefOutOfRange { .. })
            )
        };
        let needs_arguments = function.param_types.iter().any(&unrealizable)
            || unrealizable(&function.return_type)
            || unrealizable(&function.throws_type)
            || function.bytecode.constants.iter().any(
                |constant| matches!(constant, ConstValue::Type(template) if unrealizable(template)),
            );
        needs_arguments.then(|| Self::callable_display_name(function))
    }

    /// Resolve `method_name` of the interface `iface_value` names for the
    /// `Self` type `self_ty` — the shared tail of `MakeVirtualBoundMethod`
    /// (which DERIVES `self_ty` from the receiver value) and
    /// `MakeVirtualFunction` (which takes it as a PASSED type operand). Either
    /// way the resolution key is `(Self type, interface, method)`; coherence
    /// guarantees at most one matching rule. Returns the resolved impl
    /// method's function pointer and the callee's complete frame: the impl's
    /// realized frame followed by `method_type_args`.
    ///
    /// `world_anchor` selects the dynamic world the resolution runs in: the
    /// receiver value or the `Self` type value, whichever the caller popped —
    /// both carry their owning runtime package.
    fn resolve_virtual_method(
        &self,
        world_anchor: Value,
        self_ty: &bex_vm_types::RealizedTy,
        iface_value: Value,
        method_name: &str,
        method_type_args: TakenTypeArgs,
    ) -> Result<(HeapPtr, Vec<bex_vm_types::RealizedTy>), VmError> {
        let (iface_head, iface_args) = {
            let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
            match self.get_object(iface_ptr) {
                Object::Type(type_value) => match &type_value.ty {
                    bex_vm_types::RealizedTy::Interface(head, args, _assoc, _attr) => {
                        (*head, args.clone())
                    }
                    other => unreachable!(
                        "virtual-method interface operand must be an Interface type, \
                         found {other:?}"
                    ),
                },
                other => unreachable!(
                    "as_object_ptr(Type) guarantees a Type object, found {:?}",
                    ObjectType::of(other)
                ),
            }
        };
        let resolver = crate::package_baml::ImplResolver::for_value(self, world_anchor);
        let (rule, bound_args) = resolver
            .resolve_implements_rule(self_ty, iface_head, &iface_args)
            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                method: method_name.to_string(),
            })?;
        let method = resolver
            .rule_method_impl(&rule, method_name)
            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                method: method_name.to_string(),
            })?
            .method;
        let mut frame = resolver.realize_frame(&method.frame, &bound_args)?;
        // Only `.tys` reaches the callee frame. `method_type_args.values` (the
        // exact `TypeValue`s) is dropped, which is sound here: a type argument
        // carries its declaration heads inside the `RealizedTy` itself, so the
        // callee resolves the same declarations without a separate metadata
        // channel. `VirtualCall` still threads the exact values via
        // `append_virtual_method_type_args` so `LoadType<T>` reuses the
        // created-once value rather than allocating an equal twin.
        frame.extend(method_type_args.tys);
        Ok((method.fqn, frame))
    }

    /// The value's concrete type as a [`ConcreteRealizedTy`] — the invariant every
    /// runtime value's type satisfies (a concrete top with realized arguments, no
    /// type variables) made explicit in the type. `None` for a value kind that
    /// has no such type (a compile-time definition object or an opaque native handle).
    ///
    /// Primitives construct their leaf directly; an `Instance` narrows its stored
    /// `class_type_args` into the argument list (so `Box<int>` resolves the `Box`
    /// impl at `T = int`); an enum `Variant` maps to its enum; a container narrows
    /// its element/key/value types; a future reports the `Future<T, E>` its spawn
    /// site was typed at; a `Cell` is transparent. A value's arguments
    /// are realized by construction, so a per-argument narrow (see [`realized_arg`])
    /// fails (→ `None`) only if a residual type variable leaked in — a bug.
    ///
    /// The interface resolver wants the loose `RuntimeTy`, so the sole such caller
    /// widens the result back. The `IsType` value matcher goes through
    /// [`Self::value_singleton_ty`] instead — it needs the value's *most precise*
    /// type, which for a primitive is its singleton, not its base leaf.
    pub(crate) fn value_concrete_ty(
        &self,
        value: Value,
    ) -> Option<bex_vm_types::ConcreteRealizedTy> {
        use baml_type::TyAttr;
        use bex_vm_types::ConcreteRealizedTy;
        if value.as_int().is_some() {
            return Some(ConcreteRealizedTy::Int {
                attr: TyAttr::default(),
            });
        }
        if value.as_bool().is_some() {
            return Some(ConcreteRealizedTy::Bool {
                attr: TyAttr::default(),
            });
        }
        if value.is_null() {
            return Some(ConcreteRealizedTy::Null {
                attr: TyAttr::default(),
            });
        }
        Some(match self.get_object(value.as_object_ptr()?) {
            Object::Float(_) => ConcreteRealizedTy::Float {
                attr: TyAttr::default(),
            },
            Object::Bigint(_) => ConcreteRealizedTy::Bigint {
                attr: TyAttr::default(),
            },
            Object::String(_) => ConcreteRealizedTy::String {
                attr: TyAttr::default(),
            },
            Object::Uint8Array(_) => ConcreteRealizedTy::Uint8Array {
                attr: TyAttr::default(),
            },
            Object::Instance(inst) => match self.get_object(inst.class) {
                Object::Class(class) => {
                    // Media values are `Object::Instance`s of the std media classes
                    // (`baml.media.{Image,Audio,Video,Pdf}`), but their concrete
                    // type is the `image`/`audio`/… primitive
                    // (`ConcreteRealizedTy::Media`) — which is how the impl registry
                    // keys `implement I for image`. Return that, not the class.
                    if let Some(kind) = crate::package_baml::json::media_kind_from_fqn(
                        class.name.display_name().as_str(),
                    ) {
                        ConcreteRealizedTy::Media(kind, TyAttr::default())
                    } else {
                        debug_assert_eq!(inst.class_type_args.len(), class.generic_param_count);
                        // A generic instance's stored `class_type_args` are already
                        // realized (`Box<int>` ⇒ `T = int`), so they are exactly the
                        // `ConcreteRealizedTy::Class` argument list.
                        ConcreteRealizedTy::Class(
                            bex_vm_types::TypeHead::new(inst.class, class.type_tag),
                            inst.class_type_args.clone(),
                            TyAttr::default(),
                        )
                    }
                }
                other => unreachable!(
                    "Instance.class must point to a Class, found {:?}",
                    ObjectType::of(other)
                ),
            },
            Object::Variant(v) => match self.get_object(v.enm) {
                Object::Enum(e) => ConcreteRealizedTy::Enum(
                    bex_vm_types::TypeHead::new(v.enm, e.type_tag),
                    TyAttr::default(),
                ),
                other => unreachable!(
                    "Variant.enm must point to an Enum, found {:?}",
                    ObjectType::of(other)
                ),
            },
            // A `type` value's concrete type is the `reflect.Type` metatype
            // itself. The nine kind views are ordinary wrapper classes whose
            // instances take the `Object::Instance` arm above; nothing about a
            // type value's membership depends on classifying its payload.
            Object::Type(_) => ConcreteRealizedTy::Type {
                attr: TyAttr::default(),
            },
            // Arrays/maps carry their element/key/value types, so the faithful
            // `list<T>` / `map<K, V>` is reconstructed from the value itself.
            Object::Array(arr) => {
                ConcreteRealizedTy::List(Box::new((*arr.element_ty).clone()), TyAttr::default())
            }
            Object::Map(map) => ConcreteRealizedTy::Map {
                key: Box::new((*map.key_ty).clone()),
                value: Box::new((*map.value_ty).clone()),
                attr: TyAttr::default(),
            },
            // A cell is a transparent capture/mutable-binding slot, not a value
            // of its own: its concrete type is that of the value it holds.
            Object::Cell(cell) => return self.value_concrete_ty(cell.load()),

            // ── Function-pointer values ──────────────────────────────────────
            // These are user-facing callables; their concrete type is the
            // function's signature templates materialized against the realized
            // frame the value carries, so one minted in a generic frame is as
            // precise as any other.
            Object::Closure(closure) => {
                // SAFETY: `closure.function` points to a live `Function`, the
                // same invariant `resolve_callable_target` relies on.
                match unsafe { closure.function.get() } {
                    Object::Function(f) => {
                        function_object_ty(self, f, &closure.captured_type_args, false).ok()?
                    }
                    _ => return None,
                }
            }
            Object::GenericFunction(gf) => {
                // Resolve the underlying function through the global table, as
                // at call time; its `type_args` are the frame the signature
                // templates materialize against.
                let target = self.generic_function_authored_ptr(gf).ok()?;
                match self.get_object(target) {
                    Object::Function(f) => {
                        function_object_ty(self, f, &gf.type_args, false).ok()?
                    }
                    _ => return None,
                }
            }
            Object::HostClosure(hc) => ConcreteRealizedTy::Function {
                // The host-closure signature is already stored as realized types.
                params: (*hc.params).clone().into(),
                ret: Box::new((*hc.ret_ty).clone()),
                throws: Box::new((*hc.throws_ty).clone()),
                attr: TyAttr::default(),
            },
            // A bound method's type is its function's type with the receiver
            // already applied, so the leading `self` parameter drops. Its
            // complete curried frame is on the object, so a generic method
            // reconstructs as precisely as any other callable.
            Object::BoundMethod(bm) => {
                // SAFETY: `bm.function` points to a live `Function`, the same
                // invariant `resolve_callable_target` relies on.
                match unsafe { bm.function.get() } {
                    Object::Function(f) => function_object_ty(self, f, &bm.type_args, true).ok()?,
                    _ => return None,
                }
            }

            // `Object::Function` is NOT a function-pointer value — it is the
            // internal function representation that acts as the type constructor
            // for the callables above, so like the other compile-time definition
            // objects (a package, class, enum, interface, type alias, or impl
            // rule) it is never a *data value* reaching a type test.
            Object::Function(_)
            | Object::Package(_)
            | Object::Class(_)
            | Object::Enum(_)
            | Object::TypeAlias(_)
            | Object::Interface(_)
            | Object::ImplRule(_) => return None,

            // A future carries the `<T, E>` it was spawned at (resolved against
            // the spawning frame), so its concrete type is the faithful
            // `Future<T, E>` — the subject of `is`/`match` arms and
            // `reflect.Type.of`.
            Object::Future(fut) => ConcreteRealizedTy::Future(
                Box::new(fut.returns().clone()),
                Box::new(fut.throws().clone()),
                TyAttr::default(),
            ),
            // An `UnscheduledFuture` is the engine's spawn-request slot, consumed
            // before control returns to the VM. It is never a value user code can
            // hold, so it has no type of its own.
            Object::UnscheduledFuture(_) => return None,

            // Opaque native handles are not BAML data types at all.
            Object::RustData(_) | Object::Collector(_) => return None,

            // A GC-debug sentinel is never a live value.
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => return None,
        })
    }

    /// Build the allocation-light receiver half of a static virtual-dispatch
    /// cache key. Static class tags are immutable and unique in one program,
    /// so non-generic instances avoid rebuilding and hashing their qualified
    /// class name on every method call. Runtime-built classes stay off this
    /// cache because their dispatch tables are live.
    fn static_virtual_receiver_key(&self, value: Value) -> Option<StaticVirtualReceiverKey> {
        if let Some(ptr) = value.as_object_ptr()
            && let Object::Instance(instance) = self.get_object(ptr)
            && let Object::Class(class) = self.get_object(instance.class)
            && self.heap.is_compile_time_ptr(instance.class)
        {
            return Some(StaticVirtualReceiverKey::Class {
                type_tag: class.type_tag.as_i64(),
                type_args: instance.class_type_args.clone(),
            });
        }
        self.value_concrete_ty(value)
            .map(bex_vm_types::RealizedTy::from)
            .map(StaticVirtualReceiverKey::Other)
    }

    /// Runtime callers use the same immutable-rule cache as static callers,
    /// but their movable function-object pointer is tagged for GC forwarding.
    /// Keep this uncommon key construction out of the ordinary interpreter
    /// path so static bytecode retains its existing hot-loop layout.
    fn runtime_cache_function_addr(&self, callable: HeapPtr) -> Option<usize> {
        let function_object = match self.get_object(callable) {
            Object::Function(_) => callable,
            Object::Closure(closure) => closure.function,
            Object::BoundMethod(method) => method.function,
            Object::GenericFunction(function) => {
                self.generic_function_authored_ptr(function).ok()?
            }
            _ => return None,
        };
        matches!(self.get_object(function_object), Object::Function(_))
            .then_some(function_object.as_ptr() as usize | 1)
    }

    #[inline(never)]
    fn runtime_virtual_call_cache_key(
        &self,
        callable: HeapPtr,
        iface_value: Value,
        receiver: Value,
    ) -> Result<Option<StaticVirtualCallKey>, VmError> {
        let Some(caller_function_addr) = self.runtime_cache_function_addr(callable) else {
            return Ok(None);
        };
        let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
        let Object::Type(type_value) = self.get_object(iface_ptr) else {
            unreachable!("as_object_ptr(Type) guarantees a Type object")
        };
        let (interface_head, interface_args) = match &type_value.ty {
            bex_vm_types::RealizedTy::Interface(head, args, _, _) => (*head, args.clone()),
            other => unreachable!(
                "VirtualCall interface operand must be an Interface type, found {other:?}"
            ),
        };
        Ok(self
            .static_virtual_receiver_key(receiver)
            .map(|receiver| StaticVirtualCallKey {
                caller_function_addr,
                call_pc: self.cur_pc,
                receiver,
                interface_head,
                interface_args,
            }))
    }

    #[inline(never)]
    fn runtime_load_type_cache_key(
        &self,
        frame_idx: usize,
        template: &bex_vm_types::TyTemplate,
        constant: usize,
    ) -> Option<(usize, usize)> {
        if <&bex_vm_types::RealizedTy>::try_from(template).is_err()
            || !matches!(&self.frames[frame_idx],
                Frame::Bytecode(frame)
                    if frame.type_metadata.as_ref()
                        .is_none_or(|metadata| !metadata.binds_runtime_declaration(&self.heap)))
        {
            return None;
        }
        Some((
            self.runtime_cache_function_addr(self.frames[frame_idx].function())?,
            constant,
        ))
    }

    /// Runtime package that owns a value's nominal/callable definition.
    /// Static values and standalone runtime-constructed types return null.
    pub(crate) fn value_runtime_package(&self, value: Value) -> HeapPtr {
        let Some(ptr) = value.as_object_ptr() else {
            return HeapPtr::null();
        };
        match self.get_object(ptr) {
            Object::Instance(instance) => match self.get_object(instance.class) {
                Object::Class(class) => class.owner,
                _ => HeapPtr::null(),
            },
            Object::Variant(variant) => match self.get_object(variant.enm) {
                Object::Enum(enm) => enm.owner,
                _ => HeapPtr::null(),
            },
            // A type value has no owner of its own: it owns nothing, it *names*
            // things. The package is whichever one declared the type's head.
            Object::Type(value) => match &value.ty {
                bex_vm_types::RealizedTy::Class(head, ..)
                | bex_vm_types::RealizedTy::Enum(head, ..)
                | bex_vm_types::RealizedTy::Interface(head, ..)
                | bex_vm_types::RealizedTy::TypeAlias(head, ..)
                    if head.is_resolved() =>
                {
                    match self.get_object(head.ptr()) {
                        Object::Class(class) => class.owner,
                        Object::Enum(enm) => enm.owner,
                        Object::Interface(interface) => interface.owner,
                        Object::TypeAlias(alias) => alias.owner,
                        _ => HeapPtr::null(),
                    }
                }
                _ => HeapPtr::null(),
            },
            Object::Function(function) => function.runtime_package,
            Object::GenericFunction(function) => function.runtime_package,
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(function) => function.runtime_package,
                _ => HeapPtr::null(),
            },
            Object::BoundMethod(method) => match unsafe { method.function.get() } {
                Object::Function(function) => function.runtime_package,
                _ => HeapPtr::null(),
            },
            Object::Cell(cell) => self.value_runtime_package(cell.load()),
            _ => HeapPtr::null(),
        }
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

        // Create heap with compile-time objects, additionally allocating the
        // per-package `Object::Package` / `Object::ImplRule` objects.
        let (heap, package_index) =
            crate::package_load::build_heap_with_packages(compile_time_objects, &bytecode.packages);

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

        let error_class_ptrs = resolve_error_class_ptrs(&package_index);
        let panic_class_ptrs = resolve_panic_class_ptrs(&package_index);
        Ok(Self::new(
            heap,
            globals,
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
            Arc::from(Vec::<String>::new()),
            Arc::new(package_index),
            Arc::new(crate::package_load::DynDispatchTables::default()),
            error_class_ptrs,
            panic_class_ptrs,
        ))
    }

    /// Bootstraps the VM preparing the given callable to run.
    ///
    /// `function` may point to an [`Object::Function`], [`Object::Closure`],
    /// [`Object::GenericFunction`], or [`Object::HostClosure`]. Closure entry
    /// points are used by BEP-034
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
        let positional = match self.get_object(function) {
            Object::Function(_) => vec![],
            Object::Closure(closure) => closure.captured_type_args.to_vec(),
            Object::GenericFunction(gf) => gf.type_args.to_vec(),
            Object::HostClosure(_) => vec![],
            other => panic!("expect callable as entry point, got {other:?}"),
        };
        // The captured/specialized type args are positional (De Bruijn order);
        // pair each with the callee's generic-param name so they round-trip
        // through the named `set_entry_point_with_type_args` channel. A lambda
        // (spawn closure) has no declared param names — its captured args are
        // inherited positional slots — so fall back to the index as a key; the
        // named lowering then emits the unnamed bindings in order.
        let param_names = self.entry_point_generic_param_names(function);
        let type_args: IndexMap<String, bex_vm_types::RealizedTy> = positional
            .into_iter()
            .enumerate()
            .map(|(i, ty)| {
                (
                    param_names.get(i).cloned().unwrap_or_else(|| i.to_string()),
                    ty,
                )
            })
            .collect();
        self.set_entry_point_with_type_args(function, args, type_args);
    }

    /// Like [`Self::set_entry_point`], but seeds the entry frame's
    /// `type_args` slot. Use when the host invokes a generic function
    /// (e.g. a user function with `<T>`) and needs to thread `T` through.
    ///
    /// Accepts *named* `TypeVar` bindings (`name -> type`, insertion order is the
    /// host's De Bruijn order) and lowers them to the positional `type_args` slot
    /// here — the one place where the callee `HeapPtr` is resolved (so its
    /// generic-param names are known) and the entry frame is built. Each binding
    /// is placed at the index of the matching generic param in the callee's De
    /// Bruijn-ordered param list (enclosing class params first, then the
    /// function's own params), recovered from `Function::display_type_params`.
    /// Unbound slots default to the unknown/top type and unrecognized names are
    /// ignored — both rollout-safe, mirroring the wire decode default.
    ///
    /// Bytecode entry points are pushed directly. Native and sysop entries are
    /// wrapped in a synthetic bytecode caller that executes either
    /// `CALL <native>; RETURN` or `SYS_OP <sysop>; RETURN`, giving the normal VM
    /// machinery a bytecode frame to resume into.
    pub fn set_entry_point_with_type_args(
        &mut self,
        function: HeapPtr,
        args: &[Value],
        type_args: IndexMap<String, bex_vm_types::RealizedTy>,
    ) {
        self.set_entry_point_with_type_values(function, args, type_args, IndexMap::new());
    }

    /// Entry-point variant that preserves exact definition-carrying host type
    /// values in the generic slots. Structural/static bindings omit entries in
    /// `type_values` and retain the normal reconstruction path.
    pub fn set_entry_point_with_type_values(
        &mut self,
        function: HeapPtr,
        args: &[Value],
        type_args: IndexMap<String, bex_vm_types::RealizedTy>,
        type_values: IndexMap<String, TypeValue>,
    ) {
        debug_assert!(
            matches!(
                self.get_object(function),
                Object::Function(_)
                    | Object::Closure(_)
                    | Object::GenericFunction(_)
                    | Object::HostClosure(_)
            ),
            "expect callable as entry point, got {:?}",
            self.get_object(function)
        );

        // A fresh top-level run starts with no in-flight throw, so drop the
        // per-run throw bookkeeping from the previous run. These vectors are
        // GC roots; on a reused VM (engine package init, playground evals)
        // stale entries would keep every previously thrown value live and
        // grow the per-throw scans without bound. Guarded on an empty frame
        // stack so a nested entry over live frames cannot wipe in-flight
        // throw state.
        if self.frames.is_empty() {
            self.thrown_value_causes.clear();
            self.thrown_value_contexts.clear();
            self.preserved_throw_contexts.clear();
        }

        // Lower the named bindings onto the positional De Bruijn slot against the
        // callee's generic params before seeding the frame.
        let param_names = self.entry_point_generic_param_names(function);
        let type_args = lower_named_type_args(&param_names, type_args);
        let type_values = lower_named_type_values(&param_names, type_values);

        // Host closures have no backing `Object::Function`, so enter them
        // through a tiny `CALL_INDIRECT; RETURN` bytecode wrapper. This is the
        // same dispatch path an ordinary BAML expression uses for a host
        // callable and therefore yields `BamlHostCallHostValue` to the engine.
        if matches!(self.get_object(function), Object::HostClosure(_)) {
            debug_assert!(type_args.is_empty(), "host closures have no type arguments");
            self.push_host_closure_trampoline_frame(function, args);
            return;
        }

        // Normalize a specialized callable entry point to its concrete inner
        // function (`dispatch_ptr`) and stored specialization, so the bytecode
        // frame and native/sysop trampoline always see an Object::Function.
        let mut dispatch_ptr = function;
        let mut effective_type_args = type_args;
        let mut effective_type_values = type_values;
        let (callable_kind, entry_function_id, entry_capture_class) =
            match self.get_object(function) {
                Object::Function(f) => (
                    f.kind,
                    f.function_id,
                    if matches!(&f.body_meta, Some(FunctionMeta::Llm { .. })) {
                        FunctionCaptureClass::Llm
                    } else {
                        FunctionCaptureClass::Ordinary
                    },
                ),
                Object::Closure(closure) => {
                    let func_obj = unsafe { closure.function.get() };
                    match func_obj {
                        Object::Function(f) => (
                            f.kind,
                            f.function_id,
                            if matches!(&f.body_meta, Some(FunctionMeta::Llm { .. })) {
                                FunctionCaptureClass::Llm
                            } else {
                                FunctionCaptureClass::Ordinary
                            },
                        ),
                        other => unreachable!("expect closure function, got {other:?}"),
                    }
                }
                Object::GenericFunction(gf) => {
                    effective_type_args = gf.type_args.to_vec();
                    effective_type_values = vec![None; effective_type_args.len()];
                    dispatch_ptr = self
                        .generic_function_authored_ptr(gf)
                        .expect("generic function global resolves to a function");
                    match unsafe { dispatch_ptr.get() } {
                        Object::Function(f) => (
                            f.kind,
                            f.function_id,
                            if matches!(&f.body_meta, Some(FunctionMeta::Llm { .. })) {
                                FunctionCaptureClass::Llm
                            } else {
                                FunctionCaptureClass::Ordinary
                            },
                        ),
                        other => unreachable!("expect generic function inner, got {other:?}"),
                    }
                }
                other => unreachable!("expect function or closure as entry point, got {other:?}"),
            };

        match callable_kind {
            FunctionKind::Bytecode => {
                self.pending_call_type_args.clone_from(&effective_type_args);
                self.pending_call_type_values
                    .clone_from(&effective_type_values);
                let type_metadata = if effective_type_values.iter().all(Option::is_none) {
                    None
                } else {
                    Some(Box::new(FrameTypeMetadata {
                        values: effective_type_values,
                    }))
                };
                self.stack.extend(args.iter().copied());
                // The thread-root call: parent_call_id is 0 on a fresh VM.
                let capture_plan = self.prof_resolve_capture_plan(entry_capture_class, None);
                let mut capture_mask = VmCaptureMask::from_capture_plan(capture_plan);
                let (call_id, parent_call_id, start_accepted) =
                    self.prof_enter_call(entry_function_id, None, capture_plan);
                if !start_accepted {
                    capture_mask = VmCaptureMask::disabled();
                }
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function: dispatch_ptr,
                    instruction_ptr: 0,
                    locals_offset: StackIndex::from_raw(0),
                    type_args: effective_type_args,
                    type_metadata,
                    faulting_pc: 0,
                    call_id,
                    parent_call_id,
                    capture_mask,
                    function_id: entry_function_id,
                }));

                // Entry functions need the same frame-local pre-allocation as normal
                // bytecode calls now that INIT_LOCALS is gone from bytecode.
                self.allocate_real_locals_for_frame(dispatch_ptr)
                    .expect("entry point must be a valid function frame");
            }
            FunctionKind::Native(_) | FunctionKind::SysOp(_) => {
                self.push_trampoline_frame(
                    dispatch_ptr,
                    args,
                    effective_type_args,
                    &effective_type_values,
                    callable_kind,
                );
            }
            FunctionKind::NativeUnresolved => {
                unreachable!("entry point kind is not directly invokable: {callable_kind:?}");
            }
        }
    }

    /// The callee's De Bruijn-ordered generic-param names (bare, bounds
    /// stripped), recovered from the resolved `Object::Function`. Mirrors the
    /// `Function`/`Closure`/`GenericFunction` normalization in
    /// [`Self::set_entry_point_with_type_args`]. Empty when the entry has no
    /// generic params or its function object can't be resolved.
    fn entry_point_generic_param_names(&self, function: HeapPtr) -> Vec<String> {
        let display_type_params = match self.get_object(function) {
            Object::Function(f) => Some(&f.display_type_params),
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(f) => Some(&f.display_type_params),
                _ => None,
            },
            Object::GenericFunction(gf) => {
                let target = self.generic_function_authored_ptr(gf).ok();
                match target.map(|p| unsafe { p.get() }) {
                    Some(Object::Function(f)) => Some(&f.display_type_params),
                    _ => None,
                }
            }
            _ => None,
        };
        display_type_params
            .map(|params| {
                params
                    .iter()
                    // `display_type_params` may render bounds ("T extends Foo");
                    // the bare TypeVar name is the leading whitespace-free token.
                    .map(|p| p.split_whitespace().next().unwrap_or(p).to_string())
                    .collect()
            })
            .unwrap_or_default()
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
        type_args: Vec<bex_vm_types::RealizedTy>,
        type_values: &[Option<TypeValue>],
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
                for (index, ty) in type_args.into_iter().enumerate() {
                    let ty_ptr = match type_values.get(index).and_then(Clone::clone) {
                        Some(value) => self.alloc_type(value),
                        None => self.alloc_type(bex_vm_types::types::TypeValue::new(ty)),
                    };
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

        let display_return_type = return_type.to_string();
        let entry_function = Function {
            name: format!("$entry::{callee_name}"),
            source_file: String::new(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type,
            throws_type,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,
            capture: bex_vm_types::FunctionCaptureProps::disabled(),
            function_id: 0, // synthetic; not in the profiling function table
            runtime_package: HeapPtr::null(),
        };
        let entry_ptr = self.tlab.alloc(Object::Function(Box::new(entry_function)));

        // Synthetic `$entry::` wrapper frame; the wrapped native/sysop emits
        // its own pair through the normal Call/SysOp instruction paths.
        let capture_plan = self.prof_resolve_capture_plan(FunctionCaptureClass::Ordinary, None);
        let mut capture_mask = VmCaptureMask::from_capture_plan(capture_plan);
        let (call_id, parent_call_id, start_accepted) = self.prof_enter_call(0, None, capture_plan);
        if !start_accepted {
            capture_mask = VmCaptureMask::disabled();
        }
        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            type_metadata: None,
            faulting_pc: 0,
            call_id,
            parent_call_id,
            capture_mask,
            function_id: 0,
        }));
    }

    /// Enter a host-owned callable as a VM root by reproducing the normal
    /// indirect-call stack shape: arguments first, callable on top. The
    /// synthetic wrapper gives the yielded host sys-op a bytecode frame to
    /// resume into and a normal `Return` path after the host result arrives.
    fn push_host_closure_trampoline_frame(&mut self, closure: HeapPtr, args: &[Value]) {
        let (arity, return_type, throws_type) = match self.get_object(closure) {
            Object::HostClosure(hc) => {
                // A host closure's signature is already realized, and a realized
                // type is a valid template — the trampoline frame seeds no type
                // args, so nothing is left to substitute.
                (
                    hc.arity,
                    bex_vm_types::TyTemplate::from((*hc.ret_ty).clone()),
                    bex_vm_types::TyTemplate::from((*hc.throws_ty).clone()),
                )
            }
            other => unreachable!("expect host closure as entry point, got {other:?}"),
        };
        debug_assert_eq!(
            args.len(),
            arity,
            "host closure entry point received the wrong number of arguments"
        );

        self.pending_call_type_args.clear();
        self.stack.extend(args.iter().copied());
        self.stack.push(Value::object(closure));

        let mut bytecode = bytecode::Bytecode {
            instructions: vec![Instruction::CallIndirect, Instruction::Return],
            ..bytecode::Bytecode::default()
        };
        bytecode.compact = Some(bytecode.lower_to_compact());

        let display_return_type = return_type.to_string();
        let entry_function = Function {
            name: "$entry::<host-callable>".to_string(),
            source_file: String::new(),
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type,
            throws_type,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,
            capture: bex_vm_types::FunctionCaptureProps::disabled(),
            function_id: 0,
            runtime_package: HeapPtr::null(),
        };
        let entry_ptr = self.tlab.alloc(Object::Function(Box::new(entry_function)));
        let capture_plan = self.prof_resolve_capture_plan(FunctionCaptureClass::Ordinary, None);
        let mut capture_mask = VmCaptureMask::from_capture_plan(capture_plan);
        let (call_id, parent_call_id, start_accepted) = self.prof_enter_call(0, None, capture_plan);
        if !start_accepted {
            capture_mask = VmCaptureMask::disabled();
        }
        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            type_metadata: None,
            faulting_pc: 0,
            call_id,
            parent_call_id,
            capture_mask,
            function_id: 0,
        }));
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
            Err(panic) => Err(VmError::thrown_fresh(self.panic_to_exception_value(panic))),
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
            Err(panic) => Err(VmError::thrown_fresh(self.panic_to_exception_value(panic))),
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
        self.lookup_type_by_fqn(name)
            .unwrap_or_else(|| panic!("resolve_class: class {name:?} not found"))
    }

    /// Look up a function by its fully-qualified name by scanning `vm.globals`.
    ///
    /// Returns `Some(ptr)` for the first `Object::Function` whose `name` matches,
    /// or `None` if no such function exists in the global pool.
    ///
    /// This is O(globals) and intended for use in native methods that need to
    /// dispatch to a dynamically resolved FREE stdlib function (e.g.
    /// `baml.json.to`). Not suitable for hot paths; callers that need repeated
    /// lookups should cache the result. Interface-machinery bodies are
    /// excluded: a body's `name` is display-only (its identity is the
    /// implements relation), so this scan must never become a name→body
    /// channel.
    pub fn find_function_by_name(&self, name: &str) -> Option<HeapPtr> {
        for v in self.globals.as_slice(self.proof()) {
            if let Some(ptr) = v.as_object_ptr() {
                if let Object::Function(f) = self.get_object(ptr) {
                    if f.name == name && !f.is_interface_body {
                        return Some(ptr);
                    }
                }
            }
        }
        None
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
            Object::GenericFunction(gf) => {
                let func_ptr = self.generic_function_authored_ptr(gf)?;
                // SAFETY: function globals hold compile-time Function objects.
                let func_obj = unsafe { func_ptr.get() };
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

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn store_local_value(&mut self, local_var_index: StackIndex, value: Value) {
        self.stack.set_at(local_var_index, value);
    }

    pub fn error_to_exception_value(&mut self, error: VmBamlError) -> Value {
        let (class, fields) = match error {
            VmBamlError::InvalidArgument { message } => (
                ErrorClass::InvalidArgument,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::ParseError { message } => (
                ErrorClass::ParseError,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::Io { message } => (
                ErrorClass::Io,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::Timeout {
                message,
                duration_ms,
            } => (
                ErrorClass::Timeout,
                vec![
                    Value::object(self.alloc_string(message)),
                    duration_ms.map_or(Value::NULL, Value::int),
                ],
            ),
            VmBamlError::Unsupported { message } => (
                ErrorClass::Unsupported,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::AccessError { message } => (
                ErrorClass::AccessError,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::RenderPrompt { message } => (
                ErrorClass::RenderPrompt,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::NotImplemented { message } => (
                ErrorClass::NotImplemented,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::LlmClient { message } => (
                ErrorClass::LlmClient,
                vec![Value::object(self.alloc_string(message))],
            ),
            VmBamlError::DevOther { message } => (
                ErrorClass::DevOther,
                vec![Value::object(self.alloc_string(message))],
            ),
            // Field order matches the `HostCallable` class in
            // `ns_errors/errors.baml`: message, class_name, language,
            // traceback?, _handle. `traceback?` surfaces as `Null` when
            // absent; `language` is the empty string when absent (kept
            // non-null for class-field type consistency). `_handle` is
            // an opaque `$rust_type` slot — populated as
            // `Object::RustData(Arc<HostValueArc>)` when a same-host
            // rehydration handle is attached, else `Null`.
            VmBamlError::HostCallable {
                class_name,
                message,
                traceback,
                language,
                handle,
            } => {
                let message_val = Value::object(self.alloc_string(message));
                let class_name_val = Value::object(self.alloc_string(class_name));
                let language_val = Value::object(self.alloc_string(language.unwrap_or_default()));
                let traceback_val =
                    traceback.map_or(Value::NULL, |t| Value::object(self.alloc_string(t)));
                // `handle` is required: `HostCallable` always carries a
                // reference to the originating host exception. Materialize
                // it as `Object::RustData(Arc<HostValueArc>)` so the BAML
                // class's `_handle` slot can be downcast back to the
                // original host-value reference on round-trip.
                let dyn_arc: std::sync::Arc<dyn std::any::Any + Send + Sync> = handle;
                let handle_val = Value::object(self.tlab.alloc(Object::RustData(dyn_arc)));
                (
                    ErrorClass::HostCallable,
                    vec![
                        message_val,
                        class_name_val,
                        language_val,
                        traceback_val,
                        handle_val,
                    ],
                )
            }
        };
        self.alloc_error_value(class, fields)
    }

    pub(crate) fn alloc_error_value(&mut self, class: ErrorClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.error_class_ptrs[class as usize];
        let instance_ptr = self.tlab.alloc(Object::Instance(Instance::new(
            class_ptr,
            Box::new([]),
            fields,
        )));
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
                let file = Value::object(self.alloc_string(loc.file_path.clone()));
                #[allow(clippy::cast_possible_wrap)]
                let line = Value::int(loc.error_line as i64);
                let function_name = Value::object(self.alloc_string(loc.function_name.clone()));
                self.alloc_error_value(ErrorClass::StackFrame, vec![file, line, function_name])
            })
            .collect();

        // Reflection array of `StackFrame` error values; no single declared
        // element type.
        let frames_array = Value::object(
            self.tlab
                .alloc_array(bex_vm_types::RealizedTy::unknown(), frames),
        );
        self.alloc_error_value(ErrorClass::StackTrace, vec![frames_array])
    }

    /// Construct a `baml.errors.ErrorContext` for a thrown value: the error
    /// itself, the `StackTrace` where it was thrown, and the `cause` it
    /// superseded while unwinding (or `Value::NULL` for a fresh error).
    ///
    /// Field order — error, `stack_trace`, cause — matches the class declared in
    /// `ns_errors/error_context.baml` (the constructor ABI). Only called when a
    /// catch handler binds the second `catch (e, ctx)` parameter.
    pub(crate) fn alloc_error_context(
        &mut self,
        error: Value,
        trace: &[StackFrame],
        cause: Value,
    ) -> Value {
        let stack_trace = self.alloc_stack_trace(trace);
        self.alloc_error_value(ErrorClass::ErrorContext, vec![error, stack_trace, cause])
    }

    pub fn panic_to_exception_value(&mut self, panic: VmPanic) -> Value {
        let (class, fields) = match panic {
            VmPanic::DivisionByZero { left, .. } => (PanicClass::DivisionByZero, vec![left]),
            VmPanic::IntegerOverflow { message } => {
                let msg = Value::object(self.alloc_string(message));
                (PanicClass::IntegerOverflow, vec![msg])
            }
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
                let key = Value::object(self.alloc_string("(unknown)".to_string()));
                (PanicClass::MapKeyNotFound, vec![key])
            }
            VmPanic::StackOverflow => {
                let msg = Value::object(self.alloc_string("stack overflow".to_string()));
                (PanicClass::StackOverflow, vec![msg])
            }
            VmPanic::AssertionFailed => {
                let msg = Value::object(self.alloc_string("assertion failed".to_string()));
                (PanicClass::AssertionFailed, vec![msg])
            }
            VmPanic::Unreachable => {
                let msg = Value::object(self.alloc_string("unreachable code executed".to_string()));
                (PanicClass::Unreachable, vec![msg])
            }
            VmPanic::Cancelled => {
                let msg = Value::object(self.alloc_string("operation cancelled".to_string()));
                (PanicClass::Cancelled, vec![msg])
            }
            VmPanic::UserPanic { message } => {
                let msg = Value::object(self.alloc_string(message));
                (PanicClass::UserPanic, vec![msg])
            }
            VmPanic::Exit { code } => (PanicClass::Exit, vec![Value::int(code)]),
            VmPanic::AllocFailure { message } => {
                let msg = Value::object(self.alloc_string(message));
                (PanicClass::AllocFailure, vec![msg])
            }
            VmPanic::HostUnavailable { resource, message } => {
                let resource = Value::object(self.alloc_string(resource));
                let message = Value::object(self.alloc_string(message));
                (PanicClass::HostUnavailable, vec![resource, message])
            }
            VmPanic::NegativeBitShift { message } => {
                let msg = Value::object(self.alloc_string(message));
                (PanicClass::NegativeBitShift, vec![msg])
            }
            // Field order matches the `HostContractViolation` class in
            // `ns_panics/panics.baml`: message, class_name?, language?.
            // The optionals surface as `Null` when absent.
            VmPanic::HostContractViolation {
                message,
                class_name,
                language,
            } => {
                let msg = Value::object(self.alloc_string(message));
                let class_name_val =
                    class_name.map_or(Value::NULL, |c| Value::object(self.alloc_string(c)));
                let language_val =
                    language.map_or(Value::NULL, |l| Value::object(self.alloc_string(l)));
                (
                    PanicClass::HostContractViolation,
                    vec![msg, class_name_val, language_val],
                )
            }
        };
        self.alloc_panic_value(class, fields)
    }

    fn invalid_field_access_error(&mut self, field_index: usize, field_count: usize) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::InvalidFieldAccess {
            field_index,
            field_count,
        }))
    }

    /// Destructure a virtual-dispatch interface operand into the `(name, input args)`
    /// pair the impl resolver selects on. Associated types are *outputs* of an impl,
    /// so they are deliberately not part of the key.
    fn pop_interface_operand(
        &mut self,
        iface_value: Value,
    ) -> Result<(bex_vm_types::TypeHead, Box<[bex_vm_types::RealizedTy]>), VmError> {
        let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
        match self.get_object(iface_ptr) {
            Object::Type(type_value) => match &type_value.ty {
                bex_vm_types::RealizedTy::Interface(head, args, _assoc, _attr) => {
                    Ok((*head, args.clone()))
                }
                other => unreachable!(
                    "virtual field access interface operand must be an Interface type, \
                     found {other:?}"
                ),
            },
            other => unreachable!(
                "as_object_ptr(Type) guarantees a Type object, found {:?}",
                ObjectType::of(other)
            ),
        }
    }

    /// The receiver's physical field slot for interface field `field_index`: read
    /// `Self` off the receiver's runtime concrete type, resolve its single
    /// `implements` rule for the interface (coherence guarantees at most one), then
    /// index the rule's baked `field_links`.
    ///
    /// Both failures are compiler/VM inconsistencies rather than user-reachable
    /// conditions — the type checker proved the receiver implements the interface
    /// before emitting the access, and `field_links` is total over the interface's
    /// declared fields (E0124) — so they surface as internal errors.
    fn resolve_virtual_field_slot(
        &mut self,
        receiver: Value,
        iface_head: bex_vm_types::TypeHead,
        iface_args: &[bex_vm_types::RealizedTy],
        field_index: usize,
    ) -> Result<usize, VmError> {
        let self_ty =
            bex_vm_types::RealizedTy::from(self.value_concrete_ty(receiver).unwrap_or_else(|| {
                unreachable!(
                    "value of kind {:?} cannot be a virtual field-access receiver",
                    self.type_of(&receiver)
                )
            }));
        let slot = crate::package_baml::ImplResolver::for_value(self, receiver)
            .resolve_implements_rule(&self_ty, iface_head, iface_args)
            .and_then(|(rule, _bound_args)| rule.field_links.get(field_index).copied());
        let slot = slot.ok_or_else(|| VmInternalError::UnresolvedVirtualFieldAccess {
            interface: baml_type::HeadDisplay::head_display_name(&iface_head),
            field_index,
        })?;
        Ok(slot as usize)
    }

    /// Encode an i64 arithmetic result into the i63 range, or throw
    /// `IntegerOverflow`.
    ///
    /// Use this for `+`, `-`, `/`, `%`, and unary `-`: two operands already in
    /// the i63 range can never overflow i64 under these ops (the widest case,
    /// `INT_MAX - INT_MIN = 2^63 - 1`, is exactly `i64::MAX`), so the raw i64
    /// result is well-defined and only the i63 range needs checking via
    /// [`Value::try_int`]. The `l`/`op`/`r` context is formatted only on the
    /// cold overflow path, so the hot path is just one range-checked encode.
    #[inline]
    fn finish_int(&mut self, v: i64, l: i64, op: char, r: i64) -> Result<Value, VmError> {
        match Value::try_int(v) {
            Some(val) => Ok(val),
            None => Err(self.integer_overflow(format!("{l} {op} {r} overflows int"))),
        }
    }

    /// Encode a *checked* `int` result, or throw `IntegerOverflow`. Used for
    /// `*`, where two i63 operands genuinely can exceed i64 (e.g.
    /// `INT_MAX * INT_MAX`): `checked` is `None` on i64 overflow, and
    /// [`Value::try_int`] then enforces the tighter i63 range.
    #[inline]
    fn int_arith_result(
        &mut self,
        checked: Option<i64>,
        l: i64,
        op: char,
        r: i64,
    ) -> Result<Value, VmError> {
        match checked.and_then(Value::try_int) {
            Some(v) => Ok(v),
            None => Err(self.integer_overflow(format!("{l} {op} {r} overflows int"))),
        }
    }

    /// Build a catchable `baml.panics.IntegerOverflow` throw. Cold path only.
    #[cold]
    #[inline(never)]
    fn integer_overflow(&mut self, message: String) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::IntegerOverflow { message }))
    }

    /// Cold-path `IntegerOverflow` for the tagged add/sub fast paths: untags the
    /// operands (which the hot path deliberately skips) only to format the
    /// message. `l`/`r` are `Int`-tagged Values.
    #[cold]
    #[inline(never)]
    fn tagged_int_overflow(&mut self, l: Value, op: char, r: Value) -> VmError {
        let lv = l.as_int().unwrap_or(0);
        let rv = r.as_int().unwrap_or(0);
        self.integer_overflow(format!("{lv} {op} {rv} overflows int"))
    }

    /// Build a catchable `baml.panics.NegativeBitShift` throw. Cold path only.
    #[cold]
    #[inline(never)]
    fn negative_bit_shift(&mut self, count: i64) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::NegativeBitShift {
            message: format!("bit shift count is negative: {count}"),
        }))
    }

    /// Evaluate `int << r` with the shared i63 semantics used by constant
    /// folding. A negative count throws `NegativeBitShift`.
    #[inline]
    fn int_shl(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        let Some(l) = Int63::new(l) else {
            verifier_unreachable!()
        };
        match l.shift_left(r) {
            Ok(value) => Ok(Value::int(value.get())),
            Err(IntShiftError::NegativeCount(count)) => Err(self.negative_bit_shift(count)),
        }
    }

    /// Evaluate `int >> r` with the shared i63 semantics used by constant
    /// folding. A negative count throws `NegativeBitShift`.
    #[inline]
    fn int_shr(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        let Some(l) = Int63::new(l) else {
            verifier_unreachable!()
        };
        match l.shift_right(r) {
            Ok(value) => Ok(Value::int(value.get())),
            Err(IntShiftError::NegativeCount(count)) => Err(self.negative_bit_shift(count)),
        }
    }

    /// Allocate a `baml.panics.*` class instance using pre-resolved pointers.
    pub fn alloc_panic_value(&mut self, class: PanicClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.panic_class_ptrs[class as usize];
        let instance_ptr = self.tlab.alloc(Object::Instance(Instance::new(
            class_ptr,
            Box::new([]),
            fields,
        )));
        Value::object(instance_ptr)
    }

    /// Unwinds error values (both thrown and panics).
    /// Name of the innermost frame's function, for attributing work started
    /// from the current execution point (e.g. a spawn's provenance label).
    /// Cheap single-frame variant of `Self::capture_stack_trace`.
    pub fn current_function_name(&self) -> Option<String> {
        let frame = self.frames.last()?;
        let func = self.get_object(frame.function()).as_callable().ok()?;
        Some(func.name.clone())
    }

    fn capture_stack_trace(&self) -> Vec<StackFrame> {
        // The innermost (topmost) bytecode frame's live PC lives in `cur_pc`;
        // outer frames recorded their call-site PC in `faulting_pc` at call time.
        let top_bc = self
            .frames
            .iter()
            .rposition(|f| matches!(f, Frame::Bytecode(_)));
        self.frames
            .iter()
            .enumerate()
            .filter_map(|(idx, frame)| {
                let func = self.get_object(frame.function()).as_callable().ok()?;
                match frame {
                    Frame::Bytecode(frame) => {
                        let pc = if Some(idx) == top_bc {
                            self.cur_pc
                        } else {
                            frame.faulting_pc
                        };
                        let error_line = if let Some(compact) = &func.bytecode.compact {
                            compact.source_line_for_pc(pc)
                        } else {
                            func.bytecode.source_line_for_pc(pc)
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

    /// Walk the call stack outward from the current frame looking for an
    /// exception handler.
    ///
    /// On `Ok(())` a handler was found and the VM is positioned at it.
    fn prof_throw_site_for_frame(&self, depth: usize, pc: usize) -> Option<VmEventSourceLocation> {
        let Frame::Bytecode(frame) = self.frames.get(depth)? else {
            return None;
        };
        let function = self.get_object(frame.function).as_callable().ok()?;
        let entry = if let Some(compact) = &function.bytecode.compact {
            compact.line_entry_for_pc(pc)
        } else {
            function.bytecode.line_entry_for_pc(pc)
        }?;
        Some(Self::event_source_location_for_line_entry(entry))
    }

    fn prof_unwind_origin_for_frame(
        &self,
        depth: usize,
        pc: usize,
        source: VmUnwindSource,
        origin_span_already_terminated: bool,
    ) -> VmUnwindOrigin {
        let Some(Frame::Bytecode(frame)) = self.frames.get(depth) else {
            return VmUnwindOrigin::unresolved(source);
        };
        let throw_site = self
            .prof_throw_site_for_frame(depth, pc)
            .map(|site| VmThrowSite {
                file_id: site.file_id,
                line: site.line,
                start_offset: site.start_offset,
                end_offset: site.end_offset,
            });
        VmUnwindOrigin {
            throw_call_id: frame.call_id,
            throw_function_id: frame.function_id,
            throw_site,
            source,
            selected_error: frame.capture_mask.selected && frame.capture_mask.error,
            manual_eligible: frame.capture_mask.manual,
            origin_span_already_terminated,
        }
    }

    fn prof_start_error_event(
        &mut self,
        thrown: VmThrown,
        throw_call_id: u64,
        throw_function_id: u32,
        throw_site: Option<VmEventSourceLocation>,
        first_selected_call_id: u64,
        manual_eligible: bool,
    ) -> VmProfilerUnwindState {
        let (Some((process_euid, engine_id)), Some(handle), Some(session)) = (
            self.bex_ref_seed,
            self.prof_boundary_handle,
            self.profiler_session.as_ref().map(Arc::clone),
        ) else {
            return VmProfilerUnwindState::Lost;
        };
        let Some(reservation) = bex_events::prof::backend::reserve_session_error_attempt(
            &session,
            handle,
            manual_eligible,
        ) else {
            bex_events::prof::backend::record_session_error_attempt_loss(&session, handle);
            return VmProfilerUnwindState::Lost;
        };
        let Some(unwind_ordinal) = self.prof_unwind_ordinal.checked_add(1) else {
            bex_events::prof::backend::record_session_error_attempt_loss(&session, handle);
            return VmProfilerUnwindState::Lost;
        };
        self.prof_unwind_ordinal = unwind_ordinal;
        let thread_ref = bex_events::ids::ThreadRef {
            process_euid,
            engine_id,
            thread_id: BexThreadId(self.prof_thread_id),
        };
        let id = ErrorCaptureId {
            thread_ref,
            unwind_ordinal,
        };
        let call_ref = |call_id| bex_events::ids::CallRef {
            process_euid,
            engine_id,
            thread_id: thread_ref.thread_id,
            call_id: BexCallId(call_id),
        };
        bex_events::prof::backend::submit_session_error_attempt(
            &session,
            handle,
            ErrorCaptureAttempt {
                id,
                throw_call_ref: call_ref(throw_call_id),
                throw_function_id: ProfFunctionId(throw_function_id),
                first_selected_call_ref: call_ref(first_selected_call_id),
                throw_site: throw_site.map(|site| ThrowSite {
                    file_id: site.file_id,
                    line: site.line,
                    start_offset: site.start_offset,
                    end_offset: site.end_offset,
                }),
                kind: match thrown.profiler_kind {
                    ProfilerErrorKind::Fresh => ErrorUnwindKind::Fresh,
                    ProfilerErrorKind::Rethrow => ErrorUnwindKind::Rethrow,
                },
                source: match thrown.origin.source {
                    VmUnwindSource::Bytecode => ErrorSource::Bytecode,
                    VmUnwindSource::NativeCall => ErrorSource::NativeCall,
                    VmUnwindSource::EngineCall => ErrorSource::EngineCall,
                    VmUnwindSource::FutureResume => ErrorSource::FutureResume,
                },
                manual_eligible,
            },
            reservation,
        );
        match bex_events::prof::backend::reserve_session_error_value(
            &session,
            handle,
            manual_eligible,
        ) {
            Ok(value_reservation) => {
                if self.pending_error_captures.try_reserve(1).is_ok() {
                    self.pending_error_captures.push(VmErrorCaptureEvent {
                        id,
                        value: thrown.value,
                        manual_eligible,
                        reservation: value_reservation,
                    });
                } else {
                    bex_events::prof::backend::complete_session_error_value(
                        &session,
                        handle,
                        id,
                        ValueState::Lost(ValueLossReason::CopyFailed),
                    );
                }
            }
            Err(reason) => bex_events::prof::backend::complete_session_error_value(
                &session,
                handle,
                id,
                ValueState::Lost(reason),
            ),
        }
        VmProfilerUnwindState::Capturing(id)
    }

    fn prof_terminal_error(
        &self,
        state: VmProfilerUnwindState,
        call_id: u64,
        manual_eligible: bool,
    ) {
        let target = match state {
            VmProfilerUnwindState::NotApplicable => return,
            VmProfilerUnwindState::Capturing(id) => TerminalErrorTarget::Capture(id),
            VmProfilerUnwindState::Lost => TerminalErrorTarget::Lost(
                ErrorCaptureLossReason::ErrorCaptureAttemptTransportExceeded,
            ),
        };
        let (Some((process_euid, engine_id)), Some(handle), Some(session)) = (
            self.bex_ref_seed,
            self.prof_boundary_handle,
            self.profiler_session.as_ref(),
        ) else {
            return;
        };
        let Some(reservation) = bex_events::prof::backend::reserve_session_error_attempt(
            session,
            handle,
            manual_eligible,
        ) else {
            bex_events::prof::backend::record_session_terminal_error_loss(session, handle);
            return;
        };
        bex_events::prof::backend::submit_session_terminal_error(
            session,
            handle,
            bex_events::ids::CallRef {
                process_euid,
                engine_id,
                thread_id: BexThreadId(self.prof_thread_id),
                call_id: BexCallId(call_id),
            },
            target,
            reservation,
        );
    }

    fn try_unwind_exception(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        mut thrown: VmThrown,
    ) -> Result<(), VmError> {
        if thrown.origin.throw_call_id == 0
            && let Some(depth) = self
                .frames
                .iter()
                .rposition(|frame| matches!(frame, Frame::Bytecode(_)))
        {
            thrown.origin =
                self.prof_unwind_origin_for_frame(depth, self.cur_pc, thrown.origin.source, false);
        }
        let exception_value = thrown.value;
        let is_rethrow = thrown.language_is_rethrow;

        // BEP-042 cause chain: identify the error currently being handled at
        // the throw site (read-only, before unwinding mutates frames/slots).
        // If this throw happened inside a handler body, that handler's caught
        // error becomes the new error's `cause`.
        //
        // A *rethrow* re-raises an already-thrown value: a bare re-raise inside
        // a handler, the no-match fall-through that re-enters this funnel, a
        // non-throwing `defer` pad's transparent re-raise of the in-flight
        // error, or `ThrowIfPanic` (all pass `is_rethrow`). None is a *new*
        // failure "during handling of" the caught error, so none may graft
        // another link onto the chain by re-running the cause walk here — in
        // particular the no-match fall-through and the defer pad both re-raise
        // from inside a handler body, which the walk would mis-read as a
        // self-link.
        //
        // Instead we reuse the cause the value's *original* (fresh) throw site
        // computed, keyed by the value in `thrown_value_causes`. This preserves
        // a pre-existing chain across a transparent re-raise (the defer-pad
        // case: `mid` throws B while handling A, so B's recorded cause is
        // ErrorContext(A); the pad's re-raise of B recovers A instead of
        // nulling it) while keeping the deliberate null cause for a genuine
        // rethrow that never superseded an error (no recorded entry -> NULL).
        // The recorded value is the superseded error, never the re-raised
        // value's own context, so this can never form a self-link.
        let preserved = if is_rethrow {
            self.recorded_throw_context(exception_value)
        } else {
            self.take_preserved_throw_context(exception_value)
        };
        let (trace, cause_context) = if let Some(context) = preserved {
            (context.trace, context.cause)
        } else {
            let trace = Arc::from(self.capture_stack_trace());
            let cause = if is_rethrow {
                self.recorded_throw_cause(exception_value)
            } else {
                self.find_cause_context()
            };
            (trace, cause)
        };
        if !is_rethrow {
            self.record_throw_cause(exception_value, cause_context);
            self.record_throw_context(exception_value, Arc::clone(&trace), cause_context);
        }

        // Frames popped by this unwind close with a status derived from the
        // thrown value's class (Exited / Cancelled / Errored) — chosen once
        // here; per-frame truthful whether or not a handler catches it.
        let unwind_status = self.prof_unwind_status(exception_value);

        let origin = self
            .frames
            .iter()
            .enumerate()
            .rev()
            .find_map(|(depth, frame)| match frame {
                Frame::Bytecode(frame) => {
                    Some((depth, frame.call_id, frame.function_id, frame.capture_mask))
                }
                Frame::Native(_) => None,
            });
        let derived_site =
            origin.and_then(|(depth, _, _, _)| self.prof_throw_site_for_frame(depth, self.cur_pc));
        let derived_origin = origin.map(|(_, call_id, function_id, mask)| {
            (
                call_id,
                function_id,
                mask.selected && mask.error,
                mask.manual,
            )
        });
        let origin_call_id = if thrown.origin.throw_call_id == 0 {
            derived_origin.map_or(0, |origin| origin.0)
        } else {
            thrown.origin.throw_call_id
        };
        let origin_function_id = if thrown.origin.throw_function_id == 0 {
            derived_origin.map_or(0, |origin| origin.1)
        } else {
            thrown.origin.throw_function_id
        };
        let origin_selected = if thrown.origin.throw_call_id == 0 {
            derived_origin.is_some_and(|origin| origin.2)
        } else {
            thrown.origin.selected_error
        };
        let origin_manual = if thrown.origin.throw_call_id == 0 {
            derived_origin.is_some_and(|origin| origin.3)
        } else {
            thrown.origin.manual_eligible
        };
        let origin_site = thrown
            .origin
            .throw_site
            .map(|site| VmEventSourceLocation {
                file_id: site.file_id,
                line: site.line,
                column: 0,
                start_offset: site.start_offset,
                end_offset: site.end_offset,
            })
            .or(derived_site);
        let mut profiler_state = if origin_selected {
            self.prof_start_error_event(
                thrown,
                origin_call_id,
                origin_function_id,
                origin_site,
                origin_call_id,
                origin_manual,
            )
        } else {
            VmProfilerUnwindState::NotApplicable
        };
        if origin_selected && thrown.origin.origin_span_already_terminated {
            self.prof_terminal_error(profiler_state, origin_call_id, origin_manual);
        }

        // The innermost (first) bytecode frame's faulting PC is the live
        // `cur_pc`; outer frames use the call-site PC they recorded at call time.
        let mut innermost_bc = true;

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
                    // Terminal unwind with a native entry frame: natives
                    // emitted nothing on entry, so nothing to close here.
                    return Err(VmError::thrown_fresh(exception_value));
                }
                self.frames.pop();
                continue; // try next outer frame
            }

            // From here, frame is guaranteed Bytecode.
            let Frame::Bytecode(frame) = frame else {
                unreachable!("non-Native frames already handled above");
            };
            let (frame_faulting_pc, frame_call_id, frame_capture_mask, frame_locals_offset) = (
                frame.faulting_pc,
                frame.call_id,
                frame.capture_mask,
                frame.locals_offset,
            );

            // Innermost bytecode frame uses the live `cur_pc`; outer frames use
            // the call-site PC recorded in `faulting_pc` when they descended.
            let faulting_pc = if innermost_bc {
                self.cur_pc
            } else {
                frame_faulting_pc
            };
            innermost_bc = false;

            // Load the function for this frame to access its exception table.
            // SAFETY: See `load_function` doc comment.
            let frame_function = unsafe { self.load_function(depth)? };

            // Find the INNERMOST exception table entry covering this PC: the
            // NARROWEST range — the largest `start_pc`, then the smallest
            // `end_pc`, and for byte-identical ranges the LATEST table entry
            // (`max_by` keeps the last maximal element). A region contributes
            // one entry per coalesced run of its protected blocks; nested
            // regions' protected PC sets are subsets of their enclosing
            // regions', so around any PC the inner region's range is contained
            // in the outer's (narrowest = innermost), and identical ranges are
            // emitted outer-first (the emitter's stable sort preserves creation
            // order), so the last match is the inner handler. Picking the first
            // covering entry would route to the OUTERMOST handler, mis-routing
            // any throw that reaches the table (e.g. an exception escaping a
            // called function, or a runtime panic). Cold path — does not
            // affect the hot per-instruction loop.
            // Use compact exception table when available (byte-offset PCs),
            // otherwise fall back to the legacy instruction-index table.
            let handler_entry = if let Some(compact) = &frame_function.bytecode.compact {
                compact
                    .exception_handlers_for_pc(faulting_pc)
                    .max_by(|a, b| {
                        a.start_pc
                            .cmp(&b.start_pc)
                            .then_with(|| b.end_pc.cmp(&a.end_pc))
                    })
                    .cloned()
            } else {
                frame_function
                    .bytecode
                    .exception_handlers_for_pc(faulting_pc)
                    .max_by(|a, b| {
                        a.start_pc
                            .cmp(&b.start_pc)
                            .then_with(|| b.end_pc.cmp(&a.end_pc))
                    })
                    .cloned()
            };
            if let Some(entry) = handler_entry {
                // Found a handler in this frame. Truncate the eval stack back
                // to just after the frame's locals region (removes stale
                // temporaries from interrupted expressions).
                let locals_offset = frame_locals_offset;
                let locals_end =
                    locals_offset.raw() + frame_function.arity + frame_function.real_local_count;
                self.stack.truncate(locals_end);

                // Store the exception value in the designated error slot.
                let error_stack_slot =
                    Self::local_slot_stack_index(locals_offset, entry.error_slot);
                self.stack[error_stack_slot] = exception_value;

                // Store stack trace in stack_trace_slot if the catch clause binds it.
                if entry.has_stack_trace_slot() {
                    // BEP-042 Part 3: the second `catch (e, ctx)` binding is an
                    // `ErrorContext` — the thrown value, its trace, and the
                    // error it superseded (`cause_context`, computed at the
                    // funnel top before unwinding).
                    let ctx_value =
                        self.alloc_error_context(exception_value, &trace, cause_context);
                    let ctx_slot =
                        Self::local_slot_stack_index(locals_offset, entry.stack_trace_slot);
                    self.stack[ctx_slot] = ctx_value;
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

            if frame_capture_mask.selected && frame_capture_mask.error {
                if profiler_state == VmProfilerUnwindState::NotApplicable {
                    profiler_state = self.prof_start_error_event(
                        thrown,
                        origin_call_id,
                        origin_function_id,
                        origin_site,
                        frame_call_id,
                        frame_capture_mask.manual,
                    );
                }
                self.prof_terminal_error(profiler_state, frame_call_id, frame_capture_mask.manual);
            }

            // No handler in this frame -- pop it and try the caller.
            if self.frames.len() <= 1 {
                // No more frames to unwind through. The remaining entry
                // frame stays on the stack (stack-trace capture reads it),
                // but its call is over: close its profiling pair so an
                // unhandled throw keeps Call/End balance. (Other fatal exits
                // — true VM-internal errors — can still leave open calls;
                // those are process-level bugs, not program errors.)
                if let Some(Frame::Bytecode(bf)) = self.frames.last() {
                    let (call_id, parent_call_id) = (bf.call_id, bf.parent_call_id);
                    self.prof_exit_call(call_id, parent_call_id, unwind_status);
                }
                return Err(VmError::ThrownUnhandled {
                    value: exception_value,
                    trace: trace.to_vec(),
                });
            }

            let popped = self.frames.pop().expect("frame stack is not empty");
            match popped {
                Frame::Bytecode(bf) => {
                    self.stack.drain(bf.locals_offset..);
                    // Unwound frames close with the unwind status (Errored /
                    // Cancelled / Exited by thrown class); native frames emit
                    // nothing (they emitted nothing on entry — keep entry/exit
                    // symmetric per FunctionKind, plan §6 invariant 3).
                    self.prof_exit_call(bf.call_id, bf.parent_call_id, unwind_status);
                }
                Frame::Native(_) => {} // native frames own no stack region
            }
        }
    }

    /// BEP-042 cause chain: find the error currently being *handled* at the
    /// throw site, by walking the live frames (read-only). A throw whose PC
    /// lies in a handler body — a `HandlerContextEntry` range in the
    /// `handler_context_table` — is "during handling of" that handler's caught
    /// error, which becomes the new error's `cause`. Returns `Value::NULL`
    /// when no enclosing handler is active (a fresh, unchained error).
    ///
    /// Mirrors the PC selection of [`Self::try_unwind_exception`]'s frame walk
    /// (innermost bytecode frame uses the live `cur_pc`; outer frames use the
    /// recorded call-site `faulting_pc`), but never mutates and stops at the
    /// innermost active handler.
    fn find_cause_context(&self) -> Value {
        let mut innermost_bc = true;
        for depth in (0..self.frames.len()).rev() {
            let Frame::Bytecode(bf) = &self.frames[depth] else {
                continue; // native frames hold no handler bodies
            };
            let pc = if innermost_bc {
                self.cur_pc
            } else {
                bf.faulting_pc
            };
            innermost_bc = false;

            // SAFETY: same `load_function` contract as the unwind walk.
            let Ok(func) = (unsafe { self.load_function(depth) }) else {
                return Value::NULL;
            };
            // Innermost (narrowest) handler body wins — largest handler_pc.
            let entry = if let Some(compact) = &func.bytecode.compact {
                compact.handler_context_for_pc(pc)
            } else {
                func.bytecode.handler_context_for_pc(pc)
            };
            if let Some(entry) = entry {
                // The cause is the enclosing handler's `ErrorContext`, which
                // lives in its context slot — itself a link in the chain (with
                // its own `cause`). It exists only if that handler bound `ctx`;
                // a handler that didn't materialize one has no context object
                // to chain, so we stop with null rather than mis-linking to a
                // further-out error.
                if entry.has_stack_trace_slot() {
                    let slot =
                        Self::local_slot_stack_index(bf.locals_offset, entry.stack_trace_slot);
                    return self.stack[slot];
                }
                return Value::NULL;
            }
        }
        Value::NULL
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
            Object::GenericFunction(gf) => {
                // Keep the wrapper ptr as callee identity (so
                // execute_call_from_locals_offset can extract type_args); resolve
                // its authored executable target for arity.
                let func_ptr = self.generic_function_authored_ptr(gf)?;
                let func_obj = unsafe { func_ptr.get() };
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
    /// `reflect.Type.of<T>()` inside generic methods invoked indirectly).
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

    /// The class-level type arguments to curry into a bound method whose
    /// receiver is `receiver`: the receiver instance's `class_type_args` (De
    /// Bruijn class-param order — the method's `Self`), or empty for a
    /// non-instance receiver (a primitive, `type`, `uint8array`, …, which has no
    /// class generics). Captured at `MakeBoundMethod` time so the value is fully
    /// realized; installed as the callee's `frame.type_args` at `CallIndirect`
    /// (see the `Object::BoundMethod` arm of `execute_call_from_locals_offset`).
    ///
    /// This is EXACT, not an approximation: `MakeBoundMethod`'s only callees
    /// are class-inherent methods, whose owner frame IS the class frame. An
    /// implements-block method referenced in value position binds through
    /// `MakeVirtualBoundMethod` (or a shim's `shim_rule_method`) instead,
    /// where the impl rule's `realize_frame` supplies the owner frame a
    /// receiver's class args cannot express (blanket impls, inherited
    /// defaults).
    pub(crate) fn bound_method_curried_type_args(
        &self,
        receiver: Value,
    ) -> Box<[bex_vm_types::RealizedTy]> {
        match receiver.as_object_ptr() {
            Some(ptr) => match self.get_object(ptr) {
                Object::Instance(inst) => inst.class_type_args.clone(),
                _ => Box::new([]),
            },
            None => Box::new([]),
        }
    }

    // ── BEX profiling event stream (bex_events::prof) ──────────────────

    fn prof_resolve_capture_plan(
        &mut self,
        capture_class: FunctionCaptureClass,
        local_id: Option<crate::package_boundary::id::LocalIdCaptureOverrides>,
    ) -> CapturePlan {
        let is_boundary_root = std::mem::take(&mut self.prof_boundary_root_pending);
        let local_id = local_id.map(|overrides| LocalIdOverrides {
            inputs: overrides.inputs,
            output: overrides.output,
            error: overrides.error,
        });
        self.root_profiler
            .resolve_capture_plan(is_boundary_root, capture_class, local_id)
    }

    /// The innermost live call's profiling id (`0` = at thread root). The
    /// engine reads this as the `parent_call_id` of a spawn edge; M1's `$id`
    /// surface reads it too.
    #[must_use]
    pub fn current_call_id(&self) -> u64 {
        self.current_call_id
    }

    pub fn install_boundary_id_for_current_call(
        &mut self,
        boundary_id: bex_events::ids::BoundaryId,
    ) {
        let call_id = self.current_call_id();
        if call_id == 0 {
            return;
        }
        let encoded = boundary_id.to_wire_string();
        if let Some(top) = self.id_overrides.last_mut()
            && top.0 == call_id
        {
            top.1 = encoded;
        } else {
            self.id_overrides.push((call_id, encoded));
        }
        self.prof_push_set_function_id(call_id, boundary_id.as_bytes());
    }

    /// Mints the next per-call id. Unconditional — call ids are `$id`
    /// language semantics (M1 reads them); only ring writes are gated.
    #[inline]
    fn mint_call_id(&mut self) -> u64 {
        self.call_id_counter += 1;
        self.call_id_counter
    }

    /// Encodes one profiling record directly into a reserved slot of the
    /// supplied per-resume ring snapshot. Callers do the profiling-off gate
    /// (they pass the already-unwrapped `&Ring`); the slot is sized from
    /// [`bex_events::prof::record::RawRecord::encoded_len`] and initialized in
    /// place by `encode_to` — no intermediate stack buffer, no zeroing.
    #[inline]
    fn prof_push_record(
        &self,
        ring: &bex_events::prof::Ring,
        rec: &bex_events::prof::record::RawRecord<'_>,
    ) -> bool {
        // Encode straight into the ring slot: no intermediate stack buffer,
        // no 41-byte zeroing, and one copy instead of two (encode→buf→ring).
        let len = rec.encoded_len();
        // SAFETY: the engine refreshed `prof_ring` from this OS thread's TLS
        // at the top of the current exec resume (D5a), and exec never crosses
        // an `.await`, so this thread is still the ring's live claimant. If
        // exec ever yields mid-step, this model must be revisited (plan §6,
        // invariant 4). Callers hold the `Some(ring)` so the off-check is not
        // repeated here. `encode_to` writes exactly `encoded_len` bytes,
        // initializing the whole slot before commit.
        #[expect(unsafe_code, reason = "ring push contract upheld by D5a refresh")]
        let committed = unsafe {
            ring.push_with(len, |slot| {
                rec.encode_to(slot);
            })
        };
        if !committed {
            self.prof_note_transport_loss();
        }
        committed
    }

    fn prof_note_transport_loss(&self) {
        let (Some(session), Some(handle)) =
            (self.profiler_session.as_ref(), self.prof_boundary_handle)
        else {
            return;
        };
        bex_events::prof::backend::record_session_transport_loss(session, handle);
    }

    /// The ring to push a structural record into, or `None` after accounting
    /// for the record that will not be pushed. `prof_ring` is `None` both
    /// when profiling is off (no session/handle: nothing to account) and
    /// when the transport governor denied this thread a ring segment; only the
    /// latter owns a boundary handle, and every record it would have pushed is
    /// one `StructuralTransportExceeded` loss (§8.4), not one per exec resume.
    #[inline]
    fn prof_ring_for_push(&self) -> Option<&'static bex_events::prof::Ring> {
        if self.prof_ring.is_none() {
            self.prof_note_transport_loss();
        }
        self.prof_ring
    }

    /// Resolve the caller-side source span for a bytecode frame/PC pair.
    fn call_site_source_for_frame(
        &self,
        frame_idx: usize,
        pc: usize,
    ) -> Option<CallSiteSourceSpan> {
        let Frame::Bytecode(frame) = self.frames.get(frame_idx)? else {
            return None;
        };
        let func = self.get_object(frame.function).as_callable().ok()?;
        let entry = if let Some(compact) = &func.bytecode.compact {
            compact.line_entry_for_pc(pc)
        } else {
            func.bytecode.line_entry_for_pc(pc)
        }?;
        let file_id = entry.span.file_id.as_u32();
        if file_id == u32::MAX {
            return None;
        }
        Some(CallSiteSourceSpan {
            file_id,
            start_offset: u32::from(entry.span.range.start()),
            end_offset: u32::from(entry.span.range.end()),
            line: u32::try_from(entry.line).unwrap_or(u32::MAX),
        })
    }

    fn event_source_location_for_line_entry(
        entry: &bytecode::LineTableEntry,
    ) -> VmEventSourceLocation {
        VmEventSourceLocation {
            file_id: entry.span.file_id.as_u32(),
            line: u32::try_from(entry.line).unwrap_or(u32::MAX),
            column: 0,
            start_offset: u32::from(entry.span.range.start()),
            end_offset: u32::from(entry.span.range.end()),
        }
    }

    /// Call-entry bookkeeping for a frame about to be pushed: mints the call
    /// id, updates `current_call_id`, and emits `CallFunction`. Returns
    /// `(call_id, parent_call_id)` for the frame literal.
    #[inline]
    fn prof_enter_call(
        &mut self,
        function_id: u32,
        call_site: Option<CallSiteSourceSpan>,
        capture_plan: CapturePlan,
    ) -> (u64, u64, bool) {
        let parent_call_id = self.current_call_id;
        let call_id = self.mint_call_id();
        self.current_call_id = call_id;
        let start_accepted = self.prof_ring_for_push().is_some_and(|ring| {
            self.prof_push_record(
                ring,
                &bex_events::prof::record::RawRecord::CallFunction {
                    flags: capture_plan.to_call_flags(),
                    thread_id: BexThreadId(self.prof_thread_id),
                    call_id: BexCallId(call_id),
                    parent_call_id: BexCallId(parent_call_id),
                    function_id: ProfFunctionId(function_id),
                    call_site,
                    ts_ticks: bex_events::prof::clock::now_ticks(),
                },
            )
        });
        (call_id, parent_call_id, start_accepted)
    }

    /// Call-exit bookkeeping for a popped frame: restores the caller as the
    /// current call and emits `EndFunction`.
    #[inline]
    fn prof_exit_call(
        &mut self,
        call_id: u64,
        parent_call_id: u64,
        status: bex_events::prof::record::FunctionEndStatus,
    ) {
        self.current_call_id = parent_call_id;
        // Drop the exiting call's `$id` override (and any stale deeper
        // entries — `>=` self-heals if a frame ever pops without an exit),
        // restoring the caller's override underneath. Unconditional: `$id`
        // is language semantics, not profiling.
        while self
            .id_overrides
            .last()
            .is_some_and(|(cid, _)| *cid >= call_id)
        {
            self.id_overrides.pop();
        }
        let awaited = self.prof_take_await(call_id);
        if let Some(ring) = self.prof_ring_for_push() {
            let ts_ticks = bex_events::prof::clock::now_ticks();
            let record = match awaited {
                Some((await_ns, await_count)) => {
                    bex_events::prof::record::RawRecord::EndFunctionAwaited {
                        status,
                        thread_id: BexThreadId(self.prof_thread_id),
                        call_id: BexCallId(call_id),
                        ts_ticks,
                        await_ns,
                        await_count,
                    }
                }
                None => bex_events::prof::record::RawRecord::EndFunction {
                    status,
                    thread_id: BexThreadId(self.prof_thread_id),
                    call_id: BexCallId(call_id),
                    ts_ticks,
                },
            };
            self.prof_push_record(ring, &record);
        }
    }

    pub fn prof_enable_await_accumulator(&mut self) {
        if self.prof_await.is_none() {
            self.prof_await = Some(AwaitAccumulator::new());
        }
    }

    #[must_use]
    pub const fn prof_current_call_id(&self) -> u64 {
        self.current_call_id
    }

    /// Fold one completed semantic VM suspension. `elapsed_ns = None` records
    /// an invalid-clock interval without allocating a sparse call entry.
    pub fn prof_record_await(
        &mut self,
        call_id: u64,
        elapsed_ns: Option<u64>,
        memory: &ProfilerMemoryGovernor,
    ) {
        let Some(accumulator) = &mut self.prof_await else {
            return;
        };
        accumulator.health.intervals_started =
            match accumulator.health.intervals_started.checked_add(1) {
                Some(value) => value,
                None => {
                    accumulator.health.counter_saturated = true;
                    u64::MAX
                }
            };
        let Some(elapsed_ns) = elapsed_ns else {
            accumulator.health.clock_invalid = accumulator.health.clock_invalid.saturating_add(1);
            return;
        };
        if call_id == 0 {
            accumulator.health.clock_invalid = accumulator.health.clock_invalid.saturating_add(1);
            return;
        }
        if let Some(entry) = accumulator
            .entries
            .iter_mut()
            .rev()
            .find(|entry| entry.call_id == call_id)
        {
            entry.await_ns = match entry.await_ns.checked_add(elapsed_ns) {
                Some(value) => value,
                None => {
                    accumulator.health.counter_saturated = true;
                    u64::MAX
                }
            };
            entry.await_count = match entry.await_count.checked_add(1) {
                Some(value) => value,
                None => {
                    accumulator.health.counter_saturated = true;
                    u32::MAX
                }
            };
            return;
        }

        if accumulator.entries.len() == accumulator.entries.capacity() {
            let old_capacity = accumulator.entries.capacity();
            let new_capacity = old_capacity.max(1).saturating_mul(2);
            let additional_entries = new_capacity.saturating_sub(old_capacity);
            let additional_bytes = (additional_entries as u64)
                .saturating_mul(std::mem::size_of::<AwaitAccumulatorEntry>() as u64);
            let reservation_result = match &mut accumulator.reservation {
                Some(reservation) => reservation.try_grow(additional_bytes),
                None => memory
                    .try_reserve(
                        ReservationClass::General,
                        Owner::ActiveCalls,
                        additional_bytes,
                    )
                    .map(|reservation| accumulator.reservation = Some(reservation)),
            };
            if reservation_result.is_err() {
                accumulator.health.memory_exceeded =
                    accumulator.health.memory_exceeded.saturating_add(1);
                return;
            }
            accumulator.entries.reserve_exact(additional_entries);
        }
        accumulator.entries.push(AwaitAccumulatorEntry {
            call_id,
            await_ns: elapsed_ns,
            await_count: 1,
        });
    }

    #[must_use]
    pub fn prof_take_await(&mut self, call_id: u64) -> Option<(u64, u32)> {
        let accumulator = self.prof_await.as_mut()?;
        let index = accumulator
            .entries
            .iter()
            .rposition(|entry| entry.call_id == call_id)?;
        let entry = accumulator.entries.remove(index);
        Some((entry.await_ns, entry.await_count))
    }

    #[must_use]
    pub fn prof_await_health(&self) -> AwaitAccumulatorHealth {
        self.prof_await
            .as_ref()
            .map_or(AwaitAccumulatorHealth::default(), |state| state.health)
    }

    /// Call ids of every currently-open call frame, innermost first — the
    /// engine's cancel drain (§7 decision 2) closes these. Native
    /// continuation frames mint no call records and are skipped.
    pub fn prof_open_call_ids(&self) -> impl Iterator<Item = u64> + '_ {
        self.frames.iter().rev().filter_map(|frame| match frame {
            Frame::Bytecode(bf) => Some(bf.call_id),
            Frame::Native(_) => None,
        })
    }

    /// Maps an in-flight exception value to the `FunctionEndStatus` the
    /// frames it unwinds close with: `Exited` for `baml.panics.Exit`,
    /// `Cancelled` for `baml.panics.Cancelled`, `Errored` for everything
    /// else (reconciliation §7 decisions 1–3). The status describes the
    /// frame's fate, not the program's outcome — it is chosen once at unwind
    /// start and stays valid whether or not a handler later catches the
    /// value (the frames are gone either way). Class identity is a pointer
    /// compare against the pre-resolved panic classes, mirroring the
    /// engine's class-tag recognition (`extract_exit_code`).
    fn prof_unwind_status(
        &self,
        exception_value: Value,
    ) -> bex_events::prof::record::FunctionEndStatus {
        use bex_events::prof::record::FunctionEndStatus;
        let Some(ptr) = exception_value.as_object_ptr() else {
            return FunctionEndStatus::Errored;
        };
        let Object::Instance(instance) = self.get_object(ptr) else {
            return FunctionEndStatus::Errored;
        };
        let class = Some(&instance.class);
        if class == self.panic_class_ptrs.get(PanicClass::Exit as usize) {
            FunctionEndStatus::Exited
        } else if class == self.panic_class_ptrs.get(PanicClass::Cancelled as usize) {
            FunctionEndStatus::Cancelled
        } else {
            FunctionEndStatus::Errored
        }
    }

    /// [`Self::prof_unwind_status`] for a native error that has not been
    /// materialized into a heap value yet (the inline native-pair close —
    /// e.g. `baml.sys.exit`'s own pair closes `Exited`).
    fn prof_native_error_status(
        &self,
        err: &VmRustFnError,
    ) -> bex_events::prof::record::FunctionEndStatus {
        use bex_events::prof::record::FunctionEndStatus;
        match err {
            VmRustFnError::Panic(VmPanic::Exit { .. }) => FunctionEndStatus::Exited,
            VmRustFnError::Panic(VmPanic::Cancelled) => FunctionEndStatus::Cancelled,
            VmRustFnError::Thrown { value, .. } => self.prof_unwind_status(*value),
            _ => FunctionEndStatus::Errored,
        }
    }

    /// Sys-op call entry: mints the id and emits `CallFunction`; the engine
    /// emits the matching `EndFunction` when the op completes (it takes
    /// [`BexVm::pending_sysop_call_id`]). `current_call_id` is left alone —
    /// a sys-op makes no nested VM calls.
    #[inline]
    fn prof_enter_sysop(
        &mut self,
        function_id: u32,
        call_site: Option<CallSiteSourceSpan>,
        capture_plan: CapturePlan,
        capture_mask: VmCaptureMask,
    ) -> (u64, VmCaptureMask) {
        let parent_call_id = self.current_call_id;
        let call_id = self.mint_call_id();
        self.pending_sysop_call_id = Some(call_id);
        self.pending_sysop_function_id = Some(function_id);
        let start_accepted = self.prof_ring_for_push().is_some_and(|ring| {
            self.prof_push_record(
                ring,
                &bex_events::prof::record::RawRecord::CallFunction {
                    flags: capture_plan.to_call_flags(),
                    thread_id: BexThreadId(self.prof_thread_id),
                    call_id: BexCallId(call_id),
                    parent_call_id: BexCallId(parent_call_id),
                    function_id: ProfFunctionId(function_id),
                    call_site,
                    ts_ticks: bex_events::prof::clock::now_ticks(),
                },
            )
        });
        let effective_mask = if start_accepted {
            capture_mask
        } else {
            VmCaptureMask::disabled()
        };
        self.pending_sysop_capture_mask = effective_mask;
        (call_id, effective_mask)
    }

    /// `baml.id.set()` support: records the `$id` override in the event
    /// stream (tag 0x05). Gated on the ring like every emission; the
    /// override semantics themselves work with profiling off.
    pub(crate) fn prof_push_set_function_id(&mut self, call_id: u64, id: [u8; 16]) {
        if let Some(ring) = self.prof_ring_for_push() {
            self.prof_push_record(
                ring,
                &bex_events::prof::record::RawRecord::SetFunctionId {
                    thread_id: BexThreadId(self.prof_thread_id),
                    call_id: BexCallId(call_id),
                    id,
                    ts_ticks: bex_events::prof::clock::now_ticks(),
                },
            );
        }
    }

    /// An inline native call pair (`PR4b`). Emitted only after the native
    /// completed inline (`Done`/`Error`) — `YieldToCall` natives are
    /// continuation-based and stay transparent in the event stream (their
    /// callback calls attribute to the bytecode caller); tracking them
    /// through the CPS frames is a follow-up. `start_ticks` is captured before
    /// the native ran, so the pair still spans its real duration.
    #[inline]
    fn prof_emit_native_pair(
        &mut self,
        function_id: u32,
        start_ticks: u64,
        status: bex_events::prof::record::FunctionEndStatus,
        call_site: Option<CallSiteSourceSpan>,
    ) -> u64 {
        // Mint before the ring gate: call ids are `$id` semantics and must
        // not depend on whether profiling is on (plan §6, invariant 5).
        let parent_call_id = self.current_call_id;
        let call_id = self.mint_call_id();
        let Some(ring) = self.prof_ring_for_push() else {
            // Two records (the pair) would have been pushed.
            self.prof_note_transport_loss();
            return call_id;
        };
        // Both records in one push: one bounds check + one Release store
        // for the pair (the ring moves whole records; two at once is fine).
        let mut buf = [0u8; bex_events::prof::record::CALL_FUNCTION_LEN
            + bex_events::prof::record::END_FUNCTION_LEN];
        let call_len = bex_events::prof::record::RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(self.prof_thread_id),
            call_id: BexCallId(call_id),
            parent_call_id: BexCallId(parent_call_id),
            function_id: ProfFunctionId(function_id),
            call_site,
            ts_ticks: start_ticks,
        }
        .encode_to(&mut buf);
        let end_len = bex_events::prof::record::RawRecord::EndFunction {
            status,
            thread_id: BexThreadId(self.prof_thread_id),
            call_id: BexCallId(call_id),
            ts_ticks: bex_events::prof::clock::now_ticks(),
        }
        .encode_to(&mut buf[call_len..]);
        // SAFETY: same D5a contract as prof_push_record.
        #[expect(unsafe_code, reason = "ring push contract upheld by D5a refresh")]
        let committed = unsafe { ring.push(&buf[..call_len + end_len]) };
        if !committed {
            self.prof_note_transport_loss();
        }
        call_id
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
    ///   args\[1\] = `args_pack`  (`Object::Array` of `[positional: Object::Array, optional: Object::Map]`)
    ///   args\[2\] = `ret_ty`     (`Object::Type<TypeValue>`) — `type_arg_0` (`T`)
    ///   args\[3\] = `throws_ty`  (`Object::Type<TypeValue>`) — `type_arg_1` (`E`)
    ///
    /// TODO: `throws_ty` is packed here but the engine doesn't yet read it
    /// — a future phase will validate the host's thrown value against `E`
    /// at the completion site and panic
    /// `baml.panics.HostContractViolation` on mismatch.
    fn host_closure_call_sysop(
        &mut self,
        closure_ptr: HeapPtr,
        user_args: Vec<Value>,
        call_site: Option<CallSiteSourceSpan>,
    ) -> VmExecState {
        // Read arity + return/throws types out of the closure, then drop the
        // borrow before allocating (a TLAB allocation may move/collect heap
        // objects).
        let (arity, ret_ty, throws_ty, params) = match self.get_object(closure_ptr) {
            Object::HostClosure(hc) => (
                hc.arity,
                hc.ret_ty.as_ref().clone(),
                hc.throws_ty.as_ref().clone(),
                hc.params.as_ref().clone(),
            ),
            // Every caller gates on `Object::HostClosure` before calling.
            _ => unreachable!("host_closure_call_sysop requires an Object::HostClosure"),
        };
        debug_assert_eq!(
            user_args.len(),
            arity,
            "HostClosure call: drained {} args but declared arity is {arity}",
            user_args.len(),
        );
        // Split the positional call args by the callable's declared params:
        // required (leading) args stay positional; supplied optionals are
        // collected into a name→value map. An omitted optional (the `OmittedArg`
        // sentinel) is dropped — it can't cross the host boundary, and dropping
        // it lets the host's own language-level default apply. The two halves
        // ride as a `[positional_array, optional_map]` pack so the bridge can
        // apply its calling convention (TS `$opts`, Python kwargs) without the
        // callee type on the wire.
        let mut positional: Vec<Value> = Vec::new();
        let mut optional: IndexMap<bex_vm_types::BexStr, Value> = IndexMap::new();
        for (i, val) in user_args.into_iter().enumerate() {
            match params.get(i) {
                Some(p) if p.is_optional() => {
                    if val.is_omitted() {
                        continue;
                    }
                    let name = p
                        .name
                        .as_ref()
                        .map(|n| n.as_str().to_string())
                        .unwrap_or_else(|| format!("arg{i}"));
                    optional.insert(bex_vm_types::BexStr::from(name), val);
                }
                _ => positional.push(val),
            }
        }
        // Host-call ABI plumbing: positional args, named args, and the
        // `[positional, optional]` wrapper are heterogeneous by construction —
        // a host callable accepts arbitrary argument types.
        let positional_ptr = self
            .tlab
            .alloc_array(bex_vm_types::RealizedTy::unknown(), positional);
        let optional_ptr = self.tlab.alloc_map(
            bex_vm_types::RealizedTy::string(),
            bex_vm_types::RealizedTy::unknown(),
            optional,
        );
        let args_array_ptr = self.tlab.alloc_array(
            bex_vm_types::RealizedTy::unknown(),
            vec![Value::object(positional_ptr), Value::object(optional_ptr)],
        );
        let ret_ty_ptr = self.alloc_type(bex_vm_types::types::TypeValue::new(ret_ty));
        let throws_ty_ptr = self.alloc_type(bex_vm_types::types::TypeValue::new(throws_ty));
        // PR4b: host-closure calls ride the sys-op pair too. No Function
        // object backs them, so function_id 0 (unassigned).
        let capture_plan = self.prof_resolve_capture_plan(FunctionCaptureClass::Ordinary, None);
        self.prof_enter_sysop(
            0,
            call_site,
            capture_plan,
            VmCaptureMask::from_capture_plan(capture_plan),
        );
        VmExecState::SysOp {
            operation: bex_vm_types::SysOp::BamlHostCallHostValue,
            args: vec![
                Value::object(closure_ptr),
                Value::object(args_array_ptr),
                Value::object(ret_ty_ptr),
                Value::object(throws_ty_ptr),
            ],
        }
    }

    /// Drain a sys-op's arguments off the eval stack and produce the
    /// [`VmExecState::SysOp`] yield that hands control to the engine.
    ///
    /// This is the single implementation of "run a `$rust_io_function`",
    /// shared by the dedicated `OpCode::SysOp` handler (a statically-known
    /// direct call) and the general call funnel
    /// ([`Self::execute_call_from_locals_offset`], reached when a sys-op is
    /// invoked as a callable value — virtual/interface dispatch, a bound-method
    /// value, or a callback handed to a native higher-order builtin). Both
    /// present the op's `arity` arguments as the top of the stack, so a sys-op
    /// is dispatched identically however it is reached.
    ///
    /// `callee_fn_ptr` must point at an `Object::Function` whose kind is
    /// [`FunctionKind::SysOp`]. The kind check below is genuinely load-bearing
    /// for the `OpCode::SysOp` caller (its `as_object_ptr` does not verify the
    /// kind — see there); the funnel caller has already matched on the kind, so
    /// for it the check is redundant. Do not delete it.
    fn dispatch_sysop_yield(
        &mut self,
        callee_fn_ptr: HeapPtr,
        runtime_id: Option<Value>,
        frame_idx: usize,
    ) -> Result<VmExecState, VmError> {
        let (sys_op, function_id, arity, capture_class, param_names) = {
            let obj = self.get_object(callee_fn_ptr);
            let Object::Function(f) = obj else {
                return Err(VmInternalError::TypeError {
                    expected: FunctionType::SysOp.into(),
                    got: ObjectType::of(obj).into(),
                }
                .into());
            };
            let FunctionKind::SysOp(sys_op) = f.kind else {
                return Err(VmInternalError::TypeError {
                    expected: FunctionType::SysOp.into(),
                    got: FunctionType::from(&f.kind).into(),
                }
                .into());
            };
            (
                sys_op,
                f.function_id,
                f.arity,
                if matches!(&f.body_meta, Some(FunctionMeta::Llm { .. })) {
                    FunctionCaptureClass::Llm
                } else {
                    FunctionCaptureClass::Ordinary
                },
                f.param_names.clone(),
            )
        };
        let args_offset = self
            .stack
            .len()
            .checked_sub(arity)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(arity))?;
        let args_offset = StackIndex::from_raw(args_offset);
        let call_args: Vec<Value> = self.stack.drain(args_offset..).collect();
        // PR4b: sys-op calls (LLM calls included) appear on the timeline as a
        // CallFunction here; the engine emits the matching EndFunction once the
        // op completes.
        let call_site_source = self.call_site_source_for_frame(frame_idx, self.cur_pc);
        let explicit_local_id = runtime_id
            .map(|value| self.consume_local_id_value(value))
            .transpose()?;
        let capture_plan = self.prof_resolve_capture_plan(
            capture_class,
            explicit_local_id.as_ref().map(|id| id.capture),
        );
        let capture_mask = VmCaptureMask::from_capture_plan(capture_plan);
        let (call_id, capture_mask) =
            self.prof_enter_sysop(function_id, call_site_source, capture_plan, capture_mask);
        if let Some(explicit_local_id) = &explicit_local_id {
            self.install_consumed_local_id_for_sysop(call_id, explicit_local_id);
        }
        let entries: Vec<(String, Value)> = call_args
            .iter()
            .enumerate()
            .map(|(index, value)| {
                let name = param_names
                    .get(index)
                    .cloned()
                    .unwrap_or_else(|| format!("arg{index}"));
                (name, *value)
            })
            .collect();
        self.maybe_capture_named_inputs(call_id, &entries, capture_mask);
        Ok(VmExecState::SysOp {
            operation: sys_op,
            args: call_args,
        })
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "the call decoder passes independent bytecode operands and interpreter cursors"
    )]
    fn execute_call_from_locals_offset(
        &mut self,
        callee_ptr: HeapPtr,
        locals_offset: StackIndex,
        arg_count: usize,
        runtime_id: Option<Value>,
        runtime_type_check: bool,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        // Record the caller's call-site PC before descending. Once a callee
        // frame is pushed, this (now-outer) frame is no longer the innermost, so
        // its live PC must be persisted into `faulting_pc` for correct unwinding
        // and stack traces. `cur_pc` holds this call instruction's start.
        let call_site = self.cur_pc;
        let call_site_source = self.call_site_source_for_frame(*frame_idx, call_site);
        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
            bf.faulting_pc = call_site;
        }

        // Classify the callee with a single heap deref, extracting everything
        // the slow paths need. A plain `Function` — the overwhelmingly common
        // case, including all recursion — needs neither closure captures nor
        // bound-method class args, so it takes the empty fast path. The
        // `closure_type_args` are a Closure's captured type args; the
        // `bound_method_class_type_args` are the bound method's curried type args
        // (De Bruijn ordering: class args → `Self` ++ explicit call-site args,
        // matching enclosing_generic_params() which puts class params first).
        // Curried at `MakeBoundMethod` time (see `bound_method_curried_type_args`)
        // so every callable value seeds its frame from its own type-args field —
        // the same mechanism as `Closure::captured_type_args` /
        // `GenericFunction::type_args`. Both are injected into the new
        // BytecodeFrame after it is created.
        let (is_host, closure_type_args, bound_method_class_type_args): (
            bool,
            Box<[bex_vm_types::RealizedTy]>,
            Box<[bex_vm_types::RealizedTy]>,
        ) = match self.get_object(callee_ptr) {
            Object::HostClosure(_) => (true, Box::new([]), Box::new([])),
            Object::Closure(c) => (false, c.captured_type_args.clone(), Box::new([])),
            Object::BoundMethod(bm) => (false, Box::new([]), bm.type_args.clone()),
            // Plain Function (fast path) and everything else: no extra args.
            _ => (false, Box::new([]), Box::new([])),
        };

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
        if is_host {
            if runtime_id.is_some() {
                return Err(self.invalid_argument_vm_error(
                    "explicit $id is not supported for host-callable values",
                ));
            }
            let user_args: Vec<Value> = self.stack.drain(locals_offset..).collect();
            return Ok(Some(self.host_closure_call_sysop(
                callee_ptr,
                user_args,
                call_site_source,
            )));
        }

        // Specialized function wrappers seed frame.type_args so type-reifying
        // bodies resolve their declaration's generics at runtime.
        let specialized_type_args: Box<[bex_vm_types::RealizedTy]> =
            match self.get_object(callee_ptr) {
                Object::GenericFunction(gf) => gf.type_args.clone(),
                _ => Box::new([]),
            };

        // Resolve the callee: either a plain Function, a Closure, or a BoundMethod
        // wrapping one. `callee_fn_ptr` is the heap pointer of the resolved
        // `Object::Function` itself (unwrapped from any Closure/BoundMethod/
        // specialized wrapper), carried on call notifications so the engine can map
        // it to a `FunctionId` without a name lookup.
        let (callee, callee_fn_ptr) = match self.get_object(callee_ptr) {
            Object::Function(f) => (f, callee_ptr),
            Object::Closure(c) => {
                // SAFETY: closure.function is a compile-time or TLAB-allocated
                // Function object whose lifetime is at least as long as the closure.
                let func_obj: &'static Object = unsafe { c.function.get() };
                match func_obj {
                    Object::Function(f) => (f, c.function),
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
                    Object::Function(f) => (f, bm.function),
                    _ => {
                        return Err(VmInternalError::TypeError {
                            expected: FunctionType::Callable.into(),
                            got: ObjectType::of(func_obj).into(),
                        }
                        .into());
                    }
                }
            }
            Object::GenericFunction(gf) => {
                let func_ptr = self.generic_function_authored_ptr(gf)?;
                // SAFETY: the function global slot holds a compile-time Function
                // object whose lifetime spans the whole program.
                let func_obj: &'static Object = unsafe { func_ptr.get() };
                match func_obj {
                    Object::Function(f) => (f, func_ptr),
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
        let callee_arity = callee.arity;
        let callee_kind = callee.kind;
        let callee_name = callee.name.clone();
        let callee_function_id = callee.function_id;
        let callee_capture_class = if matches!(&callee.body_meta, Some(FunctionMeta::Llm { .. })) {
            FunctionCaptureClass::Llm
        } else {
            FunctionCaptureClass::Ordinary
        };
        let callee_param_names = callee.param_names.clone();
        if arg_count != callee_arity {
            return Err(VmInternalError::InvalidArgumentCount {
                expected: callee_arity,
                got: arg_count,
            }
            .into());
        }
        if runtime_type_check {
            // `unreflect(...)` deliberately hides its concrete type from TIR,
            // so marker calls must validate executable generic metadata at the
            // first boundary that sees the realized type. Keep the metadata
            // clones and type-vector assembly entirely inside this rare branch;
            // ordinary calls pay one predicted-not-taken bit test only.
            let callee_param_types = callee.param_types.clone();
            let callee_generic_param_bounds = callee.generic_param_bounds.clone();
            let needs_bound_check = callee_generic_param_bounds
                .iter()
                .any(|bounds| !bounds.is_empty());
            let needs_argument_check = callee_param_types
                .iter()
                .any(|param| !param.is_fully_concrete());
            if needs_bound_check || needs_argument_check {
                let mut effective_type_args = if !bound_method_class_type_args.is_empty() {
                    bound_method_class_type_args.to_vec()
                } else if !specialized_type_args.is_empty() {
                    specialized_type_args.to_vec()
                } else {
                    closure_type_args.to_vec()
                };
                effective_type_args.extend_from_slice(&self.pending_call_type_args);
                if needs_bound_check {
                    self.validate_runtime_generic_bounds(
                        &callee_name,
                        &callee_generic_param_bounds,
                        &effective_type_args,
                    )?;
                }
                if needs_argument_check {
                    self.validate_runtime_call_arguments(
                        &callee_param_types,
                        &effective_type_args,
                        locals_offset,
                    )?;
                }
            }
        }
        // Check if we've reached the max call stack size.
        if self.frames.len() >= MAX_FRAMES {
            return Err(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            ));
        }

        match callee_kind {
            FunctionKind::Native(func_ptr) => {
                if runtime_id.is_some() {
                    return Err(self.invalid_argument_vm_error(
                        "explicit $id is not supported for native builtins",
                    ));
                }
                // Native builtins reject explicit LocalId and are never the
                // outer user-visible LLM function. They remain CCT-only.
                let capture_mask = VmCaptureMask::disabled();
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

                // For a generic-instantiation-valued native callee, seed the
                // native's type args so it reads them via
                // `current_call_type_args()` — the direct-call path sets these
                // from LoadType operands, but an indirect call through the value
                // carries them on the wrapper. A pooled `GenericFunction`
                // (`let f = baml.json.from_string<User>`) carries them on
                // `specialized_type_args`; a closure-wrapped value
                // (`let g = baml.json.from_string; let f = g<User>`) carries them
                // on the closure's `captured_type_args`. Use whichever is set.
                let native_type_args: &[bex_vm_types::RealizedTy] =
                    if !specialized_type_args.is_empty() {
                        &specialized_type_args
                    } else {
                        &closure_type_args
                    };
                let restore_pending = if !native_type_args.is_empty() {
                    Some(std::mem::replace(
                        &mut self.pending_call_type_args,
                        native_type_args.to_vec(),
                    ))
                } else {
                    None
                };
                // PR4b: inline-native call pair. Capture the start stamp
                // before running; the pair is emitted only if the native
                // completes inline (Done/Error) — YieldToCall natives are
                // continuation-based and stay transparent (see
                // prof_emit_native_pair).
                let native_ticks_start = if self.prof_ring.is_some() {
                    bex_events::prof::clock::now_ticks()
                } else {
                    0
                };
                let native_result = func(self, &args);
                if let Some(prev) = restore_pending {
                    self.pending_call_type_args = prev;
                }

                // Run Rust native function, converting NativeCallResult → VmError.
                match native_result {
                    NativeCallResult::Done(v) => {
                        let call_id = self.prof_emit_native_pair(
                            callee_function_id,
                            native_ticks_start,
                            bex_events::prof::record::FunctionEndStatus::Ok,
                            call_site_source,
                        );
                        self.maybe_queue_call_output(
                            call_id,
                            self.current_call_id,
                            capture_mask,
                            v,
                        );
                        self.stack.push(v);
                    }
                    NativeCallResult::Error(e) => {
                        // Status by error class: baml.sys.exit's own pair
                        // closes Exited, a cancel panic Cancelled (§7 1–3).
                        let status = self.prof_native_error_status(&e);
                        let call_id = self.prof_emit_native_pair(
                            callee_function_id,
                            native_ticks_start,
                            status,
                            call_site_source,
                        );
                        let vm_error = self.native_error_to_vm_error(e);
                        let mut thrown = match vm_error {
                            VmError::Thrown(thrown) => thrown,
                            other => return Err(other),
                        };
                        thrown.origin = VmUnwindOrigin {
                            throw_call_id: call_id,
                            throw_function_id: callee_function_id,
                            throw_site: None,
                            source: VmUnwindSource::NativeCall,
                            selected_error: capture_mask.selected && capture_mask.error,
                            manual_eligible: capture_mask.manual,
                            origin_span_already_terminated: true,
                        };
                        return Err(VmError::Thrown(thrown));
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

                        let result = self.execute_call_from_locals_offset_with_type_args(
                            real_callee,
                            cb_locals,
                            arg_count,
                            CallOptions {
                                runtime_id: None,
                                type_args: &callback_type_args,
                                type_values: &[],
                                runtime_type_check: false,
                            },
                            frame_idx,
                            function,
                        );

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
                } else if !specialized_type_args.is_empty() {
                    // GenericFunction value (`foo<int>`): seed its concrete args.
                    specialized_type_args
                } else {
                    closure_type_args
                };
                let explicit_local_id = runtime_id
                    .map(|value| self.consume_local_id_value(value))
                    .transpose()?;
                let capture_plan = self.prof_resolve_capture_plan(
                    callee_capture_class,
                    explicit_local_id.as_ref().map(|id| id.capture),
                );
                let mut capture_mask = VmCaptureMask::from_capture_plan(capture_plan);
                let (call_id, parent_call_id, start_accepted) =
                    self.prof_enter_call(callee_function_id, call_site_source, capture_plan);
                if !start_accepted {
                    capture_mask = VmCaptureMask::disabled();
                }
                if let Some(explicit_local_id) = &explicit_local_id {
                    self.install_consumed_local_id_for_call(call_id, explicit_local_id);
                }
                self.maybe_capture_call_inputs(
                    &callee_param_names,
                    call_id,
                    locals_offset,
                    arg_count,
                    capture_mask,
                );
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function: callee_ptr,
                    instruction_ptr: 0,
                    locals_offset,
                    type_args: initial_type_args.into_vec(),
                    type_metadata: None,
                    faulting_pc: 0,
                    call_id,
                    parent_call_id,
                    capture_mask,
                    function_id: callee_function_id,
                }));
                self.allocate_real_locals_for_frame(callee_ptr)?;

                // Update frame_idx to point to the new frame.
                *frame_idx = self.frames.len() - 1;

                // No per-call engine yield here: per-call lifecycle flows
                // through the profiling ring (prof_enter_call above), which
                // costs one memcpy + one Release store instead of breaking
                // out of the exec loop on every call.
                // SAFETY: See `load_function` doc comment.
                *function = unsafe { self.load_function(*frame_idx)? };
            }

            FunctionKind::SysOp(_) => {
                // A sys-op reached as a callable value — virtual/interface
                // dispatch, a bound-method value, or a callback handed to a
                // native higher-order builtin — runs through the same engine
                // yield as a direct `OpCode::SysOp`: drain its args and suspend.
                // The resolved sys-op `Function`'s arity already counts the
                // receiver, so the top-of-stack args are exactly what the op's
                // glue expects. On resume the engine pushes the result and the
                // caller's post-call store binds it, identically to a returning
                // bytecode callee.
                //
                // `dispatch_sysop_yield` drains `stack.len() - arity`; every
                // funnel caller positions the args as the exact top of stack, so
                // that window is `locals_offset`. Assert it rather than thread
                // `locals_offset` through the shared helper.
                debug_assert_eq!(
                    self.stack.len().checked_sub(callee_arity),
                    Some(locals_offset.raw()),
                    "sysop dispatch: args must be the top {callee_arity} stack slots \
                     (len {}, locals_offset {})",
                    self.stack.len(),
                    locals_offset.raw(),
                );
                // Sys-ops do not thread type arguments: a method-level-generic
                // sys-op is rejected at compile time (E0153), and class/interface
                // generics are type-erased for the op's glue. Any type args here
                // would be silently dropped, so fail closed in debug.
                debug_assert!(
                    closure_type_args.is_empty()
                        && bound_method_class_type_args.is_empty()
                        && specialized_type_args.is_empty(),
                    "sysop dispatch received type args, which it cannot thread to the op",
                );
                return Ok(Some(self.dispatch_sysop_yield(
                    callee_fn_ptr,
                    runtime_id,
                    *frame_idx,
                )?));
            }

            FunctionKind::NativeUnresolved => {
                // This should never happen - native functions should be resolved
                // by attach_builtins() before the VM runs.
                panic!(
                    "Unresolved native function '{callee_name}' - did you forget to call attach_builtins()?"
                );
            }
        }

        if self.early_yield.should_early_yield() {
            return Ok(Some(VmExecState::EarlyYield));
        }
        Ok(None)
    }

    /// Validate a callee's executable generic-bound metadata against the
    /// realized type arguments that will seed its frame.
    fn validate_runtime_generic_bounds(
        &mut self,
        callee_name: &str,
        generic_param_bounds: &[Vec<bex_vm_types::types::InterfaceBound>],
        type_args: &[bex_vm_types::RealizedTy],
    ) -> Result<(), VmError> {
        for (index, bounds) in generic_param_bounds.iter().enumerate() {
            let Some(actual) = type_args.get(index) else {
                if bounds.is_empty() {
                    continue;
                }
                return Err(VmInternalError::TypeSubstitution {
                    message: format!(
                        "generic call to `{callee_name}` omitted runtime type argument #{index}"
                    ),
                }
                .into());
            };
            for bound in bounds {
                let requested_args = bound
                    .args
                    .iter()
                    .map(|arg| {
                        arg.substitute(type_args, self).map_err(|error| {
                            VmInternalError::TypeSubstitution {
                                message: error.to_string(),
                            }
                        })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                let requested_assoc = bound
                    .assoc
                    .iter()
                    .map(|(name, ty)| {
                        ty.substitute(type_args, self)
                            .map(|ty| (name.clone(), ty))
                            .map_err(|error| VmInternalError::TypeSubstitution {
                                message: error.to_string(),
                            })
                    })
                    .collect::<Result<Vec<_>, _>>()?;
                if crate::package_baml::ImplResolver::new(self).type_implements(
                    actual,
                    bound.interface,
                    &requested_args,
                    &requested_assoc,
                ) {
                    continue;
                }

                let diagnostic = runtime_type::mismatched_types();
                let error = crate::package_reflect::type_kinds::alloc_compilation_error(
                    self,
                    &[diagnostic],
                );
                return Err(VmError::thrown_fresh(error));
            }
        }
        Ok(())
    }

    /// Revalidate value arguments whose declared type depends on a runtime
    /// type argument.  Static explicit type arguments already passed this
    /// check in TIR; marker arguments reach it with their concrete type for
    /// the first time here (M-5), before the callee body can run.
    fn validate_runtime_call_arguments(
        &mut self,
        param_types: &[bex_vm_types::TyTemplate],
        type_args: &[bex_vm_types::RealizedTy],
        locals_offset: StackIndex,
    ) -> Result<(), VmError> {
        for (index, param) in param_types.iter().enumerate() {
            if param.is_fully_concrete() {
                continue;
            }
            let value = self.stack[StackIndex::from_raw(locals_offset.raw() + index)];
            if value.is_omitted() {
                continue;
            }
            let matches = crate::type_match::value_matches_template(self, value, param, type_args)?;
            if matches {
                continue;
            }
            param.substitute(type_args, self).map_err(|error| {
                VmInternalError::TypeSubstitution {
                    message: error.to_string(),
                }
            })?;
            let diagnostic = runtime_type::mismatched_types();
            let error =
                crate::package_reflect::type_kinds::alloc_compilation_error(self, &[diagnostic]);
            return Err(VmError::thrown_fresh(error));
        }
        Ok(())
    }

    /// Convert a [`VmRustFnError`] into the corresponding [`VmError`].
    fn native_error_to_vm_error(&mut self, err: VmRustFnError) -> VmError {
        match err {
            VmRustFnError::Panic(panic) => {
                VmError::thrown_fresh(self.panic_to_exception_value(panic))
            }
            VmRustFnError::BamlError(err) => {
                VmError::thrown_fresh(self.error_to_exception_value(err))
            }
            VmRustFnError::InternalError(err) => VmError::InternalError(err),
            VmRustFnError::Thrown {
                value,
                profiler_kind,
            } => VmError::Thrown(VmThrown {
                value,
                profiler_kind,
                language_is_rethrow: false,
                origin: VmUnwindOrigin::unresolved(VmUnwindSource::NativeCall),
            }),
        }
    }

    fn execute_call_from_locals_offset_with_type_args(
        &mut self,
        callee_ptr: HeapPtr,
        locals_offset: StackIndex,
        arg_count: usize,
        options: CallOptions<'_>,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        let previous_type_args =
            std::mem::replace(&mut self.pending_call_type_args, options.type_args.to_vec());
        let previous_type_values = std::mem::replace(
            &mut self.pending_call_type_values,
            options.type_values.to_vec(),
        );
        let frames_before = self.frames.len();
        let result = self.execute_call_from_locals_offset(
            callee_ptr,
            locals_offset,
            arg_count,
            options.runtime_id,
            options.runtime_type_check,
            frame_idx,
            function,
        );
        self.pending_call_type_args = previous_type_args;
        self.pending_call_type_values = previous_type_values;
        // FOLLOW-UP (not a defect today): the rooted copy of the values is
        // dropped one line above, and the writes below read `options`, which
        // borrows a caller *local* that no GC root covers. A collection between
        // the two would forward the rooted copy and leave these pointers stale.
        // It is unreachable as written — `execute_call_from_locals_offset` only
        // pushes a frame and sizes the eval stack, with no TLAB allocation, and
        // the native path that can allocate pushes no bytecode frame, so the
        // guard below declines. Recorded because this lane now carries recovered
        // identities as well as method-level ones, so the day something on that
        // path starts allocating, this is where it bites.
        //
        // A definition overlay can arrive without any type-argument slots of its
        // own — interface dispatch hands one down for a method that declares no
        // generics — so the metadata lane is written whenever any of the three
        // has something to say, not only when the frame widens.
        if (!options.type_args.is_empty() || !options.type_values.is_empty())
            && self.frames.len() == frames_before + 1
            && *frame_idx == frames_before
            && let Some(Frame::Bytecode(frame)) = self.frames.get_mut(frames_before)
        {
            let initial_type_arg_count = frame.type_args.len();
            frame.type_args.extend_from_slice(options.type_args);
            if !options.type_values.is_empty() {
                let metadata = frame
                    .type_metadata
                    .get_or_insert_with(|| Box::new(FrameTypeMetadata::default()));
                metadata.values.resize(initial_type_arg_count, None);
                metadata.values.extend(
                    (0..options.type_args.len())
                        .map(|slot| options.type_values.get(slot).cloned().flatten()),
                );
            }
        }
        result
    }

    fn init_spread(
        &mut self,
        dest_value: Value,
        source_value: Value,
        field_copy_set: &bytecode::FieldCopySet,
    ) -> Result<(), VmError> {
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
                if dest.try_load_field(copy.dest).is_none() {
                    invalid_field_access = Some((copy.dest, dest.field_len()));
                    break;
                }
                let Some(new_value) = source.try_load_field(copy.source) else {
                    invalid_field_access = Some((copy.source, source.field_len()));
                    break;
                };
                copied_fields.push((copy.dest, new_value));
            }
            if let Some((index, field_count)) = invalid_field_access {
                return Err(self.invalid_field_access_error(index, field_count));
            }

            copied_fields
        };

        for (dest_field, new_value) in copied_fields {
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

        Ok(())
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

        let class_type_args: Box<[bex_vm_types::RealizedTy]> = if ntypeargs == 0 {
            Box::default()
        } else {
            self.take_type_args(base + field_value_count, ntypeargs)?
                .tys
                .into()
        };

        let fields = if field_value_count == class_field_count
            && plan
                .fields
                .iter()
                .copied()
                .enumerate()
                .all(|(idx, field_idx)| idx == field_idx)
        {
            self.stack
                .drain(StackIndex::from_raw(base)..StackIndex::from_raw(base + field_value_count))
                .collect()
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
            Object::GenericFunction(gf) => {
                let func_ptr = self.generic_function_authored_ptr(gf)?;
                // SAFETY: function globals hold compile-time Function objects.
                let func_obj: &'static Object = unsafe { func_ptr.get() };
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

        // BAML_KPERF: read PMCs around this exec() on the current worker thread.
        let kp = crate::kperf::enabled();
        let (kp_start, ops_start) = if kp {
            (crate::kperf::exec_start(), self.op_count)
        } else {
            (None, 0)
        };

        let result = match self.exec_inner() {
            Err(VmError::InternalError(err)) => {
                let trace = self.capture_stack_trace();
                Err(VmError::TracedInternalError { source: err, trace })
            }
            other => other,
        };

        if kp {
            crate::kperf::exec_end(kp_start, self.op_count - ops_start);
        }
        result
    }

    /// Inject an exception value from outside the VM's execution loop (e.g. from
    /// the engine's `SysOp` result handler) and let the VM's normal exception
    /// unwinder walk frames and match handlers — the same path a `throw`
    /// opcode or an internal bytecode throw site takes.
    ///
    /// On `Ok(())` a handler was found: `self.frames` is now at the catching
    /// frame, the exception value is stored in the handler's binding slot, the
    /// instruction pointer is at the handler's PC, and the next [`Self::exec`]
    /// resumes the catch body.
    /// On `Err(VmError::ThrownUnhandled { .. })` (or, for a degenerate
    /// Native-only frame stack, `Err(VmError::thrown_fresh(..))`) no handler
    /// matched; the caller should route the result through whatever path it
    /// uses for any other VM unhandled throw.
    ///
    /// The unwinder reloads the `function` reference for each frame it visits,
    /// so the `function` out-param it receives is only an initial seed — we
    /// pick the topmost Bytecode frame's `Function` for that role, walking
    /// past any Native frames at the top (which the unwinder would pop
    /// unconditionally).
    pub fn try_handle_external_exception(&mut self, exception_value: Value) -> Result<(), VmError> {
        self.try_handle_external_thrown(VmThrown::fresh(
            exception_value,
            VmUnwindSource::EngineCall,
        ))
    }

    pub fn try_handle_external_thrown(&mut self, thrown: VmThrown) -> Result<(), VmError> {
        let exception_value = thrown.value;
        if self.frames.is_empty() {
            let trace = self.capture_stack_trace();
            return Err(VmError::ThrownUnhandled {
                value: exception_value,
                trace,
            });
        }
        // Walk down from the top to find a Bytecode frame to seed `function`
        // from. Native frames carry a continuation, not a `Function`, so
        // `load_function` would fail on them — and the unwinder would pop
        // them anyway. If every frame is Native, no bytecode catch handler
        // can match; surface as unhandled.
        let mut seed_idx = self.frames.len() - 1;
        while !matches!(&self.frames[seed_idx], Frame::Bytecode(_)) {
            if seed_idx == 0 {
                let trace = self.capture_stack_trace();
                return Err(VmError::ThrownUnhandled {
                    value: exception_value,
                    trace,
                });
            }
            seed_idx -= 1;
        }
        let mut frame_idx = self.frames.len() - 1;
        // SAFETY: see `load_function` doc — the frame at `seed_idx` is a
        // Bytecode frame whose `function` pointer is valid for `&'static
        // Function` while we hold `&mut self`.
        let mut function = unsafe { self.load_function(seed_idx)? };
        self.try_unwind_exception(&mut frame_idx, &mut function, thrown)
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
                    // A `type` value has no identity beyond the type it
                    // denotes: two are equal exactly when they are mutual
                    // subtypes (TYPE_SYSTEM.md, "Equivalence and canonical
                    // forms"), decided against this VM's program facts.
                    CmpOp::Eq => self.equivalent(lt.ty.as_ty(), rt.ty.as_ty()),
                    CmpOp::NotEq => !self.equivalent(lt.ty.as_ty(), rt.ty.as_ty()),
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
            match op {
                // Both `/` and `%` by zero throw DivisionByZero (the `%` guard
                // matches the specialized `ModInt` opcode — without it, `l % 0`
                // on this generic path would raw-Rust-panic).
                BinOp::Div | BinOp::Mod if r == 0 => {
                    return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                        VmPanic::DivisionByZero {
                            left: Value::int(l),
                            right: Value::int(r),
                        },
                    )));
                }
                // Arithmetic is checked: overflow throws IntegerOverflow rather
                // than wrapping or raw-Rust-panicking. Only `*` can overflow
                // i64 from i63 operands (so it needs checked_mul); +, -, /, %
                // can't, so a plain op + i63 range-check suffices. And/Or/Xor of
                // two i63 values stay in range, and `<<` truncates back into it
                // (see `int_shl`); both shifts still reject a negative count.
                BinOp::Add => self.finish_int(l.wrapping_add(r), l, '+', r)?,
                BinOp::Sub => self.finish_int(l.wrapping_sub(r), l, '-', r)?,
                BinOp::Mul => self.int_arith_result(l.checked_mul(r), l, '*', r)?,
                BinOp::Div => self.finish_int(l / r, l, '/', r)?,
                BinOp::Mod => self.finish_int(l % r, l, '%', r)?,
                BinOp::BitAnd => Value::int(l & r),
                BinOp::BitOr => Value::int(l | r),
                BinOp::BitXor => Value::int(l ^ r),
                BinOp::Shl => self.int_shl(l, r)?,
                BinOp::Shr => self.int_shr(l, r)?,
            }
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
                        Value::object(self.alloc_float(l))
                    };
                    let right_v = if right.is_object() {
                        right
                    } else {
                        Value::object(self.alloc_float(r))
                    };
                    return Err(VmError::thrown_fresh(self.panic_to_exception_value(
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
            Value::object(self.alloc_float(f))
        } else if left.is_object() && right.is_object() && op == BinOp::Add {
            let ls = self.as_string(&left)?;
            let rs = self.as_string(&right)?;
            let result = bex_str::BexStr::concat(ls.clone(), rs.clone());
            Value::object(self.alloc_string(result))
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
                        VmError::Thrown(thrown) => {
                            self.try_unwind_exception(&mut frame_idx, &mut function, thrown)?;
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

                        let ecflo_outcome = self.execute_call_from_locals_offset_with_type_args(
                            real_callee,
                            cb_locals,
                            arg_count,
                            CallOptions {
                                runtime_id: None,
                                type_args: &callback_type_args,
                                type_values: &[],
                                runtime_type_check: false,
                            },
                            &mut frame_idx,
                            &mut function,
                        );

                        let ecflo_result = match ecflo_outcome {
                            Ok(result) => result,
                            Err(VmError::Thrown(thrown)) => {
                                self.try_unwind_exception(&mut frame_idx, &mut function, thrown)?;
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
                    Err(VmError::Thrown(thrown)) => {
                        self.try_unwind_exception(&mut frame_idx, &mut function, thrown)?;
                        break;
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
        // VM-op counter for the `kperf` profiler. Compiled out entirely in
        // normal builds (it is pure measurement scaffolding and adds a store +
        // memory dependency on the hottest path); kperf reads cycles and
        // instructions retired straight from the hardware counters, so the op
        // count is only needed for the informational per-op breakdown.
        #[cfg(feature = "kperf")]
        {
            self.op_count += 1;
        }

        // Record the innermost frame's current instruction start cheaply (one
        // flat field store), instead of writing the frame's `faulting_pc` every
        // op (which needs a bounds-checked index + enum match + store). Outer
        // frames record their call-site PC at call time; read sites resolve the
        // innermost frame from `cur_pc`.
        self.cur_pc = *pc - 1;

        // SAFETY: OpCode is #[repr(u8)] and the compact bytecode is produced by our
        // own encoder which only emits valid opcode bytes.
        #[allow(unsafe_code)]
        let op: OpCode = unsafe { std::mem::transmute(op_byte) };

        // Tagged-int comparisons skip untagging by comparing bits directly
        // (see `Value::tagged_int_add_checked` for the encoding rationale; the
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
                    // SAFETY: dispatch loop always runs with a Bytecode frame on top.
                    #[allow(unsafe_code)]
                    let Frame::Bytecode(bf) = (unsafe { self.frames.get_unchecked(*frame_idx) })
                    else {
                        unreachable!()
                    };
                    let stack_slot = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack.get_at(stack_slot);
                    self.stack.push(value);
                }

                OpCode::StoreVar => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    // SAFETY: dispatch loop always runs with a Bytecode frame on top.
                    #[allow(unsafe_code)]
                    let Frame::Bytecode(bf) = (unsafe { self.frames.get_unchecked(*frame_idx) })
                    else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value = self.stack.ensure_pop();
                    self.store_local_value(local_var_index, value);
                }

                // ── Operand-movement superinstructions (CPython-style) ────────
                // LoadVar2(a, b) == `LoadVar(a); LoadVar(b)`: push both locals.
                OpCode::LoadVar2 => {
                    let a = { read_u32_unchecked(code, pc) as usize };
                    let b = { read_u32_unchecked(code, pc) as usize };
                    #[allow(unsafe_code)]
                    let Frame::Bytecode(bf) = (unsafe { self.frames.get_unchecked(*frame_idx) })
                    else {
                        unreachable!()
                    };
                    let off = bf.locals_offset;
                    let va = self.stack.get_at(Self::local_slot_stack_index(off, a));
                    let vb = self.stack.get_at(Self::local_slot_stack_index(off, b));
                    self.stack.push(va);
                    self.stack.push(vb);
                }
                // StoreVar2(a, b) == `StoreVar(a); StoreVar(b)`: pop TOS into
                // local[a], then pop into local[b].
                OpCode::StoreVar2 => {
                    let a = { read_u32_unchecked(code, pc) as usize };
                    let b = { read_u32_unchecked(code, pc) as usize };
                    #[allow(unsafe_code)]
                    let Frame::Bytecode(bf) = (unsafe { self.frames.get_unchecked(*frame_idx) })
                    else {
                        unreachable!()
                    };
                    let off = bf.locals_offset;
                    let sa = Self::local_slot_stack_index(off, a);
                    let sb = Self::local_slot_stack_index(off, b);
                    let va = self.stack.ensure_pop();
                    let vb = self.stack.ensure_pop();
                    self.store_local_value(sa, va);
                    self.store_local_value(sb, vb);
                }

                OpCode::StoreVarLoadVar => {
                    let slot = { read_u32_unchecked(code, pc) as usize };
                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let local_var_index = Self::local_slot_stack_index(bf.locals_offset, slot);
                    let value_slot = self.stack.ensure_slot_from_top(0);
                    let value = self.stack[value_slot];
                    self.store_local_value(local_var_index, value);
                }

                // ── LoadGlobal / StoreGlobal ──────────────────────────────────
                OpCode::LoadGlobal => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.load_global_in(function.runtime_package, global_idx);
                    self.stack.push(value);
                }

                OpCode::StoreGlobal => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let value = self.stack.ensure_pop();
                    // Only valid during `$init`; post-init globals are frozen in `Arc<[Value]>`
                    // and a write here is a VM internal error.
                    self.store_global_in(function.runtime_package, global_idx, value)?;
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
                }

                // ── VirtualLoadField / VirtualStoreField ──────────────────────
                // The field analogue of `VirtualCall`: the operand indexes the
                // *interface's* declared fields, and the receiver's resolved impl
                // maps that to a physical slot. Open-world by construction — nothing
                // here enumerates implementors, so a class from a later-loaded
                // package resolves exactly like a local one.
                OpCode::VirtualLoadField => {
                    let field_index = { read_u32_unchecked(code, pc) as usize };
                    let iface_value = self.stack.ensure_pop();
                    let (iface_qtn, iface_args) = self.pop_interface_operand(iface_value)?;
                    let receiver = self.stack.ensure_pop();
                    let slot = self.resolve_virtual_field_slot(
                        receiver,
                        iface_qtn,
                        &iface_args,
                        field_index,
                    )?;
                    let obj_ptr = self.as_object_ptr(receiver, ObjectType::Instance)?;
                    let load_result = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        instance
                            .try_load_field(slot)
                            .ok_or_else(|| instance.field_len())
                    };
                    let value = match load_result {
                        Ok(value) => value,
                        Err(length) => return Err(self.invalid_field_access_error(slot, length)),
                    };
                    self.stack.push(value);
                }

                OpCode::VirtualStoreField => {
                    let field_index = { read_u32_unchecked(code, pc) as usize };
                    let iface_value = self.stack.ensure_pop();
                    let (iface_qtn, iface_args) = self.pop_interface_operand(iface_value)?;
                    let new_value = self.stack.ensure_pop();
                    let receiver = self.stack.ensure_pop();
                    let slot = self.resolve_virtual_field_slot(
                        receiver,
                        iface_qtn,
                        &iface_args,
                        field_index,
                    )?;
                    let obj_ptr = self.as_object_ptr(receiver, ObjectType::Instance)?;
                    let store_error = {
                        let Object::Instance(instance) = self.get_object(obj_ptr) else {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Instance.into(),
                                got: ObjectType::of(self.get_object(obj_ptr)).into(),
                            }
                            .into());
                        };
                        (slot >= instance.field_len()).then_some(instance.field_len())
                    };
                    if let Some(length) = store_error {
                        return Err(self.invalid_field_access_error(slot, length));
                    }
                    self.heap.write_barrier(obj_ptr, new_value);
                    let Object::Instance(instance) = self.get_object(obj_ptr) else {
                        unreachable!("already type-checked above");
                    };
                    instance.store_field(slot, new_value);
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
                    self.init_spread(
                        dest_value,
                        source_value,
                        &function.bytecode.field_copy_sets[idx],
                    )?;
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
                    // The declared element type rides on top of the `size`
                    // elements: a preceding `LoadType` pushed it, already resolved
                    // against the frame's type args. Pop it before the values.
                    let element_ty = self.ensure_pop_type()?;
                    let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                    let array: Vec<Value> = self.stack.drain(drain_range).collect();
                    let array_index = self.tlab.alloc_array(element_ty, array);
                    self.stack.push(Value::object(array_index));
                }

                OpCode::AllocMap => {
                    let n = { read_u32_unchecked(code, pc) as usize };
                    // The declared value type rides on top, the key type just
                    // below it (two `LoadType`s after the entries, already
                    // resolved against the frame's type args). Pop both before
                    // the entries.
                    let value_ty = self.ensure_pop_type()?;
                    let key_ty = self.ensure_pop_type()?;
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
                    let obj_index = self.tlab.alloc_map(key_ty, value_ty, map);
                    self.stack.push(Value::object(obj_index));
                }

                OpCode::AllocInstance => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let ntypeargs = { read_u16_unchecked(code, pc) } as usize;
                    let class_ptr = self.idx_to_ptr(ObjectIndex::from_raw(raw as usize));

                    let class_type_args: Box<[bex_vm_types::RealizedTy]> = if ntypeargs == 0 {
                        Box::default()
                    } else {
                        self.pop_type_args(ntypeargs)?.tys.into()
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
                        return Err(VmError::thrown_fresh(self.panic_to_exception_value(
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
                OpCode::SysOp | OpCode::SysOpWithRuntimeId => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let runtime_id = if matches!(op, OpCode::SysOpWithRuntimeId) {
                        Some(self.stack.ensure_pop())
                    } else {
                        None
                    };
                    let callee = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.load_global_in(function.runtime_package, callee);
                    // `as_object_ptr` only unwraps the value to a heap pointer
                    // (the `FunctionType` argument is error-message metadata, not
                    // an assertion). `dispatch_sysop_yield`'s own kind check is
                    // therefore the load-bearing validation on this path — it
                    // rejects a non-sys-op global before draining and yields.
                    let callee_ptr =
                        self.as_object_ptr(callee_value, FunctionType::SysOp.into())?;
                    return Ok(Some(
                        self.dispatch_sysop_yield(callee_ptr, runtime_id, *frame_idx)?,
                    ));
                }

                // ── Spawn (BEP-034) ────────────────────────────────────────────
                OpCode::Spawn => {
                    // Stack layout (pushed by emit in this order): closure, name,
                    // config, the future's `T`, the future's `E`. So pop in
                    // reverse: `E` (top), `T`, config, name, closure. The two
                    // types were pushed by `LoadType`, already resolved against
                    // this frame's type args, and travel with the request so the
                    // engine can type the heap `Future` it allocates.
                    let throws = self.ensure_pop_type()?;
                    let returns = self.ensure_pop_type()?;
                    // `config` is the optional `baml.spawn.SpawnConfig` from a
                    // `with baml.spawn.options(...)` clause, or null.
                    let config_value = self.stack.ensure_pop();
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
                    let config_ptr = if config_value.is_null() {
                        None
                    } else if let Some(ptr) = config_value.as_object_ptr()
                        && matches!(unsafe { ptr.get() }, Object::Instance(_))
                    {
                        // Must be an instance (`baml.spawn.SpawnParams`) — an
                        // arbitrary heap object here would turn a local type
                        // error into a VM→engine contract break downstream.
                        Some(ptr)
                    } else {
                        return Err(VmInternalError::TypeError {
                            expected: Type::Object(ObjectType::Instance),
                            got: self.type_of(&config_value),
                        }
                        .into());
                    };
                    let pending_future = bex_vm_types::types::UnscheduledFuture {
                        closure: closure_ptr,
                        name: name_ptr,
                        config: config_ptr,
                        returns,
                        throws,
                    };
                    let object_index = self
                        .tlab
                        .alloc(Object::UnscheduledFuture(Box::new(pending_future)));
                    let source_span = self.call_site_source_for_frame(*frame_idx, self.cur_pc);
                    return Ok(Some(VmExecState::Spawn {
                        future: object_index,
                        source_span,
                    }));
                }

                // ── Call ──────────────────────────────────────────────────────
                OpCode::Call | OpCode::CallWithRuntimeId => {
                    let raw = read_u32_unchecked(code, pc);
                    let (ntypeargs, runtime_type_check) =
                        bex_vm_types::bytecode::decode_call_type_args(read_u16_unchecked(code, pc));
                    let runtime_id = if matches!(op, OpCode::CallWithRuntimeId) {
                        Some(self.stack.ensure_pop())
                    } else {
                        None
                    };
                    let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let callee_value = self.load_global_in(function.runtime_package, callee_global);
                    let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;

                    let args_offset = self.stack.len().checked_sub(arg_count + ntypeargs).ok_or(
                        VmInternalError::NotEnoughItemsOnStack(arg_count + ntypeargs),
                    )?;
                    let locals_offset = StackIndex::from_raw(args_offset);

                    // Save pc as return address before pushing new frame.
                    let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                        verifier_unreachable!()
                    };
                    bf.instruction_ptr = *pc;

                    let result = if ntypeargs == 0
                        && self.pending_call_type_args.is_empty()
                        && self.pending_call_type_values.is_empty()
                    {
                        // Ordinary non-generic calls have no pending type lane
                        // to save or clear. Keep them off the richer BEP-066
                        // exact-value path so a tiny function call does not
                        // construct and move several empty collections.
                        self.execute_call_from_locals_offset(
                            callee_ptr,
                            locals_offset,
                            arg_count,
                            runtime_id,
                            runtime_type_check,
                            frame_idx,
                            function,
                        )
                    } else {
                        let type_args = self.take_type_args_below_values(ntypeargs, arg_count)?;
                        self.execute_call_from_locals_offset_with_type_args(
                            callee_ptr,
                            locals_offset,
                            arg_count,
                            CallOptions {
                                runtime_id,
                                type_args: &type_args.tys,
                                type_values: &type_args.values,
                                runtime_type_check,
                            },
                            frame_idx,
                            function,
                        )
                    };
                    return result;
                }

                // ── VirtualCall ───────────────────────────────────────────────
                // Open-world interface dispatch: resolve the method at runtime
                // from the receiver's concrete `Self` type, then take the shared
                // frame-push call path (mirrors `Call`). Stack layout (top last):
                // `[arg_0 (receiver), …, arg_{nargs-1}, iface_type, method_name]`.
                OpCode::VirtualCall | OpCode::VirtualCallWithRuntimeId => {
                    let nargs = read_u16_unchecked(code, pc) as usize;
                    let (ntypeargs, runtime_type_check) =
                        bex_vm_types::bytecode::decode_call_type_args(read_u16_unchecked(code, pc));
                    let runtime_id = if matches!(op, OpCode::VirtualCallWithRuntimeId) {
                        Some(self.stack.ensure_pop())
                    } else {
                        None
                    };

                    // Pop the method name (top) then the interface type. Keep
                    // both as values until after the static-cache probe: their
                    // contents are immutable at this bytecode site, and turning
                    // them into owned names on every hit was measurable in
                    // interface-heavy loops.
                    let method_value = self.stack.ensure_pop();
                    let iface_value = self.stack.ensure_pop();

                    // A parameterized interface operand carries the runtime
                    // definitions of its arguments (BEP-066). Resolution reads
                    // only the realized `ty` off it, so without carrying the
                    // overlay into the callee the impl body would lose the
                    // argument's runtime declarations — reflection, rendering
                    // and SAP inside an interface method would fail on a type
                    // the caller can use fine.
                    //
                    // The clone is `O(defs)` on every dispatch that carries any
                    let method_type_args = if ntypeargs == 0 {
                        None
                    } else {
                        Some(self.take_type_args_below_values(ntypeargs, nargs)?)
                    };

                    let args_offset = self
                        .stack
                        .len()
                        .checked_sub(nargs)
                        .ok_or(VmInternalError::NotEnoughItemsOnStack(nargs))?;
                    // `Self` is the receiver's runtime concrete type; coherence makes
                    // `(Self, iface<args>)` resolve to at most one impl. Off that rule
                    // the method resolves through `rule_method_impl` (the provided
                    // row, or the interface's default on a miss). `nargs` equals the
                    // method's arity — the interface fixes the parameter count, so
                    // every impl agrees. The rule borrows `self`; scope it so the
                    // borrow ends before the `&mut self` call below.
                    let receiver = self.stack[StackIndex::from_raw(args_offset)];
                    let cache_key = if function.runtime_package.is_null() {
                        let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
                        let Object::Type(type_value) = self.get_object(iface_ptr) else {
                            unreachable!("as_object_ptr(Type) guarantees a Type object")
                        };
                        let (interface_head, interface_args) = match &type_value.ty {
                            bex_vm_types::RealizedTy::Interface(head, args, _, _) => {
                                (*head, args.clone())
                            }
                            other => unreachable!(
                                "VirtualCall interface operand must be an Interface type, found {other:?}"
                            ),
                        };
                        self.static_virtual_receiver_key(receiver).map(|receiver| {
                            StaticVirtualCallKey {
                                caller_function_addr: std::ptr::from_ref(*function) as usize,
                                call_pc: self.cur_pc,
                                receiver,
                                interface_head,
                                interface_args,
                            }
                        })
                    } else {
                        self.runtime_virtual_call_cache_key(
                            self.frames[*frame_idx].function(),
                            iface_value,
                            receiver,
                        )?
                    };
                    let cached_target = cache_key
                        .as_ref()
                        .and_then(|key| self.static_virtual_call_cache.get(key))
                        .cloned();
                    let (callee_ptr, mut type_args) = if let Some(target) = cached_target {
                        (target.callee, target.frame_type_args)
                    } else {
                        let method_name = self.as_string(&method_value)?.to_string();
                        let (iface_qtn, iface_args) = {
                            let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
                            match self.get_object(iface_ptr) {
                                // The interface's input args select among a type's
                                // impls of the same interface at several
                                // instantiations. Associated types are outputs,
                                // not part of the resolver key.
                                Object::Type(type_value) => match &type_value.ty {
                                    bex_vm_types::RealizedTy::Interface(
                                        qtn,
                                        args,
                                        _assoc,
                                        _attr,
                                    ) => (*qtn, args.clone()),
                                    other => unreachable!(
                                        "VirtualCall interface operand must be an Interface type, found {other:?}"
                                    ),
                                },
                                other => unreachable!(
                                    "as_object_ptr(Type) guarantees a Type object, found {:?}",
                                    ObjectType::of(other)
                                ),
                            }
                        };
                        // `Self` is the receiver value's realized concrete type.
                        let self_ty = bex_vm_types::RealizedTy::from(
                            self.value_concrete_ty(receiver).unwrap_or_else(|| {
                                unreachable!(
                                    "value of kind {:?} cannot be a virtual-call receiver",
                                    self.type_of(&receiver)
                                )
                            }),
                        );
                        let resolver = crate::package_baml::ImplResolver::for_value(self, receiver);
                        let (rule, bound_args) = resolver
                            .resolve_implements_rule(&self_ty, iface_qtn, &iface_args)
                            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                                method: method_name.clone(),
                            })?;
                        let method = resolver
                            .rule_method_impl(&rule, method_name.as_str())
                            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                                method: method_name.clone(),
                            })?
                            .method;
                        // `fqn` is the resolved callee's heap pointer (provided
                        // row or adopted interface default) — invoke it directly.
                        let callee = method.fqn;
                        // Seed the callee frame: the impl's frame realized against
                        // its bound args (the impl's own generics for a provided
                        // method, or `[Self, interface args..]` for an adopted
                        // default — associated types are never frame slots), then
                        // the method-level type args — matching the callee's
                        // De Bruijn layout `[owner… ++ method…]`.
                        let frame = resolver.realize_frame(&method.frame, &bound_args)?;
                        let cacheable = rule.is_static();
                        if cacheable && let Some(cache_key) = cache_key {
                            self.static_virtual_call_cache.insert(
                                cache_key,
                                StaticVirtualCallTarget {
                                    callee,
                                    frame_type_args: frame.clone(),
                                },
                            );
                        }
                        (callee, frame)
                    };
                    // The receiver's class-level slots need no exact carrier:
                    // the resolver realizes them off `Self` as head-carrying
                    // types, so `reflect.Type.of<T>()` in an impl or default-method
                    // body dereferences the caller's own declaration rather
                    // than deriving a fresh identity for it.
                    let type_values = match method_type_args.as_ref() {
                        Some(method) => append_virtual_method_type_args(&mut type_args, method),
                        None => Vec::new(),
                    };

                    let locals_offset = StackIndex::from_raw(args_offset);

                    // Save pc as return address before pushing the new frame.
                    let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                        verifier_unreachable!()
                    };
                    bf.instruction_ptr = *pc;

                    let result = if type_args.is_empty()
                        && method_type_args.is_none()
                        && self.pending_call_type_args.is_empty()
                        && self.pending_call_type_values.is_empty()
                    {
                        self.execute_call_from_locals_offset(
                            callee_ptr,
                            locals_offset,
                            nargs,
                            runtime_id,
                            runtime_type_check,
                            frame_idx,
                            function,
                        )
                    } else {
                        self.execute_call_from_locals_offset_with_type_args(
                            callee_ptr,
                            locals_offset,
                            nargs,
                            CallOptions {
                                runtime_id,
                                type_args: &type_args,
                                type_values: &type_values,
                                runtime_type_check,
                            },
                            frame_idx,
                            function,
                        )
                    };
                    return result;
                }

                // ── CallIndirect ──────────────────────────────────────────────
                OpCode::CallIndirect | OpCode::CallIndirectWithRuntimeId => {
                    let runtime_id = if matches!(op, OpCode::CallIndirectWithRuntimeId) {
                        Some(self.stack.ensure_pop())
                    } else {
                        None
                    };
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
                        if runtime_id.is_some() {
                            return Err(self.invalid_argument_vm_error(
                                "explicit $id is not supported for host-callable values",
                            ));
                        }
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
                        let call_site_source =
                            self.call_site_source_for_frame(*frame_idx, self.cur_pc);
                        return Ok(Some(self.host_closure_call_sysop(
                            callee_ptr,
                            user_args,
                            call_site_source,
                        )));
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
                        let _popped = self.stack.ensure_pop();
                        let args_offset = self
                            .stack
                            .len()
                            .checked_sub(visible_arity)
                            .ok_or(VmInternalError::NotEnoughItemsOnStack(visible_arity))?;
                        self.stack.insert(args_offset, receiver);
                        let locals_offset = StackIndex::from_raw(args_offset);
                        // Pass the BoundMethod pointer (not its inner function) so
                        // `execute_call_from_locals_offset` seeds the receiver's
                        // `class_type_args` into the new frame — generic instance
                        // methods invoked through a bound-method value would
                        // otherwise start with an empty class-type-arg prefix. The
                        // receiver is already on the stack, and the helper resolves
                        // the inner function itself without re-inserting it.
                        if let Some(state) = self.execute_call_from_locals_offset(
                            callee_ptr,
                            locals_offset,
                            full_arity,
                            runtime_id,
                            false,
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
                            runtime_id,
                            false,
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

                    let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                        unreachable!()
                    };
                    let (popped_call_id, popped_parent_call_id, capture_mask, locals_offset) = (
                        bf.call_id,
                        bf.parent_call_id,
                        bf.capture_mask,
                        bf.locals_offset,
                    );
                    self.maybe_queue_call_output(
                        popped_call_id,
                        popped_parent_call_id,
                        capture_mask,
                        result,
                    );
                    self.stack.drain(locals_offset..);
                    self.stack.push(result);
                    self.frames.pop();
                    self.prof_exit_call(
                        popped_call_id,
                        popped_parent_call_id,
                        bex_events::prof::record::FunctionEndStatus::Ok,
                    );
                    // Update frame_idx so the outer loop detects the frame change
                    // and re-extracts code/pc/function for the parent frame.
                    if !self.frames.is_empty() {
                        *frame_idx = self.frames.len() - 1;
                    }
                    if self.frames.is_empty() {
                        return Ok(Some(VmExecState::Complete(self.stack.ensure_pop())));
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
                            FutureRead::Error(value) => {
                                awaiting.mark_observed();
                                let origin = self.prof_unwind_origin_for_frame(
                                    *frame_idx,
                                    self.cur_pc,
                                    VmUnwindSource::FutureResume,
                                    false,
                                );
                                return Err(VmError::Thrown(
                                    VmThrown::rethrow(value, VmUnwindSource::FutureResume, false)
                                        .with_origin(origin),
                                ));
                            }
                            FutureRead::Cancelled => {
                                let value = self.panic_to_exception_value(VmPanic::Cancelled);
                                let origin = self.prof_unwind_origin_for_frame(
                                    *frame_idx,
                                    self.cur_pc,
                                    VmUnwindSource::FutureResume,
                                    false,
                                );
                                return Err(VmError::Thrown(
                                    VmThrown::fresh(value, VmUnwindSource::FutureResume)
                                        .with_origin(origin),
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

                // ── AwaitAny (BEP-034 baml.future.__await_any) ─────────────────
                OpCode::AwaitAny => {
                    // Like Await, this opcode re-executes on resume, so the
                    // array operand is peeked (not popped) until a winner is
                    // found. Rewinding by `AWAIT_ANY_OPCODE_LEN` puts `pc`
                    // back at the opcode byte for re-execution.
                    const AWAIT_ANY_OPCODE_LEN: usize = 1;
                    let top = self.stack.ensure_stack_top();
                    let array_val = self.stack[top];
                    // Scan the input futures in order. The first non-pending
                    // future (settled with a value, error, or cancellation)
                    // is the winner; otherwise gather the pending ids to park
                    // on. `race`/`any` are built on "first to settle", so we
                    // do not distinguish success from failure here.
                    let mut pending_ids: Vec<FutureId> = Vec::new();
                    let mut winner: Option<usize> = None;
                    {
                        let arr = self.as_array(&array_val)?;
                        for (i, elem) in arr.iter().enumerate() {
                            let fut_ptr = self.as_object_ptr(
                                *elem,
                                bex_vm_types::types::FutureType::Any.into(),
                            )?;
                            let Object::Future(fut) = self.get_object(fut_ptr) else {
                                return Err(VmInternalError::TypeError {
                                    expected: bex_vm_types::types::FutureType::Any.into(),
                                    got: ObjectType::of(self.get_object(fut_ptr)).into(),
                                }
                                .into());
                            };
                            match fut.read() {
                                FutureRead::Pending(id) => pending_ids.push(id),
                                // Ready / Error / Cancelled / InternalError all
                                // count as "settled" — this index has won.
                                _ => {
                                    winner = Some(i);
                                    break;
                                }
                            }
                        }
                    }
                    match winner {
                        Some(i) => {
                            self.stack.pop();
                            self.stack.push(Value::int(i as i64));
                            if self.early_yield.should_early_yield() {
                                return Ok(Some(VmExecState::EarlyYield));
                            }
                        }
                        None => {
                            // No input has settled yet — park until the first
                            // does. Rewind pc so the opcode re-executes (with
                            // the array still on the stack) once resumed.
                            *pc -= AWAIT_ANY_OPCODE_LEN;
                            return Ok(Some(VmExecState::AwaitAny(pending_ids)));
                        }
                    }
                }

                // ── Throw ─────────────────────────────────────────────────────
                OpCode::Throw | OpCode::Rethrow => {
                    let is_rethrow = op == OpCode::Rethrow;
                    let value = self.stack.ensure_pop();
                    let origin = self.prof_unwind_origin_for_frame(
                        *frame_idx,
                        self.cur_pc,
                        VmUnwindSource::Bytecode,
                        false,
                    );
                    // Save pc before unwinding (handler lookup needs it).
                    if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                        bf.instruction_ptr = *pc;
                    }
                    let thrown = if is_rethrow {
                        VmThrown::rethrow(value, VmUnwindSource::Bytecode, true)
                    } else {
                        VmThrown::fresh(value, VmUnwindSource::Bytecode)
                    }
                    .with_origin(origin);
                    self.try_unwind_exception(frame_idx, function, thrown)?;
                    // A handler was found; sync the local `pc` to its entry.
                    // When the handler is in the SAME frame, the dispatch loop
                    // would otherwise `continue` with the stale post-throw `pc`
                    // instead of jumping to the handler. (Cross-frame unwinds
                    // reload `pc` on the frame switch, so this is a no-op there.)
                    // Cold path — does not affect the hot per-instruction loop.
                    if let Some(Frame::Bytecode(bf)) = self.frames.get(*frame_idx) {
                        *pc = bf.instruction_ptr;
                    }
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

                OpCode::RuntimeIsType => {
                    let expected_value = self.stack.ensure_pop();
                    // A kind view filters as the `type` value it wraps.
                    let expected_value = crate::package_reflect::type_kinds::as_view_type_value(
                        self,
                        expected_value,
                    )
                    .unwrap_or(expected_value);
                    let value = self.stack.ensure_pop();
                    // `is unreflect(t)` filters on *nominal* identity: the
                    // scrutinee's declaration must be the one `t` denotes, at
                    // the same instantiation. Declaration identity is the
                    // qualified name — program-unique for compiled
                    // declarations, creation-unique for runtime ones.
                    //
                    // BUG: a `t` denoting a non-nominal type (`string`,
                    // `int | null`, a list) can never match, so the pattern
                    // silently fails instead of testing membership.
                    let expected_nominal = expected_value
                        .as_object_ptr()
                        .and_then(|ptr| match self.get_object(ptr) {
                            Object::Type(type_value) => Some(&type_value.ty),
                            _ => None,
                        })
                        .and_then(nominal_identity);
                    let actual_nominal = self
                        .value_concrete_ty(value)
                        .map(bex_vm_types::RealizedTy::from);
                    let matched = expected_nominal.is_some()
                        && expected_nominal == actual_nominal.as_ref().and_then(nominal_identity);
                    self.stack.push(Value::bool(matched));
                }

                // ── IsType ────────────────────────────────────────────────────
                OpCode::IsType => {
                    let const_idx = { read_u32_unchecked(code, pc) as usize };
                    let value = self.stack.ensure_pop();
                    let raw_const = &function.bytecode.constants[const_idx];
                    let resolved_const = function.bytecode.resolved_constants[const_idx];
                    let result = self.value_matches_type_constant(
                        *frame_idx,
                        value,
                        raw_const,
                        resolved_const,
                    )?;
                    self.stack.push(Value::bool(result));
                }

                OpCode::NarrowBind => {
                    let const_idx = { read_u32_unchecked(code, pc) as usize };
                    let destination = { read_u32_unchecked(code, pc) as usize };
                    let value = self.stack.ensure_pop();
                    let raw_const = &function.bytecode.constants[const_idx];
                    let resolved_const = function.bytecode.resolved_constants[const_idx];
                    let matched = self.value_matches_type_constant(
                        *frame_idx,
                        value,
                        raw_const,
                        resolved_const,
                    )?;
                    if matched {
                        let Frame::Bytecode(bf) = &self.frames[*frame_idx] else {
                            unreachable!()
                        };
                        let destination =
                            Self::local_slot_stack_index(bf.locals_offset, destination);
                        self.stack[destination] = value;
                    }
                    self.stack.push(Value::bool(matched));
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
                        let origin = self.prof_unwind_origin_for_frame(
                            *frame_idx,
                            self.cur_pc,
                            VmUnwindSource::Bytecode,
                            false,
                        );
                        // Save pc before unwinding (handler lookup needs it).
                        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                            bf.instruction_ptr = *pc;
                        }
                        self.try_unwind_exception(
                            frame_idx,
                            function,
                            VmThrown::rethrow(value, VmUnwindSource::Bytecode, true)
                                .with_origin(origin),
                        )?;
                        // Sync the local `pc` to the handler entry (see
                        // OpCode::Throw): a same-frame rethrow — an inner
                        // wildcard catch rethrowing a panic to an outer catch in
                        // the same function — must jump to the handler instead of
                        // falling through to the not-a-panic continuation.
                        if let Some(Frame::Bytecode(bf)) = self.frames.get(*frame_idx) {
                            *pc = bf.instruction_ptr;
                        }
                    }
                    if self.early_yield.should_early_yield() {
                        return Ok(Some(VmExecState::EarlyYield));
                    }
                }

                // ── Unreachable ───────────────────────────────────────────────
                OpCode::Unreachable => {
                    return Err(VmError::thrown_fresh(
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

                    let captured_type_args = self.pop_type_args(ntypeargs)?;

                    let function_ptr = self.idx_to_ptr(ObjectIndex::from_raw(obj_idx_raw));
                    let closure = Object::Closure(Closure {
                        function: function_ptr,
                        captures: captures.into_boxed_slice(),
                        captured_type_args: captured_type_args.tys.into_boxed_slice(),
                    });
                    let ptr = self.tlab.alloc(closure);
                    self.stack.push(Value::object(ptr));
                }

                // ── LoadType ──────────────────────────────────────────────────
                OpCode::LoadType => {
                    let idx = { read_u32_unchecked(code, pc) as usize };
                    let static_cache_key = match &function.bytecode.constants[idx] {
                        ConstValue::Type(template)
                            if function.runtime_package.is_null()
                                && <&bex_vm_types::RealizedTy>::try_from(template).is_ok()
                                && matches!(&self.frames[*frame_idx],
                                    Frame::Bytecode(frame)
                                        if frame.type_metadata.as_ref()
                                            .is_none_or(|metadata| !metadata.binds_runtime_declaration(&self.heap))) =>
                        {
                            Some((std::ptr::from_ref(*function) as usize, idx))
                        }
                        ConstValue::Type(template) if !function.runtime_package.is_null() => {
                            self.runtime_load_type_cache_key(*frame_idx, template, idx)
                        }
                        _ => None,
                    };
                    let cached_value = static_cache_key
                        .and_then(|key| self.static_load_type_cache.get(&key).copied())
                        .map(Value::object);
                    let value = if let Some(value) = cached_value {
                        value
                    } else {
                        let template = match &function.bytecode.constants[idx] {
                            ConstValue::Type(t) => t.clone(),
                            _ => {
                                return Err(VmInternalError::UnexpectedConstantKind.into());
                            }
                        };

                        let exact_dynamic =
                            if let bex_vm_types::TyTemplate::TypeArgRef(slot) = &template {
                                match &self.frames[*frame_idx] {
                                    Frame::Bytecode(frame) => frame
                                        .type_metadata
                                        .as_ref()
                                        .and_then(|metadata| metadata.values.get(*slot as usize))
                                        .and_then(Clone::clone),
                                    Frame::Native(_) => None,
                                }
                            } else {
                                None
                            };
                        let value = if let Some(type_value) = exact_dynamic {
                            Value::object(self.tlab.alloc_type(type_value))
                        } else {
                            let ty: bex_vm_types::RealizedTy = {
                                // A fully-realized template narrows to `RealizedTy` in a
                                // single validation walk — no substitution environment
                                // needed. Otherwise resolve its frame refs (and reduce any
                                // projection) against the frame's realized type args; the
                                // result must be realized or it is an internal error, never
                                // a `unknown` erasure.
                                if let Ok(realized) =
                                    <&bex_vm_types::RealizedTy>::try_from(&template)
                                {
                                    realized.clone()
                                } else {
                                    let frame_type_args =
                                        if let Frame::Bytecode(bf) = &self.frames[*frame_idx] {
                                            bf.type_args.clone()
                                        } else {
                                            vec![]
                                        };
                                    template.substitute(&frame_type_args, self).map_err(|e| {
                                        VmInternalError::TypeSubstitution {
                                            message: e.to_string(),
                                        }
                                    })?
                                }
                            };
                            if let Some(type_ptr) = self.current_runtime_declaration_type(&ty) {
                                Value::object(type_ptr)
                            } else {
                                Value::object(self.tlab.alloc_type(TypeValue::new(ty)))
                            }
                        };
                        if let Some(cache_key) = static_cache_key
                            && let Some(ptr) = value.as_object_ptr()
                        {
                            self.static_load_type_cache.insert(cache_key, ptr);
                        }
                        value
                    };
                    self.stack.push(value);
                }

                // ── Lexical runtime type binding ────────────────────────────
                OpCode::BindType => {
                    let slot = read_u32_unchecked(code, pc) as usize;
                    let value = self.stack.ensure_pop();
                    let ptr = self.as_object_ptr(value, ObjectType::Type)?;
                    let Object::Type(type_value) = self.get_object(ptr) else {
                        unreachable!("ObjectType::Type was validated above")
                    };
                    let type_value = (**type_value).clone();
                    let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] else {
                        unreachable!("compact bytecode runs in a bytecode frame")
                    };
                    frame
                        .type_args
                        .resize(slot + 1, bex_vm_types::RealizedTy::unknown());
                    frame.type_args[slot] = type_value.ty.clone();
                    let metadata = frame
                        .type_metadata
                        .get_or_insert_with(|| Box::new(FrameTypeMetadata::default()));
                    metadata.values.resize(slot + 1, None);
                    metadata.values[slot] = Some(type_value);
                }
                // ── Package.current() ───────────────────────────────────────
                OpCode::LoadCurrentPackage => {
                    let idx = read_u32_unchecked(code, pc) as usize;
                    let package_name = {
                        let value = function.bytecode.resolved_constants[idx];
                        self.as_string(&value)?.to_string()
                    };
                    let value =
                        crate::package_reflect::reflect::current_package_value(self, &package_name);
                    self.stack.push(value);
                }

                // ── MakeBoundMethod ───────────────────────────────────────────
                OpCode::MakeBoundMethod => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let receiver = self.stack.ensure_pop();
                    let callee_value = self.load_global_in(function.runtime_package, global_idx);
                    let function_ptr =
                        self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                    // Curry the receiver's class type args (→ `Self`) into the
                    // value now, so the bound method is fully realized and the
                    // `CallIndirect` that invokes it needs no type-arg operands.
                    // (Method-level fn generics — `b.m<int>` — are not yet
                    // curried here; that needs turbofish-on-member-access
                    // support and would append after these.)
                    let type_args = self.bound_method_curried_type_args(receiver);
                    let bound = Object::BoundMethod(BoundMethod {
                        function: function_ptr,
                        receiver,
                        type_args,
                    });
                    let ptr = self.tlab.alloc(bound);
                    self.stack.push(Value::object(ptr));
                }

                // ── MakeVirtualBoundMethod ────────────────────────────────────
                // The value analogue of `VirtualCall`: resolve the interface
                // method from the receiver's concrete `Self` at *bind* time (the
                // receiver value — and hence its type — is fixed here), producing
                // a regular `BoundMethod` that additionally carries the impl's
                // realized frame type args (a blanket impl's or inherited
                // default's frame, which the receiver's class args can't express).
                // Stack (top last): `[receiver, type_args…, iface_type, method_name]`.
                OpCode::MakeVirtualBoundMethod => {
                    let ntypeargs = read_u16_unchecked(code, pc) as usize;
                    let method_value = self.stack.ensure_pop();
                    let method_name = self.as_string(&method_value)?.to_string();
                    let iface_value = self.stack.ensure_pop();
                    // The method-level type args (a generic interface method's own
                    // generics, specialized at the reference site) sit below the
                    // interface type; they append to the resolved impl frame.
                    let method_type_args = self.pop_type_args(ntypeargs)?;
                    let receiver = self.stack.ensure_pop();
                    // `Self` is the receiver value's realized concrete type.
                    let self_ty = bex_vm_types::RealizedTy::from(
                        self.value_concrete_ty(receiver).unwrap_or_else(|| {
                            unreachable!(
                                "value of kind {:?} cannot be a virtual bound-method receiver",
                                self.type_of(&receiver)
                            )
                        }),
                    );
                    let (function_ptr, type_args) = self.resolve_virtual_method(
                        receiver,
                        &self_ty,
                        iface_value,
                        &method_name,
                        method_type_args,
                    )?;
                    let bound = Object::BoundMethod(BoundMethod {
                        function: function_ptr,
                        receiver,
                        type_args: type_args.into_boxed_slice(),
                    });
                    let ptr = self.tlab.alloc(bound);
                    self.stack.push(Value::object(ptr));
                }

                // ── MakeVirtualFunction ───────────────────────────────────────
                OpCode::MakeVirtualFunction => {
                    let ntypeargs = read_u16_unchecked(code, pc) as usize;
                    let method_value = self.stack.ensure_pop();
                    let method_name = self.as_string(&method_value)?.to_string();
                    let iface_value = self.stack.ensure_pop();
                    let method_type_args = self.pop_type_args(ntypeargs)?;
                    let self_type_value = self.stack.ensure_pop();
                    // `Self` is PASSED: a written (or frame-realized) type
                    // operand — the only dispatch source for a method with no
                    // `self` receiver. The compiler guarantees it is a
                    // dispatchable concrete type by the time it gets here.
                    let self_ty = {
                        let self_ptr = self.as_object_ptr(self_type_value, ObjectType::Type)?;
                        match self.get_object(self_ptr) {
                            Object::Type(type_value) => type_value.ty.clone(),
                            other => unreachable!(
                                "as_object_ptr(Type) guarantees a Type object, found {:?}",
                                ObjectType::of(other)
                            ),
                        }
                    };
                    let (function_ptr, type_args) = self.resolve_virtual_method(
                        self_type_value,
                        &self_ty,
                        iface_value,
                        &method_name,
                        method_type_args,
                    )?;
                    // A capture-less closure is the unbound-callable carrier:
                    // calling it seeds `frame.type_args` from
                    // `captured_type_args`, exactly like the pooled
                    // `GenericFunction` path (`MakeGenericFunctionFromValue`).
                    let closure = Object::Closure(bex_vm_types::types::Closure {
                        function: function_ptr,
                        captures: Box::new([]),
                        captured_type_args: type_args.into_boxed_slice(),
                    });
                    let ptr = self.tlab.alloc(closure);
                    self.stack.push(Value::object(ptr));
                }

                // ── MakeGenericFunction ───────────────────────────────────────
                OpCode::MakeGenericFunction => {
                    let raw = { read_u32_unchecked(code, pc) };
                    let function_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                    let ntypeargs = { read_u16_unchecked(code, pc) as usize };
                    let type_args = self.pop_type_args(ntypeargs)?;
                    let gf = Object::GenericFunction(bex_vm_types::GenericFunction {
                        function: function_global,
                        type_args: type_args.tys.into_boxed_slice(),
                        runtime_package: function.runtime_package,
                    });
                    let ptr = self.tlab.alloc(gf);
                    self.stack.push(Value::object(ptr));
                }

                // ── MakeGenericFunctionFromValue ──────────────────────────────
                // Specialize a runtime callable value with explicit type args
                // (`g<int>` where `g` is a local function value). Wrap it in a
                // Closure whose `captured_type_args` are seeded into the frame on
                // call — reusing the closure call path so the specialization is
                // honoured at runtime instead of being erased.
                OpCode::MakeGenericFunctionFromValue => {
                    let ntypeargs = { read_u16_unchecked(code, pc) as usize };
                    // The callable value was pushed last (top of stack); the
                    // resolved `Object::Type` args sit beneath it.
                    let callable = self.stack.ensure_pop();
                    let type_args = self.pop_type_args(ntypeargs)?;
                    // Resolve the callable to its inner Function pointer (and any
                    // captures, if it is already a closure).
                    let callable_ptr =
                        self.as_object_ptr(callable, FunctionType::Callable.into())?;
                    // A plain function or closure is wrapped in a closure
                    // carrying the type args (closures carry over their existing
                    // `captured_type_args` — the outer/class generic environment
                    // — then append the new instantiation args in call order, so
                    // a later indirect call seeds a complete `frame.type_args`).
                    //
                    // A `BoundMethod` (`let f = p.method<int>`) cannot be wrapped
                    // in a closure without losing its receiver, so it is passed
                    // through unchanged: the explicit type args are dropped, but
                    // the call still dispatches with the correct `self`. This is
                    // correct for the common case of a method that does not reify
                    // `T` at runtime, and — unlike closure-wrapping it — never
                    // crashes. (`GenericFunction` does not reach here: TIR rejects
                    // type args on an already-specialized value.)
                    let wrap: Option<(HeapPtr, Vec<Value>, Vec<bex_vm_types::RealizedTy>)> =
                        match self.get_object(callable_ptr) {
                            Object::Function(_) => Some((callable_ptr, Vec::new(), Vec::new())),
                            Object::Closure(c) => Some((
                                c.function,
                                c.captures.to_vec(),
                                c.captured_type_args.to_vec(),
                            )),
                            _ => None,
                        };
                    match wrap {
                        Some((function_ptr, captures, mut captured_type_args)) => {
                            captured_type_args.extend(type_args.tys);
                            let closure = Object::Closure(Closure {
                                function: function_ptr,
                                captures: captures.into_boxed_slice(),
                                captured_type_args: captured_type_args.into_boxed_slice(),
                            });
                            let ptr = self.tlab.alloc(closure);
                            self.stack.push(Value::object(ptr));
                        }
                        None => self.stack.push(callable),
                    }
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
                    // inner scope before any `&mut self` call. A negative index
                    // counts from the end; one that still lands outside the
                    // array reports the original index in the panic.
                    let load_result: Result<Value, (i64, usize)> = {
                        match self.get_object(array_obj_index) {
                            Object::Array(arr) => {
                                let guard = arr.lock();
                                let len = guard.len();
                                match crate::array_index::resolve_index(i, len) {
                                    Some(idx) => Ok(guard[idx]),
                                    None => Err((i, len)),
                                }
                            }
                            Object::Uint8Array(bytes) => {
                                let guard = bytes.lock();
                                let len = guard.len();
                                match crate::array_index::resolve_index(i, len) {
                                    Some(idx) => Ok(Value::int(i64::from(guard[idx]))),
                                    None => Err((i, len)),
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
                            return Err(VmError::thrown_fresh(self.panic_to_exception_value(
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
                            return Err(VmError::thrown_fresh(
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
                    let Some(i) = index_value.as_int() else {
                        return Err(VmInternalError::TypeError {
                            expected: bex_vm_types::types::Type::Int,
                            got: self.type_of(&index_value),
                        }
                        .into());
                    };
                    let new_value_u8: Option<u8> =
                        new_value.as_int().map(|v| (v.cast_unsigned() & 0xFF) as u8);
                    let store_result: Result<(), (i64, usize)> = {
                        match self.get_object(array_object_index) {
                            Object::Array(arr) => {
                                let mut guard = arr.lock_mut();
                                let len = guard.len();
                                match crate::array_index::resolve_index(i, len) {
                                    Some(idx) => {
                                        guard[idx] = new_value;
                                        Ok(())
                                    }
                                    None => Err((i, len)),
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
                                match crate::array_index::resolve_index(i, len) {
                                    Some(idx) => {
                                        guard[idx] = byte_v;
                                        Ok(())
                                    }
                                    None => Err((i, len)),
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
                    match store_result {
                        Ok(()) => {}
                        Err((idx, len)) => {
                            return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                                VmPanic::IndexOutOfBounds {
                                    index: idx,
                                    length: len,
                                },
                            )));
                        }
                    }
                    self.heap.write_barrier(array_object_index, new_value);
                }

                OpCode::StoreMapElement => {
                    let new_value = self.stack.ensure_pop();
                    let key_value = self.stack.ensure_pop();
                    let map_value = self.stack.ensure_pop();
                    let key_index = self.as_object_ptr(key_value, ObjectType::String)?;
                    let key = self.get_object(key_index).as_string()?.clone();
                    let map_index = self.as_object_ptr(map_value, ObjectType::Map)?;
                    let store_result: Result<(), ObjectType> = {
                        match self.get_object(map_index) {
                            Object::Map(map) => {
                                let mut guard = map.lock_mut();
                                guard.insert(key, new_value);
                                Ok(())
                            }
                            other => Err(ObjectType::of(other)),
                        }
                    };
                    match store_result {
                        Ok(()) => {}
                        Err(got) => {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Map.into(),
                                got: got.into(),
                            }
                            .into());
                        }
                    }
                    self.heap.write_barrier(map_index, new_value);
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
                // Operands are statically known to be `int`, so we untag via
                // `as_int` and operate on raw `i64`. Every op is checked: a
                // result outside the i63 range throws a catchable
                // `baml.panics.IntegerOverflow` (via `int_arith_result`)
                // rather than silently wrapping (old `tagged_int_add`/`_sub`)
                // or raw-Rust-panicking (old `Value::int(l * r)`). The overflow
                // branch is cold and ~never taken, so the hot path is the
                // checked op plus one predicted-not-taken branch.
                OpCode::AddInt => {
                    let r = self.stack.ensure_pop();
                    let l = self.stack.ensure_pop();
                    // Overflow-checked tagged add: hot path stays branchless
                    // (no untag/retag), cold path untags only for the message.
                    match Value::tagged_int_add_checked(l, r) {
                        Some(v) => self.stack.push(v),
                        None => return Err(self.tagged_int_overflow(l, '+', r)),
                    }
                }
                OpCode::SubInt => {
                    let r = self.stack.ensure_pop();
                    let l = self.stack.ensure_pop();
                    match Value::tagged_int_sub_checked(l, r) {
                        Some(v) => self.stack.push(v),
                        None => return Err(self.tagged_int_overflow(l, '-', r)),
                    }
                }
                OpCode::MulInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = self.int_arith_result(l.checked_mul(r), l, '*', r)?;
                    self.stack.push(v);
                }
                OpCode::DivInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    if r == 0 {
                        return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::int(l),
                                right: Value::int(r),
                            },
                        )));
                    }
                    // r != 0 guaranteed above; `INT_MIN / -1` = 2^62 fits i64
                    // (INT_MIN is -2^62, not i64::MIN) but not i63, so the
                    // range-check in finish_int catches it.
                    let v = self.finish_int(l / r, l, '/', r)?;
                    self.stack.push(v);
                }
                OpCode::ModInt => {
                    let Some(r) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self.stack.ensure_pop().as_int() else {
                        std::hint::unreachable_unchecked()
                    };
                    if r == 0 {
                        return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: Value::int(l),
                                right: Value::int(r),
                            },
                        )));
                    }
                    // |l % r| < |r| <= 2^62, always within i63 range.
                    let v = self.finish_int(l % r, l, '%', r)?;
                    self.stack.push(v);
                }

                // ── Specialized float arithmetic (skip type dispatch) ─────────
                OpCode::AddFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = Value::object(self.alloc_float(l + r));
                    self.stack.push(v);
                }
                OpCode::SubFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = Value::object(self.alloc_float(l - r));
                    self.stack.push(v);
                }
                OpCode::MulFloat => {
                    let Some(r) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = value_as_float(self.stack.ensure_pop()) else {
                        std::hint::unreachable_unchecked()
                    };
                    let v = Value::object(self.alloc_float(l * r));
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
                        return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                            VmPanic::DivisionByZero {
                                left: left_v,
                                right: right_v,
                            },
                        )));
                    }
                    let v = Value::object(self.alloc_float(l / r));
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
                    // `!` negates TRUTHINESS (B-1563) - `!0` and `if (0)`
                    // agree, closing B-1071's asymmetry.
                    let val = self.stack.ensure_pop();
                    let truthy = self.is_truthy(val);
                    self.stack.push(Value::bool(!truthy));
                }
                OpCode::Truthy => {
                    let val = self.stack.ensure_pop();
                    let truthy = self.is_truthy(val);
                    self.stack.push(Value::bool(truthy));
                }
                OpCode::Neg => {
                    let val = self.stack.ensure_pop();
                    if let Some(n) = val.as_int() {
                        // Negating INT_MIN = -2^62 yields 2^62, which fits i64
                        // (INT_MIN != i64::MIN) but not i63, so the range-check
                        // in try_int catches it.
                        match Value::try_int(n.wrapping_neg()) {
                            Some(v) => self.stack.push(v),
                            None => {
                                return Err(self.integer_overflow(format!("-({n}) overflows int")));
                            }
                        }
                    } else if let Some(n) = value_as_float(val) {
                        let v = Value::object(self.alloc_float(-n));
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
                    // This is the innermost executing frame, so its live PC is
                    // `cur_pc` (the frame's `faulting_pc` is no longer updated
                    // per-op; it only holds outer frames' call-site PCs).
                    let cur_pc = self.cur_pc;
                    let source_location = if let Frame::Bytecode(bf) = &self.frames[*frame_idx] {
                        let pc = cur_pc;
                        let func_obj = self.get_object(bf.function).as_callable().ok();
                        func_obj
                            .and_then(|func| {
                                if let Some(compact) = &func.bytecode.compact {
                                    compact.line_entry_for_pc(pc)
                                } else {
                                    func.bytecode.line_entry_for_pc(pc)
                                }
                            })
                            .map(Self::event_source_location_for_line_entry)
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

        roots.extend(
            self.pending_call_captures
                .iter()
                .filter_map(|event| event.value.as_object_ptr()),
        );
        // Native generic calls have no bytecode frame while the builtin is
        // executing, so this is the only owner of their realized type heads.
        // A runtime declaration bound to T must remain live across any GC the
        // native performs or triggers through a continuation.
        for ty in &self.pending_call_type_args {
            ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
        for value in self.pending_call_type_values.iter().flatten() {
            value.ty.visit_heads(&mut |head| {
                if head.is_resolved() {
                    roots.push(head.ptr());
                }
            });
        }
        roots.extend(
            self.pending_error_captures
                .iter()
                .filter_map(|event| event.value.as_object_ptr()),
        );
        roots.extend(self.static_load_type_cache.values().copied());
        // Both key (thrown value) and cause context are heap pointers.
        for (value, cause) in &self.thrown_value_causes {
            roots.extend(value.as_object_ptr());
            roots.extend(cause.as_object_ptr());
        }
        for (value, context) in self
            .thrown_value_contexts
            .iter()
            .chain(&self.preserved_throw_contexts)
        {
            roots.extend(value.as_object_ptr());
            roots.extend(context.cause.as_object_ptr());
        }

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

        for event in &mut self.pending_call_captures {
            if let Some(ptr) = event.value.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                event.value = Value::object(new_ptr);
            }
        }
        for ty in &mut self.pending_call_type_args {
            ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        }
        for value in self.pending_call_type_values.iter_mut().flatten() {
            value.ty.visit_heads_mut(&mut |head| {
                if head.is_resolved()
                    && let Some(&moved) = roots.get(&head.ptr())
                {
                    head.forward_to(moved);
                }
            });
        }
        for event in &mut self.pending_error_captures {
            if let Some(ptr) = event.value.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                event.value = Value::object(new_ptr);
            }
        }
        // Forward a tagged runtime identity (`ptr | 1`) through the GC's
        // forwarding map. The probe key is rebuilt from the untagged address
        // and used only as a map key — `HeapPtr`'s `Eq`/`Hash` compare the
        // pointer alone (the `heap_debug` epoch is excluded), and the key is
        // never dereferenced. Probing `roots` directly keeps this O(1) per
        // cache entry: `forward_roots` runs once per permit holder per GC, so
        // materializing a by-address copy of the whole forwarding map here
        // multiplied every GC by the live thread count (a corpus-scale run
        // spent most of its wall clock inside that collect, world parked).
        // A tagged identity absent from the map keeps today's behavior: the
        // memo entry is dropped and re-derived from the authoritative
        // created-once table on the owning package.
        let forward_identity = |identity: usize| -> Option<usize> {
            if identity & 1 == 0 {
                return Some(identity);
            }
            let addr = (identity & !1) as *mut Object;
            // SAFETY: the pointer is used only as a lookup key; it is never
            // dereferenced, so validity of the pointee is not required.
            #[cfg(not(feature = "heap_debug"))]
            let key = unsafe { HeapPtr::from_ptr(addr) };
            // SAFETY: as above; the epoch does not participate in `Eq`/`Hash`,
            // so a zero epoch probes correctly.
            #[cfg(feature = "heap_debug")]
            let key = unsafe { HeapPtr::from_ptr(addr, 0) };
            roots
                .get(&key)
                .map(|forwarded| (forwarded.as_ptr() as usize) | 1)
        };
        let old_load_type_cache = std::mem::take(&mut self.static_load_type_cache);
        for ((identity, index), ptr) in old_load_type_cache {
            if let Some(identity) = forward_identity(identity) {
                self.static_load_type_cache
                    .insert((identity, index), roots.get(&ptr).copied().unwrap_or(ptr));
            }
        }
        let old_virtual_call_cache = std::mem::take(&mut self.static_virtual_call_cache);
        for (mut key, target) in old_virtual_call_cache {
            if let Some(identity) = forward_identity(key.caller_function_addr) {
                key.caller_function_addr = identity;
                self.static_virtual_call_cache.insert(key, target);
            }
        }
        for (value, cause) in &mut self.thrown_value_causes {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                *value = Value::object(new_ptr);
            }
            if let Some(ptr) = cause.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                *cause = Value::object(new_ptr);
            }
        }
        for (value, context) in self
            .thrown_value_contexts
            .iter_mut()
            .chain(&mut self.preserved_throw_contexts)
        {
            if let Some(ptr) = value.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                *value = Value::object(new_ptr);
            }
            if let Some(ptr) = context.cause.as_object_ptr()
                && let Some(&new_ptr) = roots.get(&ptr)
            {
                context.cause = Value::object(new_ptr);
            }
        }

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
