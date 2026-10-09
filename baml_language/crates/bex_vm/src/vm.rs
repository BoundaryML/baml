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

use baml_type::{Int63, IntShiftError, Name, normalize::TypeContext};
use btel_types::context::{Context, ContextPatch};
use smallvec::SmallVec;

use crate::{
    telemetry::{
        CallTypeArgs, ClockDuration, ClockInstant, FrameTelemetry, InvocationOutcome,
        TelemetryState,
    },
    vec_ext::{CopyVecExt, VecExt},
};

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
use bex_heap::{BexHeap, Tlab};
use bex_vm_types::{
    BinOp, CmpOp, FunctionKind, FutureRead, GlobalIndex, HeapPtr, Object, ObjectIndex, ObjectPool,
    ObjectType, PanicClass, PermitProof, RustDataArc as _, StackIndex, UnaryOp, Value, Variant,
    VmGlobals,
    bytecode::{self, Instruction},
    types::{
        BoundMethod, Closure, ConstValue, Function, FunctionOrigin, FunctionType, Instance, Type,
        TypeValue,
    },
};
use indexmap::IndexMap;

use crate::{
    errors::{StackFrame, VmBamlError, VmError, VmInternalError, VmPanic, VmRustFnError, VmThrown},
    indexable::{EvalStack, EvalStackTrait},
    package_baml::{NativeCallResult, NativeFunction},
    types::ObjectTrait,
};

/// Mutable access to a heap object through [`BexVm::get_object_mut`], with
/// the mutation metered.
///
/// Measures what the object keeps alive outside its slot when taken and again
/// when dropped, and charges the difference to the VM's allocation account.
/// It adds nothing to the access itself: that the object is not reachable from
/// anywhere else meanwhile is `get_object_mut`'s contract, not this guard's.
pub struct ObjectMut<'a> {
    object: &'a mut Object,
    debt: &'a bex_vm_types::AllocDebt,
    before: usize,
}

impl ObjectMut<'_> {
    fn footprint(object: &mut Object) -> usize {
        let mut meter = bex_vm_types::Meter::charge();
        object.measure(&mut meter);
        meter.total()
    }
}

impl std::ops::Deref for ObjectMut<'_> {
    type Target = Object;
    fn deref(&self) -> &Object {
        self.object
    }
}

impl std::ops::DerefMut for ObjectMut<'_> {
    fn deref_mut(&mut self) -> &mut Object {
        self.object
    }
}

impl Drop for ObjectMut<'_> {
    fn drop(&mut self) {
        let after = Self::footprint(self.object);
        let before = self.before;
        let delta = if after >= before {
            isize::try_from(after - before).unwrap_or(isize::MAX)
        } else {
            isize::try_from(before - after).map_or(isize::MIN, |shrink| -shrink)
        };
        self.debt.add(delta);
    }
}

/// Max call stack size.
pub const MAX_FRAMES: usize = 256;

#[derive(Clone, Copy)]
struct CallOptions<'a> {
    type_args: &'a [bex_vm_types::RealizedTy],
    type_values: &'a [Option<TypeValue>],
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
    /// Producer-only invocation state. `None` means the invocation is hidden.
    pub(crate) telemetry: Option<FrameTelemetry>,
    context: Context,
}

struct CallTrace {
    telemetry: crate::telemetry::TraceConfig,
    context: Option<Arc<ContextPatch>>,
}

/// The target's existing frame waits here while its declaration hook runs.
/// Contains no heap pointers: arguments and callable/type metadata stay rooted
/// in the ordinary frame and evaluation stack across suspension and GC.
struct PendingTraceHook {
    frame: usize,
    caller: Option<usize>,
    caller_pc: u32,
    trace: Option<CallTrace>,
    evaluating: bool,
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
    /// Nearest bytecode caller. Hidden native continuations are transparent to
    /// call-path attribution and await ownership; this avoids walking ancestors.
    pub(crate) bytecode_caller: usize,
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
        EarlyYieldCheck, FunctionKind, GlobalPool, HeapPtr, Object, ObjectIndex, RootHaver, Value,
        ValueKind, VmGlobals,
        bytecode::Bytecode,
        types::{BoundMethod, Closure, Function, FunctionOrigin, TypeValue, type_tags},
    };

    use super::{
        BexVm, Frame, FrameTypeMetadata, TakenTypeArgs, VmExecState,
        append_virtual_method_type_args, value_type_tag,
    };
    use crate::{
        indexable::EvalStack,
        package_baml::{NativeCallResult, NativeFunction},
        telemetry::{TelemetryPolicies, TelemetryState},
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
            stdlib_heads: Arc::new(crate::package_load::StdlibHeads::default()),

            thread_id: 0,

            context_transfers: Vec::new(),
            argv: Arc::from([]),
            pending_call_trace: None,
            pending_trace_hooks: Vec::new(),
            inherited_hook_suppression: false,
            pending_host_trace: None,
            entry_trace: None,
            entry_trace_context: None,
            root_context: btel_types::context::Context::default(),
            pending_call_type_args: Vec::new(),
            pending_call_type_values: Vec::new(),
            pending_call_passes_type_args: false,
            static_load_type_cache: HashMap::new(),
            static_virtual_call_cache: HashMap::new(),
            telemetry: Some(TelemetryState::new_root(
                Arc::new(TelemetryPolicies::new()),
                btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
                #[cfg(not(target_arch = "wasm32"))]
                crate::telemetry::test_runtime(),
            )),
            trace_scope: btel_types::RecordingId::generate(),
            pending_telemetry_wait: None,
            packages: Arc::new(crate::package_load::PackageIndex::default()),
            dynamic_dispatch: Arc::new(crate::package_load::DynDispatchTables::default()),
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
            docstring: None,
            declared_name: None,
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Native(native as *const ()),
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type: bex_vm_types::TyTemplate::Int,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type: "int".to_string(),
            throws_type: bex_vm_types::TyTemplate::Never,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,

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
    fn public_span_identity_uses_the_emitted_local_id() {
        use bex_vm_types::{ConstValue, bytecode::Instruction, trace::SpanId};

        use crate::telemetry::{SpanRecord, TelemetryPolicy};

        let Object::Function(mut function) = native_function_object() else {
            unreachable!()
        };
        function.kind = FunctionKind::Bytecode;
        function.bytecode = Bytecode {
            instructions: vec![Instruction::LoadConst(0), Instruction::Return],
            constants: vec![ConstValue::Int(42)],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        let mut vm = test_vm(vec![Object::Function(function)]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        let Object::Function(function) = vm.get_object(entry) else {
            unreachable!()
        };
        vm.telemetry
            .as_ref()
            .unwrap()
            .set_policy(
                function,
                TelemetryPolicy {
                    span_from_entry: true,
                    ..TelemetryPolicy::NONE
                },
            )
            .unwrap();
        vm.set_entry_point(entry, &[]);
        let public_id = vm.current_span_id().unwrap();
        assert!(
            matches!(vm.exec().unwrap(), VmExecState::Complete(value) if value == Value::int(42))
        );
        let local_id = vm
            .telemetry
            .as_ref()
            .unwrap()
            .span_records()
            .iter()
            .find_map(|record| match record {
                SpanRecord::FunctionSpanCompletionOk { id, .. } => Some(*id),
                _ => None,
            })
            .expect("completed span");
        assert_eq!(public_id, SpanId::new(vm.trace_scope, local_id));
    }

    #[test]
    fn network_span_wrappers_record_through_the_vm_telemetry() {
        use crate::telemetry::{ClockInstant, InvocationOutcome, SpanRecord};

        let mut vm = test_vm(Vec::new());
        let request = sys_types::network::NetworkRequest {
            method: "GET".to_owned(),
            url: "https://example.com/".to_owned(),
            ..Default::default()
        };
        let span = vm.open_network_span(&request).unwrap();
        vm.network_event(
            span,
            ClockInstant::from_ticks(1),
            &sys_types::network::NetworkEventKind::Close,
        );
        // Bare test VMs have no error classes: the error is captured as is,
        // but never quoting the raw URL or its query.
        let raw = "https://example.com/v1?key=secret";
        let message = format!("failed for url ({raw}), query key=secret");
        let error = vm.tlab.alloc_string(message.as_str());
        vm.close_network_span(
            span,
            ClockInstant::from_ticks(2),
            InvocationOutcome::Errored,
            Some(Value::object(error)),
            &[raw],
        );
        let records = vm.telemetry.as_ref().unwrap().span_records();
        let [
            SpanRecord::ThreadSpanAnnouncement { .. },
            SpanRecord::NetworkSpanAnnouncement(announcement),
            SpanRecord::NetworkEvent(close),
            SpanRecord::NetworkSpanCompletion(completion),
        ] = records
        else {
            panic!("unexpected records: {records:?}");
        };
        assert_eq!(announcement.id, span);
        assert_eq!(&*announcement.url, "https://example.com/");
        assert_eq!(close.name, btel_records::NetworkEventName::Close);
        let captured = completion.error.as_ref().unwrap();
        let Some(btel_snapshot::SnapshotValue::String(text)) = captured.value() else {
            panic!("the error is a string");
        };
        let hashed = crate::telemetry::network_hash_for_tests("secret");
        assert_eq!(
            captured.string(text).as_str(),
            format!("failed for url (https://example.com/v1?key={hashed}), query key={hashed}")
        );
        // The program's value is untouched.
        assert_eq!(
            vm.as_string(&Value::object(error)).unwrap().as_str(),
            message
        );
        vm.telemetry = None;
        assert_eq!(vm.open_network_span(&request), None);
    }

    #[test]
    fn hidden_native_execution_has_only_thread_events() {
        use crate::telemetry::{InvocationOutcome, SpanRecord};

        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);
        assert!(vm.frames.iter().all(|frame| match frame {
            Frame::Bytecode(frame) => frame.telemetry.is_none(),
            Frame::Native(_) => true,
        }));
        assert!(
            matches!(vm.exec().unwrap(), VmExecState::Complete(value) if value == Value::int(42))
        );
        vm.finish_telemetry(InvocationOutcome::Ok);
        assert!(vm.telemetry.as_ref().unwrap().timing_records().is_empty());
        assert!(matches!(
            vm.telemetry.as_ref().unwrap().span_records(),
            [
                SpanRecord::ThreadSpanAnnouncement { .. },
                SpanRecord::ThreadSpanCompletion {
                    outcome: InvocationOutcome::Ok,
                    ..
                }
            ]
        ));
    }

    // Invalid instruction ordering cannot be exercised from BAML source.
    #[cfg(all(debug_assertions, not(target_arch = "wasm32")))]
    #[test]
    #[should_panic(expected = "SetCallTrace must be followed by a call instruction")]
    fn trace_prefix_rejects_a_non_call_instruction() {
        use bex_vm_types::bytecode::Instruction;

        let Object::Function(mut function) = native_function_object() else {
            unreachable!()
        };
        function.kind = FunctionKind::Bytecode;
        function.bytecode = Bytecode {
            instructions: vec![Instruction::SetCallTrace, Instruction::Return],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        let mut vm = test_vm(vec![Object::Function(function)]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.set_entry_point(entry, &[]);
        let _ = vm.exec();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn hidden_native_callbacks_preserve_visible_paths_and_reentry() {
        use crate::{
            package_baml::{Continuation, PassThroughContinuation},
            telemetry::{SpanRecord, TimingRecord},
        };

        struct Again(HeapPtr);
        impl Continuation for Again {
            fn call(self: Box<Self>, _vm: &mut BexVm, _value: Value) -> NativeCallResult {
                NativeCallResult::YieldToCall {
                    callee: self.0,
                    args: vec![Value::bool(false)],
                    type_args: vec![],
                    continuation: Box::new(PassThroughContinuation),
                }
            }
            fn gc_roots(&self) -> Vec<HeapPtr> {
                vec![self.0]
            }
            fn apply_forwarding(&mut self, roots: &HashMap<HeapPtr, HeapPtr>) {
                self.0 = roots.get(&self.0).copied().unwrap_or(self.0);
            }
        }
        fn invoke_twice(_vm: &mut BexVm, args: &[Value]) -> NativeCallResult {
            let callee = args[0].as_object_ptr().unwrap();
            NativeCallResult::YieldToCall {
                callee,
                args: vec![Value::bool(false)],
                type_args: vec![],
                continuation: Box::new(Again(callee)),
            }
        }

        // Producer events and compiler origin are invisible to a BAML test.
        // Use real compiled lambdas and both first/resumed native callbacks.
        let mut program = baml_test_support::compile_source(
            r#"
            function Invoke(f: (bool) -> int) -> int { f(false) }
            function F(again: bool) -> int { if (again) { Invoke(F) } else { 7 } }
            function G(again: bool) -> int { 17 }
            function Main() -> int {
                let local = (again: bool) => { 11 };
                F(true) + Invoke(G) + Invoke(local)
            }
        "#,
        );
        let entry = program.rendered_callables()["user.Main"].object.raw();
        let invoke = program.rendered_callables()["user.Invoke"].object.raw();
        let Object::Function(function) = &mut program.objects[ObjectIndex::from_raw(invoke)] else {
            unreachable!()
        };
        function.kind = FunctionKind::Native(invoke_twice as NativeFunction as *const ());
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => {}
                VmExecState::Complete(value) => {
                    assert_eq!(value, Value::int(35));
                    break;
                }
                other => panic!("unexpected execution state: {other:?}"),
            }
        }
        let timings: Vec<_> = vm
            .telemetry
            .as_ref()
            .unwrap()
            .timing_records()
            .iter()
            .filter_map(|event| match event {
                TimingRecord::FunctionTimingCompletion {
                    call_path, reentry, ..
                } => Some((*call_path, *reentry)),
                TimingRecord::ThreadSelected { .. } | TimingRecord::SysOpTime { .. } => None,
            })
            .collect();
        assert_eq!(
            timings.len(),
            8,
            "F, G, and the compiled lambda must all be observed"
        );
        assert!(timings[0].1 && timings[1].1 && !timings[2].1);
        assert_eq!(timings[0].0, timings[2].0);
        assert_eq!(
            timings[1].0, timings[2].0,
            "resumed callbacks also fold as visible reentry"
        );
        assert_eq!(
            timings[3], timings[4],
            "both G callbacks have the same visible call site"
        );
        assert_eq!(
            timings[5], timings[6],
            "both lambda callbacks have the same visible call site"
        );
        assert!(timings[3..].iter().all(|(_, reentry)| !reentry));
        let mut named_paths = HashMap::new();
        for event in vm.telemetry.as_ref().unwrap().span_records() {
            if let SpanRecord::CallPathDefined {
                call_path,
                visible_caller,
                callee,
                ..
            } = event
            {
                let callee = vm.heap.function_metadata(vm.proof(), *callee).unwrap();
                let caller =
                    visible_caller.map(|id| vm.heap.function_metadata(vm.proof(), id).unwrap().fqn);
                named_paths.insert(*call_path, (caller, callee.fqn));
            }
        }
        // Definitions carry registered IDs for the authored functions, never
        // the hidden native Invoke wrapper, including both lambda callbacks.
        assert_eq!(
            named_paths[&timings[0].0],
            (Some("user.Main".into()), "user.F".into())
        );
        assert_eq!(
            named_paths[&timings[3].0],
            (Some("user.Main".into()), "user.G".into())
        );
        assert_eq!(named_paths[&timings[5].0].0.as_deref(), Some("user.Main"));
        assert!(named_paths[&timings[5].0].1.contains("<lambda"));
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn await_and_async_sysop_start_waits_at_their_semantic_boundaries() {
        use bex_vm_types::{Future, RealizedTy, types::FutureId};

        use crate::telemetry::{ClockDuration, TimingRecord};

        for sysop in [false, true] {
            for pending in [false, true] {
                let body = if sysop {
                    "baml.sys.sleep(baml.time.Duration.from_nanoseconds(0n)); 7"
                } else {
                    "await f"
                };
                let program = baml_test_support::compile_source(&format!(
                    "function Main(f: baml.future.Future<int, never>) -> int {{ {body} }}"
                ));
                let entry = program.rendered_callables()["user.Main"].object.raw();
                let mut vm =
                    BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
                let future = vm.tlab.alloc(Object::Future(Future::pending(
                    FutureId::from_usize(0),
                    RealizedTy::int(),
                    RealizedTy::never(),
                    tokio_util::sync::CancellationToken::new(),
                )));
                let settle = |vm: &BexVm| {
                    let Object::Future(value) = vm.get_object(future) else {
                        unreachable!()
                    };
                    // This isolated VM owns the heap; no collector runs in this test.
                    assert!(unsafe { value.settle_ready(vm.heap.as_ref(), future, Value::int(7)) });
                };
                if !sysop && !pending {
                    settle(&vm);
                }
                vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[Value::object(future)]);
                assert!(
                    vm.pending_telemetry_wait.is_none(),
                    "future creation is not an await"
                );
                let mut state = vm.exec().unwrap();
                if sysop {
                    assert!(matches!(state, VmExecState::SysOp { .. }));
                    assert!(
                        vm.pending_telemetry_wait.is_none(),
                        "yielding a sys-op is not an await"
                    );
                    if pending {
                        // Only the engine's Async arm performs this step.
                        vm.begin_sys_op_telemetry_wait();
                    }
                } else if pending {
                    assert!(matches!(state, VmExecState::Await(_)));
                }
                if pending {
                    let (owner, _) = vm
                        .take_telemetry_wait()
                        .expect("suspension starts the wait");
                    assert!(
                        vm.take_telemetry_wait().is_none(),
                        "wait ownership is transferred once"
                    );
                    // Simulate the engine reading the end clock before reacquisition.
                    vm.record_telemetry_wait(Some(owner), ClockDuration::from_ticks(7));
                }
                if sysop {
                    vm.stack.push(Value::NULL);
                    state = vm.exec().unwrap();
                } else if pending {
                    settle(&vm);
                    state = vm.exec().unwrap();
                }
                assert!(matches!(state, VmExecState::Complete(value) if value == Value::int(7)));
                assert!(vm.pending_telemetry_wait.is_none());
                let waits: Vec<_> = vm
                    .telemetry
                    .as_ref()
                    .unwrap()
                    .timing_records()
                    .iter()
                    .filter_map(|event| {
                        if let TimingRecord::FunctionTimingCompletion { await_time, .. } = event {
                            Some(await_time.get().get())
                        } else {
                            None
                        }
                    })
                    .collect();
                assert_eq!(waits.iter().sum::<u64>(), if pending { 7 } else { 0 });
                assert_eq!(
                    waits.iter().filter(|&&ticks| ticks != 0).count(),
                    usize::from(pending)
                );
            }
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn gc_polling_progress_survives_engine_handoffs() {
        use bex_vm_types::{ConstValue, bytecode::Instruction};

        let Object::Function(mut function) = native_function_object() else {
            unreachable!()
        };
        function.kind = FunctionKind::Bytecode;
        // Every iteration hands control to the engine before the backedge.
        // No single exec call reaches the polling interval on its own.
        function.bytecode = Bytecode {
            instructions: vec![
                Instruction::LoadConst(0),
                Instruction::LoadConst(1),
                Instruction::SendEvent,
                Instruction::Jump(-3),
            ],
            constants: vec![
                ConstValue::Object(ObjectIndex::from_raw(1)),
                ConstValue::Int(42),
            ],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        let mut vm = test_vm(vec![
            Object::Function(function),
            Object::String("tick".into()),
        ]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.set_entry_point(entry, &[]);
        vm.early_yield = EarlyYieldCheck::with_interval(Arc::new(AtomicBool::new(true)), 3);

        for _ in 0..3 {
            assert!(matches!(vm.exec().unwrap(), VmExecState::Event { .. }));
        }
        assert!(matches!(vm.exec().unwrap(), VmExecState::EarlyYield));
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
        use crate::package_baml::{BamlPackageBaml, PackageBamlImpl};

        let (mut vm, native_ptr) = vm_with_native_entry();

        vm.set_entry_point(native_ptr, &[]);
        let trampoline = trampoline_ptr(&vm);

        let Object::Function(function) = vm.get_object(trampoline) else {
            unreachable!()
        };
        assert_eq!(function.telemetry_function_id, None);
        assert!(
            vm.telemetry
                .as_ref()
                .unwrap()
                .set_policy(function, crate::telemetry::TelemetryPolicy::NONE)
                .is_err()
        );
        let cloned = function.clone();
        assert_eq!(
            cloned.telemetry_function_id, None,
            "GC preserves unsupported capability"
        );
        let copy = PackageBamlImpl::deep_copy(&mut vm, &Value::object(trampoline)).unwrap();
        let Object::Function(copy) = vm.get_object(copy.as_object_ptr().unwrap()) else {
            unreachable!()
        };
        assert_eq!(
            copy.telemetry_function_id, None,
            "copying cannot enable telemetry"
        );

        // A real function copy gets a fresh ID and resets the cloned
        // registration flag (the source is already statically registered).
        let Object::Function(original) = vm.get_object(native_ptr) else {
            unreachable!()
        };
        let original_id = original.telemetry_function_id.unwrap();
        let copied = PackageBamlImpl::deep_copy(&mut vm, &Value::object(native_ptr)).unwrap();
        let copied_ptr = copied.as_object_ptr().unwrap();
        let Object::Function(copied) = vm.get_object(copied_ptr) else {
            unreachable!()
        };
        let copied_id = copied.telemetry_function_id.unwrap();
        assert_ne!(copied_id, original_id);
        assert!(vm.heap.function_metadata(vm.proof(), copied_id).is_none());
        // SAFETY: the running test VM excludes GC and the copy is fully linked.
        unsafe {
            assert_eq!(
                vm.heap.register_telemetry_function(copied_ptr),
                Some(copied_id)
            );
        }
        assert!(vm.heap.function_metadata(vm.proof(), copied_id).is_some());

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
            matches!(vm.get_object(moved_trampoline), Object::Function(f) if f.name == "$entry::test_native" && f.telemetry_function_id.is_none()),
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
            stream_done: false,
            type_tag: tag,
            has_cleanup: false,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
        })));
        let bound = TypeValue::new(bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
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
            stream_done: false,
            type_tag: tag,
            has_cleanup: false,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
        })));
        let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.type_args = vec![bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
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
            stream_done: false,
            type_tag: tag,
            has_cleanup: false,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
        })));
        vm.pending_call_type_args = vec![bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
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
        let exact = TypeValue::new(bex_vm_types::RealizedTy::Class(head, Box::new([])));

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

/// The `baml.errors.Context` of an error the unwinder is carrying, until a
/// handler lands it.
enum InFlightContext {
    /// An error that already has its context: a caught error raised again, an
    /// error awaited from another task, or a conversion of the error a handler
    /// is handling.
    Carried(Value),
    /// A new failure's trace and cause. The `Context` object is built only if
    /// a handler lands it.
    Fresh {
        trace: Arc<[StackFrame]>,
        cause: Value,
    },
}

/// A conversion (`UnknownError.from` / `with_message`) of the error a handler
/// is handling: the next new throw of `target` from inside that handler takes
/// over the handled error's trace and cause, as if it were that error.
struct ContextTransfer {
    /// The handler's `baml.errors.Context`, which identifies the handled error.
    handling: Value,
    /// The value the conversion produced.
    target: Value,
}

mod error_evidence;
use error_evidence::RaiseEntry;

/// The parameter list a callable value executes with; see
/// [`BexVm::callee_params`].
enum CalleeParams<'a> {
    Function(&'a Function),
    Host(&'a bex_vm_types::HostClosure),
}

impl CalleeParams<'_> {
    fn arity(&self) -> usize {
        match self {
            Self::Function(function) => function.arity,
            Self::Host(host) => host.arity,
        }
    }

    fn layout(&self) -> baml_type::CallLayout {
        match self {
            Self::Function(function) => function.argument_layout(),
            Self::Host(host) => baml_type::CallLayout::from_modes(
                host.params
                    .iter()
                    .map(|param| (param.name.clone(), param.mode)),
            ),
        }
    }

    fn layout_is(&self, layout: &baml_type::CallLayout) -> bool {
        match self {
            Self::Function(function) => function.argument_layout_is(layout),
            Self::Host(_) => self.layout() == *layout,
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

    /// The stdlib declarations the runtime recognizes structurally, resolved
    /// once from `packages` and shared across spawned VMs.
    stdlib_heads: Arc<crate::package_load::StdlibHeads>,

    /// Engine-local logical thread identity for structured logs.
    pub thread_id: u64,

    /// VM-owned producer state. It deliberately has no transport or buffer.
    telemetry: Option<TelemetryState>,
    pub(crate) trace_scope: btel_types::RecordingId,

    /// Starts at VM Await suspension, or when the engine receives an async
    /// sys-op. Taken across the heap-permit release; lookup failures leave it
    /// here for the explicit completion boundary to finish.
    pending_telemetry_wait: Option<(usize, ClockInstant)>,

    /// Context transfers registered by the conversions that ran inside a
    /// handler still active on this thread (see [`ContextTransfer`]). Stale
    /// entries, whose handler has finished without throwing the converted
    /// value, are dropped at the next registration.
    context_transfers: Vec<ContextTransfer>,

    /// Process arguments exposed by `baml.sys.argv()`. Shared across VMs.
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
    pending_call_trace: Option<CallTrace>,
    pending_trace_hooks: Vec<PendingTraceHook>,
    /// Inherited by a spawned BAML task independently of the parent's lifetime.
    inherited_hook_suppression: bool,
    pending_host_trace: Option<CallTrace>,
    entry_trace: Option<crate::telemetry::TraceConfig>,
    entry_trace_context: Option<Arc<ContextPatch>>,
    root_context: Context,
    pending_call_type_args: Vec<bex_vm_types::RealizedTy>,
    pending_call_type_values: Vec<Option<TypeValue>>,
    /// Set by a call that passes type arguments, for its callee's entry
    /// only: telemetry records the frame's type arguments before the call's
    /// writeback appends `pending_call_type_args` to the new frame.
    pending_call_passes_type_args: bool,

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

    /// VM yields a `spawn` to the engine.
    ///
    /// - Input: `plan` points at the `baml.spawn.Plan<T, E>` instance to
    ///   start; [`crate::package_baml::spawn_launch`] reads what it describes.
    /// - Output: the engine allocates the pending future at the plan's types,
    ///   starts the task on a new `BexThread`, and pushes the future pointer
    ///   onto the VM stack. Terminal transitions (`Ready`/`Error`/`Cancelled`/
    ///   `InternalError`) happen later via the `FutureManager`; the VM only
    ///   ever sees `Pending` directly after this yield.
    Spawn {
        plan: HeapPtr,
        telemetry: Option<crate::telemetry::ThreadSpawnContext>,
        context: Context,
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

    /// A decoded structured log. The VM has already handed its recording
    /// snapshot to Btel; the engine can still deliver existing live output.
    Log {
        level: btel_records::LogLevel,
        event_name: Option<Arc<str>>,
        data: Value,
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
    /// The executable's callables by rendered name, derived from the package
    /// tables ([`bex_vm_types::Program::rendered_callables`]): the view behind
    /// every surface that starts from a host-supplied name.
    pub resolved_function_names: HashMap<String, (ObjectIndex, FunctionKind)>,
    /// Per-package program structure (global-index-keyed). The loader allocates
    /// the heap `Object::Package` / `Object::ImplRule` objects and the
    /// `vm.packages` index from this, resolving each `ObjectIndex` to a
    /// compile-time `HeapPtr`.
    pub packages: Vec<bex_vm_types::types::ProgramPackage>,
    /// The world's root package as an ordinal into `packages`: the viewpoint
    /// every host-supplied name resolves from.
    pub root: u32,
}

/// Convert a compiled `Program` to a `BytecodeProgram` with native functions attached.
///
/// This is the bridge between compilation output and VM execution. It:
/// 1. Attaches native function implementations to builtin functions
/// 2. Builds resolved name lookups for functions, classes, and enums
pub fn convert_program(program: bex_vm_types::Program) -> Result<BytecodeProgram, VmInternalError> {
    convert_program_with_trace_hooks(program, true)
}

/// Select the immutable execution image before any frame can enter it.
pub fn convert_program_with_trace_hooks(
    program: bex_vm_types::Program,
    enabled: bool,
) -> Result<BytecodeProgram, VmInternalError> {
    // The one road from an executable into a VM: a decoded program is
    // checked against the format's laws here, and everything after — the
    // rendered views, the loader — indexes into a program that keeps them.
    program.validate()?;
    let callables = program.rendered_callables();
    // Convert objects, attaching native functions
    let mut objects: Vec<Object> = program
        .objects
        .into_iter()
        .map(crate::package_baml::attach_builtins)
        .collect::<Result<Vec<_>, _>>()?;

    if !enabled {
        disable_declared_trace_hooks(&mut objects);
    }
    prepare_compact_code(&mut objects, &program.globals);

    // The by-name view of the executable's callables, from the package
    // tables: declared functions and class methods only. Lambdas, helpers,
    // and interface bodies own no name a host can start from.
    let resolved_function_names = callables
        .into_iter()
        .map(|(name, callable)| {
            let Object::Function(function) = &objects[callable.object.raw()] else {
                unreachable!("a rendered callable is a function object")
            };
            (name, (callable.object, function.kind))
        })
        .collect();

    Ok(BytecodeProgram {
        objects: ObjectPool::from_vec(objects),
        globals: program.globals,
        resolved_function_names,
        packages: program.packages,
        root: program.root,
    })
}

/// Telemetry-off engines keep argument defaults and the authored body but
/// bypass the hook prologue. Instruction indices stay stable; compact PCs are
/// rebuilt afterwards, before there are any frames or suspended continuations.
fn disable_declared_trace_hooks(objects: &mut [Object]) {
    for object in objects {
        let Object::Function(function) = object else {
            continue;
        };
        let instructions = &mut function.bytecode.instructions;
        if let Some(begin) = instructions
            .iter()
            .position(|instruction| matches!(instruction, Instruction::BeginTraceHook(_)))
        {
            let finish = instructions
                .iter()
                .position(|instruction| matches!(instruction, Instruction::EndTraceHook))
                .expect("hook decision boundary");
            let offset = isize::try_from(finish + 1 - begin).expect("hook prologue fits a jump");
            instructions[begin] = Instruction::Jump(offset);
            // An unreachable no-op replaces the end marker, so all entry paths
            // recognize this callee as ordinary and start no pending hook.
            instructions[finish] = Instruction::Pop(0);
        }
        for instruction in instructions {
            if matches!(
                instruction,
                Instruction::TraceHookHidden
                    | Instruction::TraceHookTiming
                    | Instruction::TraceHookSpan
                    | Instruction::TraceHookRich
                    | Instruction::TraceHookEmptySpan
            ) {
                *instruction = Instruction::Pop(0);
            }
            if let Instruction::CallHooked { callee, ntypeargs } = *instruction {
                *instruction = Instruction::Call { callee, ntypeargs };
            }
        }
    }
}

/// Rebuild executable streams after loader changes (including float boxing),
/// then specialize calls against that final image. The serializable instruction
/// streams remain unchanged. Always rebuild before selecting fast paths so a
/// previous specialization cannot outlive a change to constants or globals.
pub fn prepare_compact_code(objects: &mut [Object], globals: &[bex_vm_types::ConstValue]) {
    for object in objects.iter_mut() {
        if let Object::Function(function) = object {
            function.bytecode.compact = Some(function.bytecode.lower_to_compact());
        }
    }
    crate::call_specialize::specialize_calls(objects, globals);
}

/// Resolve the heap pointers for the builtin `baml.errors.*` classes, indexed by
/// [`ErrorClass`] discriminant. The result is identical for every VM sharing a
/// `packages` index, so it is resolved once and shared (`Arc`) across spawns
/// rather than re-resolved per [`BexVm::new`].
///
/// One extra entry follows the error classes, at `DURATION_CLASS_PTR_INDEX`:
/// `baml.time.Duration`, which `baml.errors.Timeout.duration` is an instance of.
pub fn resolve_error_class_ptrs(packages: &crate::package_load::PackageIndex) -> Arc<[HeapPtr]> {
    ErrorClass::ALL
        .iter()
        .map(ErrorClass::fqn)
        .chain(std::iter::once("baml.time.Duration"))
        .map(|fqn| {
            crate::package_load::lookup_type_by_fqn(packages, fqn)
                .unwrap_or_else(|| panic!("error class {fqn:?} not in packages"))
        })
        .collect()
}

/// Where `baml.time.Duration` sits in the error class table: right after the
/// last [`ErrorClass`].
const DURATION_CLASS_PTR_INDEX: usize = ErrorClass::ALL.len();

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
    use baml_type::FunctionParamMode;
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
                Object::Enum(_) => type_tags::ENUM,
                Object::RustData(_) => type_tags::UNKNOWN,
                Object::Type(_) => type_tags::TYPE,
                Object::Class(_) => type_tags::UNKNOWN,
                Object::TypeAlias(_) => type_tags::UNKNOWN,
                Object::Interface(_) => type_tags::UNKNOWN,
                Object::Package(_) => type_tags::UNKNOWN,
                Object::ImplRule(_) => type_tags::UNKNOWN,
                Object::Tombstone => Object::tombstone_reached(),
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
        stdlib_heads: Arc<crate::package_load::StdlibHeads>,
        telemetry: Option<TelemetryState>,
        trace_scope: btel_types::RecordingId,
    ) -> Self {
        // Defer the first TLAB chunk reservation until the first `tlab.alloc`,
        // which the engine reaches only after the VM has been registered as a
        // permit holder via `HeapPermitManager::new_permit` and a permit is
        // active. Eagerly calling `Tlab::new` here would reserve a chunk
        // *before* registration, leaving the cursor stale across any GC that
        // fires in the engine's pre-permit window.
        let tlab = Tlab::new_empty(Arc::clone(&heap));

        // `error_class_ptrs` / `panic_class_ptrs` / `stdlib_heads` are resolved
        // once from `packages` by the caller (`resolve_error_class_ptrs` /
        // `resolve_panic_class_ptrs` / `StdlibHeads::resolve`) and shared
        // across spawned VMs.

        let early_yield = EarlyYieldCheck::new(
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
        );

        let early_yield =
            early_yield.with_gc_pressure(heap.gc_pressure(), bex_heap::gc_policy::POLL_INTERVAL);

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
            stdlib_heads,

            thread_id: 0,

            context_transfers: Vec::new(),
            argv,
            pending_call_trace: None,
            pending_trace_hooks: Vec::new(),
            inherited_hook_suppression: false,
            pending_host_trace: None,
            entry_trace: None,
            entry_trace_context: None,
            root_context: Context::default(),
            pending_call_type_args: Vec::new(),
            pending_call_type_values: Vec::new(),
            pending_call_passes_type_args: false,
            static_load_type_cache: HashMap::new(),
            static_virtual_call_cache: HashMap::new(),
            telemetry,
            trace_scope,
            pending_telemetry_wait: None,
            packages,
            dynamic_dispatch,
        }
    }

    /// Materialize a new executable object. Imported pointers bypass this;
    /// moving GC preserves the existing identity without re-registration.
    pub(crate) fn alloc_runtime_object(
        &mut self,
        object: Object,
    ) -> Result<HeapPtr, VmInternalError> {
        match object {
            Object::Function(function) => Ok(self.tlab.alloc_function(function)?),
            other => Ok(self.tlab.alloc(other)),
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

    pub(crate) fn current_span_id(&self) -> Option<bex_vm_types::trace::SpanId> {
        self.telemetry.as_ref()?;
        let Frame::Bytecode(frame) = self.frames.last()? else {
            return None;
        };
        frame
            .telemetry?
            .span_id()
            .map(|local| bex_vm_types::trace::SpanId::new(self.trace_scope, local))
    }

    fn trace_attachment_error(&mut self, message: &str) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::UserPanic {
            message: message.to_owned(),
        }))
    }

    pub fn current_context(&self) -> &Context {
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Bytecode(frame) => Some(&frame.context),
                Frame::Native(_) => None,
            })
            .unwrap_or(&self.root_context)
    }

    pub fn hooks_suppressed(&self) -> bool {
        self.inherited_hook_suppression
            || self.pending_trace_hooks.iter().any(|hook| hook.evaluating)
    }

    pub fn inherit_hook_suppression(&mut self, suppressed: bool) {
        assert!(
            self.frames.is_empty(),
            "hook suppression must precede task entry"
        );
        self.inherited_hook_suppression = suppressed;
    }

    fn begin_trace_hook(&mut self, frame: usize, with_settings: bool) -> Result<Value, VmError> {
        if self.telemetry.is_none() || self.hooks_suppressed() {
            self.finish_trace_hook(frame, Value::NULL)?;
            return Ok(Value::NULL);
        }
        let pending = self.pending_trace_hooks.last_mut().expect("hooked entry");
        debug_assert_eq!(pending.frame, frame);
        pending.evaluating = true;
        if !with_settings {
            return Ok(Value::bool(true));
        }
        let trace = pending.trace.as_ref();
        let config = trace.map(|trace| trace.telemetry).unwrap_or_default();
        let context = trace.and_then(|trace| trace.context.clone());
        // SAFETY: the frame roots the callable under the active heap permit.
        let function = unsafe { self.load_function(frame)? };
        let mut options = self.telemetry.as_ref().map_or_else(
            || crate::telemetry::ordinary_settings(function, config),
            |state| state.entry_settings(function, config),
        );
        options.context = context;
        Ok(crate::package_trace::alloc_settings(self, &options))
    }

    fn finish_trace_hook(&mut self, frame_idx: usize, result: Value) -> Result<(), VmError> {
        let returned = if result.is_null() {
            None
        } else {
            Some(self.trace_config(result)?)
        };
        self.finish_trace_hook_config(frame_idx, returned)
    }

    fn finish_trace_hook_config(
        &mut self,
        frame_idx: usize,
        returned: Option<CallTrace>,
    ) -> Result<(), VmError> {
        let mut pending = self.pending_trace_hooks.pop().expect("hooked entry");
        debug_assert_eq!(pending.frame, frame_idx);
        if let Some(returned) = returned {
            let previous = pending.trace.take();
            let reserved = previous
                .as_ref()
                .is_some_and(|trace| trace.telemetry.reserved_id.is_some());
            let (under, over) = if reserved {
                (
                    returned.context.as_ref(),
                    previous.as_ref().and_then(|trace| trace.context.as_ref()),
                )
            } else {
                (
                    previous.as_ref().and_then(|trace| trace.context.as_ref()),
                    returned.context.as_ref(),
                )
            };
            let context = match (under, over) {
                (Some(under), Some(over)) => {
                    let mut patch = under.as_ref().clone();
                    patch.compose(over.as_ref().clone());
                    (!patch.is_empty()).then(|| Arc::new(patch))
                }
                (Some(patch), None) | (None, Some(patch)) => Some(patch.clone()),
                (None, None) => None,
            };
            let telemetry = if reserved {
                let prior = previous.unwrap().telemetry;
                crate::telemetry::TraceConfig {
                    inputs: Some(
                        prior.inputs == Some(true) || returned.telemetry.inputs == Some(true),
                    ),
                    output: Some(
                        prior.output == Some(true) || returned.telemetry.output == Some(true),
                    ),
                    error: Some(
                        prior.error == Some(true) || returned.telemetry.error == Some(true),
                    ),
                    ..prior
                }
            } else {
                returned.telemetry
            };
            pending.trace = Some(CallTrace { telemetry, context });
        }
        // The pending frame inherited the caller's context; hook mutations
        // belonged to its own frame. Apply only the returned options here.
        let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
            unreachable!()
        };
        if let Some(patch) = pending
            .trace
            .as_ref()
            .and_then(|trace| trace.context.as_deref())
        {
            frame.context = frame.context.with_patch(patch);
        }
        if self.telemetry.is_none() {
            return Ok(());
        }
        let context = frame.context.clone();
        let locals_offset = frame.locals_offset;
        let type_args = frame.type_args.clone();
        // SAFETY: all input slots, callable and metadata remain rooted.
        let function = unsafe { self.load_function(frame_idx)? };
        let callee = self
            .frame_function_identity(frame_idx)
            .expect("hook target identity");
        let caller = pending
            .caller
            .and_then(|index| self.frame_function_identity(index));
        let observed = pending.caller.is_some_and(|index| matches!(&self.frames[index], Frame::Bytecode(frame) if frame.telemetry.is_some()));
        // Resolve captured parameter cells without allocating for ordinary arities.
        let args: SmallVec<[Value; 8]> = self.stack.0
            [locals_offset.raw()..locals_offset.raw() + function.arity]
            .iter()
            .map(|value| {
                value
                    .as_object_ptr()
                    .and_then(|ptr| match self.get_object(ptr) {
                        Object::Cell(cell) => Some(cell.load()),
                        _ => None,
                    })
                    .unwrap_or(*value)
            })
            .collect();
        if let Some(state) = &mut self.telemetry {
            state.set_context(context);
            let telemetry = unsafe {
                state.enter_bytecode_with_trace(
                    function,
                    callee,
                    caller,
                    pending.caller_pc,
                    observed,
                    &args,
                    pending.trace.as_ref().map(|trace| trace.telemetry),
                    CallTypeArgs {
                        carried: &type_args,
                        passed: &[],
                    },
                    |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
                )
            };
            let Frame::Bytecode(frame) = &mut self.frames[frame_idx] else {
                unreachable!()
            };
            frame.telemetry = telemetry;
        }
        Ok(())
    }

    pub fn set_root_context(&mut self, context: Context) {
        assert!(
            self.frames.is_empty(),
            "root context must precede invocation entry"
        );
        self.root_context = context;
    }

    pub fn invocation_ancestry(&self) -> Option<crate::telemetry::ThreadSpawnContext> {
        self.telemetry
            .as_ref()
            .map(TelemetryState::hidden_spawn_context)
    }

    pub fn set_invocation_ancestry(&mut self, context: &crate::telemetry::ThreadSpawnContext) {
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.configure_spawn(context);
        }
    }

    pub fn set_entry_trace(
        &mut self,
        options: &bex_vm_types::trace::TraceOptionsData,
        reserved_id: Option<btel_types::TelemetryId>,
    ) {
        self.entry_trace_context.clone_from(&options.context);
        self.entry_trace = Some(crate::telemetry::TraceConfig {
            mode: options.mode,
            inputs: options.inputs,
            output: options.output,
            error: options.error,
            reserved_id,
        });
    }

    fn decode_log_event(
        &self,
        payload: Value,
    ) -> Result<(btel_records::LogLevel, Option<Arc<str>>, Value), VmInternalError> {
        use btel_records::LogLevel;
        let map = self.as_string_map(&payload)?;
        let level = map
            .get("level")
            .ok_or(VmInternalError::InvalidLogEvent("missing level"))?;
        let level = match self.as_string(level)?.as_str() {
            "info" => LogLevel::Info,
            "debug" => LogLevel::Debug,
            "warn" => LogLevel::Warn,
            "error" => LogLevel::Error,
            _ => return Err(VmInternalError::InvalidLogEvent("unknown level")),
        };
        let name = map
            .get("event_name")
            .filter(|value| !value.is_null() && !value.is_omitted())
            .map(|value| {
                self.as_string(value)
                    .map(|name| Arc::<str>::from(name.as_str()))
            })
            .transpose()?;
        let data = *map
            .get("data")
            .ok_or(VmInternalError::InvalidLogEvent("missing data"))?;
        Ok((level, name, data))
    }

    fn record_log(
        &mut self,
        frame_idx: usize,
        level: btel_records::LogLevel,
        name: Option<Arc<str>>,
        data: Value,
    ) {
        if self.telemetry.is_none() {
            return;
        }
        let Some(function) = self.frame_function_identity(frame_idx) else {
            return;
        };
        // SAFETY: SendEvent executes under the heap permit and roots its frame.
        let (_, function) =
            unsafe { Self::register_call_path_functions(&self.heap, None, function) };
        let context = self.current_context().clone();
        let state = self.telemetry.as_mut().unwrap();
        state.set_context(context);
        // SAFETY: neither capture nor publication releases the VM heap permit.
        unsafe {
            state.record_log(
                function,
                u32::try_from(self.cur_pc).unwrap_or(u32::MAX),
                level,
                name,
                data,
            );
        }
    }

    fn restore_caller_context(&mut self, frame_idx: usize) {
        let Some(state) = &mut self.telemetry else {
            return;
        };
        let context = self.frames[..frame_idx]
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Bytecode(frame) => Some(frame.context.clone()),
                Frame::Native(_) => None,
            })
            .unwrap_or_else(|| self.root_context.clone());
        state.set_context(context);
    }

    fn trace_config(&mut self, value: Value) -> Result<CallTrace, VmError> {
        use bex_vm_types::trace::{ReservationError, ReservedSpanData, TraceOptionsData};

        let handle = self
            .as_instance(&value)
            .ok()
            .and_then(|instance| (!instance.fields.is_empty()).then(|| instance.load_field(0)));
        let Some(handle) = handle else {
            return Err(
                self.trace_attachment_error("$trace expects trace.Options or trace.ReservedSpan")
            );
        };
        let (options, reserved_id) =
            if let Ok(options) = self.as_rust_data::<TraceOptionsData>(&handle) {
                (options.clone(), None)
            } else if let Ok(reservation) = self.as_rust_data::<ReservedSpanData>(&handle) {
                let local = match reservation.attach(self.trace_scope) {
                    Ok(local) => local,
                    Err(error) => {
                        return Err(self.trace_attachment_error(match error {
                            ReservationError::WrongScope => {
                                "trace.ReservedSpan belongs to a different runtime"
                            }
                            ReservationError::AlreadyAttached => {
                                "trace.ReservedSpan can only be attached once"
                            }
                        }));
                    }
                };
                (reservation.options.clone(), Some(local))
            } else {
                return Err(self
                    .trace_attachment_error("$trace expects trace.Options or trace.ReservedSpan"));
            };
        Ok(CallTrace {
            telemetry: crate::telemetry::TraceConfig {
                mode: options.mode,
                inputs: options.inputs,
                output: options.output,
                error: options.error,
                reserved_id,
            },
            context: options.context,
        })
    }

    /// The `type` value a type-operand position received. Every such
    /// position's contract is `reflect.Type | reflect.TypeView`: a kind view
    /// converts to the `type` it wraps, and a pending builder reference
    /// resolves to its built type (or throws naming the un-frozen builder).
    /// Shared by the call type-argument lane and `BindType`, so the two
    /// boundaries cannot drift on what a type operand may be.
    fn type_operand_value(&mut self, value: Value) -> Result<TypeValue, VmError> {
        let value =
            crate::package_reflect::type_kinds::as_view_type_value(self, value).unwrap_or(value);
        let direct = value.as_object_ptr().and_then(|ptr| {
            let Object::Type(type_value) = self.get_object(ptr) else {
                return None;
            };
            Some((**type_value).clone())
        });
        if let Some(type_value) = direct {
            return Ok(type_value);
        }
        if let Some(result) =
            crate::package_reflect::runtime_class_builder::coerce_pending_type_arg(self, value)
        {
            return result.map_err(VmError::thrown_fresh);
        }
        // Static checking admits only `reflect.Type | reflect.TypeView` and a
        // pending builder type here, so any other value is a compiler bug,
        // not a program error.
        Err(VmInternalError::TypeError {
            expected: Type::Object(ObjectType::Type),
            got: self.type_of(&value),
        }
        .into())
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
        // `reflect.Type | reflect.TypeView`; a kind view converts to the
        // `type` value it wraps, so everything downstream sees `Object::Type`
        // only. This is the same conversion `type_operand_value` applies (and
        // is idempotent with it): normalizing the slots up front is what lets
        // the all-static scan below read them without a fallible call, and
        // what keeps a view from reaching the fast path's `Object::Type`
        // assumption.
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
            let type_value = self.type_operand_value(value)?;
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

    /// The error a `baml.errors.Context` describes.
    fn context_error(&self, context: Value) -> Result<Value, VmInternalError> {
        let instance = self.as_instance(&context)?;
        Ok(crate::package_baml::view::errors::Context { instance }.error())
    }

    /// The trace a `baml.errors.Context` records, as the unwinder reports it
    /// when the error escapes every handler.
    fn context_trace(&self, context: Value) -> Result<Vec<StackFrame>, VmInternalError> {
        use crate::package_baml::view::errors;

        let instance = self.as_instance(&context)?;
        let stack_trace = errors::Context { instance }.stack_trace();
        let instance = self.as_instance(&stack_trace)?;
        errors::StackTrace { instance }
            .frames(self)
            .iter()
            .map(|frame| {
                let frame = errors::StackFrame {
                    instance: self.as_instance(frame)?,
                };
                Ok(StackFrame {
                    function_name: frame.function_name(self).to_string(),
                    file_path: frame.file(self).to_string(),
                    error_line: usize::try_from(frame.line()).unwrap_or_default(),
                })
            })
            .collect()
    }

    /// A `baml.errors.Context` for `error` with `source`'s trace and cause:
    /// what a conversion of the handled error throws with.
    fn retargeted_context(
        &mut self,
        source: Value,
        error: Value,
    ) -> Result<Value, VmInternalError> {
        let (stack_trace, cause) = {
            let instance = self.as_instance(&source)?;
            let source = crate::package_baml::view::errors::Context { instance };
            (
                source.stack_trace(),
                source.cause(self).unwrap_or(Value::NULL),
            )
        };
        Ok(self.alloc_error_value(ErrorClass::Context, vec![error, stack_trace, cause]))
    }

    /// Record that `target` is a conversion of `source`, the error the
    /// innermost active handler is handling (`UnknownError.from` and
    /// `with_message`): the next new throw of `target` from inside that
    /// handler throws with `source`'s trace and cause. A conversion of any
    /// other value records nothing.
    pub(crate) fn transfer_context(&mut self, source: Value, target: Value) {
        let handling = self.find_cause_context();
        if handling == Value::NULL || self.context_error(handling) != Ok(source) {
            return;
        }
        let active = self.active_handler_contexts();
        self.context_transfers.retain(|transfer| {
            transfer.handling != handling && active.contains(&transfer.handling)
        });
        self.context_transfers
            .push(ContextTransfer { handling, target });
    }

    /// The context a new throw of `value` during handling of the error with
    /// context `handling` takes over from a conversion, if one was recorded.
    fn take_context_transfer(&mut self, handling: Value, value: Value) -> Option<Value> {
        let index = self
            .context_transfers
            .iter()
            .position(|transfer| transfer.handling == handling && transfer.target == value)?;
        Some(self.context_transfers.swap_remove(index).handling)
    }

    /// The contexts of every handler whose body is running on this thread, in
    /// every frame and at every nesting depth.
    fn active_handler_contexts(&self) -> Vec<Value> {
        let mut contexts = Vec::new();
        let mut innermost_bc = true;
        for depth in (0..self.frames.len()).rev() {
            let Frame::Bytecode(bf) = &self.frames[depth] else {
                continue;
            };
            let pc = if innermost_bc {
                self.cur_pc
            } else {
                bf.faulting_pc
            };
            innermost_bc = false;
            // SAFETY: same `load_function` contract as the unwind walk.
            let Ok(func) = (unsafe { self.load_function(depth) }) else {
                continue;
            };
            let slots: Vec<usize> = match &func.bytecode.compact {
                Some(compact) => compact
                    .handler_contexts_for_pc(pc)
                    .map(|entry| entry.context_slot)
                    .collect(),
                None => func
                    .bytecode
                    .handler_contexts_for_pc(pc)
                    .map(|entry| entry.context_slot)
                    .collect(),
            };
            contexts.extend(
                slots
                    .into_iter()
                    .map(|slot| self.stack[Self::local_slot_stack_index(bf.locals_offset, slot)]),
            );
        }
        contexts
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

    /// The `Object::Package` the root spells as `pkg`, if any.
    fn package(&self, pkg: &Name) -> Option<&bex_vm_types::types::Package> {
        self.get_object(self.packages.accessible(pkg)?).as_package()
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
            // A runtime package's edges carry its declared dependencies and
            // the host image's prelude, so a name it can write resolves
            // exactly as its compile resolved it; anything else is invisible.
            let dependency = current.accessible(current_ptr, qtn.package())?;
            return self.get_object(dependency).as_package();
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

    /// The stdlib declarations the runtime recognizes structurally.
    pub fn stdlib_heads(&self) -> &crate::package_load::StdlibHeads {
        &self.stdlib_heads
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
    /// name — the package as the root reaches it, then the item path — from
    /// the root's viewpoint. For builtin (prelude) types referenced by
    /// constant FQN; not valid for `user`-package types, whose rendered name
    /// elides the package — use [`Self::lookup_type`] there.
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
    /// Whatever the object comes to keep alive outside its slot while the
    /// returned guard is held — or stops keeping alive — is charged to this
    /// VM's allocation account when the guard drops. Replacing the object
    /// wholesale is charged the same way, so an object built in place costs
    /// what one built before allocation would.
    ///
    /// # Safety
    ///
    /// Caller must ensure exclusive access to the heap object itself: nothing
    /// else may reach it while the guard is held, including through another
    /// guard from this VM. A borrow of the VM proves nothing about the object,
    /// which is why this takes `&self`. Mutator paths for shared state should
    /// use [`Self::get_object`] plus the object's interior synchronization
    /// instead.
    ///
    // BUG: this fabricates `&mut Object` from a heap every VM shares, and the
    // exclusivity above is upheld by convention at each caller rather than by
    // construction: deep-copy placeholders are not yet published, graft
    // relocates unit objects before their package is published, the class
    // builder owns the declarations it fills in, and session state is guarded
    // by the session's `busy` flag. The shape the rest of the heap already
    // uses is `&Object` plus interior synchronization (locked containers,
    // atomic value slots); giving declaration and session objects the same
    // would remove this API.
    #[inline]
    pub fn get_object_mut(&self, ptr: HeapPtr) -> ObjectMut<'_> {
        debug_assert!(
            !self.heap.is_compile_time_ptr(ptr),
            "Cannot mutate compile-time object"
        );
        // SAFETY: caller upholds the object-exclusivity contract documented above.
        let object = unsafe { ptr.get_mut() };
        ObjectMut {
            before: ObjectMut::footprint(object),
            object,
            debt: self.tlab.alloc_debt(),
        }
    }

    /// Settle payload spending with the heap when enough has accumulated, and
    /// if any settlement took spending over the GC budget, poll for a
    /// collection at the next control-flow check rather than up to an
    /// interval later. Runs wherever a lot can have been spent since the last
    /// check: after a native call, after a continuation resumes, at the start
    /// and end of a run, and after an operation that allocates in bulk.
    #[inline]
    fn settle(&mut self) {
        if self.tlab.alloc_debt().balance().unsigned_abs()
            >= bex_heap::SETTLE_QUANTUM.unsigned_abs()
        {
            self.tlab.flush_alloc_debt();
        }
        if self.tlab.take_budget_crossed() {
            self.early_yield.poll_soon();
        }
    }

    /// One control-flow check: cheap until the poll interval elapses, when
    /// the VM settles everything it has spent before reading the flags, so a
    /// loop that only grows containers still has its spending seen.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn should_early_yield(&mut self) -> bool {
        self.early_yield.tick() && self.poll_for_yield()
    }

    #[cold]
    #[inline(never)]
    fn poll_for_yield(&mut self) -> bool {
        self.tlab.flush_alloc_debt();
        // The poll reads the pressure flag the settlement may have raised.
        self.tlab.take_budget_crossed();
        self.early_yield.poll()
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

    fn load_global_in(&self, package: HeapPtr, index: GlobalIndex) -> Value {
        if package.as_ptr().is_null() {
            return self.globals.get(self.proof(), index);
        }
        let Object::Package(package) = self.get_object(package) else {
            unreachable!("runtime owner does not point to Object::Package")
        };
        let bex_vm_types::types::Slots::Own { cells, .. } = &package.slots else {
            unreachable!("a runtime function's owner owns its cells")
        };
        cells
            .get(index.raw())
            .map(bex_vm_types::AtomicValueSlot::load)
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
        let bex_vm_types::types::Slots::Own { cells, initialized } = &package.slots else {
            unreachable!("a runtime function's owner owns its cells")
        };
        if *initialized {
            return Err(VmInternalError::StoreGlobalAfterInit);
        }
        let slot = cells
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
            .objects
            .own()
            .and_then(|objects| objects.get(idx.raw()))
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
            Object::Uint8Array(bytes) => Ok(bytes.lock_mut(self.tlab.alloc_debt())),
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
                // A parameter that states no error type reaches the boundary
                // as `unknown` (its inferred effect parameter erases to it):
                // foreign code may surface a native exception, so an unstated
                // error contract is opaque rather than empty. A declared
                // contract — `never` included — is kept and enforced.
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
        use baml_type::Freshness;
        use bex_vm_types::RealizedTy;
        let literal = |lit| RealizedTy::Literal(lit, Freshness::Regular);
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
                // A variant is the one value of its own variant type.
                Object::Variant(variant) => match self.get_object(variant.enm) {
                    Object::Enum(enm) => {
                        return Some(RealizedTy::EnumVariant(
                            bex_vm_types::TypeHead::new(variant.enm, enm.type_tag),
                            baml_type::Name::new(&enm.variants[variant.index].name),
                        ));
                    }
                    other => unreachable!(
                        "Variant.enm must point to an Enum, found {:?}",
                        ObjectType::of(other)
                    ),
                },
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
                    bex_vm_types::RealizedTy::Interface(head, args, _assoc) => {
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
        let implementation = resolver
            .resolve_implementation(self_ty, iface_head, &iface_args)
            .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                method: method_name.to_string(),
            })?;
        let (callee, mut frame) = resolver.implementation_method(&implementation, method_name)?;
        // Only `.tys` reaches the callee frame. `method_type_args.values` (the
        // exact `TypeValue`s) is dropped, which is sound here: a type argument
        // carries its declaration heads inside the `RealizedTy` itself, so the
        // callee resolves the same declarations without a separate metadata
        // channel. `VirtualCall` still threads the exact values via
        // `append_virtual_method_type_args` so `LoadType<T>` yields the
        // exact value the caller passed rather than an equal twin built
        // from the type.
        frame.extend(method_type_args.tys);
        Ok((callee, frame))
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
        use bex_vm_types::ConcreteRealizedTy;
        if value.as_int().is_some() {
            return Some(ConcreteRealizedTy::Int);
        }
        if value.as_bool().is_some() {
            return Some(ConcreteRealizedTy::Bool);
        }
        if value.is_null() {
            return Some(ConcreteRealizedTy::Null);
        }
        Some(match self.get_object(value.as_object_ptr()?) {
            Object::Float(_) => ConcreteRealizedTy::Float,
            Object::Bigint(_) => ConcreteRealizedTy::Bigint,
            Object::String(_) => ConcreteRealizedTy::String,
            Object::Uint8Array(_) => ConcreteRealizedTy::Uint8Array,
            Object::Instance(inst) => match self.get_object(inst.class) {
                Object::Class(class) => {
                    // Media values are `Object::Instance`s of the std media classes
                    // (`baml.media.{Image,Audio,Video,Pdf}`), but their concrete
                    // type is the `image`/`audio`/… primitive
                    // (`ConcreteRealizedTy::Media`) — which is how the impl registry
                    // keys `implement I for image`. Return that, not the class.
                    if let Some(kind) = self
                        .stdlib_heads
                        .media_kind(bex_vm_types::TypeHead::new(inst.class, class.type_tag))
                    {
                        ConcreteRealizedTy::Media(kind)
                    } else {
                        debug_assert_eq!(inst.class_type_args.len(), class.generic_param_count);
                        // A generic instance's stored `class_type_args` are already
                        // realized (`Box<int>` ⇒ `T = int`), so they are exactly the
                        // `ConcreteRealizedTy::Class` argument list.
                        ConcreteRealizedTy::Class(
                            bex_vm_types::TypeHead::new(inst.class, class.type_tag),
                            inst.class_type_args.clone(),
                        )
                    }
                }
                other => unreachable!(
                    "Instance.class must point to a Class, found {:?}",
                    ObjectType::of(other)
                ),
            },
            Object::Variant(v) => match self.get_object(v.enm) {
                Object::Enum(e) => {
                    ConcreteRealizedTy::Enum(bex_vm_types::TypeHead::new(v.enm, e.type_tag))
                }
                other => unreachable!(
                    "Variant.enm must point to an Enum, found {:?}",
                    ObjectType::of(other)
                ),
            },
            // A `type` value's concrete type is the `reflect.Type` metatype
            // itself. The nine kind views are ordinary wrapper classes whose
            // instances take the `Object::Instance` arm above; nothing about a
            // type value's membership depends on classifying its payload.
            Object::Type(_) => ConcreteRealizedTy::Type,
            // Arrays/maps carry their element/key/value types, so the faithful
            // `list<T>` / `map<K, V>` is reconstructed from the value itself.
            Object::Array(arr) => ConcreteRealizedTy::List(Box::new((*arr.element_ty).clone())),
            Object::Map(map) => ConcreteRealizedTy::Map {
                key: Box::new((*map.key_ty).clone()),
                value: Box::new((*map.value_ty).clone()),
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
            ),
            // Opaque native handles are not BAML data types at all.
            Object::RustData(_) => return None,

            Object::Tombstone => Object::tombstone_reached(),

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
            bex_vm_types::RealizedTy::Interface(head, args, _) => (*head, args.clone()),
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

    /// `owner` if it is a runtime package: a compile-time package (a static
    /// declaration's owner) and null (a declaration still being minted) are
    /// not.
    pub(crate) fn runtime_owner(&self, owner: HeapPtr) -> Option<HeapPtr> {
        (!owner.is_null() && !self.heap.is_compile_time_ptr(owner)).then_some(owner)
    }

    /// Runtime package that owns a value's nominal/callable definition.
    /// Static values return null.
    pub(crate) fn value_runtime_package(&self, value: Value) -> HeapPtr {
        let Some(ptr) = value.as_object_ptr() else {
            return HeapPtr::null();
        };
        let owner = match self.get_object(ptr) {
            Object::Instance(instance) => match self.get_object(instance.class) {
                Object::Class(class) => class.owner.package(),
                _ => None,
            },
            Object::Variant(variant) => match self.get_object(variant.enm) {
                Object::Enum(enm) => enm.owner.package(),
                _ => None,
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
                        Object::Class(class) => class.owner.package(),
                        Object::Enum(enm) => enm.owner.package(),
                        Object::Interface(interface) => Some(interface.owner),
                        Object::TypeAlias(alias) => Some(alias.owner),
                        _ => None,
                    }
                }
                _ => None,
            },
            Object::Function(function) => Some(function.runtime_package),
            Object::GenericFunction(function) => Some(function.runtime_package),
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(function) => Some(function.runtime_package),
                _ => None,
            },
            Object::BoundMethod(method) => match unsafe { method.function.get() } {
                Object::Function(function) => Some(function.runtime_package),
                _ => None,
            },
            Object::Cell(cell) => return self.value_runtime_package(cell.load()),
            _ => None,
        };
        owner
            .and_then(|owner| self.runtime_owner(owner))
            .unwrap_or_else(HeapPtr::null)
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
            Object::Array(arr) => Ok(arr.lock_mut(self.tlab.alloc_debt())),
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

    /// Snapshot a string-keyed map for native string-only boundaries.
    pub fn as_string_map(
        &self,
        value: &Value,
    ) -> Result<IndexMap<bex_vm_types::BexStr, Value>, VmInternalError> {
        self.as_map(value)?
            .iter()
            .map(|(key, value)| Ok((self.as_string(key)?.clone(), *value)))
            .collect()
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
            Object::Map(map) => Ok(map.lock_mut(self.tlab.alloc_debt())),
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
        prepare_compact_code(&mut compile_time_objects, &bytecode.globals);

        // Create heap with compile-time objects, additionally allocating the
        // per-package `Object::Package` / `Object::ImplRule` objects.
        let (heap, package_index) = crate::package_load::build_heap_with_packages(
            compile_time_objects,
            &bytecode.packages,
            bytecode.root,
        );

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
        let stdlib_heads = Arc::new(crate::package_load::StdlibHeads::resolve(&package_index));
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
            stdlib_heads,
            Some(TelemetryState::new_root(
                Arc::new(crate::telemetry::TelemetryPolicies::new()),
                btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
                #[cfg(not(target_arch = "wasm32"))]
                btel_processor::TelemetryRuntime::new().map_err(|error| {
                    VmInternalError::BridgeFailure {
                        message: format!("telemetry processor startup: {error}"),
                    }
                })?,
            )),
            btel_types::RecordingId::generate(),
        ))
    }

    /// Bootstraps the VM preparing the given callable to run.
    ///
    /// `function` may point to an [`Object::Function`], [`Object::Closure`],
    /// [`Object::GenericFunction`], [`Object::HostClosure`], or
    /// [`Object::BoundMethod`] - every callable a `() -> T` value can be.
    /// Closure entry points are the common case for a `spawn { ... }`: the
    /// compiler lowers the body to a lambda, wraps it in a closure that
    /// carries the captured environment, then hands the closure pointer to a
    /// fresh `BexThread` which calls `set_entry_point`. A hand-built plan's
    /// body, or the wrapper a modifier installs, can be any of the others
    /// (`plan.wrap(self.around)` is a bound method).
    pub fn set_entry_point(&mut self, function: HeapPtr, args: &[Value]) {
        // A bound method enters as its function, with the receiver as the
        // leading argument and the type arguments it curried at bind time -
        // the frame `CallIndirect` builds for the same value.
        if let Object::BoundMethod(bound) = self.get_object(function) {
            let inner = bound.function;
            let type_args = bound.type_args.to_vec();
            let args: Vec<Value> = std::iter::once(bound.receiver)
                .chain(args.iter().copied())
                .collect();
            self.set_entry_point_positional(inner, &args, type_args);
            return;
        }
        // Spawn entry points can pass a `Closure` (carrying captured type
        // args from the surrounding scope); host calls typically pass a bare
        // `Function` and want no type args. Either way, fan out to
        // `set_entry_point_with_type_args` so the type-arg-aware host call
        // path (`BexEngine::call_function_bound_args`) shares one
        // implementation with the spawn path.
        let positional = match self.get_object(function) {
            Object::Function(_) => vec![],
            Object::Closure(closure) => closure.captured_type_args.to_vec(),
            Object::GenericFunction(gf) => gf.type_args.to_vec(),
            Object::HostClosure(_) => vec![],
            other => panic!("expect callable as entry point, got {other:?}"),
        };
        self.set_entry_point_positional(function, args, positional);
    }

    /// [`Self::set_entry_point`] with the callee's type arguments already in
    /// positional (De Bruijn) order.
    fn set_entry_point_positional(
        &mut self,
        function: HeapPtr,
        args: &[Value],
        positional: Vec<bex_vm_types::RealizedTy>,
    ) {
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
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_disabled_telemetry();
        #[cfg(not(target_arch = "wasm32"))]
        let _telemetry_scope = self.telemetry.as_ref().map(TelemetryState::execution_scope);
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

        // The callee may declare trailing optionals `args` does not supply: a
        // function value of type `() -> T` can be a function with defaulted
        // parameters, and every LLM function has some. They are laid over the
        // callee's slots as for a call through a function value, so an omitted
        // optional reads the omission sentinel and the callee computes its
        // default.
        let args = self
            .lay_over_callee_slots(function, args.to_vec())
            .unwrap_or_else(|error| {
                unreachable!("an entry point is started with arguments that fit it: {error}")
            });
        let args = args.as_slice();

        // A fresh top-level run starts with no in-flight throw, so drop the
        // per-run throw bookkeeping from the previous run. These vectors are
        // GC roots; on a reused VM (engine package init, playground evals)
        // stale entries would keep every previously thrown value live and
        // grow the per-throw scans without bound. Guarded on an empty frame
        // stack so a nested entry over live frames cannot wipe in-flight
        // throw state.
        if self.frames.is_empty() {
            if let Some(telemetry) = &mut self.telemetry {
                telemetry.set_context(self.root_context.clone());
                telemetry.start_thread();
                telemetry.clear_error_book();
            }
            self.context_transfers.clear();
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
            if let Some(patch) = self.entry_trace_context.take() {
                self.root_context = self.root_context.with_patch(&patch);
                if let Some(state) = &mut self.telemetry {
                    state.set_context(self.root_context.clone());
                }
            }
            self.push_host_closure_trampoline_frame(function, args);
            return;
        }

        // Normalize a specialized callable entry point to its concrete inner
        // function (`dispatch_ptr`) and stored specialization, so the bytecode
        // frame and native/sysop trampoline always see an Object::Function.
        let mut dispatch_ptr = function;
        let mut effective_type_args = type_args;
        let mut effective_type_values = type_values;
        let callable_kind = match self.get_object(function) {
            Object::Function(f) => f.kind,
            Object::Closure(closure) => {
                let func_obj = unsafe { closure.function.get() };
                match func_obj {
                    Object::Function(f) => f.kind,
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
                    Object::Function(f) => f.kind,
                    other => unreachable!("expect generic function inner, got {other:?}"),
                }
            }
            other => unreachable!("expect function or closure as entry point, got {other:?}"),
        };

        let hooked_entry = self
            .get_object(dispatch_ptr)
            .as_callable()
            .is_ok_and(|callee| {
                callee
                    .bytecode
                    .compact
                    .as_ref()
                    .is_some_and(|code| code.trace_hook_finish_pc.is_some())
            });
        if !hooked_entry && let Some(patch) = self.entry_trace_context.take() {
            self.root_context = self.root_context.with_patch(&patch);
            if let Some(state) = &mut self.telemetry {
                state.set_context(self.root_context.clone());
            }
        }
        match callable_kind {
            FunctionKind::Bytecode => {
                let hooked = hooked_entry;
                if hooked {
                    self.pending_trace_hooks.push(PendingTraceHook {
                        frame: self.frames.len(),
                        caller: None,
                        caller_pc: 0,
                        trace: self.entry_trace.take().map(|telemetry| CallTrace {
                            telemetry,
                            context: self.entry_trace_context.take(),
                        }),
                        evaluating: false,
                    });
                }
                let frame_telemetry = if !hooked && self.telemetry.is_some() {
                    // SAFETY: the active heap permit prevents collection here. A
                    // closure remains in the frame because capture opcodes need it,
                    // while producer identity uses its underlying function.
                    let (function_ref, function_identity) = match unsafe { dispatch_ptr.get() } {
                        Object::Function(function) => (function.as_ref(), dispatch_ptr),
                        Object::Closure(closure) => match unsafe { closure.function.get() } {
                            Object::Function(function) => (function.as_ref(), closure.function),
                            other => {
                                unreachable!(
                                    "closure entry must reference a function, got {other:?}"
                                )
                            }
                        },
                        other => unreachable!("bytecode entry must be callable, got {other:?}"),
                    };
                    // SAFETY: arguments and callable remain live under the heap permit.
                    self.telemetry.as_mut().and_then(|telemetry| unsafe {
                        telemetry.enter_bytecode_with_trace(
                            function_ref,
                            function_identity,
                            None,
                            0,
                            false,
                            args,
                            self.entry_trace.take(),
                            CallTypeArgs {
                                carried: &effective_type_args,
                                passed: &[],
                            },
                            |caller, callee| {
                                Self::register_call_path_functions(&self.heap, caller, callee)
                            },
                        )
                    })
                } else {
                    None
                };
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
                self.frames.push(Frame::Bytecode(BytecodeFrame {
                    function: dispatch_ptr,
                    instruction_ptr: 0,
                    locals_offset: StackIndex::from_raw(0),
                    type_args: effective_type_args,
                    type_metadata,
                    faulting_pc: 0,
                    telemetry: frame_telemetry,
                    context: self.root_context.clone(),
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
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type,
            throws_type,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,

            runtime_package: HeapPtr::null(),
        };
        let explicit_trace = self.entry_trace.is_some();
        let entry_ptr = if explicit_trace {
            self.tlab
                .alloc_function(Box::new(entry_function))
                .expect("entry telemetry function identity")
        } else {
            self.tlab.alloc(Object::Function(Box::new(entry_function)))
        };
        let frame_telemetry = self.entry_trace.take().and_then(|trace| {
            // SAFETY: the entry and arguments are live under the active permit.
            let Object::Function(function) = (unsafe { entry_ptr.get() }) else {
                unreachable!()
            };
            self.telemetry.as_mut().and_then(|telemetry| unsafe {
                telemetry.enter_bytecode_with_trace(
                    function,
                    entry_ptr,
                    None,
                    0,
                    false,
                    args,
                    Some(trace),
                    CallTypeArgs::default(),
                    |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
                )
            })
        });

        // Synthetic `$entry::` wrapper frame; the wrapped native/sysop emits
        // its own pair through the normal Call/SysOp instruction paths.
        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            type_metadata: None,
            faulting_pc: 0,
            telemetry: frame_telemetry,
            context: self.root_context.clone(),
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
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            local_names: Vec::new(),
            debug_locals: Vec::new(),
            span: baml_type::Span::fake(),
            return_type,
            param_names: Vec::new(),
            param_types: Vec::new(),
            param_has_default: Vec::new(),
            display_type_params: Vec::new(),
            type_param_names: Vec::new(),
            generic_param_bounds: Vec::new(),
            display_param_types: Vec::new(),
            display_return_type,
            throws_type,
            origin: FunctionOrigin::Internal,
            is_interface_body: false,
            native_key: None,
            body_meta: None,

            runtime_package: HeapPtr::null(),
        };
        let explicit_trace = self.entry_trace.is_some();
        let entry_ptr = if explicit_trace {
            self.tlab
                .alloc_function(Box::new(entry_function))
                .expect("entry telemetry function identity")
        } else {
            self.tlab.alloc(Object::Function(Box::new(entry_function)))
        };
        let frame_telemetry = self.entry_trace.take().and_then(|trace| {
            // SAFETY: the entry and arguments are live under the active permit.
            let Object::Function(function) = (unsafe { entry_ptr.get() }) else {
                unreachable!()
            };
            self.telemetry.as_mut().and_then(|telemetry| unsafe {
                telemetry.enter_bytecode_with_trace(
                    function,
                    entry_ptr,
                    None,
                    0,
                    false,
                    args,
                    Some(trace),
                    CallTypeArgs::default(),
                    |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
                )
            })
        });
        self.frames.push(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: Vec::new(),
            type_metadata: None,
            faulting_pc: 0,
            telemetry: frame_telemetry,
            context: self.root_context.clone(),
        }));
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

    /// Checked counterpart of [`Self::pop_bigint_operand`] for the generic
    /// `BinOp` path, where no static type vouches for the operand: `None`
    /// unless `v` is an `int` or an `Object::Bigint`. Classifies without
    /// widening an `int` to a `BigInt`.
    fn bigint_operand_of(&self, v: Value) -> Option<BigintOperand> {
        if let Some(n) = v.as_int() {
            Some(BigintOperand::Int(n))
        } else if let Some(ptr) = v.as_object_ptr() {
            matches!(self.get_object(ptr), Object::Bigint(_)).then_some(BigintOperand::Heap(ptr))
        } else {
            None
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

    /// Evaluate a bigint arithmetic / bitwise / shift op for the specialized
    /// `*Bigint` opcodes and the generic `BinOp` path.
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
    pub fn as_rust_data<T: bex_vm_types::BexRustData>(
        &self,
        value: &Value,
    ) -> Result<&T, VmInternalError> {
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
                        got: arc.payload_type_id(),
                    })
            }
            _ => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::RustData),
                got: self.type_of(value),
            }),
        }
    }

    /// Clone the `Arc` out of the `$rust_type` field `field` of the instance
    /// `holder`, so a native can keep using the payload while it borrows
    /// `&mut BexVm` to allocate.
    pub fn rust_data_field<T: bex_vm_types::BexRustData>(
        &self,
        holder: &Value,
        field: usize,
    ) -> Result<Arc<T>, VmInternalError> {
        let handle = self.as_instance(holder)?.load_field(field);
        let Some(ptr) = handle.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::RustData),
                got: self.type_of(&handle),
            });
        };
        match self.get_object(ptr) {
            Object::RustData(arc) => {
                let got = arc.payload_type_id();
                arc.clone()
                    .downcast_payload::<T>()
                    .map_err(|_| VmInternalError::RustTypeError {
                        expected: TypeId::of::<T>(),
                        got,
                    })
            }
            _ => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::RustData),
                got: self.type_of(&handle),
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
            // Field order matches the `Timeout` class in
            // `ns_errors/errors.baml`: message, duration, timeout_type.
            VmBamlError::Timeout {
                message,
                duration,
                timeout_type,
            } => {
                let duration = duration.map_or(Value::NULL, |d| self.alloc_duration(d));
                (
                    ErrorClass::Timeout,
                    vec![
                        Value::object(self.alloc_string(message)),
                        duration,
                        Value::object(self.alloc_string(timeout_type)),
                    ],
                )
            }
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
            VmBamlError::LlmClient { message } => (
                ErrorClass::LlmClient,
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
                let dyn_arc: std::sync::Arc<dyn bex_vm_types::BexRustData> = handle;
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

    /// A `baml.time.Duration` instance (`{ _nanoseconds: bigint }`), or null
    /// when the class is not loaded (bare test VMs).
    fn alloc_duration(&mut self, duration: std::time::Duration) -> Value {
        let Some(class_ptr) = self
            .error_class_ptrs
            .get(DURATION_CLASS_PTR_INDEX)
            .copied()
            .filter(|ptr| !ptr.is_null())
        else {
            return Value::NULL;
        };
        // A `u128` nanosecond count is far below the bigint size limit.
        let nanos = Value::object(self.tlab.alloc(Object::Bigint(std::sync::Arc::new(
            num_bigint::BigInt::from(duration.as_nanos()),
        ))));
        Value::object(self.tlab.alloc(Object::Instance(Instance::new(
            class_ptr,
            Box::new([]),
            vec![nanos],
        ))))
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
    /// and the outer `StackTrace` wrapper.
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

    /// Construct a `baml.errors.Context` for a thrown value: the error
    /// itself, the `StackTrace` where it was thrown, and the `cause` it
    /// superseded while unwinding (or `Value::NULL` for a fresh error).
    ///
    /// Field order — error, `stack_trace`, cause — matches the class declared in
    /// `ns_errors/error_context.baml` (the constructor ABI). Built when a
    /// handler first lands a new error, whether or not it binds `ctx`.
    pub(crate) fn alloc_error_context(
        &mut self,
        error: Value,
        trace: &[StackFrame],
        cause: Value,
    ) -> Value {
        let stack_trace = self.alloc_stack_trace(trace);
        self.alloc_error_value(ErrorClass::Context, vec![error, stack_trace, cause])
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
                bex_vm_types::RealizedTy::Interface(head, args, _assoc) => {
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
            None => Err(self.integer_overflow(bex_lang::int::overflow_message(l, op, r))),
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
            None => Err(self.integer_overflow(bex_lang::int::overflow_message(l, op, r))),
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
        self.integer_overflow(bex_lang::int::overflow_message(lv, op, rv))
    }

    /// Build a catchable `baml.panics.NegativeBitShift` throw. Cold path only.
    #[cold]
    #[inline(never)]
    fn negative_bit_shift(&mut self, count: i64) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::NegativeBitShift {
            message: bex_lang::int::negative_bit_shift_message(count),
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
                if self
                    .pending_trace_hooks
                    .iter()
                    .any(|hook| hook.frame == idx && hook.evaluating)
                {
                    return None;
                }
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
                            error_line,
                        })
                    }
                    Frame::Native(_) => Some(StackFrame {
                        function_name: func.name.clone(),
                        file_path: func.source_file.clone(),
                        error_line: 0,
                    }),
                }
            })
            .collect()
    }

    /// The stack trace a caller sees on a thrown value or an escaping panic:
    /// [`Self::capture_stack_trace`] with the standard-library frames removed.
    ///
    /// A `<builtin>/…` frame is nothing user code can act on, and a native
    /// builtin never produced one - it pushes no frame at all. Filtering here
    /// keeps a builtin whose body is written in BAML (`baml.sys.panic`,
    /// `baml.sys.exit`) indistinguishable from one written in Rust. Internal
    /// errors keep the full trace, where those frames are the point.
    fn capture_user_stack_trace(&self) -> Vec<StackFrame> {
        self.capture_stack_trace()
            .into_iter()
            .filter(|frame| !frame.is_builtin())
            .collect()
    }

    fn try_unwind_exception(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        thrown: VmThrown,
    ) -> Result<(), VmError> {
        self.unwind_exception(frame_idx, function, thrown, RaiseEntry::Vm)
    }

    fn unwind_exception(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        thrown: VmThrown,
        entry: RaiseEntry,
    ) -> Result<(), VmError> {
        let exception_value = thrown.value;
        let telemetry_outcome = self.telemetry_outcome_for_exception(exception_value);

        // BEP-042: the context this error lands with. A caught error raised
        // again — `throw` of its binding, a catch's no-match fall-through or
        // `throw_if_panic`, a `defer` pad re-raising the in-flight error — and
        // one awaited from another task carry the context of their original
        // throw, trace and cause intact. The context travels with the error,
        // so two errors that happen to be equal values never share one. It is
        // the error's only while it describes this value: a binding reassigned
        // before its `throw` makes that a new throw. A new throw inside a
        // handler body is "during handling of" that handler's error, which
        // becomes its cause, unless it throws a conversion of that error
        // (`UnknownError.from`), which takes over the error's own trace and
        // cause.
        let (mut context, converted) = match thrown.context {
            Some(context) if self.context_error(context)? == exception_value => {
                (InFlightContext::Carried(context), false)
            }
            Some(_) | None => {
                let cause = self.find_cause_context();
                match self.take_context_transfer(cause, exception_value) {
                    Some(handled) => (
                        InFlightContext::Carried(
                            self.retargeted_context(handled, exception_value)?,
                        ),
                        true,
                    ),
                    None => (
                        InFlightContext::Fresh {
                            trace: Arc::from(self.capture_user_stack_trace()),
                            cause,
                        },
                        false,
                    ),
                }
            }
        };
        // Exception-path telemetry; `None` unless a recording wants raises.
        let carried = match &context {
            InFlightContext::Carried(context) => Some(*context),
            InFlightContext::Fresh { .. } => None,
        };
        let mut evidence = self.begin_error_evidence(
            &thrown,
            entry,
            Some((*frame_idx, *function)),
            carried,
            converted,
        );

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
                    // No bytecode handler remains for this native entry frame.
                    if let Some(evidence) = evidence.take() {
                        self.error_not_caught(
                            evidence,
                            btel_records::UnwindResult::EscapedToNative,
                        );
                    }
                    return Err(VmError::thrown_fresh(exception_value));
                }
                // SAFETY: the branch above established that at least two frames remain.
                unsafe { self.frames.no_return_pop() };
                continue; // try next outer frame
            }

            // From here, frame is guaranteed Bytecode.
            let Frame::Bytecode(frame) = frame else {
                unreachable!("non-Native frames already handled above");
            };
            let (frame_faulting_pc, frame_locals_offset) = (frame.faulting_pc, frame.locals_offset);

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
            let frame_function = match unsafe { self.load_function(depth) } {
                Ok(frame_function) => frame_function,
                Err(error) => {
                    if let Some(evidence) = evidence.take() {
                        self.error_not_caught(evidence, btel_records::UnwindResult::Aborted);
                    }
                    return Err(error.into());
                }
            };

            // Recoverable hook failures land at the decision boundary, before
            // the target starts. Task cancellation keeps unwinding normally.
            if self
                .pending_trace_hooks
                .last()
                .is_some_and(|hook| hook.frame == depth && hook.evaluating)
            {
                let cancelled = self.as_instance(&exception_value).is_ok_and(|instance| {
                    instance.class == self.panic_class_ptrs[PanicClass::Cancelled as usize]
                });
                if !cancelled {
                    let finish = frame_function
                        .bytecode
                        .compact
                        .as_ref()
                        .and_then(|code| code.trace_hook_finish_pc)
                        .expect("hook finish PC");
                    self.stack.truncate(
                        frame_locals_offset.raw()
                            + frame_function.arity
                            + frame_function.real_local_count,
                    );
                    self.stack.push(Value::NULL);
                    let Frame::Bytecode(frame) = &mut self.frames[depth] else {
                        unreachable!()
                    };
                    frame.instruction_ptr = finish;
                    if let Some(evidence) = evidence.take() {
                        self.error_hook_fallback(evidence);
                    }
                    self.restore_caller_context(depth);
                    *frame_idx = depth;
                    *function = frame_function;
                    return Ok(());
                }
            }

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

                // Store the error's `baml.errors.Context` in the context
                // slot: a rethrow from this handler carries it, and a throw
                // during its body takes it as its cause.
                let ctx_value = match &context {
                    InFlightContext::Carried(context) => *context,
                    InFlightContext::Fresh { trace, cause } => {
                        self.alloc_error_context(exception_value, trace, *cause)
                    }
                };
                let ctx_slot = Self::local_slot_stack_index(locals_offset, entry.context_slot);
                self.stack[ctx_slot] = ctx_value;

                if let Some(evidence) = evidence.take() {
                    self.error_caught(
                        evidence,
                        depth,
                        frame_function,
                        entry.context_slot,
                        ctx_slot.raw(),
                        entry.handler_pc,
                    );
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
            let recorded = if self.frame_wants_error_capture(depth, frame_function) {
                self.recorded_context(&mut context, exception_value)
            } else {
                exception_value
            };
            self.complete_unwound_frame(
                depth,
                frame_function,
                telemetry_outcome,
                recorded,
                evidence.as_mut(),
            );
            if let Some(evidence) = &mut evidence {
                self.error_frame_unwound(evidence, depth);
            }
            if self.frames.len() <= 1 {
                if let Some(evidence) = evidence.take() {
                    self.error_not_caught(evidence, btel_records::UnwindResult::Unhandled);
                }
                let trace = match &context {
                    InFlightContext::Carried(context) => self.context_trace(*context)?,
                    InFlightContext::Fresh { trace, .. } => trace.to_vec(),
                };
                return Err(VmError::ThrownUnhandled {
                    value: exception_value,
                    trace,
                });
            }

            let popped = self.frames.pop().expect("frame stack is not empty");
            self.pending_trace_hooks.retain(|hook| hook.frame < depth);
            match popped {
                Frame::Bytecode(bf) => {
                    self.stack.drain(bf.locals_offset..);
                    // Restore the caller's identity after unwinding this frame.
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
                // The cause is the enclosing handler's `baml.errors.Context`,
                // which lives in its context slot — itself a link in the chain
                // (with its own `cause`).
                let slot = Self::local_slot_stack_index(bf.locals_offset, entry.context_slot);
                return self.stack[slot];
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

    /// The parameter list a callable value executes with: the `Function`
    /// behind a closure, bound method or generic wrapper, or a host closure's
    /// declared parameters.
    fn callee_params(&self, callee: HeapPtr) -> Result<CalleeParams<'_>, VmInternalError> {
        let function = match self.get_object(callee) {
            Object::Function(function) => return Ok(CalleeParams::Function(function)),
            Object::HostClosure(host) => return Ok(CalleeParams::Host(host)),
            Object::Closure(closure) => closure.function,
            Object::BoundMethod(method) => method.function,
            Object::GenericFunction(generic) => self.generic_function_authored_ptr(generic)?,
            other => {
                return Err(VmInternalError::TypeError {
                    expected: FunctionType::Callable.into(),
                    got: ObjectType::of(other).into(),
                });
            }
        };
        match self.get_object(function) {
            Object::Function(function) => Ok(CalleeParams::Function(function)),
            other => Err(VmInternalError::TypeError {
                expected: FunctionType::Callable.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    /// The layout the compiler recorded for the call instruction at `pc`.
    /// A site without one already pushes the callee's own slots.
    fn recorded_call_layout(
        function: &'static Function,
        pc: usize,
    ) -> Option<&'static baml_type::CallLayout> {
        function.bytecode.compact.as_ref()?.call_layouts.get(&pc)
    }

    /// Reshape the values on top of the stack from the slots `caller` pushed
    /// to the slots `callee` reads (see [`baml_type::CallLayout::map_to`]):
    /// required values keep their order, supplied optionals move to the
    /// callee's slot of the same name, and optionals the caller never
    /// mentioned receive the omission sentinel. Returns the callee's value
    /// count. Type arguments and the caller's locals below the value lane
    /// are untouched.
    fn remap_call_arguments(
        &mut self,
        caller: &baml_type::CallLayout,
        callee: HeapPtr,
    ) -> Result<usize, VmInternalError> {
        let params = self.callee_params(callee)?;
        if params.layout_is(caller) {
            return Ok(caller.len());
        }
        let target = params.layout();
        let mapping = caller
            .map_to(&target)
            .map_err(VmInternalError::CallLayout)?;
        let offset = self
            .stack
            .len()
            .checked_sub(caller.len())
            .ok_or(VmInternalError::NotEnoughItemsOnStack(caller.len()))?;
        let values: Vec<Value> = self.stack.drain(StackIndex::from_raw(offset)..).collect();
        self.stack.extend(
            mapping
                .into_iter()
                .map(|slot| slot.map_or(Value::OMITTED_ARG, |index| values[index])),
        );
        Ok(target.len())
    }

    /// Prepare a `YieldToCall`-style invocation: if `callee` is a
    /// `BoundMethod`, insert the receiver at the front of `args`. The returned
    /// `HeapPtr` is `callee` unchanged — keeping the `BoundMethod` identity so
    /// that `execute_call_from_locals_offset` can extract the receiver's
    /// `class_type_args` to seed `frame.type_args` (needed for
    /// `reflect.Type.of<T>()` inside generic methods invoked indirectly).
    /// `execute_call_from_locals_offset` and `load_function` both unwrap the
    /// `BoundMethod` to its inner `Function` for dispatch.
    ///
    /// A native yields either the callee's complete frame (`reflect.call_any`
    /// fills every slot, omitted optionals included) or only the required
    /// values in order (the higher-order builtins). An argument list as long
    /// as the callee's parameter list is the complete frame; a shorter one is
    /// laid over the callee's required slots, so a bound method with extra
    /// optionals is callable as a narrower function value.
    fn resolve_bound_method_callee(
        &self,
        callee: HeapPtr,
        args: &mut Vec<Value>,
    ) -> Result<HeapPtr, VmInternalError> {
        if let Object::BoundMethod(bm) = self.get_object(callee) {
            // Prepend receiver so the inner function sees [self, arg1, ..., argN].
            args.insert(0, bm.receiver);
        }
        *args = self.lay_over_callee_slots(callee, std::mem::take(args))?;
        Ok(callee)
    }

    /// `args` laid over `callee`'s parameter slots. A list as long as the
    /// callee's parameter list is its complete frame and stays as it is; a
    /// shorter one supplies the required parameters in order, and every
    /// optional it omits receives the omission sentinel.
    fn lay_over_callee_slots(
        &self,
        callee: HeapPtr,
        args: Vec<Value>,
    ) -> Result<Vec<Value>, VmInternalError> {
        let params = self.callee_params(callee)?;
        if args.len() == params.arity() {
            return Ok(args);
        }
        let mapping = baml_type::CallLayout::positional(args.len())
            .map_to(&params.layout())
            .map_err(VmInternalError::CallLayout)?;
        Ok(mapping
            .into_iter()
            .map(|slot| slot.map_or(Value::OMITTED_ARG, |index| args[index]))
            .collect())
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
    /// receiver's class args cannot express (blanket impls, adopted
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
        trace: Option<CallTrace>,
    ) -> VmExecState {
        self.pending_host_trace = trace;
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

    pub fn take_host_call_options(&mut self) -> bex_vm_types::trace::HostCallOptions {
        let Some(trace) = self.pending_host_trace.take() else {
            return bex_vm_types::trace::HostCallOptions::default();
        };
        bex_vm_types::trace::HostCallOptions {
            options: bex_vm_types::trace::TraceOptionsData {
                mode: trace.telemetry.mode,
                inputs: trace.telemetry.inputs,
                output: trace.telemetry.output,
                error: trace.telemetry.error,
                context: trace.context,
            },
            reserved_id: trace.telemetry.reserved_id,
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
    fn dispatch_sysop_yield(&mut self, callee_fn_ptr: HeapPtr) -> Result<VmExecState, VmError> {
        let (sys_op, arity) = {
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
            (sys_op, f.arity)
        };
        let args_offset = self
            .stack
            .len()
            .checked_sub(arity)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(arity))?;
        let args_offset = StackIndex::from_raw(args_offset);
        let call_args: Vec<Value> = self.stack.drain(args_offset..).collect();

        Ok(VmExecState::SysOp {
            operation: sys_op,
            args: call_args,
        })
    }

    fn execute_call_from_locals_offset(
        &mut self,
        callee_ptr: HeapPtr,
        locals_offset: StackIndex,
        arg_count: usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        let trace = self.pending_call_trace.take();
        let passes_type_args = std::mem::take(&mut self.pending_call_passes_type_args);
        // Record the caller's call-site PC before descending. Once a callee
        // frame is pushed, this (now-outer) frame is no longer the innermost, so
        // its live PC must be persisted into `faulting_pc` for correct unwinding
        // and stack traces. `cur_pc` holds this call instruction's start.
        let call_site = self.cur_pc;

        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
            bf.faulting_pc = call_site;
        }

        // SAFETY: the active heap permit prevents collection throughout call
        // setup, including stack/frame growth. Reuse this object and its resolved
        // function only within this execution interval; exec reloads the function
        // from the rooted frame after a yield.
        let callee_object: &'static Object = unsafe { callee_ptr.get() };

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
        if matches!(callee_object, Object::HostClosure(_)) {
            let user_args: Vec<Value> = self.stack.drain(locals_offset..).collect();
            let state = self.host_closure_call_sysop(callee_ptr, user_args, trace);
            return Ok(Some(state));
        }

        // Borrow wrapper type args until a frame or native call needs ownership.
        // Plain functions use empty slices without temporary owned containers.
        let (closure_type_args, bound_method_class_type_args, specialized_type_args): (
            &[bex_vm_types::RealizedTy],
            &[bex_vm_types::RealizedTy],
            &[bex_vm_types::RealizedTy],
        ) = match callee_object {
            Object::Closure(c) => (&c.captured_type_args, &[], &[]),
            Object::BoundMethod(bm) => (&[], &bm.type_args, &[]),
            Object::GenericFunction(gf) => (&[], &[], &gf.type_args),
            _ => (&[], &[], &[]),
        };

        // Resolve the callee: either a plain Function, a Closure, or a BoundMethod
        // wrapping one. `callee_fn_ptr` is the heap pointer of the resolved
        // `Object::Function` itself (unwrapped from any Closure/BoundMethod/
        // specialized wrapper), retained by the frame for execution and GC.
        let (callee, callee_fn_ptr) = match callee_object {
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
        if trace.is_some() && !matches!(callee_kind, FunctionKind::Bytecode) {
            return Err(self.trace_attachment_error(
                "$trace is not supported on native functions or system operations",
            ));
        }
        if arg_count != callee_arity {
            return Err(VmInternalError::InvalidArgumentCount {
                expected: callee_arity,
                got: arg_count,
            }
            .into());
        }
        // Check if we've reached the max call stack size.
        if self.frames.len() >= MAX_FRAMES {
            return Err(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            ));
        }

        match callee_kind {
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
                        specialized_type_args
                    } else {
                        closure_type_args
                    };
                let restore_pending = if !native_type_args.is_empty() {
                    Some(std::mem::replace(
                        &mut self.pending_call_type_args,
                        native_type_args.to_vec(),
                    ))
                } else {
                    None
                };
                let native_result = func(self, &args);
                if let Some(prev) = restore_pending {
                    self.pending_call_type_args = prev;
                }
                self.settle();

                // Run Rust native function, converting NativeCallResult → VmError.
                match native_result {
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
                            bytecode_caller: self.bytecode_caller_index(*frame_idx),
                        }));

                        // If callee is a BoundMethod, insert receiver into args.
                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args)?;

                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        self.stack.extend(callback_args);

                        let result = self.execute_call_from_locals_offset_with_type_args(
                            real_callee,
                            cb_locals,
                            arg_count,
                            CallOptions {
                                type_args: &callback_type_args,
                                type_values: &[],
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
                let hooked = callee
                    .bytecode
                    .compact
                    .as_ref()
                    .is_some_and(|code| code.trace_hook_finish_pc.is_some());
                let caller_frame_idx = self.bytecode_caller_index(*frame_idx);
                let caller_pc = if caller_frame_idx == *frame_idx {
                    call_site
                } else {
                    let Frame::Bytecode(caller) = &self.frames[caller_frame_idx] else {
                        unreachable!()
                    };
                    caller.faulting_pc
                };
                let inherited = self.current_context();
                let context = (!hooked)
                    .then_some(trace.as_ref())
                    .flatten()
                    .as_ref()
                    .and_then(|trace| trace.context.as_deref())
                    .map_or_else(|| inherited.clone(), |patch| inherited.with_patch(patch));
                if let Some(telemetry) = &mut self.telemetry {
                    telemetry.set_context(context.clone());
                }
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

                let frame_telemetry = if !hooked && self.telemetry.is_some() {
                    let caller_frame_idx = self.bytecode_caller_index(*frame_idx);
                    let actual_caller = self
                        .visible_bytecode_caller_index(*frame_idx)
                        .and_then(|index| self.frame_function_identity(index));
                    let caller_is_observed = matches!(
                        self.frames.get(caller_frame_idx),
                        Some(Frame::Bytecode(BytecodeFrame {
                            telemetry: Some(_),
                            ..
                        }))
                    );
                    let caller_pc = if caller_frame_idx == *frame_idx {
                        call_site
                    } else {
                        let Frame::Bytecode(caller) = &self.frames[caller_frame_idx] else {
                            unreachable!("native continuation retains its bytecode caller");
                        };
                        caller.faulting_pc
                    };
                    let caller_pc = u32::try_from(caller_pc).unwrap_or(u32::MAX);
                    let args = &self.stack.0
                        [locals_offset.raw()..locals_offset.raw().saturating_add(arg_count)];
                    let type_args = CallTypeArgs {
                        carried: initial_type_args,
                        passed: if passes_type_args {
                            &self.pending_call_type_args
                        } else {
                            &[]
                        },
                    };
                    // SAFETY: arguments and callable remain live under the heap permit.
                    self.telemetry.as_mut().and_then(|telemetry| unsafe {
                        telemetry.enter_bytecode_with_trace(
                            callee,
                            callee_fn_ptr,
                            actual_caller,
                            caller_pc,
                            caller_is_observed,
                            args,
                            trace.as_ref().map(|trace| trace.telemetry),
                            type_args,
                            |caller, callee| {
                                Self::register_call_path_functions(&self.heap, caller, callee)
                            },
                        )
                    })
                } else {
                    None
                };
                if hooked {
                    self.pending_trace_hooks.push(PendingTraceHook {
                        frame: self.frames.len(),
                        caller: self.visible_bytecode_caller_index(*frame_idx),
                        caller_pc: u32::try_from(caller_pc).unwrap_or(u32::MAX),
                        trace,
                        evaluating: false,
                    });
                }
                // Construct after reserving capacity so the frame can be written
                // directly into the vector instead of moved through a temporary.
                self.frames.extend(std::iter::once_with(|| {
                    Frame::Bytecode(BytecodeFrame {
                        function: callee_ptr,
                        instruction_ptr: 0,
                        locals_offset,
                        type_args: initial_type_args.to_vec(),
                        type_metadata: None,
                        faulting_pc: 0,
                        telemetry: frame_telemetry,
                        context,
                    })
                }));
                let new_len = self.stack.len() + callee.real_local_count;
                self.stack.resize(new_len, Value::NULL);

                // Update frame_idx to point to the new frame.
                *frame_idx = self.frames.len() - 1;

                *function = callee;
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
                let state = self.dispatch_sysop_yield(callee_fn_ptr)?;
                return Ok(Some(state));
            }

            FunctionKind::NativeUnresolved => {
                // This should never happen - native functions should be resolved
                // by attach_builtins() before the VM runs.
                let callee_name = callee.name.clone();
                panic!(
                    "Unresolved native function '{callee_name}' - did you forget to call attach_builtins()?"
                );
            }
        }

        if self.should_early_yield() {
            return Ok(Some(VmExecState::EarlyYield));
        }
        Ok(None)
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
            VmRustFnError::Thrown { value, throw_kind } => VmError::Thrown(VmThrown {
                value,
                throw_kind,
                context: None,
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
        self.pending_call_passes_type_args = !options.type_args.is_empty();
        let result = self.execute_call_from_locals_offset(
            callee_ptr,
            locals_offset,
            arg_count,
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

    /// The caller must hold this heap's execution permit. Called only when a
    /// new telemetry call path is about to publish function references.
    unsafe fn register_call_path_functions(
        heap: &BexHeap,
        caller: Option<HeapPtr>,
        callee: HeapPtr,
    ) -> (Option<btel_types::FunctionId>, btel_types::FunctionId) {
        // SAFETY: the running VM excludes GC and points only at linked objects.
        unsafe {
            let caller = caller.and_then(|caller| heap.register_telemetry_function(caller));
            let callee = heap
                .register_telemetry_function(callee)
                .expect("observed callee must support telemetry");
            (caller, callee)
        }
    }

    /// Resolve a frame's callable wrapper to the underlying function object
    /// used as stable call-path identity.
    fn frame_function_identity(&self, frame_idx: usize) -> Option<HeapPtr> {
        self.callable_function_identity(self.frames.get(frame_idx)?.function())
    }

    /// The function object a callable value runs, used as stable call-path
    /// identity; `None` for a host closure, which runs no BAML function.
    fn callable_function_identity(&self, ptr: HeapPtr) -> Option<HeapPtr> {
        // SAFETY: callers hold the heap permit with `ptr` rooted (a frame's
        // function, or a plan the running instruction holds), and nothing
        // here allocates.
        match unsafe { ptr.get() } {
            Object::Function(function) => function.telemetry_function_id.map(|_| ptr),
            Object::Closure(closure) => Some(closure.function),
            Object::BoundMethod(method) => Some(method.function),
            Object::GenericFunction(function) => self.generic_function_authored_ptr(function).ok(),
            _ => None,
        }
    }

    /// Complete producer state while the frame and its function identity are
    /// still present. Taking the optional state makes completion idempotent on
    /// error funnels that retain the root frame long enough to report a trace.
    fn complete_bytecode_invocation(
        &mut self,
        frame_idx: usize,
        outcome: InvocationOutcome,
        value: Option<Value>,
    ) {
        let telemetry = match self.frames.get_mut(frame_idx) {
            Some(Frame::Bytecode(frame)) => frame.telemetry.take(),
            _ => None,
        };
        let Some(telemetry) = telemetry else {
            self.restore_caller_context(frame_idx);
            return;
        };
        // SAFETY: the frame remains rooted until after producer completion.
        let function = unsafe { self.load_function(frame_idx) }
            .expect("bytecode frame must retain its function through completion");
        self.complete_bytecode_invocation_with_function(
            frame_idx, function, telemetry, outcome, value,
        );
    }

    /// Complete a bytecode frame the unwinder is about to pop, with the
    /// function it already loaded. While a raise is recorded, the frame exits
    /// at the raise's instant: no code runs between one unwind's pops, so one
    /// clock read serves the raise and every frame it pops.
    fn complete_unwound_frame(
        &mut self,
        frame_idx: usize,
        function: &Function,
        outcome: InvocationOutcome,
        value: Value,
        evidence: Option<&mut error_evidence::UnwindEvidence>,
    ) {
        let telemetry = match self.frames.get_mut(frame_idx) {
            Some(Frame::Bytecode(frame)) => frame.telemetry.take(),
            _ => None,
        };
        let Some(telemetry) = telemetry else {
            self.restore_caller_context(frame_idx);
            return;
        };
        // A retained call's completion, including a late promotion, must
        // follow its raise's record.
        let exited_at = evidence.map(|evidence| {
            if evidence.holds_raise()
                && (telemetry.is_span()
                    || function.telemetry_policy_id.load() != btel_types::TelemetryPolicyId::NONE)
            {
                self.release_held_raise(evidence);
            }
            evidence.raised_at()
        });
        let state = self
            .telemetry
            .as_mut()
            .expect("observed invocation has telemetry");
        if let Frame::Bytecode(frame) = &self.frames[frame_idx] {
            state.set_context(frame.context.clone());
        }
        // SAFETY: the unwinder holds the heap permit, and the frame and the
        // error stay rooted until after producer completion.
        unsafe {
            match exited_at {
                Some(exited_at) => state.complete_invocation_at(
                    telemetry,
                    function,
                    outcome,
                    Some(value),
                    exited_at,
                ),
                None => state.complete_invocation(telemetry, function, outcome, Some(value)),
            }
        }
        self.restore_caller_context(frame_idx);
    }

    #[allow(clippy::inline_always, reason = "measured producer return fast path")]
    #[inline(always)]
    fn complete_bytecode_invocation_with_function(
        &mut self,
        frame_idx: usize,
        function: &Function,
        telemetry: FrameTelemetry,
        outcome: InvocationOutcome,
        value: Option<Value>,
    ) {
        let state = self
            .telemetry
            .as_mut()
            .expect("observed invocation has telemetry");
        if let Frame::Bytecode(frame) = &self.frames[frame_idx] {
            state.set_context(frame.context.clone());
        }
        // SAFETY: completion holds the heap permit and the result/error is live.
        unsafe {
            self.telemetry
                .as_mut()
                .expect("observed invocation has telemetry")
                .complete_invocation(telemetry, function, outcome, value);
        }
        self.restore_caller_context(frame_idx);
    }

    fn frame_wants_error_capture(&self, frame_idx: usize, function: &Function) -> bool {
        let (Some(Frame::Bytecode(frame)), Some(state)) =
            (self.frames.get(frame_idx), self.telemetry.as_ref())
        else {
            return false;
        };
        frame
            .telemetry
            .as_ref()
            .is_some_and(|telemetry| state.wants_error_capture(telemetry, function))
    }

    /// Whether `baml.errors.Context` can be built: bare test VMs do not load
    /// the error classes.
    fn error_context_classes_loaded(&self) -> bool {
        [
            ErrorClass::Context,
            ErrorClass::StackTrace,
            ErrorClass::StackFrame,
        ]
        .iter()
        .all(|class| {
            self.error_class_ptrs
                .get(*class as usize)
                .is_some_and(|ptr| !ptr.is_null())
        })
    }

    /// `baml.errors.Context { error, stack_trace, cause }` for a recorded
    /// error. The bare value when the error classes are not loaded (bare
    /// test VMs).
    fn error_context_value(&mut self, error: Value, trace: &[StackFrame], cause: Value) -> Value {
        if self.error_context_classes_loaded() {
            self.alloc_error_context(error, trace, cause)
        } else {
            error
        }
    }

    /// What a span that records the error in flight captures: the context a
    /// `catch (e, ctx)` would bind. A new failure's context is built the
    /// first time a span records it and carried from then on, so the handler
    /// that lands the error binds the same object: one context per
    /// occurrence. Allocation never collects, so it stays live. The bare error
    /// when the error classes are not loaded (bare test VMs).
    fn recorded_context(&mut self, context: &mut InFlightContext, error: Value) -> Value {
        match context {
            InFlightContext::Carried(carried) => *carried,
            InFlightContext::Fresh { .. } if !self.error_context_classes_loaded() => error,
            InFlightContext::Fresh { trace, cause } => {
                let built = self.alloc_error_context(error, trace, *cause);
                *context = InFlightContext::Carried(built);
                built
            }
        }
    }

    /// Charge a finished sysop's time to the innermost observed call path.
    pub fn record_sysop_time(&mut self, elapsed: ClockDuration) {
        if elapsed == ClockDuration::ZERO {
            return;
        }
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.record_sysop_time(elapsed);
        }
    }

    /// Open a span for an HTTP request about to be sent. `None` when
    /// telemetry is off or at its `Low` auto level.
    pub fn open_network_span(
        &mut self,
        request: &sys_types::network::NetworkRequest,
    ) -> Option<btel_types::TelemetryId> {
        self.telemetry.as_ref()?;
        let context = self.current_context().clone();
        let state = self.telemetry.as_mut()?;
        state.set_context(context);
        state.open_network_span(request)
    }

    /// Record an event of an open request, at an instant the IO side read.
    pub fn network_event(
        &mut self,
        span: btel_types::TelemetryId,
        at: ClockInstant,
        event: &sys_types::network::NetworkEventKind,
    ) {
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.network_event(span, at, event);
        }
    }

    /// End an open request's span. An `error` is the thrown value: its span
    /// records the `baml.errors.Context` a handler would bind, as a function
    /// span does, with each of `raw_urls` it quotes sanitized. The value the
    /// program sees is unchanged.
    pub fn close_network_span(
        &mut self,
        span: btel_types::TelemetryId,
        at: ClockInstant,
        outcome: InvocationOutcome,
        error: Option<Value>,
        raw_urls: &[&str],
    ) {
        if self.telemetry.is_none() {
            return;
        }
        let error = error.map(|error| self.error_context_value(error, &[], Value::NULL));
        let state = self.telemetry.as_mut().unwrap();
        // SAFETY: the engine calls this under the heap permit; neither the
        // context allocation above nor capture collects.
        unsafe { state.close_network_span(span, at, outcome, error, raw_urls) };
    }

    /// Name this thread's future for telemetry; call before its entry point.
    pub fn set_telemetry_thread_name(&mut self, name: &str) {
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.set_thread_name(name);
        }
    }

    /// How an escaping error ends an observed invocation: cancelled for a
    /// cancellation or `exit`, panicked for another panic, else errored.
    pub fn telemetry_outcome_for_exception(&self, value: Value) -> InvocationOutcome {
        let Some(ptr) = value.as_object_ptr() else {
            return InvocationOutcome::Errored;
        };
        let Object::Instance(instance) = self.get_object(ptr) else {
            return InvocationOutcome::Errored;
        };
        let is = |panic: PanicClass| {
            self.panic_class_ptrs
                .get(panic as usize)
                .is_some_and(|class| *class == instance.class)
        };
        // Exit ends the process on purpose: the frames it unwinds were
        // stopped, not failed.
        if is(PanicClass::Cancelled) || is(PanicClass::Exit) {
            InvocationOutcome::Cancelled
        } else if self.panic_class_ptrs.contains(&instance.class) {
            InvocationOutcome::Panicked
        } else {
            InvocationOutcome::Errored
        }
    }

    /// Resolve native continuations and pending hook targets to their caller.
    fn bytecode_caller_index(&self, frame_idx: usize) -> usize {
        let index = match &self.frames[frame_idx] {
            Frame::Bytecode(_) => frame_idx,
            Frame::Native(frame) => frame.bytecode_caller,
        };
        if let Some(pending) = self
            .pending_trace_hooks
            .iter()
            .find(|pending| pending.frame == index && pending.evaluating)
        {
            pending.caller.unwrap_or(index)
        } else {
            index
        }
    }

    /// A root entry's pending target is bookkeeping, not a visible caller of
    /// its hook. Keep the physical frame for execution without inventing a
    /// target-to-hook edge in the recorded call path.
    fn visible_bytecode_caller_index(&self, frame_idx: usize) -> Option<usize> {
        let index = self.bytecode_caller_index(frame_idx);
        match self
            .pending_trace_hooks
            .iter()
            .find(|pending| pending.frame == index && pending.evaluating)
        {
            Some(pending) => pending.caller,
            None => Some(index),
        }
    }

    /// Only the engine knows whether a yielded sys-op is asynchronous.
    pub fn begin_sys_op_telemetry_wait(&mut self) {
        if let Some(frame_idx) = self.frames.len().checked_sub(1) {
            self.begin_telemetry_wait(frame_idx);
        }
    }

    #[allow(clippy::inline_always, reason = "semantic wait producer fast path")]
    #[inline(always)]
    fn begin_telemetry_wait(&mut self, frame_idx: usize) {
        let frame_idx = self.bytecode_caller_index(frame_idx);
        if matches!(&self.frames[frame_idx], Frame::Bytecode(frame) if frame.telemetry.is_some()) {
            debug_assert!(self.pending_telemetry_wait.is_none());
            self.pending_telemetry_wait = Some((
                frame_idx,
                self.telemetry
                    .as_ref()
                    .expect("observed wait has telemetry")
                    .clock()
                    .read(),
            ));
        }
    }

    /// Transfer the wait across a non-fallible engine parking scope. Its end
    /// clock is read before reacquisition, then the saved owner is charged.
    pub fn take_telemetry_wait(&mut self) -> Option<(usize, ClockInstant)> {
        self.pending_telemetry_wait.take()
    }

    fn finish_pending_telemetry_wait(&mut self) {
        if let Some((frame_idx, start)) = self.take_telemetry_wait() {
            let elapsed = start.elapsed_until(
                self.telemetry
                    .as_ref()
                    .expect("observed wait has telemetry")
                    .clock()
                    .read(),
            );
            self.record_telemetry_wait(Some(frame_idx), elapsed);
        }
    }

    /// Charge a completed semantic wait after reacquiring the heap permit.
    /// Its duration was already measured, so reacquisition time is excluded.
    pub fn record_telemetry_wait(&mut self, frame_idx: Option<usize>, elapsed: ClockDuration) {
        if let Some(frame_idx) = frame_idx
            && let Some(Frame::Bytecode(frame)) = self.frames.get_mut(frame_idx)
            && let Some(telemetry) = &mut frame.telemetry
        {
            telemetry.add_await(elapsed);
        }
    }

    /// Cold one-time cleanup at existing VM entry/resume/finalization boundaries.
    /// Frames remain executable; only observation state and GC roots are released.
    #[cfg(not(target_arch = "wasm32"))]
    fn stop_disabled_telemetry(&mut self) {
        if self
            .telemetry
            .as_ref()
            .is_some_and(TelemetryState::is_disabled)
        {
            self.telemetry.take().unwrap().abandon();
            self.pending_telemetry_wait = None;
            for frame in &mut self.frames {
                if let Frame::Bytecode(frame) = frame {
                    frame.telemetry = None;
                }
            }
        }
    }

    /// Finalize every still-open observed invocation and then the logical
    /// thread. The engine calls this exactly once when it chooses a terminal
    /// outcome (including cancellation races and explicit process exit).
    /// A spawned future's body starts running now, after waiting to be
    /// admitted. Its entry frame was entered when it was spawned, so that
    /// invocation is restamped to start now too: the wait is the future's,
    /// not its body's.
    pub fn mark_telemetry_running(&mut self) {
        if self.telemetry.is_none() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_disabled_telemetry();
        #[cfg(not(target_arch = "wasm32"))]
        let _telemetry_scope = self.telemetry.as_ref().map(TelemetryState::execution_scope);
        self.restamp_entry_as_running();
    }

    fn restamp_entry_as_running(&mut self) {
        let Some(at) = self
            .telemetry
            .as_mut()
            .and_then(TelemetryState::mark_running)
        else {
            return;
        };
        self.restamp_entry(at);
    }

    fn restamp_entry(&mut self, at: btel_types::ClockInstant) {
        for frame in &mut self.frames {
            if let Frame::Bytecode(frame) = frame
                && let Some(telemetry) = &mut frame.telemetry
            {
                telemetry.entered_at = at;
            }
        }
    }

    pub fn finish_telemetry(&mut self, outcome: InvocationOutcome) {
        if self.telemetry.is_none() {
            return;
        }
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_disabled_telemetry();
        #[cfg(not(target_arch = "wasm32"))]
        let _telemetry_scope = self.telemetry.as_ref().map(TelemetryState::execution_scope);
        if self.telemetry.is_none() {
            return;
        }
        // A future cancelled before it ran: its entry frame starts now, and
        // `complete_thread` marks it running at the instant it completes.
        if let Some(telemetry) = self.telemetry.as_ref().filter(|t| t.is_waiting()) {
            let at = telemetry.clock().read();
            self.restamp_entry(at);
        }
        self.finish_pending_telemetry_wait();
        // Engine-side cancellation can terminate a suspended VM without an
        // exception value on its stack. Capture the same panic value that the
        // engine returns, without dispatching it into user code.
        let error = (outcome == InvocationOutcome::Cancelled
            && self
                .frames
                .iter()
                .any(|frame| matches!(frame, Frame::Bytecode(frame) if frame.telemetry.is_some()))
            && self
                .panic_class_ptrs
                .get(PanicClass::Cancelled as usize)
                .is_some_and(|class| !class.is_null()))
        .then(|| self.panic_to_exception_value(VmPanic::Cancelled))
        .map(|error| self.error_context_value(error, &[], Value::NULL));
        for frame_idx in (0..self.frames.len()).rev() {
            self.complete_bytecode_invocation(frame_idx, outcome, error);
        }
        self.telemetry.as_mut().unwrap().complete_thread(outcome);
    }

    pub fn telemetry_clock(&self) -> Option<&Arc<btel_clock::ClockEpoch>> {
        self.telemetry.as_ref().map(TelemetryState::clock)
    }
    pub fn is_telemetry_root(&self) -> bool {
        self.telemetry
            .as_ref()
            .is_some_and(TelemetryState::is_root_thread)
    }

    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    /// Wraps `exec_inner` to convert `InternalError` → `TracedInternalError`
    /// with a captured stack trace.
    pub fn exec(&mut self) -> Result<VmExecState, VmError> {
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_disabled_telemetry();
        #[cfg(not(target_arch = "wasm32"))]
        let _telemetry_scope = self.telemetry.as_ref().map(TelemetryState::execution_scope);
        self.finish_pending_telemetry_wait();
        // Keep GC polling progress across engine handoffs. Returning from
        // exec (for example, to spawn a child) does not necessarily release
        // the heap permit. The checker resets its own counter when it polls.

        // DEV_BAML_VM_KPERF: read PMCs around this exec() on the current worker thread.
        let kp = crate::kperf::enabled();
        let (kp_start, ops_start) = if kp {
            (crate::kperf::exec_start(), self.op_count)
        } else {
            (None, 0)
        };

        // The engine may have allocated on this VM's behalf while it was not
        // running (converting arguments or a sys-op result).
        self.settle();

        let result = match self.exec_inner() {
            Err(VmError::InternalError(err)) => {
                let trace = self.capture_stack_trace();
                Err(VmError::TracedInternalError { source: err, trace })
            }
            other => other,
        };

        // The VM may now park for a long time: leave nothing unsettled.
        self.tlab.flush_alloc_debt();
        self.settle();

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
        self.try_handle_external_thrown(VmThrown::fresh(exception_value))
    }

    pub fn try_handle_external_thrown(&mut self, thrown: VmThrown) -> Result<(), VmError> {
        #[cfg(not(target_arch = "wasm32"))]
        self.stop_disabled_telemetry();
        #[cfg(not(target_arch = "wasm32"))]
        let _telemetry_scope = self.telemetry.as_ref().map(TelemetryState::execution_scope);
        self.finish_pending_telemetry_wait();
        let exception_value = thrown.value;
        if self.frames.is_empty() {
            self.record_unwound_without_frames(&thrown);
            let trace = self.capture_user_stack_trace();
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
                self.record_unwound_without_frames(&thrown);
                let trace = self.capture_user_stack_trace();
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
        self.unwind_exception(&mut frame_idx, &mut function, thrown, RaiseEntry::Host)
    }

    /// An injected failure with no bytecode frame to unwind still is a raise.
    #[cold]
    fn record_unwound_without_frames(&mut self, thrown: &VmThrown) {
        if let Some(evidence) =
            self.begin_error_evidence(thrown, RaiseEntry::Host, None, None, false)
        {
            self.error_not_caught(evidence, btel_records::UnwindResult::Unhandled);
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

        #[allow(clippy::cast_precision_loss)]
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
            // Float/float that emit could not specialize, or a mixed
            // `int`/float pair whose float side was statically erased. Both
            // route through `float_order`, the same definition the specialized
            // `CmpFloat*` opcodes use, so an erased operand never changes an
            // answer — including against NaN, which this arm can see.
            Value::bool(bex_vm_types::float_order::apply(op, l, r))
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
    ///
    /// The generic `BinOp` opcode: emit falls back to it whenever it declines a
    /// specialized `*Int`/`*Float`/`*Bigint` opcode (an operand read from a
    /// spawn-shared cell, an operand whose static type it cannot resolve). It
    /// must therefore be total over every operand pair the specialized opcodes
    /// accept — including bigint and the `bigint`/`int` mix — plus string
    /// concatenation for `+`.
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
            // IEEE-754 throughout, exactly like the specialized `DivFloat`
            // opcode and the `float` operator impls: a zero divisor yields
            // `±inf` (or `NaN` for `0.0 / 0.0`). Only the integer branch above
            // panics, because it has no value to represent the result with.
            let f = match op {
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
        } else if let Some(l) = self.bigint_operand_of(left)
            && let Some(r) = self.bigint_operand_of(right)
        {
            // Spawn-shared operands intentionally use generic opcodes because
            // another task may update the cell between evaluations. Preserve
            // bigint semantics on that path instead of falling through to the
            // object/object string-concatenation case.
            self.bigint_binop(op, l, r)?
        } else if left.is_object() && right.is_object() && op == BinOp::Add {
            let ls = self.as_string(&left)?;
            let rs = self.as_string(&right)?;
            let result = bex_str::BexStr::concat(ls.clone(), rs.clone());
            let value = Value::object(self.alloc_string(result));
            // One instruction can build a string of any size.
            self.settle();
            value
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

                let resumed = nf.continuation.call(self, v);
                self.settle();
                match resumed {
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
                            bytecode_caller: nf.bytecode_caller,
                        }));

                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args)?;
                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        self.stack.extend(callback_args);

                        let ecflo_outcome = self.execute_call_from_locals_offset_with_type_args(
                            real_callee,
                            cb_locals,
                            arg_count,
                            CallOptions {
                                type_args: &callback_type_args,
                                type_values: &[],
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
            let mut code: &'static [u8] = &function.bytecode.compact.as_ref().unwrap().code;
            let Frame::Bytecode(bf) = &mut self.frames[frame_idx] else {
                unreachable!(
                    "exec_compact loop frame is always Bytecode after continuation handler"
                );
            };
            let mut pc = bf.instruction_ptr;

            // Exact calls and bytecode returns stay in `run_compact`. Other
            // transitions, unwinds, yields, and errors reach this handler. The
            // dispatch frame tracks the last instruction's frame, including
            // direct transitions, so PC writeback remains correct on handoff.
            loop {
                let mut dispatch_frame_idx = frame_idx;
                let step_result = self.run_compact(
                    &mut pc,
                    &mut frame_idx,
                    &mut function,
                    &mut code,
                    &mut dispatch_frame_idx,
                );
                if step_result.is_err() {
                    self.pending_call_trace = None;
                }

                match step_result {
                    Ok(None) if frame_idx == dispatch_frame_idx => {
                        // A native call or same-frame catch can finish without
                        // changing frames. Resume the current code at its new PC.
                        continue;
                    }
                    Ok(None) => {
                        // Frame changed (Call/Return) — Call saved pc before the
                        // call. Re-extract code/function for the new frame.
                        break;
                    }
                    Ok(Some(state)) => {
                        // Yielding — save pc to current frame so we can resume.
                        if frame_idx == dispatch_frame_idx {
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

    /// Run compact instructions, switching directly on exact calls and returns
    /// to bytecode callers. Native continuations and other frame transitions
    /// return to `exec_compact`, which retains PC writeback and error handling.
    ///
    /// `code` and `dispatch_frame_idx` follow direct transitions. Update them
    /// only after the early-yield check: a handoff must still distinguish the
    /// instruction's frame from a newly pushed callee or restored caller.
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
    fn run_compact(
        &mut self,
        pc: &mut usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        code: &mut &'static [u8],
        dispatch_frame_idx: &mut usize,
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

        loop {
            // Read opcode byte and advance PC past it.
            // SAFETY: PC is always in bounds (bytecode invariant).
            // Read opcode byte and advance PC past it.
            // SAFETY: PC is always in bounds (bytecode invariant).
            #[allow(unsafe_code)]
            let op_byte = unsafe { *code.get_unchecked(*pc) };
            *pc += 1;
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
            // floats and apply the operator through `float_order` — BAML's total
            // float order, so these stay identical to the `exec_cmpop` fallback and
            // to `baml.ops.Compare for float`. Both operands are guaranteed Float by
            // the bytecode encoder, so the `else` arms are unreachable.
            macro_rules! cmp_int_op {
            ($op:tt) => {{
                let r = self.stack.get_at(self.stack.ensure_slot_from_top(0));
                let l = self.stack.get_at(self.stack.ensure_slot_from_top(1));
                // SAFETY: the encoder validates two comparison operands.
                self.stack
                    .replace_top_n::<2>(Value::bool((l.bits() as i64) $op (r.bits() as i64)));
            }};
        }
            macro_rules! cmp_float_op {
                ($op:expr) => {{
                    let Some(r) =
                        value_as_float(self.stack.get_at(self.stack.ensure_slot_from_top(0)))
                    else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) =
                        value_as_float(self.stack.get_at(self.stack.ensure_slot_from_top(1)))
                    else {
                        std::hint::unreachable_unchecked()
                    };
                    // SAFETY: the encoder validates two comparison operands.
                    self.stack
                        .replace_top_n::<2>(Value::bool(bex_vm_types::float_order::apply(
                            $op, l, r,
                        )));
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
                        let Frame::Bytecode(bf) =
                            (unsafe { self.frames.get_unchecked(*frame_idx) })
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
                        let Frame::Bytecode(bf) =
                            (unsafe { self.frames.get_unchecked(*frame_idx) })
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
                        let Frame::Bytecode(bf) =
                            (unsafe { self.frames.get_unchecked(*frame_idx) })
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
                        let Frame::Bytecode(bf) =
                            (unsafe { self.frames.get_unchecked(*frame_idx) })
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
                            Err(length) => {
                                return Err(self.invalid_field_access_error(slot, length));
                            }
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
                        // SAFETY: the encoder validates the instruction's stack effect.
                        self.stack.truncate_suffix(n);
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
                        // Nonempty allocations are compiler-generated string maps.
                        // General-key literals use yielding Map.set calls.
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
                    OpCode::SysOp => {
                        if self.pending_call_trace.take().is_some() {
                            return Err(self.trace_attachment_error(
                                "$trace is not supported on system operations",
                            ));
                        }
                        let raw = { read_u32_unchecked(code, pc) };

                        let callee = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let callee_value = self.load_global_in(function.runtime_package, callee);
                        // `as_object_ptr` only unwraps the value to a heap pointer
                        // (the `FunctionType` argument is error-message metadata, not
                        // an assertion). `dispatch_sysop_yield`'s own kind check is
                        // therefore the load-bearing validation on this path — it
                        // rejects a non-sys-op global before draining and yields.
                        let callee_ptr =
                            self.as_object_ptr(callee_value, FunctionType::SysOp.into())?;
                        let state = self.dispatch_sysop_yield(callee_ptr)?;
                        return Ok(Some(state));
                    }

                    // ── Spawn (BEP-034) ────────────────────────────────────────────
                    OpCode::Spawn => {
                        // The operand is a `baml.spawn.Plan` instance. Anything
                        // else here would turn a local type error into a
                        // VM→engine contract break downstream.
                        let plan_value = self.stack.ensure_pop();
                        self.as_instance(&plan_value)?;
                        let plan = plan_value
                            .as_object_ptr()
                            .unwrap_or_else(|| unreachable!("an instance is an object"));
                        let telemetry = if self.telemetry.is_some() {
                            let actual_caller = self.frame_function_identity(*frame_idx);
                            let caller_pc = u32::try_from(self.cur_pc).unwrap_or(u32::MAX);
                            let body = crate::package_baml::plan_body(self, plan_value)?;
                            let callee = self.callable_function_identity(body);
                            self.telemetry.as_mut().map(|telemetry| match callee {
                                Some(callee) => telemetry.spawn_context(
                                    actual_caller,
                                    caller_pc,
                                    callee,
                                    |caller, callee| unsafe {
                                        Self::register_call_path_functions(
                                            &self.heap, caller, callee,
                                        )
                                    },
                                ),
                                None => telemetry.hidden_spawn_context(),
                            })
                        } else {
                            None
                        };
                        return Ok(Some(VmExecState::Spawn {
                            plan,
                            telemetry,
                            context: self.current_context().clone(),
                        }));
                    }

                    // ── Call ──────────────────────────────────────────────────────
                    OpCode::BeginTraceHook => {
                        let with_settings = read_i8_unchecked(code, pc) != 0;
                        let value = self.begin_trace_hook(*frame_idx, with_settings)?;
                        self.stack.push(value);
                    }
                    OpCode::TraceHookHidden
                    | OpCode::TraceHookTiming
                    | OpCode::TraceHookSpan
                    | OpCode::TraceHookRich
                    | OpCode::TraceHookEmptySpan => {
                        let returned =
                            (self.telemetry.is_some() && !self.hooks_suppressed()).then(|| {
                                CallTrace {
                                    telemetry: crate::telemetry::TraceConfig {
                                        mode: Some(match op {
                                            OpCode::TraceHookHidden => {
                                                btel_types::InvocationMode::Hidden
                                            }
                                            OpCode::TraceHookTiming => {
                                                btel_types::InvocationMode::Timing
                                            }
                                            _ => btel_types::InvocationMode::Span,
                                        }),
                                        inputs: if op == OpCode::TraceHookEmptySpan {
                                            Some(false)
                                        } else {
                                            (op == OpCode::TraceHookRich).then_some(true)
                                        },
                                        output: if op == OpCode::TraceHookEmptySpan {
                                            Some(false)
                                        } else {
                                            (op == OpCode::TraceHookRich
                                                || op == OpCode::TraceHookSpan)
                                                .then_some(true)
                                        },
                                        error: if op == OpCode::TraceHookEmptySpan {
                                            Some(false)
                                        } else {
                                            (op == OpCode::TraceHookRich
                                                || op == OpCode::TraceHookSpan)
                                                .then_some(true)
                                        },
                                        ..Default::default()
                                    },
                                    context: None,
                                }
                            });
                        self.finish_trace_hook_config(*frame_idx, returned)?;
                    }
                    OpCode::EndTraceHook => {
                        let result = self.stack.ensure_pop();
                        self.finish_trace_hook(*frame_idx, result)?;
                    }
                    OpCode::SetCallTrace => {
                        debug_assert!(
                            matches!(
                                code.get(*pc)
                                    .copied()
                                    .and_then(|byte| OpCode::try_from(byte).ok()),
                                Some(
                                    OpCode::Call
                                        | OpCode::CallExactArgs
                                        | OpCode::CallHooked
                                        | OpCode::CallIndirect
                                        | OpCode::VirtualCall
                                )
                            ),
                            "SetCallTrace must be followed by a call instruction"
                        );
                        let value = self.stack.ensure_pop();
                        self.pending_call_trace = Some(self.trace_config(value)?);
                    }
                    OpCode::Call | OpCode::CallExactArgs | OpCode::CallHooked => {
                        let raw = read_u32_unchecked(code, pc);
                        let ntypeargs = usize::from(read_u16_unchecked(code, pc));

                        let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let callee_value =
                            self.load_global_in(function.runtime_package, callee_global);

                        if op == OpCode::CallExactArgs
                            && self.pending_call_trace.is_none()
                            && self.pending_call_type_args.is_empty()
                            && self.pending_call_type_values.is_empty()
                        {
                            let callee_ptr =
                                self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                            // SAFETY: the active heap permit prevents collection
                            // during frame/stack growth, just as in general call
                            // setup. Reload from rooted frames after any yield.
                            let callee_object: &'static Object = callee_ptr.get();
                            if let Object::Function(callee) = callee_object
                                && matches!(callee.kind, FunctionKind::Bytecode)
                            {
                                debug_assert_eq!(ntypeargs, 0);
                                // Eager ok_or constructs/drops VmInternalError
                                // even on success; release assembly retains a
                                // destructor call for that large enum.
                                #[expect(clippy::unnecessary_lazy_evaluations)]
                                let locals_offset = StackIndex::from_raw(
                                    self.stack.len().checked_sub(callee.arity).ok_or_else(
                                        || VmInternalError::NotEnoughItemsOnStack(callee.arity),
                                    )?,
                                );
                                if self.frames.len() >= MAX_FRAMES {
                                    return Err(VmError::thrown_fresh(
                                        self.panic_to_exception_value(VmPanic::StackOverflow),
                                    ));
                                }
                                let context = self.current_context().clone();
                                if let Some(telemetry) = &mut self.telemetry {
                                    telemetry.set_context(context.clone());
                                }
                                let frame_telemetry = if self.telemetry.is_some() {
                                    let actual_caller = self.frame_function_identity(*frame_idx);
                                    let caller_is_observed = matches!(
                                        self.frames.get(*frame_idx),
                                        Some(Frame::Bytecode(BytecodeFrame {
                                            telemetry: Some(_),
                                            ..
                                        }))
                                    );
                                    let caller_pc = u32::try_from(self.cur_pc).unwrap_or(u32::MAX);
                                    let args = &self.stack.0[locals_offset.raw()
                                        ..locals_offset.raw().saturating_add(callee.arity)];
                                    // SAFETY: arguments and callable remain live under the heap permit.
                                    self.telemetry.as_mut().and_then(|telemetry| unsafe {
                                        telemetry.enter_bytecode(
                                            callee,
                                            callee_ptr,
                                            actual_caller,
                                            caller_pc,
                                            caller_is_observed,
                                            args,
                                            |caller, callee| {
                                                Self::register_call_path_functions(
                                                    &self.heap, caller, callee,
                                                )
                                            },
                                        )
                                    })
                                } else {
                                    None
                                };
                                let Frame::Bytecode(caller) = &mut self.frames[*frame_idx] else {
                                    verifier_unreachable!()
                                };
                                caller.instruction_ptr = *pc;
                                caller.faulting_pc = self.cur_pc;
                                self.frames.extend(std::iter::once_with(|| {
                                    Frame::Bytecode(BytecodeFrame {
                                        function: callee_ptr,
                                        instruction_ptr: 0,
                                        locals_offset,
                                        type_args: Vec::new(),
                                        type_metadata: None,
                                        faulting_pc: 0,
                                        telemetry: frame_telemetry,
                                        context,
                                    })
                                }));
                                if callee.real_local_count != 0 {
                                    let new_len = self.stack.len() + callee.real_local_count;
                                    self.stack.resize(new_len, Value::NULL);
                                }
                                *frame_idx = self.frames.len() - 1;
                                *function = callee;
                                if self.should_early_yield() {
                                    return Ok(Some(VmExecState::EarlyYield));
                                }
                                // No engine handoff: enter the known bytecode
                                // callee directly. Update the dispatch origin so
                                // later fallback calls/yields write back the PC
                                // of the instruction that actually produced them.
                                *pc = 0;
                                *code = &callee.bytecode.compact.as_ref().unwrap().code;
                                *dispatch_frame_idx = *frame_idx;
                                continue;
                            }
                        }

                        let (callee_ptr, arg_count) = self.resolve_callable_target(callee_value)?;
                        let arg_count = match Self::recorded_call_layout(function, self.cur_pc) {
                            Some(layout) => self.remap_call_arguments(layout, callee_ptr)?,
                            None => arg_count,
                        };

                        let args_offset =
                            self.stack.len().checked_sub(arg_count + ntypeargs).ok_or(
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
                                frame_idx,
                                function,
                            )
                        } else {
                            let type_args =
                                self.take_type_args_below_values(ntypeargs, arg_count)?;
                            self.execute_call_from_locals_offset_with_type_args(
                                callee_ptr,
                                locals_offset,
                                arg_count,
                                CallOptions {
                                    type_args: &type_args.tys,
                                    type_values: &type_args.values,
                                },
                                frame_idx,
                                function,
                            )
                        };
                        return result;
                    }

                    // ── VirtualCall ───────────────────────────────────────────────
                    // Open-world interface dispatch: resolve the method at runtime
                    // from the concrete `Self` type of value argument `self_arg`
                    // (the method's one `Self`-typed parameter — the `self`
                    // receiver at 0 in the common case), then take the shared
                    // frame-push call path (mirrors `Call`). Stack layout (top
                    // last): `[arg_0, …, arg_{nargs-1}, iface_type, method_name]`.
                    OpCode::VirtualCall => {
                        let nargs = read_u16_unchecked(code, pc) as usize;
                        let ntypeargs = usize::from(read_u16_unchecked(code, pc));
                        let self_arg = usize::from(read_u16_unchecked(code, pc));

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
                        // `Self` is the dispatch argument's runtime concrete type;
                        // coherence makes `(Self, iface<args>)` resolve to at most one
                        // impl. Off that rule the method resolves through
                        // `rule_method_impl` (the provided row, or the interface's
                        // default on a miss). `nargs` is the interface method's slot
                        // count in declared order — `self_arg` indexes it before the
                        // remap below, which only widens the resolved impl's optional
                        // lane. The rule borrows `self`; scope it so the borrow ends
                        // before the `&mut self` call below.
                        let receiver = self.stack[StackIndex::from_raw(args_offset + self_arg)];
                        let cache_key = if function.runtime_package.is_null() {
                            let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
                            let Object::Type(type_value) = self.get_object(iface_ptr) else {
                                unreachable!("as_object_ptr(Type) guarantees a Type object")
                            };
                            let (interface_head, interface_args) = match &type_value.ty {
                                bex_vm_types::RealizedTy::Interface(head, args, _) => {
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
                                let iface_ptr =
                                    self.as_object_ptr(iface_value, ObjectType::Type)?;
                                match self.get_object(iface_ptr) {
                                    // The interface's input args select among a type's
                                    // impls of the same interface at several
                                    // instantiations. Associated types are outputs,
                                    // not part of the resolver key.
                                    Object::Type(type_value) => match &type_value.ty {
                                        bex_vm_types::RealizedTy::Interface(qtn, args, _assoc) => {
                                            (*qtn, args.clone())
                                        }
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
                            // `Self` is the dispatch value's realized concrete type.
                            let self_ty = bex_vm_types::RealizedTy::from(
                                self.value_concrete_ty(receiver).unwrap_or_else(|| {
                                    unreachable!(
                                    "value of kind {:?} cannot be a virtual-call dispatch argument",
                                        self.type_of(&receiver)
                                    )
                                }),
                            );
                            let resolver =
                                crate::package_baml::ImplResolver::for_value(self, receiver);
                            let implementation = resolver
                                .resolve_implementation(&self_ty, iface_qtn, &iface_args)
                                .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                                    method: method_name.clone(),
                                })?;
                            let (callee, frame) = resolver
                                .implementation_method(&implementation, method_name.as_str())?;
                            let cacheable = implementation.is_static();
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
                        let nargs = match Self::recorded_call_layout(function, self.cur_pc) {
                            Some(layout) => self.remap_call_arguments(layout, callee_ptr)?,
                            None => nargs,
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
                                frame_idx,
                                function,
                            )
                        } else {
                            self.execute_call_from_locals_offset_with_type_args(
                                callee_ptr,
                                locals_offset,
                                nargs,
                                CallOptions {
                                    type_args: &type_args,
                                    type_values: &type_values,
                                },
                                frame_idx,
                                function,
                            )
                        };
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
                            let trace = self.pending_call_trace.take();
                            let _popped_callee = self.stack.ensure_pop();
                            let arity = match Self::recorded_call_layout(function, self.cur_pc) {
                                Some(layout) => self.remap_call_arguments(layout, callee_ptr)?,
                                None => arity,
                            };
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

                            let state = self.host_closure_call_sysop(callee_ptr, user_args, trace);
                            return Ok(Some(state));
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
                            let receiver = bm.receiver;
                            let _popped = self.stack.ensure_pop();
                            // The site pushed the method's visible slots; the
                            // receiver joins them as the leading required slot.
                            let recorded = Self::recorded_call_layout(function, self.cur_pc);
                            let pushed =
                                recorded.map_or(full_arity - 1, baml_type::CallLayout::len);
                            let args_offset = self
                                .stack
                                .len()
                                .checked_sub(pushed)
                                .ok_or(VmInternalError::NotEnoughItemsOnStack(pushed))?;
                            self.stack.insert(args_offset, receiver);
                            let full_arity = match recorded {
                                Some(layout) => {
                                    let mut caller = layout.clone();
                                    caller.0.insert(0, None);
                                    self.remap_call_arguments(&caller, callee_ptr)?
                                }
                                None => full_arity,
                            };
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
                                frame_idx,
                                function,
                            )? {
                                return Ok(Some(state));
                            }
                        } else {
                            let (callee_ptr, arg_count) =
                                self.resolve_callable_target(callee_value)?;
                            let _popped_callee = self.stack.ensure_pop();
                            let arg_count = match Self::recorded_call_layout(function, self.cur_pc)
                            {
                                Some(layout) => self.remap_call_arguments(layout, callee_ptr)?,
                                None => arg_count,
                            };
                            let args_offset = self
                                .stack
                                .len()
                                .checked_sub(arg_count)
                                .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count))?;
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
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                        // Return before fetching from potentially changed frame code.
                        return Ok(None);
                    }

                    // ── Return ────────────────────────────────────────────────────
                    OpCode::Return => {
                        let result = self.stack.get_at(self.stack.ensure_stack_top());

                        let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                            unreachable!()
                        };
                        let locals_offset = bf.locals_offset;
                        let frame_telemetry = bf.telemetry.take();

                        if let Some(frame_telemetry) = frame_telemetry {
                            self.complete_bytecode_invocation_with_function(
                                *frame_idx,
                                function,
                                frame_telemetry,
                                InvocationOutcome::Ok,
                                Some(result),
                            );
                        }

                        // SAFETY: the compiler guarantees a result at or above the
                        // callee's locals base. That base is therefore an existing
                        // initialized slot, even for a zero-argument function with
                        // no locals. Value is Copy, so discarded slots need no drop.
                        // No allocation or GC point occurs between writing the
                        // result and shortening the stack to the caller's new top.
                        let consumed = self.stack.len() - locals_offset.raw();
                        self.stack.replace_top_n_dynamic(consumed, result);
                        // SAFETY: this instruction is executing the bytecode frame
                        // accessed above; stack operations cannot remove that frame.
                        unsafe { self.frames.no_return_pop() };
                        let context = self.current_context().clone();
                        if let Some(telemetry) = &mut self.telemetry {
                            telemetry.set_context(context);
                        }
                        // Select the restored caller. Native continuations and
                        // early yields still hand this frame back to the outer loop.
                        if !self.frames.is_empty() {
                            *frame_idx = self.frames.len() - 1;
                        }
                        if self.frames.is_empty() {
                            return Ok(Some(VmExecState::Complete(self.stack.ensure_pop())));
                        }

                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                        if let Frame::Bytecode(caller) = &self.frames[*frame_idx] {
                            *pc = caller.instruction_ptr;
                            *function = self.load_function(*frame_idx)?;
                            *code = &function.bytecode.compact.as_ref().unwrap().code;
                            *dispatch_frame_idx = *frame_idx;
                            continue;
                        }
                        // Native continuations still need the outer loop.
                        return Ok(None);
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
                                    self.begin_telemetry_wait(*frame_idx);
                                    return Ok(Some(VmExecState::Await(future_id)));
                                }
                                FutureRead::Ready(v) => v,
                                FutureRead::Error(value) => {
                                    awaiting.mark_observed();
                                    // The producer's trace belongs to this error. A catch
                                    // in a stdlib collector may have no user frames of its
                                    // own, so capturing only the awaiter's stack loses it.
                                    let trace = awaiting.error_trace();
                                    let thrown = if trace.is_empty() {
                                        VmThrown {
                                            value,
                                            throw_kind: bex_vm_types::errors::ThrowKind::Rethrow,
                                            context: None,
                                        }
                                    } else {
                                        let context =
                                            self.alloc_error_context(value, &trace, Value::NULL);
                                        VmThrown::rethrow(value, context)
                                    };
                                    return Err(VmError::Thrown(thrown));
                                }
                                FutureRead::Cancelled => {
                                    let value = self.panic_to_exception_value(VmPanic::Cancelled);
                                    return Err(VmError::Thrown(VmThrown::fresh(value)));
                                }
                                FutureRead::InternalError(future_id) => {
                                    // Yield back to the engine; it will surface the original
                                    // error from the FutureManager's `SetOnce` (the entry is
                                    // leaked by design for InternalError).
                                    *pc -= AWAIT_OPCODE_LEN;
                                    self.begin_telemetry_wait(*frame_idx);
                                    return Ok(Some(VmExecState::Await(future_id)));
                                }
                            }
                        };
                        // SAFETY: the future operand remains on top until ready.
                        self.stack.replace_top_n::<1>(ready_value);
                        if self.should_early_yield() {
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
                                // SAFETY: the input array remains on top until ready.
                                self.stack.replace_top_n::<1>(Value::int(i as i64));
                                if self.should_early_yield() {
                                    return Ok(Some(VmExecState::EarlyYield));
                                }
                            }
                            None => {
                                // No input has settled yet — park until the first
                                // does. Rewind pc so the opcode re-executes (with
                                // the array still on the stack) once resumed.
                                *pc -= AWAIT_ANY_OPCODE_LEN;
                                self.begin_telemetry_wait(*frame_idx);
                                return Ok(Some(VmExecState::AwaitAny(pending_ids)));
                            }
                        }
                    }

                    // ── Throw ─────────────────────────────────────────────────────
                    OpCode::Throw | OpCode::Rethrow => {
                        let thrown = if op == OpCode::Rethrow {
                            let context = self.stack.ensure_pop();
                            VmThrown::rethrow(self.stack.ensure_pop(), context)
                        } else {
                            VmThrown::fresh(self.stack.ensure_pop())
                        };
                        // Save pc before unwinding (handler lookup needs it).
                        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
                            bf.instruction_ptr = *pc;
                        }
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
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                        // Return before fetching from potentially changed frame code.
                        return Ok(None);
                    }

                    // ── Jump opcodes ──────────────────────────────────────────────
                    OpCode::Jump => {
                        let offset = read_i32_unchecked(code, pc);
                        // offset is relative to instruction end (current pc)
                        *pc = (*pc as i64 + offset as i64) as usize;
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                    }

                    OpCode::PopJumpIfFalse | OpCode::PopJumpIfTrue => {
                        let offset = read_i32_unchecked(code, pc);
                        let cond = self.stack.ensure_pop();
                        if cond == Value::bool(op == OpCode::PopJumpIfTrue) {
                            *pc = (*pc as i64 + offset as i64) as usize;
                        }
                        if self.should_early_yield() {
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

                    OpCode::JumpIfFalseOrPop
                    | OpCode::JumpIfTrueOrPop
                    | OpCode::JumpIfNotNullOrPop => {
                        let offset = read_i32_unchecked(code, pc);
                        let cond = self.stack[self.stack.ensure_stack_top()];
                        let taken = match op {
                            OpCode::JumpIfFalseOrPop => cond == Value::bool(false),
                            OpCode::JumpIfTrueOrPop => cond == Value::bool(true),
                            OpCode::JumpIfNotNullOrPop => !cond.is_null(),
                            _ => unreachable!(),
                        };
                        if taken {
                            *pc = (*pc as i64 + offset as i64) as usize;
                        } else {
                            self.stack.ensure_pop();
                        }
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
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
                        if self.should_early_yield() {
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
                        let value = self.stack.get_at(self.stack.ensure_stack_top());
                        let tag = value_type_tag(value);
                        // SAFETY: the encoder validates one input value.
                        self.stack.replace_top_n::<1>(Value::int(tag));
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
                    OpCode::DenseTag => {
                        let table_idx = read_u32_unchecked(code, pc) as usize;
                        let popped = self.stack.ensure_pop();
                        let Some(value) = popped.as_int() else {
                            return Err(VmInternalError::TypeError {
                                expected: bex_vm_types::types::Type::Int,
                                got: self.type_of(&popped),
                            }
                            .into());
                        };
                        let arm = function.bytecode.switch_tables[table_idx].arm_of(value);
                        self.stack.push(Value::int(arm.map_or(-1, i64::from)));
                    }

                    // ── ThrowIfPanic ──────────────────────────────────────────────
                    OpCode::ThrowIfPanic => {
                        let context = self.stack.ensure_pop();
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
                            self.try_unwind_exception(
                                frame_idx,
                                function,
                                VmThrown::rethrow(value, context),
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
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                        // Return before fetching from potentially changed frame code.
                        return Ok(None);
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

                            let exact_dynamic = if let bex_vm_types::TyTemplate::TypeArgRef(slot) =
                                &template
                            {
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
                                        template.substitute(&frame_type_args, self).map_err(
                                            |e| VmInternalError::TypeSubstitution {
                                                message: e.to_string(),
                                            },
                                        )?
                                    }
                                };
                                Value::object(self.tlab.alloc_type(TypeValue::new(ty)))
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
                        let type_value = self.type_operand_value(value)?;
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
                        let ordinal = read_u32_unchecked(code, pc) as usize;
                        let value =
                            crate::package_reflect::reflect::current_package_value(self, ordinal);
                        self.stack.push(value);
                    }

                    // ── MakeBoundMethod ───────────────────────────────────────────
                    OpCode::MakeBoundMethod => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let receiver = self.stack.ensure_pop();
                        let callee_value =
                            self.load_global_in(function.runtime_package, global_idx);
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
                    // realized frame type args (a blanket impl's or adopted
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
                                    match bex_lang::index::resolve_index(i, len) {
                                        Some(idx) => Ok(guard[idx]),
                                        None => Err((i, len)),
                                    }
                                }
                                Object::Uint8Array(bytes) => {
                                    let guard = bytes.lock();
                                    let len = guard.len();
                                    match bex_lang::index::resolve_index(i, len) {
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
                        unreachable!("map indexing must be lowered through baml.Map.index");
                    }

                    OpCode::StoreArrayElement => {
                        let new_value = self.stack.ensure_pop();
                        let index_value = self.stack.ensure_pop();
                        let array_value = self.stack.ensure_pop();
                        let array_object_index =
                            self.as_object_ptr(array_value, ObjectType::Array)?;
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
                                    let mut guard = arr.lock_mut(self.tlab.alloc_debt());
                                    let len = guard.len();
                                    match bex_lang::index::resolve_index(i, len) {
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
                                    let mut guard = bytes.lock_mut(self.tlab.alloc_debt());
                                    let len = guard.len();
                                    match bex_lang::index::resolve_index(i, len) {
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
                        unreachable!("map assignment must be lowered through baml.Map.set");
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
                        let r = self.stack.get_at(self.stack.ensure_slot_from_top(0));
                        let l = self.stack.get_at(self.stack.ensure_slot_from_top(1));
                        // Overflow-checked tagged add: hot path stays branchless
                        // (no untag/retag), cold path untags only for the message.
                        match Value::tagged_int_add_checked(l, r) {
                            // SAFETY: the encoder validates two arithmetic operands.
                            Some(v) => self.stack.replace_top_n::<2>(v),
                            None => {
                                // Preserve the old consumed-operand stack before
                                // constructing and propagating the overflow error.
                                self.stack.truncate_suffix(2);
                                return Err(self.tagged_int_overflow(l, '+', r));
                            }
                        }
                    }
                    OpCode::SubInt => {
                        let r = self.stack.get_at(self.stack.ensure_slot_from_top(0));
                        let l = self.stack.get_at(self.stack.ensure_slot_from_top(1));
                        match Value::tagged_int_sub_checked(l, r) {
                            // SAFETY: the encoder validates two arithmetic operands.
                            Some(v) => self.stack.replace_top_n::<2>(v),
                            None => {
                                // Preserve the old error-path stack before error creation.
                                self.stack.truncate_suffix(2);
                                return Err(self.tagged_int_overflow(l, '-', r));
                            }
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
                        // IEEE-754: a zero divisor yields `±inf`, and `0.0 / 0.0`
                        // yields `NaN`. Unlike `int` and `bigint`, `float` has
                        // values for those results, so there is nothing to panic
                        // about.
                        let right_v = self.stack.ensure_pop();
                        let left_v = self.stack.ensure_pop();
                        let Some(r) = value_as_float(right_v) else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(l) = value_as_float(left_v) else {
                            std::hint::unreachable_unchecked()
                        };
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
                    OpCode::CmpFloatEq => cmp_float_op!(CmpOp::Eq),
                    OpCode::CmpFloatNotEq => cmp_float_op!(CmpOp::NotEq),
                    OpCode::CmpFloatLt => cmp_float_op!(CmpOp::Lt),
                    OpCode::CmpFloatLtEq => cmp_float_op!(CmpOp::LtEq),
                    OpCode::CmpFloatGt => cmp_float_op!(CmpOp::Gt),
                    OpCode::CmpFloatGtEq => cmp_float_op!(CmpOp::GtEq),

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
                        let val = self.stack.get_at(self.stack.ensure_stack_top());
                        let truthy = self.is_truthy(val);
                        // SAFETY: the encoder validates one input value.
                        self.stack.replace_top_n::<1>(Value::bool(!truthy));
                    }
                    OpCode::Truthy => {
                        let val = self.stack.get_at(self.stack.ensure_stack_top());
                        let truthy = self.is_truthy(val);
                        // SAFETY: the encoder validates one input value.
                        self.stack.replace_top_n::<1>(Value::bool(truthy));
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
                                    return Err(self
                                        .integer_overflow(bex_lang::int::neg_overflow_message(n)));
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
                                Object::Bigint(bi) => {
                                    Some(std::sync::Arc::new(-bi.as_ref().clone()))
                                }
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
                        let source_location = if let Frame::Bytecode(bf) = &self.frames[*frame_idx]
                        {
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
                        if event_name == bex_vm_types::bytecode::LOG_EVENT {
                            let (level, name, body) = self.decode_log_event(data)?;
                            self.record_log(*frame_idx, level, name.clone(), body);
                            return Ok(Some(VmExecState::Log {
                                level,
                                event_name: name,
                                data: body,
                                source_location,
                            }));
                        }
                        return Ok(Some(VmExecState::Event {
                            event_name,
                            data,
                            source_location,
                        }));
                    }
                }
            } // end unsafe block
        }
    }
}

impl ::bex_vm_types::RootHaver for BexVm {
    fn collect_roots(&self, roots: &mut Vec<HeapPtr>) {
        // Stack values
        roots.extend(self.stack.iter().filter_map(Value::as_object_ptr));

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
        roots.extend(self.static_load_type_cache.values().copied());
        for transfer in &self.context_transfers {
            roots.extend(transfer.handling.as_object_ptr());
            roots.extend(transfer.target.as_object_ptr());
        }

        // Frame function pointers (needed once closures are heap-allocated)
        roots.extend(self.collect_frame_roots());
        if let Some(telemetry) = &self.telemetry {
            telemetry.collect_roots(roots);
        }

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
        for transfer in &mut self.context_transfers {
            for value in [&mut transfer.handling, &mut transfer.target] {
                if let Some(ptr) = value.as_object_ptr()
                    && let Some(&new_ptr) = roots.get(&ptr)
                {
                    *value = Value::object(new_ptr);
                }
            }
        }

        // Frame function pointers (needed once closures are heap-allocated)
        for frame in &mut self.frames {
            frame.forward_roots(roots);
        }
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.forward_roots(roots);
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
