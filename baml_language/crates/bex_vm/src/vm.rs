//! BEX VM - The synchronous bytecode interpreter.
//!
//! # Unsafe Code
//!
//! This module uses unsafe code for direct heap access during instruction
//! execution: `get_object(ptr)` dereferences a `HeapPtr` to read an object for
//! type checks, field access and method dispatch. A published object is never
//! written through `&mut Object`: its mutable parts synchronize themselves
//! (`LockedContainer`, `AtomicValueSlot`, `Future::settle_*`). The
//! exceptions are a `Reservation`, written once before it is published, and
//! [`BexVm::get_object_mut`], whose `unsafe` protocols cover packages and
//! classes under construction (see its contract).
//!
//! Safety is ensured by:
//! - Single-threaded execution: Each VM instance runs on one thread at a time
//! - TLAB exclusivity: VMs have exclusive write access to their allocation regions
//! - Object-level synchronization: every other write to a published object
//!   goes through the object's own lock, atomic or settle protocol
//! - GC coordination: Garbage collection only runs when VMs are at safepoints (yielded)

#![allow(unsafe_code)]

use std::{
    collections::HashMap,
    num::NonZeroU64,
    sync::{
        Arc,
        atomic::{AtomicU64, Ordering},
    },
};

use baml_type::{Int63, IntShiftError, Name, normalize::TypeContext};
use btel_types::context::{Context, ContextPatch};
use rustc_hash::FxHashMap;
use smallvec::SmallVec;

use crate::{
    compiled_rt::{CompiledBodies, FrameAt, Step},
    telemetry::{
        CallTypeArgs, ClockDuration, ClockInstant, FrameTelemetry, InvocationOutcome,
        TelemetryState,
    },
    vec_ext::CopyVecExt,
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
    EarlyYieldCheck, RootHaver, RootVisitor, TypeHead, Weak,
    types::{ErrorClass, FutureId},
};
use ::core::any::TypeId;
#[cfg(not(target_arch = "wasm32"))]
use ::core::sync::atomic::AtomicBool;
use bex_heap::{BexHeap, Finalizable, Tlab};
use bex_vm_types::{
    BinOp, ClassShape, CmpOp, Field, FieldRef, FunctionKind, FutureRead, GlobalIndex, HeapPtr,
    InstanceRef, Object, ObjectIndex, ObjectPool, ObjectType, PanicClass, PermitProof,
    RustDataArc as _, SlotKind, SlotKindMismatch, StackIndex, UnaryOp, Value, Variant, VmGlobals,
    bytecode::{self, FieldSites, Instruction},
    types::{
        BoundMethod, Closure, ConstValue, Function, FunctionOrigin, FunctionType, Type, TypeValue,
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
///
/// On drop it also keeps the collector's finalizable list complete (BEP-042):
/// a class the edit gave a finalizer invalidates the list, since its existing
/// instances were never registered.
pub struct ObjectMut<'a> {
    object: &'a mut Object,
    debt: &'a bex_vm_types::AllocDebt,
    before: usize,
    heap: &'a BexHeap,
    finalizable_before: Finalizable,
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

        self.heap
            .note_rewrite(self.finalizable_before, Finalizable::of(self.object));
    }
}

pub(crate) mod frame_room;
pub(crate) mod frame_stack;
use frame_stack::ThreadStackBudget;
pub use frame_stack::{FrameStack, THREAD_STACK_BUDGET_BYTES};

#[derive(Clone, Copy)]
struct CallOptions<'a> {
    type_args: &'a [bex_vm_types::RealizedTy],
}

/// How [`BexVm::call_native_direct`] ended.
enum NativeDirect {
    /// The call needs the general path; nothing was changed.
    NotTaken,
    /// The native ran: hand this to the engine loop, as the general path
    /// does.
    Ran(Option<VmExecState>),
}

/// An interface bound as the VM checks it: the interface, its type arguments,
/// and the associated types the bound pins.
pub type BoundParts = (
    bex_vm_types::TypeHead,
    Box<[bex_vm_types::RealizedTy]>,
    Box<[(Name, bex_vm_types::RealizedTy)]>,
);

/// What a generic parameter's interface bound says of a type: the one
/// judgment a `let type T extends I` statement and a host's type argument for
/// a bounded parameter both make.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum BoundCheck {
    /// The type implements the interface at the bound's arguments and pinned
    /// associated types.
    Satisfied,
    /// The type is not one concrete type (a union, an interface, `unknown`,
    /// `never`, or a literal type), so no implementation can be chosen for it.
    NotConcrete,
    /// The type is concrete but does not implement the bound.
    NotImplemented,
}

/// The type arguments a call passes, realized. A type argument carries the
/// declarations it names in its own heads, so nothing rides beside it: a
/// `type` value has no identity of its own, and `LoadType<T>` builds one from
/// the frame's realized slot.
#[derive(Clone, Debug, Default)]
struct TakenTypeArgs {
    tys: Vec<bex_vm_types::RealizedTy>,
}

/// One interface-dispatch site and what it dispatched on: the key of the
/// static virtual-call and virtual-field caches. Lookups go through the
/// borrowed [`VirtualSiteQuery`], which hashes and compares the same way, so
/// a hit reads the receiver's and the interface's type arguments in place
/// instead of cloning them into a key.
#[derive(Clone, Debug)]
struct VirtualSiteKey {
    caller_function_addr: usize,
    call_pc: usize,
    receiver: VirtualReceiverKey,
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

#[derive(Clone, Debug)]
enum VirtualReceiverKey {
    /// Static program classes have immutable, program-unique tags. Generic
    /// arguments remain part of identity.
    Class {
        type_tag: i64,
        type_args: Box<[bex_vm_types::RealizedTy]>,
    },
    /// Primitive/container receivers do not have a static class object to key
    /// through, so retain their realized type as the uncommon fallback.
    Other(bex_vm_types::RealizedTy),
}

/// The borrowed form of a [`VirtualSiteKey`]: the receiver's and the
/// interface's type arguments are read where they live (the instance and the
/// interface operand's type object).
#[derive(Debug, PartialEq)]
struct VirtualSiteQuery<'a> {
    caller_function_addr: usize,
    call_pc: usize,
    receiver: VirtualReceiverQuery<'a>,
    interface_head: bex_vm_types::TypeHead,
    interface_args: &'a [bex_vm_types::RealizedTy],
}

#[derive(Debug, PartialEq, Hash)]
enum VirtualReceiverQuery<'a> {
    Class {
        type_tag: i64,
        type_args: &'a [bex_vm_types::RealizedTy],
    },
    Other(&'a bex_vm_types::RealizedTy),
}

/// Type arguments are compared, not hashed. Hashing them walked every
/// argument's type on every dispatch, and a site rarely sees two
/// instantiations of one receiver class, so they would seldom separate two
/// entries anyway: entries that differ only there share a hash and are told
/// apart by equality.
impl std::hash::Hash for VirtualSiteQuery<'_> {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.caller_function_addr.hash(state);
        self.call_pc.hash(state);
        match self.receiver {
            VirtualReceiverQuery::Class {
                type_tag,
                type_args: _,
            } => type_tag.hash(state),
            VirtualReceiverQuery::Other(ty) => ty.hash(state),
        }
        self.interface_head.hash(state);
    }
}

impl VirtualSiteQuery<'_> {
    fn to_key(&self) -> VirtualSiteKey {
        VirtualSiteKey {
            caller_function_addr: self.caller_function_addr,
            call_pc: self.call_pc,
            receiver: match self.receiver {
                VirtualReceiverQuery::Class {
                    type_tag,
                    type_args,
                } => VirtualReceiverKey::Class {
                    type_tag,
                    type_args: type_args.into(),
                },
                VirtualReceiverQuery::Other(ty) => VirtualReceiverKey::Other(ty.clone()),
            },
            interface_head: self.interface_head,
            interface_args: self.interface_args.into(),
        }
    }
}

impl VirtualSiteKey {
    fn as_query(&self) -> VirtualSiteQuery<'_> {
        VirtualSiteQuery {
            caller_function_addr: self.caller_function_addr,
            call_pc: self.call_pc,
            receiver: match &self.receiver {
                VirtualReceiverKey::Class {
                    type_tag,
                    type_args,
                } => VirtualReceiverQuery::Class {
                    type_tag: *type_tag,
                    type_args,
                },
                VirtualReceiverKey::Other(ty) => VirtualReceiverQuery::Other(ty),
            },
            interface_head: self.interface_head,
            interface_args: &self.interface_args,
        }
    }

    /// Follow a collection. The key is weak: `false` when the caller or a
    /// declaration it names is gone, and the entry must go with it.
    fn forward(&mut self, visitor: &mut dyn RootVisitor) -> bool {
        let Some(identity) = forward_cache_identity(self.caller_function_addr, visitor) else {
            return false;
        };
        self.caller_function_addr = identity;
        let receiver_live = match &mut self.receiver {
            VirtualReceiverKey::Class {
                type_tag: _,
                type_args,
            } => forward_weak_heads(type_args, visitor),
            VirtualReceiverKey::Other(ty) => forward_weak_heads(std::slice::from_mut(ty), visitor),
        };
        receiver_live
            && self.interface_head.visit_root_weak(visitor)
            && forward_weak_heads(&mut self.interface_args, visitor)
    }
}

impl std::hash::Hash for VirtualSiteKey {
    fn hash<H: std::hash::Hasher>(&self, state: &mut H) {
        self.as_query().hash(state);
    }
}

impl PartialEq for VirtualSiteKey {
    fn eq(&self, other: &Self) -> bool {
        self.as_query() == other.as_query()
    }
}

impl Eq for VirtualSiteKey {}

impl indexmap::Equivalent<VirtualSiteKey> for VirtualSiteQuery<'_> {
    fn equivalent(&self, key: &VirtualSiteKey) -> bool {
        *self == key.as_query()
    }
}

/// A per-site interface-dispatch cache. Fx-hashed: the keys are call sites
/// and program types, never program input.
type VirtualSiteCache<V> = IndexMap<VirtualSiteKey, V, rustc_hash::FxBuildHasher>;

#[derive(Clone, Debug)]
struct StaticVirtualCallTarget {
    /// Weak, like the key: the cache must not keep a runtime callee alive.
    callee: Weak,
    frame_type_args: Vec<bex_vm_types::RealizedTy>,
    /// `frame_type_args` as a slice the callee frames of cache hits share,
    /// when it is non-empty and names compile-time declarations only (so a
    /// collection has nothing in it to visit or forward). `None` otherwise,
    /// and for entries a compiled body made.
    shared_frame_type_args: Option<Arc<[bex_vm_types::RealizedTy]>>,
    /// The callee's value count when the site's pushed arguments are already
    /// the callee's own slots (the site recorded no layout, or the layout it
    /// recorded is the callee's), so a hit skips the layout lookup. `None`
    /// when the arguments must be remapped, or when the entry was made by a
    /// compiled body, which lays out its arguments itself.
    arg_count: Option<usize>,
}

/// One `VirtualCall` that [`BexVm::enter_cached_virtual_call`] entered: at
/// this site, this receiver object with this interface operand object
/// dispatched through static cache entry `entry`.
///
/// Between two collections an object neither moves nor changes its class or
/// type arguments, and the static cache only appends entries, so a call from
/// the same site on the same two objects dispatches the same way: the memo
/// answers it with four word compares instead of building, hashing and
/// comparing a cache key. An iterator adapter's `next` calls its source this
/// way on every element. A collection rebuilds the cache and clears every
/// memo.
///
/// A site that sees a new receiver on every call (`shapes[i].area()`) would
/// only pay to remember each one, so a memo that was never answered is not
/// replaced by the same site's next call, except every
/// [`VIRTUAL_CALL_MEMO_RETRY`]th time, in case the site settles down.
#[derive(Clone, Copy)]
struct VirtualCallMemo {
    /// The caller's cache identity; zero for an empty slot (no identity is
    /// zero: a static function's payload address, or a tagged pointer).
    caller: usize,
    pc: usize,
    receiver: usize,
    iface: usize,
    /// The entry's index in `static_virtual_call_cache`.
    entry: usize,
    /// Whether the memo has answered a call.
    answered: bool,
    /// The site's calls since the memo was made that it did not answer and
    /// did not replace it.
    passed_over: u32,
}

impl VirtualCallMemo {
    const EMPTY: Self = Self {
        caller: 0,
        pc: 0,
        receiver: 0,
        iface: 0,
        entry: 0,
        answered: false,
        passed_over: 0,
    };
}

/// See [`VirtualCallMemo`].
const VIRTUAL_CALL_MEMO_RETRY: u32 = 32;

/// How many sites the `VirtualCall` memo remembers at once, direct-mapped
/// by site. A power of two.
const VIRTUAL_CALL_MEMO_SLOTS: usize = 64;

/// What the `LoadType` cache holds for one type constant of one function.
enum LoadTypeSite {
    /// A fully realized constant denotes the same type in every frame, so
    /// every execution shares one object.
    Realized(Weak),
    /// A template over the frame's type arguments that reduces no
    /// associated-type projection. Substituting it reads nothing but the
    /// frame's arguments, so each argument list seen here keeps one object,
    /// matched by equality. At most [`LOAD_TYPE_SITE_BINDINGS`] lists; the
    /// oldest goes first.
    Generic(SmallVec<[(Box<[bex_vm_types::RealizedTy]>, Weak); 2]>),
    /// A template that reduces a projection. The reduction searches the
    /// running code's impl world, which neither the function nor its frame
    /// arguments pin down, so it is rebuilt on every execution.
    Projection,
}

/// How many frame-argument lists one generic `LoadType` constant remembers.
const LOAD_TYPE_SITE_BINDINGS: usize = 8;

/// Follow a static-cache function identity through a collection. A
/// compile-time function is keyed by its never-moving payload address (low
/// bit clear); a runtime function by its object address tagged in the low
/// bit, which is a weak reference: `None` once the function is gone.
fn forward_cache_identity(identity: usize, visitor: &mut dyn RootVisitor) -> Option<usize> {
    if identity & 1 == 0 {
        return Some(identity);
    }
    let addr = (identity & !1) as *mut Object;
    // SAFETY: the pointer is used only as a lookup key; it is never
    // dereferenced, so validity of the pointee is not required.
    #[cfg(not(feature = "heap_debug"))]
    let mut key = unsafe { HeapPtr::from_ptr(addr) };
    // SAFETY: as above; the epoch does not participate in `Eq`/`Hash`,
    // so a zero epoch probes correctly.
    #[cfg(feature = "heap_debug")]
    let mut key = unsafe { HeapPtr::from_ptr(addr, 0) };
    visitor
        .visit_weak(&mut key)
        .then(|| (key.as_ptr() as usize) | 1)
}

/// Whether every declaration the types name is still live, repointing each
/// that moved.
fn forward_weak_heads(tys: &mut [bex_vm_types::RealizedTy], visitor: &mut dyn RootVisitor) -> bool {
    let mut live = true;
    for ty in tys {
        ty.visit_heads_mut(&mut |head: &mut TypeHead| {
            live &= head.visit_root_weak(visitor);
        });
    }
    live
}

/// The running frame's dispatch state, handed to [`BexVm::run_fast_path`]
/// and back.
#[derive(Clone, Copy)]
struct DispatchState {
    pc: usize,
    code: &'static [u8],
    function: &'static Function,
    frame_idx: usize,
    /// `locals_offset - 1` of the frame; see `BexVm::local_at`.
    locals_base: usize,
    runtime_package: HeapPtr,
}

/// Opcodes [`BexVm::run_fast_path`] can run. The full dispatch enters that
/// fast path only in front of one of these; a wrong entry costs time, never
/// correctness, since the fast path stops before anything it does not run.
const FAST_PATH_OPS: [bool; 256] = {
    use bex_vm_types::bytecode::OpCode as Op;
    let ops = [
        Op::LoadNull,
        Op::LoadTrue,
        Op::LoadFalse,
        Op::LoadIntSmall,
        Op::LoadConst,
        Op::LoadVar,
        Op::LoadVar2,
        Op::StoreVar,
        Op::StoreVar2,
        Op::StoreVarLoadVar,
        Op::Pop,
        Op::Copy,
        Op::AddInt,
        Op::SubInt,
        Op::MulInt,
        Op::CmpIntEq,
        Op::CmpIntNotEq,
        Op::CmpIntLt,
        Op::CmpIntLtEq,
        Op::CmpIntGt,
        Op::CmpIntGtEq,
        Op::Not,
        Op::Truthy,
        Op::Jump,
        Op::PopJumpIfFalse,
        Op::PopJumpIfTrue,
        Op::JumpIfFalse,
        Op::JumpIfFalseOrPop,
        Op::JumpIfTrueOrPop,
        Op::JumpIfNotNullOrPop,
        Op::LoadField,
        Op::LoadFieldLocal,
        Op::AddLocalSmall,
        Op::LoadVarField,
        Op::LoadVarFieldLocal,
        Op::JumpIfLocalsCmp,
        Op::JumpIfLocalConstCmp,
        Op::LoadVariant,
        Op::CallExactArgs,
        Op::Return,
    ];
    let mut table = [false; 256];
    let mut i = 0;
    while i < ops.len() {
        table[ops[i] as usize] = true;
        i += 1;
    }
    table
};

/// A bytecode frame's type arguments: owned, or shared with the
/// virtual-call cache entry that dispatched to the frame's function.
///
/// Sharing is what makes a cached interface call into a generic impl (an
/// iterator adapter's `next`) cheap: entering the callee bumps a reference
/// count instead of allocating and cloning the impl's type arguments, and
/// leaving it drops one. A shared slice names compile-time declarations
/// only, which never move or die, so a collection has nothing in it to
/// visit. Only the VM builds one, after checking exactly that; arguments
/// from anywhere else are owned. Writing to the arguments (`BindType`, a
/// call's extra lane) makes them owned first.
#[derive(Clone, Debug, Default)]
pub struct FrameTypeArgs(FrameTypeArgsRepr);

#[derive(Clone, Debug)]
enum FrameTypeArgsRepr {
    Owned(Vec<bex_vm_types::RealizedTy>),
    Shared(Arc<[bex_vm_types::RealizedTy]>),
}

impl Default for FrameTypeArgsRepr {
    fn default() -> Self {
        Self::Owned(Vec::new())
    }
}

impl From<Vec<bex_vm_types::RealizedTy>> for FrameTypeArgs {
    fn from(args: Vec<bex_vm_types::RealizedTy>) -> Self {
        Self(FrameTypeArgsRepr::Owned(args))
    }
}

impl std::ops::Deref for FrameTypeArgs {
    type Target = [bex_vm_types::RealizedTy];

    fn deref(&self) -> &Self::Target {
        self.as_slice()
    }
}

impl FrameTypeArgs {
    /// Arguments shared with a virtual-call cache entry. `args` must come
    /// from [`BexVm::shareable_frame_type_args`], which admits compile-time
    /// declarations only: a collection does not visit a shared slice.
    fn shared(args: Arc<[bex_vm_types::RealizedTy]>) -> Self {
        Self(FrameTypeArgsRepr::Shared(args))
    }

    pub fn as_slice(&self) -> &[bex_vm_types::RealizedTy] {
        match &self.0 {
            FrameTypeArgsRepr::Owned(args) => args,
            FrameTypeArgsRepr::Shared(args) => args,
        }
    }

    /// The arguments as an owned vector, copying a shared slice first.
    pub fn to_mut(&mut self) -> &mut Vec<bex_vm_types::RealizedTy> {
        if let FrameTypeArgsRepr::Shared(args) = &self.0 {
            self.0 = FrameTypeArgsRepr::Owned(args.to_vec());
        }
        match &mut self.0 {
            FrameTypeArgsRepr::Owned(args) => args,
            FrameTypeArgsRepr::Shared(_) => unreachable!("made owned just above"),
        }
    }

    /// Whether dropping these arguments frees nothing: an owned vector that
    /// never allocated.
    fn owns_nothing(&self) -> bool {
        matches!(&self.0, FrameTypeArgsRepr::Owned(args) if args.capacity() == 0)
    }

    /// The arguments a collection must visit: none for a shared slice.
    fn roots_mut(&mut self) -> &mut [bex_vm_types::RealizedTy] {
        match &mut self.0 {
            FrameTypeArgsRepr::Owned(args) => args,
            FrameTypeArgsRepr::Shared(_) => &mut [],
        }
    }
}

/// Bytecode call frame — pushed when entering a bytecode function. Not
/// `Clone`: each frame holds the proof of one stack reservation.
#[derive(Debug)]
pub struct BytecodeFrame {
    /// Pointer to the running function (or closure) object.
    pub function: HeapPtr,
    /// The next instruction or resume-stub offset, or `compiled_rt::RUNNING` inside a body.
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
    pub type_args: FrameTypeArgs,
    /// Byte offset of the most recently dispatched opcode (compact path).
    /// In the legacy path this mirrors `instruction_ptr - 1` and is kept
    /// up-to-date before each `step()` call.
    /// Used by `capture_stack_trace`, `try_unwind_exception`, and event
    /// source location capture. Compiled bodies store their last reported
    /// instruction index here; see `BexVm::frame_pc`.
    pub(crate) faulting_pc: usize,
    /// Producer-only invocation state. `None` means the invocation is hidden.
    pub(crate) telemetry: Option<FrameTelemetry>,
    context: Context,
    /// The frame's stack room, reserved before it could run: what lets the
    /// dispatch loop push without a capacity check. See `vm::frame_room`.
    #[expect(dead_code, reason = "a proof: required to build a frame, never read")]
    room: frame_room::FrameRoom,
}

struct ManualActivation {
    /// The VM frame whose context the activation replaced.
    frame: usize,
    /// The level of the frame record the activation is scoped to, when the
    /// innermost frame was a directly called compiled body; `None` for a VM
    /// frame. A level is stable for its frame's lifetime; see `stack_view`.
    record: Option<usize>,
    telemetry: Option<crate::telemetry::manual::ManualScope>,
    context: Context,
}

/// What a `$trace` value selects, read from its instance's payload.
enum TraceSelection<'a> {
    Options(&'a bex_vm_types::trace::TraceOptionsData),
    Reservation(&'a bex_vm_types::trace::ReservedSpanData),
}

#[derive(Clone, Default)]
pub(crate) struct CallTrace {
    pub(crate) telemetry: crate::telemetry::TraceConfig,
    pub(crate) context: Option<Arc<ContextPatch>>,
    pub(crate) reservation: Option<bex_vm_types::trace::ReservedSpanData>,
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

impl BytecodeFrame {
    /// Whether any type bound into this frame names a runtime declaration.
    ///
    /// A frame that binds only compiled declarations cannot change what a
    /// `LoadType` constant means, so such frames share the program-wide cache.
    fn binds_runtime_declaration(&self, heap: &BexHeap) -> bool {
        self.type_args.iter().any(|ty| {
            let mut runtime = false;
            ty.visit_heads(&mut |head| {
                runtime |= head.is_resolved() && !heap.is_compile_time_ptr(head.ptr());
            });
            runtime
        })
    }
}

#[cfg(test)]
impl BytecodeFrame {
    /// A copy of this frame for tests that stack one twice.
    pub(crate) fn duplicate_for_test(&self) -> Self {
        Self {
            function: self.function,
            instruction_ptr: self.instruction_ptr,
            locals_offset: self.locals_offset,
            type_args: self.type_args.clone(),
            faulting_pc: self.faulting_pc,
            telemetry: self.telemetry,
            context: self.context.clone(),
            room: frame_room::FrameRoom::for_test(),
        }
    }
}

impl RootHaver for BytecodeFrame {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            function,
            instruction_ptr: _,
            locals_offset: _,
            type_args,
            faulting_pc: _,
            telemetry: _,
            context: _,
            room: _,
        } = self;
        function.visit_roots(visitor);
        // A frame's type arguments may name runtime declarations (a slot
        // bound from a runtime class, a receiver's live binding). The
        // realized argument is the type `LoadType` reads, so its heads must
        // survive and move with the frame. A shared slice names compile-time
        // declarations only: nothing to visit.
        type_args.roots_mut().visit_roots(visitor);
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
    /// Level of the nearest bytecode caller; see `BexVm::stack_view`. Hidden
    /// native continuations are transparent to call-path attribution and
    /// await ownership; this avoids walking ancestors.
    pub(crate) bytecode_caller: usize,
}

impl RootHaver for NativeFrame {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            function,
            continuation,
            bytecode_caller: _,
        } = self;
        function.visit_roots(visitor);
        continuation.visit_roots(visitor);
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
    use std::{collections::HashMap, num::NonZeroU64, sync::Arc};

    use bex_heap::{BexHeap, Tlab};
    use bex_vm_types::{
        EarlyYieldCheck, FunctionKind, GlobalPool, HeapPtr, Object, ObjectIndex, RootHaver,
        RootHaverExt, Value, ValueKind, VmGlobals,
        bytecode::Bytecode,
        types::{BoundMethod, Closure, Function, FunctionOrigin, type_tags},
    };
    use rustc_hash::FxHashMap;

    use super::{BexVm, Frame, FrameStack, VmExecState, value_type_tag};
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
        test_vm_on(
            BexHeap::new(compile_time_objects),
            Arc::new(crate::package_load::DynDispatchTables::default()),
        )
    }

    /// A second VM over `vm`'s heap and dynamic dispatch tables, as the
    /// engine spawns one per `BexThread`.
    pub(crate) fn test_vm_sharing(vm: &BexVm) -> BexVm {
        test_vm_on(Arc::clone(&vm.heap), Arc::clone(&vm.dynamic_dispatch))
    }

    /// A class whose `cleanup` method is `finalizer`, or with no finalizer.
    fn class_object(finalizer: Option<HeapPtr>) -> Object {
        use baml_type::Name;
        use bex_vm_types::{Class, ClassMethodDef};

        let methods: indexmap::IndexMap<Name, ClassMethodDef> = finalizer
            .into_iter()
            .map(|function_ptr| {
                (
                    Name::new("cleanup"),
                    ClassMethodDef {
                        function: ObjectIndex::from_raw(0),
                        function_ptr,
                    },
                )
            })
            .collect();
        Object::Class(Box::new(Class {
            declared_crossing: baml_type::ClassCrossing::Fields,
            path: bex_vm_types::types::LocalName::new(Vec::new(), Name::new("Resource")),
            body: bex_vm_types::ClassBody::Fields(vec![]),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::default(),
            stream_done: false,
            type_tag: baml_type::typetag::TypeTag::from_i64(0),
            cleanup: finalizer.map(|_| 0),
            methods,
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            redact: false,
        }))
    }

    /// An edit through `get_object_mut` that gives an existing class a
    /// `cleanup` finalizer invalidates the heap's finalizable list by itself:
    /// the next collection still finalizes an instance allocated before the
    /// edit. (No class had a finalizer before, so without the invalidation the
    /// collection would skip the finalizer scan entirely.)
    #[test]
    fn an_edit_that_gives_a_class_a_finalizer_keeps_older_instances_finalizable() {
        let mut vm = test_vm(vec![]);
        let finalizer = vm.tlab.alloc_string("cleanup".to_string());
        let class = vm.tlab.alloc(class_object(None));
        let older = vm.tlab.alloc_instance(class, vec![]);

        // SAFETY: single-threaded test; nothing else reaches the class.
        unsafe {
            *vm.get_object_mut(class) = class_object(Some(finalizer));
        }

        let gc_heap = std::sync::Arc::clone(&vm.heap);
        // SAFETY: single-threaded test; no VM is executing.
        let (_, _, forwarding) = unsafe { gc_heap.collect_garbage(&[]) };
        assert_eq!(
            vm.heap.take_pending_finalizers(),
            vec![(
                forwarding.get(older).unwrap(),
                forwarding.get(finalizer).unwrap()
            )]
        );
    }

    fn test_vm_on(
        heap: Arc<BexHeap>,
        dynamic_dispatch: Arc<crate::package_load::DynDispatchTables>,
    ) -> BexVm {
        let budget = super::ThreadStackBudget::default();
        BexVm {
            frames: FrameStack::new(budget.clone()),
            pending_entry_frame_panic: None,
            stack: EvalStack::new(budget),
            op_count: 0,
            cur_pc: 0,
            heap: Arc::clone(&heap),
            hotswap_enabled: false,
            early_yield: early_yield_for_test(),
            dispatch_fast_path: true,
            tlab: Tlab::new(heap),
            shapes: super::shape_cache::ShapeCache::new(),
            globals: VmGlobals::Owned(GlobalPool::new()),
            pending_globals: None,
            error_class_ptrs: Arc::from(Vec::new()),
            panic_class_ptrs: Arc::from(Vec::new()),
            stdlib_heads: Arc::new(crate::package_load::StdlibHeads::default()),

            thread_id: NonZeroU64::MIN,
            monitors: crate::package_baml::TaskMonitors::new(Arc::default(), NonZeroU64::MIN),

            context_transfers: Vec::new(),
            argv: Arc::from([]),
            pending_call_trace: None,
            thread_local_call: false,
            pending_trace_hooks: Vec::new(),
            hotswap_prepared: None,
            held_traces: Vec::new(),
            trace_scope_frame: usize::MAX,
            entry_reservation: None,
            inherited_hook_suppression: false,
            pending_host_trace: None,
            entry_trace: None,
            entry_trace_context: None,
            root_context: btel_types::context::Context::default(),
            pending_call_type_args: Vec::new(),
            compiled: super::compiled_rt::CompiledState::default(),
            pending_call_passes_type_args: false,
            static_load_type_cache: FxHashMap::default(),
            static_virtual_call_cache: super::VirtualSiteCache::default(),
            virtual_call_memo: vec![super::VirtualCallMemo::EMPTY; super::VIRTUAL_CALL_MEMO_SLOTS]
                .into_boxed_slice(),
            static_virtual_field_cache: super::VirtualSiteCache::default(),
            impl_cache: std::cell::RefCell::default(),
            json_decode_cache: crate::package_baml::json::DecodeCache::default(),
            enum_equals_memo: crate::package_baml::EnumEqualsMemo::default(),
            telemetry: Some(TelemetryState::new_root(
                Arc::new(TelemetryPolicies::new()),
                btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
                #[cfg(not(target_arch = "wasm32"))]
                crate::telemetry::test_runtime(),
            )),
            trace_scope: btel_types::new_recording_id(),
            manual_activations: Vec::new(),
            pending_telemetry_wait: None,
            checkpoint_passed: false,
            packages: Arc::new(crate::package_load::PackageIndex::default()),
            dynamic_dispatch,
        }
    }

    fn native_done(_vm: &mut BexVm, _args: &[Value]) -> NativeCallResult {
        NativeCallResult::Done(Value::int(42))
    }

    /// Hand-built test code, lowered through the load-time stack check
    /// the way every function the VM runs is.
    pub(crate) fn verified_compact(function: &Function) -> bex_vm_types::bytecode::CompactCode {
        let no_globals = |_: bex_vm_types::GlobalIndex| None;
        function
            .bytecode
            .lower_verified(
                &bex_vm_types::stack_effect::StackContext::new(&no_globals),
                function.real_local_count,
                false,
            )
            .expect("hand-built test code passes the stack check")
    }

    pub(crate) fn native_function_object() -> Object {
        let native: NativeFunction = native_done;
        Object::Function(Box::new(Function {
            identity: bex_vm_types::types::FunctionIdentity::Declared(
                bex_vm_types::types::FnPath::Free(bex_vm_types::types::LocalName::new(
                    Vec::new(),
                    baml_type::Name::new("test_native"),
                )),
            ),
            source_file: String::new(),
            docstring: None,
            arity: 0,
            real_local_count: 0,
            bytecode: Bytecode::default(),
            kind: FunctionKind::Native(native as *const (), None),
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            hotswap_redirect: bex_vm_types::types::HotswapRedirect::default(),
            compiled_body: None,
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
            native_key: None,
            body_meta: None,
            trace_passthrough: false,
            owner: HeapPtr::null(),
        }))
    }

    pub(crate) fn vm_with_native_entry() -> (BexVm, HeapPtr) {
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
        function.bytecode.compact = Some(verified_compact(&function));
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
            instructions: vec![
                Instruction::LoadConst(0),
                Instruction::SetCallTrace,
                Instruction::LoadConst(0),
                Instruction::Return,
            ],
            constants: vec![bex_vm_types::ConstValue::Null],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(verified_compact(&function));
        let mut vm = test_vm(vec![Object::Function(function)]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.set_entry_point(entry, &[]);
        let _ = vm.exec();
    }

    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn hidden_native_callbacks_preserve_visible_paths_and_reentry() {
        use ::bex_vm_types::RootVisitor;

        use crate::{
            package_baml::{Continuation, PassThroughContinuation},
            telemetry::{SpanRecord, TimingRecord},
        };

        struct Again(HeapPtr);
        impl RootHaver for Again {
            fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
                let Self(callee) = self;
                callee.visit_roots(visitor);
            }
        }
        impl Continuation for Again {
            fn call(self: Box<Self>, _vm: &mut BexVm, _value: Value) -> NativeCallResult {
                NativeCallResult::YieldToCall {
                    callee: self.0,
                    args: vec![Value::bool(false)],
                    type_args: vec![],
                    continuation: Box::new(PassThroughContinuation),
                }
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
        function.kind = FunctionKind::Native(invoke_twice as NativeFunction as *const (), None);
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

    /// Run `main` with the dispatch fast path on or off, at the telemetry
    /// level `level`, under a context every frame inherits (as the engine
    /// installs one). Returns the result and what telemetry recorded: span
    /// records, and timing completions, without clock readings, and with
    /// engine-allocated identities numbered by first appearance (see
    /// `normalized_ids`) and functions named.
    #[cfg(not(target_arch = "wasm32"))]
    fn recorded_run(
        source: &str,
        fast_path: bool,
        level: btel_settings::mode::AutoTelemetryLevel,
    ) -> (i64, Vec<String>, Vec<String>) {
        use btel_types::context::{Context, ContextPatch, ContextValue};

        use crate::telemetry::{SpanRecord, TimingRecord};

        let program = baml_test_support::compile_source(source);
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.dispatch_fast_path = fast_path;
        vm.telemetry = Some(TelemetryState::new_root(
            Arc::new(TelemetryPolicies::with_auto_level(level)),
            btel_clock::ClockRuntime::new(btel_clock::ClockMode::Monotonic).start_run(),
            crate::telemetry::test_runtime(),
        ));
        vm.set_root_context(
            Context::default().with_patch(&ContextPatch {
                metadata: [(
                    "baml_version".to_owned(),
                    Some(ContextValue::String("test".to_owned())),
                )]
                .into(),
                distinct_id: None,
            }),
        );
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        let result = loop {
            match vm.exec().unwrap() {
                VmExecState::EarlyYield => {}
                VmExecState::Complete(value) => break value.as_int().unwrap(),
                other => panic!("unexpected execution state: {other:?}"),
            }
        };

        let state = vm.telemetry.as_ref().unwrap();
        let mut ids = HashMap::new();
        let name = |id| {
            vm.heap
                .function_metadata(vm.proof(), id)
                .map(|metadata| metadata.fqn)
        };
        let mut spans = Vec::new();
        for record in state.span_records() {
            spans.push(match record {
                // Names and the caller's pc are kept whole.
                SpanRecord::CallPathDefined {
                    call_path,
                    parent_call_path,
                    visible_caller,
                    caller_pc,
                    callee,
                    edge,
                } => format!(
                    "{} from {:?} to {:?} at pc {caller_pc}",
                    normalized_ids(
                        &format!("{call_path:?} in {parent_call_path:?} ({edge:?})"),
                        &mut ids,
                    ),
                    visible_caller.and_then(name),
                    name(*callee),
                ),
                // Its clock prints durations in units that vary by run.
                SpanRecord::ThreadSpanAnnouncement {
                    id,
                    parent_id,
                    spawn_call_path,
                    name: thread_name,
                    ..
                } => format!(
                    "{} {thread_name:?}",
                    normalized_ids(
                        &format!("thread {id:?} in {parent_id:?} by {spawn_call_path:?}"),
                        &mut ids,
                    ),
                ),
                other => normalized_ids(&format!("{other:?}"), &mut ids),
            });
        }
        let mut timings = Vec::new();
        for record in state.timing_records() {
            match record {
                TimingRecord::FunctionTimingCompletion {
                    call_path,
                    outcome,
                    reentry,
                    ..
                } => timings.push(normalized_ids(
                    &format!("{call_path:?} {outcome:?} reentry {reentry}"),
                    &mut ids,
                )),
                TimingRecord::SysOpTime { call_path, .. } => {
                    timings.push(normalized_ids(&format!("sysop in {call_path:?}"), &mut ids));
                }
                TimingRecord::ThreadSelected { .. } => {}
            }
        }
        (result, spans, timings)
    }

    /// `debug` with each identity the engine allocates (a `TelemetryId`,
    /// `CallPathId` or `FunctionId`) replaced by the order it first appeared
    /// in this run (`ids`), in letters, and every other number (a clock
    /// reading) masked. Digits within a word are kept; callers keep
    /// function names out of `debug` altogether.
    #[cfg(not(target_arch = "wasm32"))]
    fn normalized_ids(debug: &str, ids: &mut HashMap<String, usize>) -> String {
        const KINDS: [&str; 3] = ["TelemetryId(", "CallPathId(", "FunctionId("];
        let mut out = String::new();
        let mut rest = debug;
        'scan: while let Some(c) = rest.chars().next() {
            for kind in KINDS {
                let Some(tail) = rest.strip_prefix(kind) else {
                    continue;
                };
                let digits = tail
                    .find(|c: char| !c.is_ascii_digit())
                    .unwrap_or(tail.len());
                if digits == 0 {
                    continue;
                }
                let next = ids.len();
                let mut ordinal = *ids
                    .entry(format!("{kind}{}", &tail[..digits]))
                    .or_insert(next);
                out.push_str(kind);
                loop {
                    out.push(char::from(b'a' + u8::try_from(ordinal % 26).unwrap()));
                    ordinal /= 26;
                    if ordinal == 0 {
                        break;
                    }
                }
                rest = &tail[digits..];
                continue 'scan;
            }
            // A digit that continues a name (`user.F1`) is part of it.
            let in_name = out
                .chars()
                .next_back()
                .is_some_and(|prev| prev.is_alphanumeric() || prev == '_');
            if c.is_ascii_digit() && !in_name {
                out.push('#');
                rest = rest.trim_start_matches(|c: char| c.is_ascii_digit() || c == '.');
            } else {
                out.push(c);
                rest = &rest[c.len_utf8()..];
            }
        }
        out
    }

    /// The dispatch fast path runs plain calls and returns with telemetry on,
    /// through the full dispatch's telemetry helpers: what it records must
    /// be exactly what the full dispatch alone records, at every level.
    #[cfg(not(target_arch = "wasm32"))]
    #[test]
    fn fast_path_records_what_the_full_dispatch_records() {
        use btel_settings::mode::AutoTelemetryLevel;

        let source = r#"
            class Vec2 {
                x: int
                y: int
                function norm2(self) -> int { self.x * self.x + self.y * self.y }
            }
            function fib(n: int) -> int { if (n <= 1) { n } else { fib(n - 1) + fib(n - 2) } }
            function leaf(n: int) -> int { n }
            function twice(n: int) -> int { leaf(n) + leaf(n) }
            function id<T>(x: T) -> T { x }
            function fail(n: int) -> int {
                if (n % 4 == 3) { throw "fail"; }
                leaf(n)
            }
            function apply(f: (int) -> int, n: int) -> int { f(n) }
            function main() -> int {
                let s = 0;
                for (let i = 0; i < 12; i += 1) {
                    s += twice(i);
                    let v = Vec2 { x: i, y: i + 1 };
                    s += v.norm2();
                    s += fail(i) catch (e) { _ => 100 };
                    s += apply(leaf, i) + id<int>(i);
                }
                let doubled = [1, 2, 3].map((n: int) -> int { twice(n) });
                s + fib(8) + doubled[2]
            }
        "#;
        for level in [
            AutoTelemetryLevel::Low,
            AutoTelemetryLevel::Medium,
            AutoTelemetryLevel::High,
        ] {
            let full = recorded_run(source, false, level);
            let fast = recorded_run(source, true, level);
            assert!(
                level == AutoTelemetryLevel::Low || !full.2.is_empty() || !full.1.is_empty(),
                "{level:?}: the run records invocations"
            );
            assert_eq!(
                fast, full,
                "{level:?}: the fast path changes what is recorded"
            );
        }
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
                    assert!(unsafe { value.settle_ready(Value::int(7)) });
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
                    vm.stack.try_push(Value::NULL).unwrap();
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
                // The engine resumes an event with a null pushed.
                Instruction::Pop(1),
                Instruction::Jump(-4),
            ],
            constants: vec![
                ConstValue::Object(ObjectIndex::from_raw(1)),
                ConstValue::Int(42),
            ],
            ..Bytecode::default()
        };
        function.bytecode.compact = Some(verified_compact(&function));
        let mut vm = test_vm(vec![
            Object::Function(function),
            Object::String("tick".into()),
        ]);
        let entry = vm.idx_to_ptr(ObjectIndex::from_raw(0));
        vm.set_entry_point(entry, &[]);
        vm.early_yield = EarlyYieldCheck::with_interval(Arc::new(AtomicBool::new(true)), 3);

        for _ in 0..3 {
            assert!(matches!(vm.exec().unwrap(), VmExecState::Event { .. }));
            // As the engine does.
            vm.stack.push(Value::NULL);
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

        let gc_heap = std::sync::Arc::clone(&vm.heap);
        let (stats, _remapped_roots, forwarding) = unsafe { gc_heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 1);
        assert!(
            forwarding.get(trampoline).is_some(),
            "active trampoline function must be forwarded by GC"
        );

        let heap = Arc::clone(&vm.heap);
        vm.forward_roots(&mut ::bex_vm_types::ForwardRoots::new(&forwarding, &*heap));

        let moved_trampoline = trampoline_ptr(&vm);
        assert!(
            matches!(vm.get_object(moved_trampoline), Object::Function(f) if f.name() == "$entry::test_native" && f.telemetry_function_id.is_none()),
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

        let gc_heap = std::sync::Arc::clone(&vm.heap);
        let (stats, _remapped_roots, forwarding) = unsafe { gc_heap.collect_garbage(&roots) };

        assert_eq!(stats.live_count, 0);
        assert!(
            stats.collected_count >= 1,
            "GC should collect the unrooted trampoline function"
        );
        assert!(
            forwarding.get(trampoline).is_none(),
            "unrooted trampoline function must not be forwarded after return"
        );
    }

    /// A `VirtualCall` the static cache answers is entered directly, its frame
    /// sharing the impl's type arguments, and the site remembers the
    /// receiver. A collection forgets it: the memo names objects by address.
    #[test]
    fn cached_virtual_calls_are_remembered_until_a_collection() {
        let program = baml_test_support::compile_source(
            r#"
            interface Step {
                function step(self) -> int throws never
            }
            class Counter<T> {
                n: int
                tag: T
                implements Step {
                    function step(self) -> int {
                        self.n += 1;
                        self.n
                    }
                }
            }
            function make() -> Step {
                Counter<string> { n: 0, tag: "x" }
            }
            function main() -> int {
                let c = make();
                let s = 0;
                for (let i = 0; i < 5; i += 1) {
                    s += c.step();
                }
                s
            }
            "#,
        );
        let entry = program.rendered_callables()["user.main"].object.raw();
        let mut vm = BexVm::from_program(program, Arc::new(AtomicBool::new(false))).unwrap();
        vm.disable_telemetry();
        vm.set_entry_point(vm.heap.compile_time_ptr(entry), &[]);
        let result = loop {
            match vm.exec().expect("execution succeeds") {
                super::VmExecState::Complete(value) => break value.as_int().unwrap(),
                super::VmExecState::EarlyYield => {}
                other => panic!("unexpected state: {other:?}"),
            }
        };
        assert_eq!(result, 1 + 2 + 3 + 4 + 5);
        let remembered: Vec<_> = vm
            .virtual_call_memo
            .iter()
            .filter(|memo| memo.caller != 0)
            .collect();
        assert_eq!(
            remembered.len(),
            1,
            "the loop's one call site is remembered"
        );
        let (_, target) = vm
            .static_virtual_call_cache
            .get_index(remembered[0].entry)
            .expect("the memo names a cache entry");
        assert!(
            target
                .shared_frame_type_args
                .as_deref()
                .is_some_and(|args| args == [bex_vm_types::RealizedTy::string()]),
            "`Counter<string>`'s frames share its type argument",
        );

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        let heap = Arc::clone(&vm.heap);
        let (_stats, _remapped_roots, forwarding) = unsafe { heap.collect_garbage(&roots) };
        vm.forward_roots(&mut ::bex_vm_types::ForwardRoots::new(&forwarding, &*heap));
        assert!(vm.virtual_call_memo.iter().all(|memo| memo.caller == 0));
    }

    /// A frame that shares a dispatch cache entry's type arguments copies
    /// them before writing (`BindType`), so the entry and the next frames
    /// that share it keep the original arguments.
    #[test]
    fn shared_frame_type_args_copy_before_a_write() {
        let shared: Arc<[bex_vm_types::RealizedTy]> =
            Arc::from(vec![bex_vm_types::RealizedTy::int()]);
        let mut args = super::FrameTypeArgs::shared(Arc::clone(&shared));
        assert!(!args.owns_nothing());
        args.to_mut().push(bex_vm_types::RealizedTy::string());
        assert_eq!(&shared[..], &[bex_vm_types::RealizedTy::int()]);
        assert_eq!(
            args.as_slice(),
            &[
                bex_vm_types::RealizedTy::int(),
                bex_vm_types::RealizedTy::string()
            ]
        );
        assert!(matches!(args.0, super::FrameTypeArgsRepr::Owned(_)));
        assert!(super::FrameTypeArgs::default().owns_nothing());
    }

    /// A declaration is reachable from a frame's bound types through the type's
    /// own head, so it is rooted and repointed with no table to keep in step.
    #[test]
    fn frame_type_args_root_and_forward_runtime_heads() {
        let (mut vm, native_ptr) = vm_with_native_entry();
        vm.set_entry_point(native_ptr, &[]);

        let tag = baml_type::typetag::TypeTag::fresh_dynamic();
        let class_ptr = vm.tlab.alloc(Object::Class(Box::new(bex_vm_types::Class {
            declared_crossing: baml_type::ClassCrossing::Fields,
            path: bex_vm_types::types::LocalName::new(Vec::new(), baml_type::Name::new("LiveArg")),
            body: bex_vm_types::ClassBody::Fields(Vec::new()),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            stream_done: false,
            type_tag: tag,
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            redact: false,
        })));
        let Some(Frame::Bytecode(frame)) = vm.frames.last_mut() else {
            panic!("expected trampoline bytecode frame");
        };
        frame.type_args = vec![bex_vm_types::RealizedTy::Class(
            bex_vm_types::TypeHead::new(class_ptr, tag),
            Box::new([]),
        )]
        .into();

        let mut roots = Vec::new();
        vm.collect_roots(&mut roots);
        assert!(
            roots.contains(&class_ptr),
            "a live realized frame argument must root its declaration without metadata",
        );

        let gc_heap = std::sync::Arc::clone(&vm.heap);
        let (_stats, _remapped_roots, forwarding) = unsafe { gc_heap.collect_garbage(&roots) };
        let moved = forwarding
            .get(class_ptr)
            .expect("the frame type argument's declaration must survive collection");
        let heap = Arc::clone(&vm.heap);
        vm.forward_roots(&mut ::bex_vm_types::ForwardRoots::new(&forwarding, &*heap));

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
            declared_crossing: baml_type::ClassCrossing::Fields,
            path: bex_vm_types::types::LocalName::new(
                Vec::new(),
                baml_type::Name::new("PendingNativeArg"),
            ),
            body: bex_vm_types::ClassBody::Fields(Vec::new()),
            description: None,
            alias: None,
            docstring: None,
            other: indexmap::IndexMap::new(),
            stream_done: false,
            type_tag: tag,
            cleanup: None,
            methods: indexmap::IndexMap::new(),
            generic_param_count: 0,
            owner: bex_vm_types::types::Owner::anonymous(),
            telemetry_definition: bex_vm_types::DefinitionCell::default(),
            redact: false,
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

        let gc_heap = std::sync::Arc::clone(&vm.heap);
        let (_stats, _remapped_roots, forwarding) = unsafe { gc_heap.collect_garbage(&roots) };
        let moved = forwarding
            .get(class_ptr)
            .expect("the pending native type argument must survive collection");
        let heap = Arc::clone(&vm.heap);
        vm.forward_roots(&mut ::bex_vm_types::ForwardRoots::new(&forwarding, &*heap));

        let bex_vm_types::RealizedTy::Class(head, ..) = &vm.pending_call_type_args[0] else {
            panic!("the pending native argument must remain the dynamic class type")
        };
        assert_eq!(head.ptr(), moved);
    }
}

impl RootHaver for Frame {
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        match self {
            Frame::Bytecode(f) => f.visit_roots(visitor),
            Frame::Native(f) => f.visit_roots(visitor),
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

/// Generated-code ABI; not a stable public API.
#[doc(hidden)]
pub mod compiled_rt;
mod error_evidence;
mod shape_cache;
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
#[allow(
    clippy::struct_excessive_bools,
    reason = "independent VM execution flags"
)]
pub struct BexVm {
    /// Call stack.
    ///
    /// On each function call we create a new [`Frame`] and push it on this
    /// stack. On each return, we destroy the frame and pop it from the stack
    /// to resume the execution of the previous frame.
    pub frames: FrameStack,

    /// Entry setup has a void API. Materialize a refused frame allocation as a
    /// BAML panic when exec starts, under the normal exception unwinder. Only
    /// StackOverflow/AllocFailure are stored here; neither carries heap roots.
    pending_entry_frame_panic: Option<VmPanic>,

    /// Evaluation stack.
    ///
    /// Arguments, locals, and operands count toward the same per-thread byte
    /// limit as `frames`. Heap objects referenced by these slots do not.
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

    /// Whether calls consult a function's hot swap redirect: some package can
    /// be swapped (BEP-087). Fixed when the VM is created.
    hotswap_enabled: bool,

    pub early_yield: EarlyYieldCheck,

    /// Whether the dispatch loop runs eligible instructions through
    /// `run_fast_path` (the default). Turning it off runs every
    /// instruction through the full dispatch; results must not change,
    /// which `dispatch_fast_path_equivalence` checks.
    pub dispatch_fast_path: bool,

    /// Thread-local allocation buffer (exclusive to this VM).
    pub tlab: Tlab,

    shapes: shape_cache::ShapeCache,

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

    /// The program's global-variable slots whose initializer has not run yet,
    /// indexed by slot — set by the engine for the VMs that run `$init` and
    /// `None` for every other VM. `LoadGlobal` of a pending slot fails with
    /// [`VmInternalError::GlobalReadBeforeInit`] instead of reading the
    /// placeholder, and `StoreGlobal` clears the slot. The compiler orders
    /// initializers so that never happens; this catches what its static
    /// analysis cannot see (a global read through a virtual call).
    pub pending_globals: Option<Box<[bool]>>,

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

    /// Engine-local logical thread identity for structured logs, and the
    /// task that owns the `baml.sync.atomic` locks this VM takes. Unique
    /// among the engine's VMs; set at construction.
    pub thread_id: NonZeroU64,

    /// The engine's `baml.sync.atomic` locks, and what this VM holds of them.
    pub(crate) monitors: crate::package_baml::TaskMonitors,

    /// VM-owned producer state. It deliberately has no transport or buffer.
    pub(crate) telemetry: Option<TelemetryState>,
    pub(crate) trace_scope: btel_types::RecordingId,
    manual_activations: Vec<ManualActivation>,

    /// Starts at VM Await suspension, or when the engine receives an async
    /// sys-op. Taken across the heap-permit release; lookup failures leave it
    /// here for the explicit completion boundary to finish.
    pending_telemetry_wait: Option<(usize, ClockInstant)>,

    /// Set by [`Self::pass_checkpoint`] once the engine has handled a
    /// [`VmExecState::Checkpoint`]. Taken by the `AwaitAny` that yielded it,
    /// which re-executes next and so cannot leak it to a later one.
    checkpoint_passed: bool,

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
    /// Set by `CallLocal` for the call it starts: a native callee runs its
    /// thread-local twin, which reads the receiver without its lock.
    thread_local_call: bool,
    pending_trace_hooks: Vec<PendingTraceHook>,
    /// A patch prepared on this VM and not yet queued. Its swaps preview here:
    /// its initializers run with the call graph it will install, while every
    /// other VM still runs the old one. Rooted, so it survives a collection
    /// while an initializer yields.
    pub(crate) hotswap_prepared: Option<crate::hotswap::PreparedPatch>,
    held_traces: Vec<(usize, CallTrace)>,
    /// Nearest frame with held configuration or an owned-span scope. Ordinary
    /// returns compare one index instead of inspecting both optional stacks.
    trace_scope_frame: usize,
    entry_reservation: Option<bex_vm_types::trace::ReservedSpanData>,
    /// Inherited by a spawned BAML task independently of the parent's lifetime.
    inherited_hook_suppression: bool,
    pending_host_trace: Option<CallTrace>,
    entry_trace: Option<crate::telemetry::TraceConfig>,
    entry_trace_context: Option<Arc<ContextPatch>>,
    root_context: Context,
    pending_call_type_args: Vec<bex_vm_types::RealizedTy>,
    /// State only compiled code uses.
    pub(crate) compiled: compiled_rt::CompiledState,
    /// Set by a call that passes type arguments, for its callee's entry
    /// only: telemetry records the frame's type arguments before the call's
    /// writeback appends `pending_call_type_args` to the new frame.
    pending_call_passes_type_args: bool,

    /// Created-once immutable type objects for `LoadType` constants, keyed
    /// by function identity and constant index: one object for a fully
    /// realized constant, one per frame-argument list for a template over the
    /// frame (see [`LoadTypeSite`]). The compact numeric key keeps the
    /// per-iteration lookup cheaper than re-hashing a type. Entries are weak
    /// throughout: one goes when its function, its type object or a
    /// declaration its arguments name does, and a type nothing else holds is
    /// simply rebuilt. Fx-hashed: the keys are function addresses and
    /// constant indexes, never program input.
    static_load_type_cache: FxHashMap<(usize, usize), LoadTypeSite>,
    /// Monomorphic static virtual-call results keyed by caller site and the
    /// receiver/interface instantiation observed there. Only immutable
    /// program-image rules enter this cache, so runtime package loading and
    /// dynamic builder registration remain live on every dynamic dispatch.
    /// Fx-hashed: the keys are call sites and program types, never program
    /// input.
    static_virtual_call_cache: VirtualSiteCache<StaticVirtualCallTarget>,
    /// The last call each of a few sites entered through
    /// `static_virtual_call_cache`, by object identity; see
    /// [`VirtualCallMemo`]. Weak, and cleared by every collection.
    virtual_call_memo: Box<[VirtualCallMemo]>,
    /// The field analogue of `static_virtual_call_cache`: the physical slot a
    /// `VirtualLoadField`/`VirtualStoreField` site resolved for a receiver
    /// type, under the same rule (only immutable program-image impls enter).
    static_virtual_field_cache: VirtualSiteCache<usize>,
    /// This VM's impl-resolver answers. The resolver borrows it for a
    /// lookup or an insert only, never across a resolution, so a nested
    /// resolution can use it too.
    pub(crate) impl_cache: std::cell::RefCell<crate::impl_cache::ImplCache>,
    /// What `baml.json` decoding has worked out about types (class plans,
    /// provided `from_json`s, which types reach one), kept between calls.
    /// Not a root: dropped at every collection.
    pub(crate) json_decode_cache: crate::package_baml::json::DecodeCache,
    /// Whether `==` on a compile-time enum is the structural default, per
    /// resolution generation: the `CallEquals` fast path's one lookup. Holds
    /// compile-time pointers only, so a collection has nothing to visit.
    pub(crate) enum_equals_memo: crate::package_baml::EnumEqualsMemo,
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
    ///   this when none has settled; otherwise it yields
    ///   [`VmExecState::Checkpoint`]).
    /// - Output (success): nothing pushed — the engine parks until the first
    ///   of these settles, then resumes the VM, which re-executes the
    ///   `AwaitAny` opcode and now finds a settled future.
    /// - Output (cancel): the thread's own cancel token fired; handled like
    ///   `Await` (settle our future as cancelled, or surface `Cancelled`).
    AwaitAny(Vec<FutureId>),

    /// `baml.future.__await_any` found an input already settled. There is
    /// nothing to wait for, but it is still a suspension point: the thread's
    /// own cancellation must be delivered here whether or not an input
    /// settled first.
    ///
    /// - Output (success): the engine calls [`BexVm::pass_checkpoint`] and
    ///   resumes; the VM re-executes the `AwaitAny` opcode and pushes the
    ///   `int` index of the first settled input, in input order.
    /// - Output (cancel): handled like `AwaitAny`.
    Checkpoint,

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
    ///   eval stack by the `OpCode::SysOp` handler, and the op's declared
    ///   `return_type`, which the engine admits the result to.
    /// - Output: a single `Value` on the VM stack.
    /// - No `Object::Future` is allocated; no `FutureManager` entry
    ///   is created.
    SysOp {
        operation: bex_vm_types::SysOp,
        args: Vec<Value>,
        /// The declared return type of the op, realized. A sys-op threads no
        /// type arguments, so the declaration realizes on its own; the one
        /// exception, `_call_host_value<T, E>`, is dispatched by the VM's own
        /// host-call path with the callable's bound types.
        return_type: bex_vm_types::RealizedTy,
        /// The declared throws type of the op, realized likewise. A value a
        /// host callback throws is admitted to it.
        throws_type: bex_vm_types::RealizedTy,
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
    convert_program_with_compiled_bodies(program, enabled, None)
}

/// Convert a program with trace hooks and attach compiled bodies to their bytecode functions.
pub fn convert_program_with_compiled_bodies(
    program: bex_vm_types::Program,
    enabled: bool,
    compiled_bodies: Option<CompiledBodies>,
) -> Result<BytecodeProgram, VmInternalError> {
    let mut bytecode = convert_program_for_load(program, enabled, compiled_bodies)?;
    prepare_compact_code(&mut bytecode.objects.0, &bytecode.globals)?;
    Ok(bytecode)
}

/// [`convert_program_with_compiled_bodies`] without lowering to compact code,
/// for a loader that changes constants (float boxing) and then calls
/// [`prepare_compact_code`] itself: lowering, and the stack check that comes
/// with it, run once.
pub fn convert_program_for_load(
    program: bex_vm_types::Program,
    enabled: bool,
    compiled_bodies: Option<CompiledBodies>,
) -> Result<BytecodeProgram, VmInternalError> {
    // The one road from an executable into a VM: a decoded program is
    // checked against the format's laws here, and everything after — the
    // rendered views, the loader — indexes into a program that keeps them.
    program.validate()?;
    let body_names = compiled_rt::validate_compiled_program(&program, compiled_bodies)?;
    let callables = program.rendered_callables();
    // Convert objects, attaching native functions
    let mut objects: Vec<Object> = program
        .objects
        .into_iter()
        .map(crate::package_baml::attach_builtins)
        .collect::<Result<Vec<_>, _>>()?;

    // Call specialization needs compiled bodies attached before compact lowering.
    if let Some(bodies) = compiled_bodies {
        compiled_rt::attach_compiled_bodies(&mut objects, bodies.table, &body_names)?;
    }

    if !enabled {
        disable_declared_trace_hooks(&mut objects);
    }

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
///
/// Every function's operand stack is verified on the way
/// ([`bex_vm_types::Bytecode::lower_verified`]); one that fails rejects the
/// program, since the dispatch loop pushes within the bound the check proves.
///
/// # Errors
///
/// A function whose operand stack the check cannot prove consistent and
/// bounded.
pub fn prepare_compact_code(
    objects: &mut [Object],
    globals: &[bex_vm_types::ConstValue],
) -> Result<(), VmInternalError> {
    // A call without a recorded layout passes its callee's arity; every
    // static callee is a global naming a function.
    let arities: Vec<Option<usize>> = globals
        .iter()
        .map(|global| match global {
            bex_vm_types::ConstValue::Object(index) => match objects.get(index.raw()) {
                Some(Object::Function(function)) => Some(function.arity),
                _ => None,
            },
            _ => None,
        })
        .collect();
    let global_arity =
        |global: bex_vm_types::GlobalIndex| arities.get(global.raw()).copied().flatten();
    let ctx = bex_vm_types::stack_effect::StackContext::new(&global_arity);
    for object in objects.iter_mut() {
        if let Object::Function(function) = object {
            let compiled = function.compiled_body.is_some();
            let mut compact = function
                .bytecode
                .lower_verified(&ctx, function.real_local_count, compiled)
                .map_err(|error| {
                    VmInternalError::InvalidProgram(bex_vm_types::types::InvalidProgram(format!(
                        "function `{}` fails the operand-stack check: {error}",
                        function.name()
                    )))
                })?;
            if compiled {
                compact
                    .code
                    .extend(crate::compiled_rt::stub::CODE.map(|op| op as u8));
            }
            function.bytecode.compact = Some(compact);
        }
    }
    crate::call_specialize::specialize_calls(objects, globals);
    Ok(())
}

/// Resolve the heap pointers for the builtin `baml.errors.*` classes, indexed by
/// [`ErrorClass`] discriminant. The result is identical for every VM sharing a
/// `packages` index, so it is resolved once and shared (`Arc`) across spawns
/// rather than re-resolved per [`BexVm::new`].
///
/// One extra entry follows the error classes, at `DURATION_CLASS_PTR_INDEX`:
/// `baml.time.Duration`, which `baml.errors.Timeout.duration` is an instance of.
///
/// # Errors
///
/// An executable that loads no `baml` package, or one whose `baml` package
/// declares no such class: the runtime cannot run without the language's
/// own error and panic classes, so the program is refused at load.
pub fn resolve_error_class_ptrs(
    packages: &crate::package_load::PackageIndex,
) -> Result<Arc<[HeapPtr]>, VmInternalError> {
    ErrorClass::ALL
        .iter()
        .map(ErrorClass::item)
        .chain(std::iter::once(crate::lang_items::BAML_TIME_DURATION))
        .map(|item| lang_declaration_required(packages, item))
        .collect()
}

/// The declaration `item` names, or the error that refuses an executable
/// without it ([`resolve_error_class_ptrs`]).
fn lang_declaration_required(
    packages: &crate::package_load::PackageIndex,
    item: baml_base::LangItem,
) -> Result<HeapPtr, VmInternalError> {
    packages.lang_declaration(item).ok_or_else(|| {
        VmInternalError::InvalidProgram(bex_vm_types::types::InvalidProgram(format!(
            "the executable does not load `{item}`: the `{}` language package is required",
            item.package().manifest_name()
        )))
    })
}

/// Every declaration the runtime's own code names by identity
/// ([`crate::lang_items::runtime_items`]) is declared by its language package
/// when the executable loads that package — what makes [`BexVm::lang_object`]
/// infallible afterwards. A package the executable does not load is not an
/// error here (a program without the standard library); the `baml` package
/// itself is required by [`resolve_error_class_ptrs`].
///
/// # Errors
///
/// A loaded language package that does not declare an item the runtime
/// names: the stdlib sources and the runtime out of step, refused at load.
pub fn require_lang_items(
    packages: &crate::package_load::PackageIndex,
) -> Result<(), VmInternalError> {
    crate::lang_items::runtime_items()
        .filter(|item| packages.lang_package(item.package()).is_some())
        .try_for_each(|item| {
            if packages.lang_declaration(item).is_some() {
                Ok(())
            } else {
                Err(VmInternalError::InvalidProgram(
                    bex_vm_types::types::InvalidProgram(format!(
                        "the executable's `{}` language package does not declare `{item}`, which the runtime names",
                        item.package().manifest_name()
                    )),
                ))
            }
        })
}

/// The standard-library tables a VM is constructed over, resolved once per
/// executable from its package index and shared by every VM over it (the
/// `Arc`s): the one road on which the runtime's requirements of the standard
/// library are checked, so no load path can skip one.
pub struct StdlibTables {
    /// [`resolve_error_class_ptrs`].
    pub error_class_ptrs: Arc<[HeapPtr]>,
    /// [`resolve_panic_class_ptrs`].
    pub panic_class_ptrs: Arc<[HeapPtr]>,
    /// [`StdlibHeads::resolve`](crate::package_load::StdlibHeads::resolve).
    pub heads: Arc<crate::package_load::StdlibHeads>,
}

impl StdlibTables {
    /// Resolve every table and check every requirement
    /// ([`require_lang_items`]).
    ///
    /// # Errors
    ///
    /// The executable loads no `baml` package, or a language package it loads
    /// does not declare an item the runtime names.
    pub fn resolve(packages: &crate::package_load::PackageIndex) -> Result<Self, VmInternalError> {
        let error_class_ptrs = resolve_error_class_ptrs(packages)?;
        let panic_class_ptrs = resolve_panic_class_ptrs(packages)?;
        require_lang_items(packages)?;
        Ok(Self {
            error_class_ptrs,
            panic_class_ptrs,
            heads: Arc::new(crate::package_load::StdlibHeads::resolve(packages)),
        })
    }
}

/// Where `baml.time.Duration` sits in the error class table: right after the
/// last [`ErrorClass`].
const DURATION_CLASS_PTR_INDEX: usize = ErrorClass::ALL.len();

/// Resolve the heap pointers for the builtin `baml.panics.*` classes, indexed by
/// [`PanicClass`] discriminant. Shared across spawns like
/// [`resolve_error_class_ptrs`].
pub fn resolve_panic_class_ptrs(
    packages: &crate::package_load::PackageIndex,
) -> Result<Arc<[HeapPtr]>, VmInternalError> {
    PanicClass::ALL
        .iter()
        .map(PanicClass::item)
        .map(|item| lang_declaration_required(packages, item))
        .collect()
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
pub(crate) fn function_callable_signature<
    C: baml_type::normalize::TypeContext<bex_vm_types::TypeHead>,
>(
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
        name: f.declared_name().map(ToString::to_string),
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
        ValueKind::Float(_) => type_tags::FLOAT,
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
                Object::Tuple(_) => type_tags::TUPLE,
                Object::Function(_) => type_tags::FUNCTION,
                Object::Closure(_) => type_tags::FUNCTION,
                Object::BoundMethod(_) => type_tags::FUNCTION,
                Object::GenericFunction(_) => type_tags::FUNCTION,
                Object::HostClosure(_) => type_tags::FUNCTION,
                Object::Cell(_) => type_tags::UNKNOWN,
                Object::Future(_) => type_tags::FUTURE,
                Object::Enum(_) => type_tags::ENUM,
                Object::Type(_) => type_tags::TYPE,
                Object::Class(_) => type_tags::UNKNOWN,
                Object::TypeAlias(_) => type_tags::UNKNOWN,
                Object::Interface(_) => type_tags::UNKNOWN,
                Object::Package(_) => type_tags::UNKNOWN,
                Object::ImplRule(_) => type_tags::UNKNOWN,
                Object::Tombstone => Object::tombstone_reached(),
                #[cfg(feature = "heap_debug")]
                Object::Sentinel(_) => type_tags::UNKNOWN,
                Object::Instance => unsafe { InstanceRef::of(ptr) }
                    .expect("an instance pointer")
                    .class_object()
                    .type_tag
                    .as_i64(),
                Object::RustInstance(instance) => {
                    let class_obj = unsafe { instance.class.get() };
                    let Object::Class(class) = class_obj else {
                        unreachable!("RustInstance.class does not point to a Class object")
                    };
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
        monitors: Arc<bex_vm_types::MonitorTable>,
        thread_id: NonZeroU64,
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
        // once from `packages` by the caller (`StdlibTables::resolve`) and
        // shared across spawned VMs.

        let early_yield = EarlyYieldCheck::new(
            #[cfg(not(target_arch = "wasm32"))]
            park_requested,
        );

        let early_yield =
            early_yield.with_gc_pressure(heap.gc_pressure(), bex_heap::gc_policy::POLL_INTERVAL);

        // Each BexThread owns a VM, including the main thread. Its stack limit
        // is independent of the shared heap and every other thread's stacks.
        let budget = ThreadStackBudget::default();
        let hotswap_enabled = heap.hotswap_on();
        Self {
            frames: FrameStack::new(budget.clone()),
            pending_entry_frame_panic: None,
            stack: EvalStack::new(budget),
            op_count: 0,
            cur_pc: 0,
            heap,
            hotswap_enabled,
            early_yield,
            dispatch_fast_path: true,
            tlab,
            shapes: shape_cache::ShapeCache::new(),
            globals,
            pending_globals: None,
            error_class_ptrs,
            panic_class_ptrs,
            stdlib_heads,

            thread_id,
            monitors: crate::package_baml::TaskMonitors::new(monitors, thread_id),

            context_transfers: Vec::new(),
            argv,
            pending_call_trace: None,
            thread_local_call: false,
            pending_trace_hooks: Vec::new(),
            hotswap_prepared: None,
            held_traces: Vec::new(),
            trace_scope_frame: usize::MAX,
            entry_reservation: None,
            inherited_hook_suppression: false,
            pending_host_trace: None,
            entry_trace: None,
            entry_trace_context: None,
            root_context: Context::default(),
            pending_call_type_args: Vec::new(),
            compiled: compiled_rt::CompiledState::default(),
            pending_call_passes_type_args: false,
            static_load_type_cache: FxHashMap::default(),
            static_virtual_call_cache: VirtualSiteCache::default(),
            virtual_call_memo: vec![VirtualCallMemo::EMPTY; VIRTUAL_CALL_MEMO_SLOTS]
                .into_boxed_slice(),
            static_virtual_field_cache: VirtualSiteCache::default(),
            impl_cache: std::cell::RefCell::default(),
            json_decode_cache: crate::package_baml::json::DecodeCache::default(),
            enum_equals_memo: crate::package_baml::EnumEqualsMemo::default(),
            telemetry,
            trace_scope,
            manual_activations: Vec::new(),
            pending_telemetry_wait: None,
            checkpoint_passed: false,
            packages,
            dynamic_dispatch,
        }
    }

    /// Materialize a new executable object into a reserved slot, once it is
    /// built knowing its own address. A function takes its telemetry identity
    /// here; imported pointers bypass this, since a moving GC preserves an
    /// existing identity without re-registration.
    ///
    /// # Safety
    ///
    /// As for [`bex_heap::Tlab::publish`]: no collection since `reservation`
    /// was made.
    pub(crate) unsafe fn publish_runtime_object(
        &mut self,
        reservation: bex_heap::Reservation,
        object: Object,
    ) -> Result<HeapPtr, VmInternalError> {
        // SAFETY: the caller upholds `publish`'s contract.
        match object {
            Object::Function(function) => {
                Ok(unsafe { self.tlab.publish_function(reservation, function) }?)
            }
            other => Ok(unsafe { self.tlab.publish(reservation, other) }),
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
        if let Some(lent) = self.compiled.lent_type_args {
            // SAFETY: set only while `with_call_type_args` runs a builtin
            // with the lender's array borrowed for it.
            #[allow(unsafe_code)]
            return unsafe { lent.0.as_ref() };
        }
        &self.pending_call_type_args
    }

    pub(crate) fn activate_manual_span(
        &mut self,
        span: &crate::telemetry::manual::ManualSpan,
    ) -> Result<(), &'static str> {
        if span.scope != self.trace_scope || span.is_closed() {
            return Err("span is closed or belongs to another runtime");
        }
        // The activation belongs to the innermost frame. A directly called
        // compiled body has no VM frame of its own: it replaces the context of
        // the VM frame it runs above, which its callees inherit, and its
        // record restores that context when the body returns.
        let (frame_idx, record) = match self.top_frame() {
            Some(FrameAt::Record(record)) => {
                (compiled_rt::record_ref(record).anchor() - 1, Some(record))
            }
            _ => (self.frames.len().saturating_sub(1), None),
        };
        let level = record.map(|record| compiled_rt::record_ref(record).level());
        if self
            .manual_activations
            .last()
            .is_some_and(|active| active.frame == frame_idx && active.record == level)
        {
            return Err("span scope is already active");
        }
        let Some(Frame::Bytecode(frame)) = self.frames.get_mut(frame_idx) else {
            return Err("span activation requires a bytecode scope");
        };
        let context = std::mem::replace(&mut frame.context, span.context.clone());
        self.manual_activations.push(ManualActivation {
            frame: frame_idx,
            record: level,
            context,
            telemetry: self.telemetry.as_mut().and_then(|state| span.enter(state)),
        });
        match record {
            Some(record) => compiled_rt::record_ref(record).scoped.set(true),
            None => self.trace_scope_frame = frame_idx,
        }
        Ok(())
    }

    fn hold_trace(&mut self, frame_idx: usize, trace: CallTrace) {
        self.held_traces.push((frame_idx, trace));
        self.trace_scope_frame = frame_idx;
    }

    #[allow(
        clippy::inline_always,
        reason = "benchmarked return hot path keeps one comparison inline and cleanup cold"
    )]
    #[inline(always)]
    fn end_manual_activation(&mut self, frame_idx: usize) {
        if self.trace_scope_frame == frame_idx {
            self.restore_trace_scope(frame_idx);
        }
    }

    #[cold]
    #[inline(never)]
    fn restore_trace_scope(&mut self, frame_idx: usize) {
        if self
            .held_traces
            .last()
            .is_some_and(|(frame, _)| *frame == frame_idx)
        {
            self.held_traces.pop();
        }
        if let Some(previous) = self
            .manual_activations
            .pop_if(|active| active.frame == frame_idx && active.record.is_none())
        {
            self.restore_activation(previous);
        }
        self.reset_trace_scope_frame();
    }

    /// End the manual span activation scoped to the record `record`, whose
    /// body is returning or being unwound.
    #[cold]
    #[inline(never)]
    pub(crate) fn end_record_activation(&mut self, record: *const compiled_rt::FrameRecord) {
        let record = compiled_rt::record_ref(record);
        record.scoped.set(false);
        let level = Some(record.level());
        if let Some(previous) = self
            .manual_activations
            .pop_if(|active| active.record == level)
        {
            self.restore_activation(previous);
            let context = self.current_context().clone();
            if let Some(telemetry) = &mut self.telemetry {
                telemetry.set_context(context);
            }
        }
    }

    fn restore_activation(&mut self, previous: ManualActivation) {
        if let (Some(saved), Some(state)) = (previous.telemetry, &mut self.telemetry) {
            saved.restore(state);
        }
        if let Some(Frame::Bytecode(frame)) = self.frames.get_mut(previous.frame) {
            frame.context = previous.context;
        }
    }

    /// The nearest VM frame with held configuration or a manual activation
    /// of its own. Activations scoped to records end with their records.
    fn reset_trace_scope_frame(&mut self) {
        self.trace_scope_frame = self
            .held_traces
            .last()
            .map(|(frame, _)| *frame)
            .into_iter()
            .chain(
                self.manual_activations
                    .iter()
                    .rev()
                    .find(|scope| scope.record.is_none())
                    .map(|scope| scope.frame),
            )
            .max()
            .unwrap_or(usize::MAX);
    }

    pub(crate) fn current_span_id(&self) -> Option<bex_vm_types::trace::SpanId> {
        self.telemetry.as_ref()?;
        self.frame_receipt(self.top_frame()?)?
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
            let reservation = previous
                .as_ref()
                .and_then(|trace| trace.reservation.clone());
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
            pending.trace = Some(CallTrace {
                telemetry,
                context,
                reservation,
            });
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
        let context = frame.context.clone();
        // Hook resolution belongs to entry. Keep its result even with recording off.
        let passthrough = unsafe { self.load_function(frame_idx)? }.trace_passthrough;
        if passthrough {
            if let Some(state) = &mut self.telemetry {
                state.set_context(context);
            }
            self.hold_trace(frame_idx, pending.trace.unwrap_or_default());
            return Ok(());
        }
        if self.telemetry.is_none() {
            return Ok(());
        }
        let Frame::Bytecode(frame) = &self.frames[frame_idx] else {
            unreachable!()
        };
        let locals_offset = frame.locals_offset;
        let type_args = frame.type_args.clone();
        // SAFETY: all input slots, callable and metadata remain rooted.
        let function = unsafe { self.load_function(frame_idx)? };
        let callee = self
            .frame_function_identity(frame_idx)
            .expect("hook target identity");
        let caller_at = pending.caller.and_then(|level| self.frame_at_level(level));
        let caller = caller_at.and_then(|at| self.frame_identity(at));
        let observed = caller_at.is_some_and(|at| self.frame_receipt(at).is_some());
        // Resolve captured parameter cells without allocating for ordinary arities.
        let args: SmallVec<[Value; 8]> = self.stack.as_slice()
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

    /// The context the invocation runs under. Telemetry takes it at once: a
    /// thread marked running before its entry frame announces itself with
    /// the context it has then.
    pub fn set_root_context(&mut self, context: Context) {
        assert!(
            self.frames.is_empty(),
            "root context must precede invocation entry"
        );
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.set_context(context.clone());
        }
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

    pub fn set_entry_reservation(&mut self, reservation: &bex_vm_types::trace::ReservedSpanData) {
        self.entry_reservation = Some(reservation.clone());
    }

    pub(crate) fn passthrough_trace(&self) -> Result<CallTrace, &'static str> {
        // A pass-through body always runs in a VM frame; a directly called
        // compiled body above one is a different invocation.
        let Some(FrameAt::Vm(frame)) = self.top_frame() else {
            return Err("trace._passthrough requires a pass-through invocation");
        };
        // Only this exact invocation can access its held config. Never search ancestors.
        self.held_traces
            .last()
            .filter(|(index, _)| *index == frame)
            .map(|(_, trace)| trace.clone())
            .ok_or("trace._passthrough requires a pass-through invocation")
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
        self.record_log_at(FrameAt::Vm(frame_idx), level, name, data);
    }

    pub(crate) fn record_log_at(
        &mut self,
        frame: FrameAt,
        level: btel_records::LogLevel,
        name: Option<Arc<str>>,
        data: Value,
    ) {
        if self.telemetry.is_none() {
            return;
        }
        let Some(function) = self.frame_identity(frame) else {
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
                Frame::Bytecode(frame) => Some(&frame.context),
                Frame::Native(_) => None,
            })
            .unwrap_or(&self.root_context);
        state.select_context(context);
    }

    /// What a `$trace` value selects: a `trace.Options`' settings or a
    /// `trace.ReservedSpan`'s reservation, or `None` for any other value.
    fn trace_selection(&self, value: Value) -> Option<TraceSelection<'_>> {
        match &self.as_rust_instance(&value).ok()?.payload {
            bex_vm_types::RustPayload::TraceOptions(options) => {
                Some(TraceSelection::Options(options))
            }
            bex_vm_types::RustPayload::ReservedSpan(reservation) => {
                Some(TraceSelection::Reservation(reservation))
            }
            _ => None,
        }
    }

    pub(crate) fn span_creation_trace(&self, value: Value) -> Result<CallTrace, &'static str> {
        let held = self.passthrough_trace()?;
        let (options, reserved_id, reservation) = match self
            .trace_selection(value)
            .ok_or("invalid trace configuration")?
        {
            TraceSelection::Options(options) => (options.clone(), None, None),
            TraceSelection::Reservation(reservation) => {
                if !held
                    .reservation
                    .as_ref()
                    .is_some_and(|claim| claim.shares_claim(reservation))
                {
                    return Err(
                        "reservation claim does not belong to this pass-through invocation",
                    );
                }
                let id = reservation.materialize(self.trace_scope)?;
                (
                    reservation.options.clone(),
                    Some(id),
                    Some(reservation.clone()),
                )
            }
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
            reservation,
        })
    }

    fn trace_config(&mut self, value: Value) -> Result<CallTrace, VmError> {
        use bex_vm_types::trace::ReservationError;

        let Some(selection) = self.trace_selection(value) else {
            return Err(
                self.trace_attachment_error("$trace expects trace.Options or trace.ReservedSpan")
            );
        };
        let (options, reserved_id, reservation) = match selection {
            TraceSelection::Options(options) => (options.clone(), None, None),
            TraceSelection::Reservation(reservation) => {
                if reservation.is_forwarded_claim()
                    && !self
                        .passthrough_trace()
                        .ok()
                        .and_then(|held| held.reservation)
                        .is_some_and(|claim| claim.shares_claim(reservation))
                {
                    return Err(self.trace_attachment_error(
                        "reservation claim can only be forwarded by its pass-through invocation",
                    ));
                }
                let local = match reservation.enter(self.trace_scope) {
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
                (
                    reservation.options.clone(),
                    Some(local),
                    Some(reservation.clone()),
                )
            }
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
            reservation,
        })
    }

    /// The `type` value a type-operand position received. Every such
    /// position's contract is `reflect.Type | reflect.TypeView`: a kind view
    /// converts to the `type` it wraps, and any other view has already been
    /// through its own `as_type()` in BAML. Shared by the call type-argument
    /// lane and `BindType`, so the two boundaries cannot drift on what a type
    /// operand may be. It never throws.
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
        // Static checking admits only `reflect.Type | reflect.TypeView` here,
        // so any other value is a compiler bug, not a program error.
        Err(VmInternalError::TypeError {
            expected: Type::Object(ObjectType::Type),
            got: self.type_of(&value),
        }
        .into())
    }

    /// The interface constraint a bound check reads, as its head, its type
    /// arguments, and the associated types it pins. The emitter loads it as
    /// the constraint's `Object::Type`, which carries exactly what the bound
    /// wrote and nothing an existential would add.
    fn bound_parts(&self, bound: Value) -> Result<BoundParts, VmError> {
        if let Some(ptr) = bound.as_object_ptr()
            && let Object::Type(type_value) = self.get_object(ptr)
            && let bex_vm_types::RealizedTy::Interface(head, args, assoc) = &type_value.ty
        {
            return Ok((*head, args.clone(), assoc.clone()));
        }
        Err(VmInternalError::TypeError {
            expected: Type::Object(ObjectType::Type),
            got: self.type_of(&bound),
        }
        .into())
    }

    /// The question a generic parameter's bound asks, at run time: does the
    /// type `value` holds implement the bound's interface, at its type
    /// arguments and the associated types it pins (the others are free)? As
    /// statically, only a concrete type can: a union, an interface, `unknown`
    /// and `never` have no single runtime type to dispatch on.
    fn satisfies_bound(&mut self, value: Value, bound: Value) -> Result<bool, VmError> {
        let ty = self.type_operand_value(value)?.ty;
        let (head, args, assoc) = self.bound_parts(bound)?;
        Ok(self.check_bound(&ty, (head, args, assoc), Some(value)) == BoundCheck::Satisfied)
    }

    /// Whether `ty` is a type a bound can hold for: one concrete type
    /// (`TYPE_SYSTEM.md`, "Generics on Functions"). A union, an interface,
    /// `unknown` and `never` have no single runtime type to dispatch on, and
    /// a literal type (`"a"`, `Side.Left`) is a subset of a concrete type
    /// rather than one: its values use the base type's members, but it is
    /// never a bound's type argument. Judged on the canonical form, since a
    /// type is its value set and not its spelling: `true | false` is `bool`,
    /// `1 | int` is `int`, and a recursive alias is the list or map it
    /// unfolds to.
    pub fn bound_admits(&self, ty: &bex_vm_types::RealizedTy) -> bool {
        baml_type::normalize::normalize(ty.as_ty(), self).is_concrete()
    }

    /// [`BoundCheck`] for `ty` against the interface bound `(head, args,
    /// assoc)`. `within` is the value the type came from, whose package may
    /// own a runtime implementation; a type with no such value (a host's type
    /// argument) is judged in the program's own world.
    pub fn check_bound(
        &self,
        ty: &bex_vm_types::RealizedTy,
        (head, args, assoc): BoundParts,
        within: Option<Value>,
    ) -> BoundCheck {
        if !self.bound_admits(ty) {
            return BoundCheck::NotConcrete;
        }
        let resolver = match within {
            Some(value) => crate::package_baml::resolve::ImplResolver::for_value(self, value),
            None => crate::package_baml::resolve::ImplResolver::new(self),
        };
        if resolver.type_implements(ty, head, &args, &assoc) {
            BoundCheck::Satisfied
        } else {
            BoundCheck::NotImplemented
        }
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
        let mut type_args = TakenTypeArgs {
            tys: Vec::with_capacity(count),
        };
        for slot in start..end {
            let value = self.stack[StackIndex::from_raw(slot)];
            type_args.tys.push(self.type_operand_value(value)?.ty);
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

    /// Take the `count` type operands below the top `value_count` values when
    /// every one is an `Object::Type` built only from compiled declarations:
    /// the case [`Self::take_type_args`] serves without a per-frame carrier.
    /// Anything else (a kind view, a type naming a runtime declaration)
    /// returns `None` and leaves the stack untouched, so the caller can take
    /// the general path. Kept out of line so the non-generic exact call stays
    /// as small as it was.
    #[inline(never)]
    fn take_plain_static_type_args(
        &mut self,
        count: usize,
        value_count: usize,
    ) -> Option<Vec<bex_vm_types::RealizedTy>> {
        let start = self.stack.len().checked_sub(count + value_count)?;
        let end = start + count;
        let mut tys = Vec::with_capacity(count);
        for slot in start..end {
            let ptr = self.stack[StackIndex::from_raw(slot)].as_object_ptr()?;
            let Object::Type(type_value) = self.get_object(ptr) else {
                return None;
            };
            if !crate::reachable::is_statically_declared(&type_value.ty) {
                return None;
            }
            tys.push(type_value.ty.clone());
        }
        drop(
            self.stack
                .drain(StackIndex::from_raw(start)..StackIndex::from_raw(end)),
        );
        Some(tys)
    }

    fn pop_type_args(&mut self, count: usize) -> Result<TakenTypeArgs, VmError> {
        self.take_type_args_below_values(count, 0)
    }

    /// The error a `baml.errors.Context` describes.
    fn context_error(&self, context: Value) -> Result<Value, VmInternalError> {
        let instance = self.as_instance(&context)?;
        Ok(crate::package_baml::view::errors::Context {
            instance: &instance,
        }
        .error())
    }

    /// The trace a `baml.errors.Context` records, as the unwinder reports it
    /// when the error escapes every handler.
    fn context_trace(&self, context: Value) -> Result<Vec<StackFrame>, VmInternalError> {
        use crate::package_baml::view::errors;

        let instance = self.as_instance(&context)?;
        let stack_trace = errors::Context {
            instance: &instance,
        }
        .stack_trace();
        let instance = self.as_instance(&stack_trace)?;
        errors::StackTrace {
            instance: &instance,
        }
        .frames(self)
        .iter()
        .map(|frame| {
            let frame = errors::StackFrame {
                instance: &self.as_instance(frame)?,
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
            let source = crate::package_baml::view::errors::Context {
                instance: &instance,
            };
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
        let active = self.handler_contexts::<true>();
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

    /// Collect active handler contexts, or just the innermost cause. The latter
    /// stays inline in the `SmallVec` and stops walking as soon as it finds one.
    fn handler_contexts<const ALL: bool>(&self) -> SmallVec<[Value; 1]> {
        let mut contexts = SmallVec::new();
        let mut innermost_bc = true;
        for frame in self.stack_view().into_iter().rev() {
            let compiled_cause = match frame {
                FrameAt::Record(_) => Some(self.frame_cause(frame)),
                FrameAt::Vm(depth) if self.compiled_at(depth).is_some() => {
                    Some(self.frame_cause(FrameAt::Vm(depth)))
                }
                FrameAt::Vm(_) => None,
            };
            if let Some(cause) = compiled_cause {
                innermost_bc = false;
                contexts.extend(cause);
            } else if let FrameAt::Vm(depth) = frame
                && let Frame::Bytecode(bf) = &self.frames[depth]
            {
                let pc = if innermost_bc {
                    self.cur_pc
                } else {
                    bf.faulting_pc
                };
                innermost_bc = false;
                // SAFETY: same `load_function` contract as the unwind walk.
                let Ok(func) = (unsafe { self.load_function(depth) }) else {
                    if ALL {
                        continue;
                    }
                    break;
                };
                let table = match &func.bytecode.compact {
                    Some(compact) => &compact.handler_context_table,
                    None => &func.bytecode.handler_context_table,
                };
                let entries = table
                    .iter()
                    .filter(|entry| pc >= entry.start_pc && pc < entry.end_pc);
                let value = |entry: &bex_vm_types::bytecode::HandlerContextEntry| {
                    self.stack[Self::local_slot_stack_index(bf.locals_offset, entry.context_slot)]
                };
                if ALL {
                    contexts.extend(entries.map(value));
                } else {
                    contexts.extend(entries.max_by_key(|entry| entry.handler_pc).map(value));
                }
            }
            if !ALL && !contexts.is_empty() {
                break;
            }
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

    /// The instance `ptr` names, or `None` when it names an object. Shared
    /// like [`Self::get_object`]: the instance's slots are atomic.
    #[inline]
    pub fn get_instance(&self, ptr: HeapPtr) -> Option<InstanceRef> {
        // SAFETY: as for `get_object`.
        unsafe { InstanceRef::of(ptr) }
    }

    /// `field` as a value, a raw float inline when it has an inline encoding.
    #[inline]
    pub fn field_to_value(&mut self, field: Field) -> Value {
        match field {
            Field::Value(value) => value,
            Field::Float(float) => self.float_value(float),
        }
    }

    /// Field `index` of `instance` as a value, boxing a raw float.
    pub fn load_field_value(&mut self, instance: InstanceRef, index: usize) -> Value {
        let field = instance.load(index);
        self.field_to_value(field)
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
        self.lookup_type_in(self.current_runtime_package(), qtn)
    }

    /// [`Self::lookup_type`] rooted at an explicitly chosen runtime package
    /// (a null pointer for the program's root).
    pub fn lookup_type_in(
        &self,
        current_ptr: HeapPtr,
        qtn: &baml_type::TypeName,
    ) -> Option<HeapPtr> {
        let package = self.package_for_type_in(current_ptr, qtn)?;
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

    /// Whether `value` is a panic: an instance of one of the `baml.panics`
    /// classes, decided by the class it points at — the stdlib's own
    /// declaration, never one that merely renders under the same name.
    pub fn is_panic(&self, value: Value) -> bool {
        self.instance_class(value)
            .is_some_and(|class| self.panic_class_ptrs.contains(&class))
    }

    /// The class of the instance `value` names, if it names one.
    #[inline]
    pub fn instance_class(&self, value: Value) -> Option<HeapPtr> {
        self.class_instance(value.as_object_ptr()?)
            .map(|(class, _)| class)
    }

    /// The instance at `ptr` a BEP-042 `cleanup` runs on, if it is one.
    pub fn cleanup_subject(&self, ptr: HeapPtr) -> Option<bex_vm_types::CleanupSubject> {
        // SAFETY: as for `get_object`.
        unsafe { bex_vm_types::CleanupSubject::at(ptr) }
    }

    /// The class the instance at `ptr` belongs to and the type arguments it
    /// was built at, for a field-backed or a `@@rust_type` instance alike.
    pub fn class_instance(&self, ptr: HeapPtr) -> Option<(HeapPtr, &[bex_vm_types::RealizedTy])> {
        // SAFETY: as for `get_object`.
        unsafe { Object::class_instance_at(ptr) }
    }

    /// The declaration of the standard library's error class `class`.
    pub fn error_class_ptr(&self, class: ErrorClass) -> HeapPtr {
        self.error_class_ptrs[class as usize]
    }

    /// Whether `value` is a panic of exactly `class`, by the class's identity.
    pub fn is_panic_of(&self, value: Value, class: PanicClass) -> bool {
        self.instance_class(value) == Some(self.panic_class_ptrs[class as usize])
    }

    /// Whether `value` is the cancellation panic, `baml.panics.Cancelled`,
    /// by its class's identity.
    pub fn is_cancelled_panic(&self, value: Value) -> bool {
        self.instance_class(value).is_some_and(|class| {
            self.panic_class_ptrs.get(PanicClass::Cancelled as usize) == Some(&class)
        })
    }

    /// The head for the declaration `qtn` names — its tag and its pointer, both
    /// read off the declaration itself.
    ///
    /// The one name→head channel inside the VM, for the boundary that
    /// genuinely starts from a name: host conversion, where a wire name
    /// arrives (the SDK boundary, until it carries identity). Everything
    /// interior already holds a head and must deref it instead — the
    /// runtime's own stdlib references go by identity
    /// ([`Self::lang_head`]): content-addressing a name into a tag would
    /// fabricate an identity rather than resolve one, and cannot see a
    /// runtime-created declaration at all.
    pub fn declaration_head(&self, qtn: &baml_type::TypeName) -> Option<bex_vm_types::TypeHead> {
        self.declaration_head_in(self.current_runtime_package(), qtn)
    }

    /// [`Self::declaration_head`] resolved against the program's root
    /// package, whatever frame is executing. A host spells a name from the
    /// root's viewpoint; resolving it against the executing frame's package
    /// would let the same spelling mean a runtime-compiled package's
    /// declaration while that package's function runs.
    pub fn root_declaration_head(
        &self,
        qtn: &baml_type::TypeName,
    ) -> Option<bex_vm_types::TypeHead> {
        self.declaration_head_in(HeapPtr::null(), qtn)
    }

    fn declaration_head_in(
        &self,
        current_ptr: HeapPtr,
        qtn: &baml_type::TypeName,
    ) -> Option<bex_vm_types::TypeHead> {
        let ptr = self
            .lookup_type_in(current_ptr, qtn)
            .or_else(|| self.lookup_interface_in(current_ptr, qtn))
            .or_else(|| self.lookup_type_alias_ptr_in(current_ptr, qtn))?;
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

    /// The class, enum, interface, or recursive type alias `item` names in its
    /// language package, by identity
    /// ([`PackageIndex::lang_declaration`](crate::package_load::PackageIndex::lang_declaration)):
    /// the package by what it is, never by a name the program gives it. `None`
    /// when the executable does not load that package (a program without the
    /// standard library); native code that is itself part of the package
    /// reads through [`Self::lang_object`].
    pub fn lang_declaration(&self, item: baml_base::LangItem) -> Option<HeapPtr> {
        self.packages.lang_declaration(item)
    }

    /// The head of the declaration `item` names, if its package is loaded.
    pub fn lang_head(&self, item: baml_base::LangItem) -> Option<bex_vm_types::TypeHead> {
        self.packages.lang_head(item)
    }

    /// [`Self::lang_declaration`] for a declaration the standard library is
    /// known to make: the native code asking is that package's own, so the
    /// package is loaded whenever the code runs, and load resolved every item
    /// of [`crate::lang_items::runtime_items`] against it
    /// ([`require_lang_items`]). Used by the generated `copy::` struct
    /// `to_value()` methods — whose items the builtins codegen derives from
    /// the stdlib sources the package was compiled from — and by native code
    /// that reaches a stdlib declaration.
    ///
    /// # Panics
    ///
    /// When `item`'s package is not loaded: native code of a package the
    /// executable does not hold is running.
    pub fn lang_object(&self, item: baml_base::LangItem) -> HeapPtr {
        self.lang_declaration(item).unwrap_or_else(|| {
            panic!("the standard library declares `{item}`, which is not loaded")
        })
    }

    /// The `Object::TypeAlias` pointer for `qtn`, if its package declares one.
    pub fn lookup_type_alias_ptr(&self, qtn: &baml_type::TypeName) -> Option<HeapPtr> {
        self.lookup_type_alias_ptr_in(self.current_runtime_package(), qtn)
    }

    /// [`Self::lookup_type_alias_ptr`] rooted at an explicitly chosen runtime
    /// package (a null pointer for the program's root).
    pub fn lookup_type_alias_ptr_in(
        &self,
        current_ptr: HeapPtr,
        qtn: &baml_type::TypeName,
    ) -> Option<HeapPtr> {
        let local = bex_vm_types::types::LocalName {
            namespace: qtn.namespace().clone(),
            name: qtn.name().clone(),
        };
        self.package_for_type_in(current_ptr, qtn)?
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

    /// Mutate an object in place through its `HeapPtr`.
    ///
    /// Whatever the object comes to keep alive outside its slot while the
    /// returned guard is held — or stops keeping alive — is charged to this
    /// VM's allocation account when the guard drops.
    ///
    /// # Safety
    ///
    /// The heap is shared by every VM, and `&self` proves nothing about the
    /// object, so the caller must guarantee that nothing else reaches it while
    /// the guard is held — including through another guard from this VM.
    /// Everything else on the heap is mutated through its own interior
    /// synchronization (locked containers, atomic slots) or built complete
    /// behind a [`bex_heap::Reservation`]. Three protocols write through this
    /// today: a package being grafted, before the load is visible; a
    /// `reflect.Session` package, under its `busy` lease; and a class the
    /// runtime class builder has not frozen yet.
    // BUG: only the first protocol meets the contract. The session lease keeps
    // other evals out, not readers: a function from an earlier submission,
    // running on another VM, reads the package's tables (and the cells box a
    // graft replaces) while they are written, and so does `Session.diagnostics`.
    // `Package._finish` is in the same position once the package's `$init` has
    // run, since anything the `$init` spawned can read the package. The class
    // builder has its own `BUG:` at its call. A package that never changes
    // after it is loaded would leave only the first protocol.
    #[inline]
    pub unsafe fn get_object_mut(&self, ptr: HeapPtr) -> ObjectMut<'_> {
        debug_assert!(
            !self.heap.is_compile_time_ptr(ptr),
            "Cannot mutate compile-time object"
        );
        // SAFETY: caller upholds the object-exclusivity contract documented above.
        let object = unsafe { ptr.get_mut() };
        ObjectMut {
            before: ObjectMut::footprint(object),
            finalizable_before: Finalizable::of(object),
            object,
            debt: self.tlab.alloc_debt(),
            heap: &self.heap,
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
        // Text a rope materialized when it was read, which its allocation
        // was not charged for (see `BexStr::unshared_heap_bytes`).
        let materialized = bex_str::take_materialized_bytes();
        if materialized != 0 {
            self.tlab.alloc_debt().grow(materialized);
        }
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
        self.tlab
            .alloc_debt()
            .grow(bex_str::take_materialized_bytes());
        self.tlab.flush_alloc_debt();
        // The poll reads the pressure flag the settlement may have raised.
        self.tlab.take_budget_crossed();
        self.early_yield.poll()
    }

    pub(crate) fn current_runtime_package(&self) -> HeapPtr {
        // A directly called body runs in its record's package.
        if let Some(FrameAt::Record(record)) = self.top_frame() {
            return compiled_rt::record_ref(record).package.get();
        }
        self.frames
            .iter()
            .rev()
            .find_map(|frame| match frame {
                Frame::Bytecode(frame) => Some(match self.get_object(frame.function) {
                    Object::Function(function) => self.runtime_package_of(function),
                    Object::GenericFunction(function) => function.runtime_package,
                    Object::Closure(closure) => match unsafe { closure.function.get() } {
                        Object::Function(function) => self.runtime_package_of(function),
                        _ => HeapPtr::null(),
                    },
                    Object::BoundMethod(method) => match unsafe { method.function.get() } {
                        Object::Function(function) => self.runtime_package_of(function),
                        _ => HeapPtr::null(),
                    },
                    _ => HeapPtr::null(),
                }),
                Frame::Native(_) => None,
            })
            .unwrap_or_else(HeapPtr::null)
    }

    #[inline]
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
        self.package_object(package_ptr, idx)
    }

    /// [`Self::idx_to_ptr`] for an operand of a function whose runtime
    /// package (`runtime_package_of`, null for a static function) the caller
    /// already holds, as the dispatch loop does: no frame walk.
    #[inline]
    fn object_in(&self, package: HeapPtr, idx: ObjectIndex) -> HeapPtr {
        if package.is_null() {
            self.heap.compile_time_ptr(idx.into_raw())
        } else {
            self.package_object(package, idx)
        }
    }

    /// Variant `variant` (an int) of the enum at `enum_idx` in `package`, for
    /// `LoadVariant`, built as `AllocVariant` builds it. A compile-time enum's
    /// variants are permanent shared objects (see `Enum::shared_variants`); an
    /// enum created at runtime has none, so its variant is allocated here.
    ///
    /// Out of line: `run_compact` is sensitive to what its arms hold.
    #[inline(never)]
    fn variant_value(
        &mut self,
        package: HeapPtr,
        enum_idx: ObjectIndex,
        variant: Value,
    ) -> Result<Value, VmError> {
        let enum_ptr = self.object_in(package, enum_idx);
        let Object::Enum(enm) = self.get_object(enum_ptr) else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Enum.into(),
                got: ObjectType::of(self.get_object(enum_ptr)).into(),
            }
            .into());
        };
        let Some(variant_index) = variant.as_int() else {
            return Err(VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Int,
                got: self.type_of(&variant),
            }
            .into());
        };
        let variant_count = enm.variants.len();
        let Some(variant_usize) = usize::try_from(variant_index)
            .ok()
            .filter(|&index| index < variant_count)
        else {
            return Err(VmError::thrown_fresh(self.panic_to_exception_value(
                VmPanic::IndexOutOfBounds {
                    index: variant_index,
                    length: variant_count,
                },
            )));
        };
        if let Some(&shared) = enm.shared_variants.get(variant_usize) {
            return Ok(Value::object(shared));
        }
        Ok(Value::object(self.tlab.alloc(Object::Variant(Variant {
            enm: enum_ptr,
            index: variant_usize,
        }))))
    }

    /// The object at `idx` in the runtime package `package_ptr`.
    pub(crate) fn package_object(&self, package_ptr: HeapPtr, idx: ObjectIndex) -> HeapPtr {
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
    #[inline]
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

    pub(crate) fn tuple_at_type(
        tuple_ty: bex_vm_types::RealizedTy,
        elements: Vec<Value>,
    ) -> Result<bex_vm_types::Tuple, VmInternalError> {
        let arity = elements.len();
        match tuple_ty {
            bex_vm_types::RealizedTy::Tuple(slots) if slots.len() == arity => {
                bex_vm_types::Tuple::new(slots, elements)
                    .map_err(|_| unreachable!("slot and element counts checked above"))
            }
            other => Err(VmInternalError::TupleTypeMismatch {
                arity,
                ty: other.to_string(),
            }),
        }
    }

    /// Get string from a Value.
    #[inline]
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
    /// futures - is truthy. The scalar fast path (inline floats included)
    /// never touches the heap; only strings, containers, and the boxed
    /// numerics dereference.
    pub fn is_truthy(&self, value: Value) -> bool {
        if let Some(truthiness) = Self::scalar_truthiness(value) {
            return truthiness;
        }
        let Some(ptr) = value.as_object_ptr() else {
            unreachable!("scalar_truthiness covers every value that is not an object")
        };
        match self.get_object(ptr) {
            Object::String(s) => !s.is_empty(),
            Object::Array(arr) => !arr.is_empty(),
            Object::Map(map) => !map.is_empty(),
            Object::Uint8Array(bytes) => !bytes.is_empty(),
            Object::Float(f) => *f != 0.0,
            Object::Bigint(n) => n.sign() != num_bigint::Sign::NoSign,
            _ => true,
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
        self.value_matches_type_constant_with_type_args(
            frame_type_args,
            value,
            raw_const,
            resolved_const,
        )
    }

    /// [`Self::value_matches_type_constant`] with explicit type arguments.
    fn value_matches_type_constant_with_type_args(
        &self,
        frame_type_args: &[bex_vm_types::RealizedTy],
        value: Value,
        raw_const: &ConstValue,
        resolved_const: Value,
    ) -> Result<bool, VmInternalError> {
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
                match value
                    .as_object_ptr()
                    .and_then(|ptr| self.class_instance(ptr))
                {
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
                    Some((instance_class, class_type_args)) if instance_class == class_ptr => {
                        debug_assert_eq!(
                            type_args_templates.len(),
                            class_type_args.len(),
                            "Class should have consistent number of generic parameters",
                        );
                        if type_args_templates.len() != class_type_args.len() {
                            return Ok(false);
                        }
                        for (template, actual) in type_args_templates.iter().zip(class_type_args) {
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
                    Some(_) | None => Ok(false),
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
                        Some(val_ptr) => match self.class_instance(val_ptr) {
                            Some((class, _)) => class == expected_ptr,
                            None => matches!(
                                self.get_object(val_ptr),
                                Object::Variant(variant) if variant.enm == expected_ptr
                            ),
                        },
                        None => false,
                    })
                } else if let Some(tag) = resolved_const.as_int() {
                    if tag == baml_type::typetag::PANIC {
                        return Ok(self.is_panic(value));
                    }
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
    /// *and* exact contents. A `float` is not the int literal `1` however
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
            baml_type::Literal::Float(repr) => value
                .as_float()
                .is_some_and(|v| repr.parse::<f64>().is_ok_and(|expected| v == expected)),
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
            .declared_name()
            .map_or_else(|| function.name(), ToString::to_string)
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
        // A type argument carries its declaration heads inside the
        // `RealizedTy` itself, so the callee resolves the same declarations.
        frame.extend(method_type_args.tys);
        Ok((callee, frame))
    }

    /// The value's concrete type as a [`bex_vm_types::ConcreteRealizedTy`] — the invariant every
    /// runtime value's type satisfies (a concrete top with realized arguments, no
    /// type variables) made explicit in the type. `None` for a value kind that
    /// has no such type (a compile-time definition object).
    ///
    /// Primitives construct their leaf directly; an `Instance` narrows its stored
    /// `class_type_args` into the argument list (so `Box<int>` resolves the `Box`
    /// impl at `T = int`); an enum `Variant` maps to its enum; a container narrows
    /// its element/key/value types; a future reports the `Future<T, E>` its spawn
    /// site was typed at; a `Cell` is transparent. A value's arguments
    /// are realized by construction, so a per-argument narrow (see `realized_arg`)
    /// fails (→ `None`) only if a residual type variable leaked in — a bug.
    ///
    /// The interface resolver and the host boundary (which infers a generic
    /// call's type arguments from the values that arrived, and names a value's
    /// type in a refusal) read this. The `IsType` value matcher goes through
    /// `value_singleton_ty` instead — it needs the value's *most precise* type,
    /// which for a primitive is its singleton, not its base leaf.
    pub fn value_concrete_ty(&self, value: Value) -> Option<bex_vm_types::ConcreteRealizedTy> {
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
        if value.is_inline_float() {
            return Some(ConcreteRealizedTy::Float);
        }
        Some(match self.get_object(value.as_object_ptr()?) {
            Object::Float(_) => ConcreteRealizedTy::Float,
            Object::Bigint(_) => ConcreteRealizedTy::Bigint,
            Object::String(_) => ConcreteRealizedTy::String,
            Object::Uint8Array(_) => ConcreteRealizedTy::Uint8Array,
            Object::Instance => {
                let inst = self.get_instance(value.as_object_ptr()?)?;
                let class = inst.class_object();
                debug_assert_eq!(inst.type_args().len(), class.generic_param_count);
                // A generic instance's stored type arguments are already
                // realized (`Box<int>` ⇒ `T = int`), so they are exactly the
                // `ConcreteRealizedTy::Class` argument list.
                ConcreteRealizedTy::Class(
                    bex_vm_types::TypeHead::new(inst.class(), class.type_tag),
                    inst.type_args().into(),
                )
            }
            Object::Variant(v) => match self.get_object(v.enm) {
                Object::Enum(e) => {
                    ConcreteRealizedTy::Enum(bex_vm_types::TypeHead::new(v.enm, e.type_tag))
                }
                other => unreachable!(
                    "Variant.enm must point to an Enum, found {:?}",
                    ObjectType::of(other)
                ),
            },
            // A `@@rust_type` class's instance is the class at its realized
            // type arguments, exactly as a plain instance is.
            // A media value is an instance of a std media class
            // (`baml.media.{Image,Audio,Video,Pdf}`), but its concrete type is
            // the `image`/`audio`/… primitive (`ConcreteRealizedTy::Media`) —
            // which is how the impl registry keys `implement I for image`.
            Object::RustInstance(bex_vm_types::RustInstance {
                payload:
                    bex_vm_types::RustPayload::Image(media)
                    | bex_vm_types::RustPayload::Audio(media)
                    | bex_vm_types::RustPayload::Pdf(media)
                    | bex_vm_types::RustPayload::Video(media),
                ..
            }) => ConcreteRealizedTy::Media(media.kind.into()),
            Object::RustInstance(instance) => match self.get_object(instance.class) {
                Object::Class(class) => {
                    debug_assert_eq!(instance.class_type_args.len(), class.generic_param_count);
                    ConcreteRealizedTy::Class(
                        bex_vm_types::TypeHead::new(instance.class, class.type_tag),
                        Box::from(instance.class_type_args.as_slice()),
                    )
                }
                other => unreachable!(
                    "RustInstance.class must point to a Class, found {:?}",
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
            Object::Tuple(tuple) => ConcreteRealizedTy::Tuple(tuple.element_tys.clone()),
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

            Object::Tombstone => Object::tombstone_reached(),

            // A GC-debug sentinel is never a live value.
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => return None,
        })
    }

    /// The receiver half of a virtual-site query, borrowed where it can be.
    /// Static class tags are immutable and unique in one program, so an
    /// instance of a compile-time class is keyed by its tag and its own type
    /// arguments, read in place. Any other receiver (a primitive, a
    /// container, an instance of a runtime-built class) is keyed by its
    /// realized concrete type, built into `scratch`.
    fn virtual_receiver_query<'a>(
        &'a self,
        value: Value,
        scratch: &'a mut Option<bex_vm_types::RealizedTy>,
    ) -> Option<VirtualReceiverQuery<'a>> {
        if let Some(ptr) = value.as_object_ptr()
            && let Some(instance) = self.get_instance(ptr)
            && self.heap.is_compile_time_ptr(instance.class())
        {
            return Some(VirtualReceiverQuery::Class {
                type_tag: instance.class_object().type_tag.as_i64(),
                type_args: instance.type_args(),
            });
        }
        *scratch = self
            .value_concrete_ty(value)
            .map(bex_vm_types::RealizedTy::from);
        let scratch: &'a Option<bex_vm_types::RealizedTy> = scratch;
        scratch.as_ref().map(VirtualReceiverQuery::Other)
    }

    /// The question a virtual-dispatch site asks its cache: this site, this
    /// receiver type, this interface instantiation. `None` when the caller
    /// has no cache identity or the receiver no concrete type. Everything is
    /// borrowed except a non-instance receiver's type, which `scratch` holds.
    fn virtual_site_query<'a>(
        &'a self,
        caller: Option<usize>,
        receiver: Value,
        iface_ptr: HeapPtr,
        scratch: &'a mut Option<bex_vm_types::RealizedTy>,
    ) -> Option<VirtualSiteQuery<'a>> {
        let caller_function_addr = caller?;
        let (interface_head, interface_args) = self.interface_operand_parts(iface_ptr);
        Some(VirtualSiteQuery {
            caller_function_addr,
            call_pc: self.cur_pc,
            receiver: self.virtual_receiver_query(receiver, scratch)?,
            interface_head,
            interface_args,
        })
    }

    /// The interface a virtual operation's operand names, and its input
    /// arguments, borrowed from the operand's type object. Associated types
    /// are outputs and play no part in selecting an impl.
    fn interface_operand_parts(
        &self,
        iface_ptr: HeapPtr,
    ) -> (bex_vm_types::TypeHead, &[bex_vm_types::RealizedTy]) {
        match self.get_object(iface_ptr) {
            Object::Type(type_value) => match &type_value.ty {
                bex_vm_types::RealizedTy::Interface(head, args, _assoc) => (*head, args),
                other => unreachable!(
                    "virtual operation interface operand must be an Interface type, found {other:?}"
                ),
            },
            other => unreachable!(
                "as_object_ptr(Type) guarantees a Type object, found {:?}",
                ObjectType::of(other)
            ),
        }
    }

    /// The running function's identity in the static caches: the address of
    /// a static function's never-moving payload, or a runtime function's
    /// tagged object address (see [`Self::runtime_cache_function_addr`]).
    #[inline]
    fn static_cache_identity(
        &self,
        function: &Function,
        runtime_package: HeapPtr,
        frame_idx: usize,
    ) -> Option<usize> {
        if runtime_package.is_null() {
            Some(std::ptr::from_ref(function) as usize)
        } else {
            self.runtime_cache_function_addr(self.frames[frame_idx].function())
        }
    }

    /// Runtime callers use the same immutable-rule cache as static callers,
    /// but their movable function-object pointer is tagged for GC forwarding.
    /// Keep this uncommon key construction out of the ordinary interpreter
    /// path so static bytecode retains its existing hot-loop layout.
    #[inline(never)]
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

    /// The cached type object for `LoadType` constant `constant` of the
    /// function `identity` names, as the frame at `frame_idx` would build it.
    #[inline]
    fn cached_load_type(
        &self,
        identity: usize,
        constant: usize,
        frame_idx: usize,
    ) -> Option<HeapPtr> {
        match self.static_load_type_cache.get(&(identity, constant))? {
            LoadTypeSite::Realized(ty) => Some(ty.ptr()),
            LoadTypeSite::Generic(bindings) => {
                let Frame::Bytecode(frame) = &self.frames[frame_idx] else {
                    return None;
                };
                bindings
                    .iter()
                    .find(|(args, _)| args[..] == frame.type_args[..])
                    .map(|(_, ty)| ty.ptr())
            }
            LoadTypeSite::Projection => None,
        }
    }

    /// Build the type object for `LoadType` constant `constant` and, when the
    /// running function has a cache `identity`, remember it.
    ///
    /// A fully-realized template narrows to `RealizedTy` in a single
    /// validation walk — no substitution environment needed. Otherwise its
    /// frame refs resolve (and any projection reduces) against the frame's
    /// realized type args; the result must be realized or it is an internal
    /// error, never an `unknown` erasure.
    #[inline(never)]
    fn load_type_uncached(
        &mut self,
        function: &'static Function,
        identity: Option<usize>,
        constant: usize,
        frame_idx: usize,
    ) -> Result<Value, VmError> {
        let ConstValue::Type(template) = &function.bytecode.constants[constant] else {
            return Err(VmInternalError::UnexpectedConstantKind.into());
        };
        let realized = <&bex_vm_types::RealizedTy>::try_from(template).ok();
        let ty = match realized {
            Some(realized) => realized.clone(),
            None => {
                let frame_type_args: &[bex_vm_types::RealizedTy] = match &self.frames[frame_idx] {
                    Frame::Bytecode(frame) => &frame.type_args,
                    Frame::Native(_) => &[],
                };
                template.substitute(frame_type_args, self).map_err(|e| {
                    VmInternalError::TypeSubstitution {
                        message: e.to_string(),
                    }
                })?
            }
        };
        let ptr = self.tlab.alloc_type(TypeValue::new(ty));
        let Some(identity) = identity else {
            return Ok(Value::object(ptr));
        };
        let key = (identity, constant);
        if realized.is_some() {
            self.static_load_type_cache
                .insert(key, LoadTypeSite::Realized(Weak::new(ptr)));
        } else if template.has_projection() {
            self.static_load_type_cache
                .insert(key, LoadTypeSite::Projection);
        } else {
            let args: Box<[bex_vm_types::RealizedTy]> = match &self.frames[frame_idx] {
                Frame::Bytecode(frame) => frame.type_args.as_slice().into(),
                Frame::Native(_) => Box::default(),
            };
            let site = self
                .static_load_type_cache
                .entry(key)
                .or_insert_with(|| LoadTypeSite::Generic(SmallVec::new()));
            match site {
                LoadTypeSite::Generic(bindings) => {
                    if bindings.len() >= LOAD_TYPE_SITE_BINDINGS {
                        bindings.remove(0);
                    }
                    bindings.push((args, Weak::new(ptr)));
                }
                LoadTypeSite::Realized(_) | LoadTypeSite::Projection => {
                    unreachable!("a constant's template does not change kind")
                }
            }
        }
        Ok(Value::object(ptr))
    }

    /// `owner` if it is a runtime package: a compile-time package (a static
    /// declaration's owner) and null (a declaration still being minted) are
    /// not.
    pub(crate) fn runtime_owner(&self, owner: HeapPtr) -> Option<HeapPtr> {
        (!owner.is_null() && !self.heap.is_compile_time_ptr(owner)).then_some(owner)
    }

    /// The runtime package a function addresses its operands through
    /// ([`Function::owner`]), or null for a static function — whose owner is
    /// a compile-time package and whose operands live in the program's pools
    /// — and for the VM's own entry trampolines, which have no owner.
    #[inline]
    pub(crate) fn runtime_package_of(&self, function: &Function) -> HeapPtr {
        self.runtime_owner(function.owner)
            .unwrap_or_else(HeapPtr::null)
    }

    pub(crate) fn hotswap_enabled(&self) -> bool {
        self.hotswap_enabled
    }

    /// What a call of `callee` enters: the hot swap installed on the function
    /// behind it, else `callee`. One hop: a replacement never follows a
    /// redirect of its own. Every call path selects here exactly once, in
    /// [`Self::execute_call_from_locals_offset`] and, for host and spawn
    /// entries, [`Self::set_entry_point_with_type_args`].
    /// Whether a call of `callee` must take the general call path, which
    /// selects a hot swap: one is installed on it, or a patch is previewed.
    #[inline]
    fn hotswap_redirects(&self, callee: &Function) -> bool {
        self.hotswap_enabled
            && (callee.hotswap_redirect.get().is_some() || self.hotswap_prepared.is_some())
    }

    fn hotswap_entry(&self, callee: HeapPtr) -> HeapPtr {
        let function_ptr = match self.get_object(callee) {
            Object::BoundMethod(method) => method.function,
            Object::Closure(closure) => closure.function,
            Object::GenericFunction(generic) => self
                .generic_function_authored_ptr(generic)
                .unwrap_or(callee),
            _ => callee,
        };
        let Object::Function(function) = self.get_object(function_ptr) else {
            return callee;
        };
        let installed = function.hotswap_redirect.get();
        let installed = match &self.hotswap_prepared {
            Some(prepared) => prepared.preview(function_ptr, installed),
            None => installed,
        };
        installed.unwrap_or(callee)
    }

    /// Runtime package that owns a value's nominal/callable definition.
    /// Static values return null.
    pub(crate) fn value_runtime_package(&self, value: Value) -> HeapPtr {
        let Some(ptr) = value.as_object_ptr() else {
            return HeapPtr::null();
        };
        let owner = match self.get_object(ptr) {
            Object::Instance => self
                .get_instance(ptr)
                .and_then(|instance| instance.class_object().owner.package()),
            Object::RustInstance(instance) => match self.get_object(instance.class) {
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
            Object::Function(function) => Some(self.runtime_package_of(function)),
            Object::GenericFunction(function) => Some(function.runtime_package),
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(function) => Some(self.runtime_package_of(function)),
                _ => None,
            },
            Object::BoundMethod(method) => match unsafe { method.function.get() } {
                Object::Function(function) => Some(self.runtime_package_of(function)),
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
    #[inline]
    pub fn as_array(
        &self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayReadGuard<'_>, VmInternalError> {
        // SAFETY: the locked specialization needs no locality proof.
        unsafe { self.as_array_in::<false>(*value) }
    }

    /// Get mutable array from a Value. Acquires the container's mutex;
    /// the returned guard derefs to `&mut Vec<Value>` and releases the
    /// lock on drop, serializing concurrent mutators under `spawn`.
    #[inline]
    pub fn as_array_mut(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayWriteGuard<'_>, VmInternalError> {
        // SAFETY: the locked specialization needs no locality proof.
        unsafe { self.as_array_mut_in::<false>(*value) }
    }

    /// [`Self::as_array`] for an array only this thread can reach, without
    /// its lock.
    ///
    /// # Safety
    ///
    /// No other thread may reach the array while the guard lives; escape
    /// analysis proves this at the call sites that select a `$local` builtin.
    #[inline]
    pub unsafe fn as_array_local(
        &self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayReadGuard<'_>, VmInternalError> {
        // SAFETY: forwarded locality contract.
        unsafe { self.as_array_in::<true>(*value) }
    }

    /// [`Self::as_array_mut`] for an array only this thread can reach,
    /// without its lock.
    ///
    /// # Safety
    ///
    /// As for [`Self::as_array_local`].
    #[inline]
    pub unsafe fn as_array_mut_local(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::ArrayWriteGuard<'_>, VmInternalError> {
        // SAFETY: forwarded locality contract.
        unsafe { self.as_array_mut_in::<true>(*value) }
    }

    /// Get map from a Value. Acquires the container's mutex.
    #[inline]
    pub fn as_map(&self, value: &Value) -> Result<bex_vm_types::MapReadGuard<'_>, VmInternalError> {
        // SAFETY: the locked specialization needs no locality proof.
        unsafe { self.as_map_in::<false>(*value) }
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
    #[inline]
    pub fn as_map_mut(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::MapWriteGuard<'_>, VmInternalError> {
        // SAFETY: the locked specialization needs no locality proof.
        unsafe { self.as_map_mut_in::<false>(*value) }
    }

    /// [`Self::as_map`] for a map only this thread can reach, without its
    /// lock.
    ///
    /// # Safety
    ///
    /// As for [`Self::as_array_local`].
    #[inline]
    pub unsafe fn as_map_local(
        &self,
        value: &Value,
    ) -> Result<bex_vm_types::MapReadGuard<'_>, VmInternalError> {
        // SAFETY: forwarded locality contract.
        unsafe { self.as_map_in::<true>(*value) }
    }

    /// [`Self::as_map_mut`] for a map only this thread can reach, without
    /// its lock.
    ///
    /// # Safety
    ///
    /// As for [`Self::as_array_local`].
    #[inline]
    pub unsafe fn as_map_mut_local(
        &mut self,
        value: &Value,
    ) -> Result<bex_vm_types::MapWriteGuard<'_>, VmInternalError> {
        // SAFETY: forwarded locality contract.
        unsafe { self.as_map_mut_in::<true>(*value) }
    }

    /// With `THREAD_LOCAL`, the caller must prove only this task reaches the container.
    #[inline]
    unsafe fn as_array_in<const THREAD_LOCAL: bool>(
        &self,
        value: Value,
    ) -> Result<bex_vm_types::ArrayReadGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        match self.get_object(ptr) {
            Object::Array(container) => Ok(if THREAD_LOCAL {
                // SAFETY: forwarded locality contract.
                unsafe { container.lock_thread_local() }
            } else {
                container.lock()
            }),
            obj => Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// With `THREAD_LOCAL`, the caller must prove only this task reaches the container.
    #[inline]
    unsafe fn as_array_mut_in<const THREAD_LOCAL: bool>(
        &mut self,
        value: Value,
    ) -> Result<bex_vm_types::ArrayWriteGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Array)?;
        match self.get_object(ptr) {
            Object::Array(container) => Ok(if THREAD_LOCAL {
                // SAFETY: forwarded locality contract.
                unsafe { container.lock_mut_thread_local(self.tlab.alloc_debt()) }
            } else {
                container.lock_mut(self.tlab.alloc_debt())
            }),
            obj => Err(VmInternalError::TypeError {
                expected: ObjectType::Array.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// With `THREAD_LOCAL`, the caller must prove only this task reaches the container.
    #[inline]
    unsafe fn as_map_in<const THREAD_LOCAL: bool>(
        &self,
        value: Value,
    ) -> Result<bex_vm_types::MapReadGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Map)?;
        match self.get_object(ptr) {
            Object::Map(container) => Ok(if THREAD_LOCAL {
                // SAFETY: forwarded locality contract.
                unsafe { container.lock_thread_local() }
            } else {
                container.lock()
            }),
            obj => Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(obj).into(),
            }),
        }
    }

    /// With `THREAD_LOCAL`, the caller must prove only this task reaches the container.
    #[inline]
    unsafe fn as_map_mut_in<const THREAD_LOCAL: bool>(
        &mut self,
        value: Value,
    ) -> Result<bex_vm_types::MapWriteGuard<'_>, VmInternalError> {
        let ptr = self.as_object_ptr(value, ObjectType::Map)?;
        match self.get_object(ptr) {
            Object::Map(container) => Ok(if THREAD_LOCAL {
                // SAFETY: forwarded locality contract.
                unsafe { container.lock_mut_thread_local(self.tlab.alloc_debt()) }
            } else {
                container.lock_mut(self.tlab.alloc_debt())
            }),
            obj => Err(VmInternalError::TypeError {
                expected: ObjectType::Map.into(),
                got: ObjectType::of(obj).into(),
            }),
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
        // Lowered below, once float boxing has settled the constants.
        let bytecode = convert_program_for_load(program, true, None)?;

        // Extract compile-time objects for the heap
        let mut compile_time_objects: Vec<Object> = bytecode.objects.into_iter().collect();

        // Box every reachable `ConstValue::Float` with no inline `Value`
        // encoding (NaN, ±inf, extreme magnitudes) into a compile-time
        // `Object::Float`.
        let float_indices = bex_vm_types::types::box_compile_time_floats(
            &mut compile_time_objects,
            &bytecode.globals,
        );
        prepare_compact_code(&mut compile_time_objects, &bytecode.globals)?;

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
                bex_vm_types::ConstValue::Float(f) if float_indices.contains_key(&f.to_bits()) => {
                    let idx = float_indices[&f.to_bits()];
                    Value::object(heap.compile_time_ptr(idx))
                }
                other => other.to_value(|idx| heap.compile_time_ptr(idx.into_raw())),
            })
            .collect();
        let globals = VmGlobals::Owned(bex_vm_types::GlobalPool::from_vec(globals_vec));

        let StdlibTables {
            error_class_ptrs,
            panic_class_ptrs,
            heads: stdlib_heads,
        } = StdlibTables::resolve(&package_index)?;
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
            Arc::default(),
            // The only task this standalone VM ever runs.
            NonZeroU64::MIN,
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
            btel_types::new_recording_id(),
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
        // A specialized wrapper's type arguments apply to whichever
        // implementation is entered.
        let carried_type_args = match self.get_object(function) {
            Object::GenericFunction(generic) if self.hotswap_enabled => {
                Some(generic.type_args.to_vec())
            }
            _ => None,
        };
        let function = if self.hotswap_enabled {
            self.hotswap_entry(function)
        } else {
            function
        };
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

        if let Err(panic) = self.frames.reserve_one() {
            self.pending_entry_frame_panic = Some(panic);
            return;
        }
        self.pending_entry_frame_panic = None;

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
            self.name_root_thread(function);
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
            if let Err(panic) = self.push_host_closure_trampoline_frame(function, args) {
                self.pending_entry_frame_panic = Some(panic);
            }
            return;
        }

        // Normalize a specialized callable entry point to its concrete inner
        // function (`dispatch_ptr`) and stored specialization, so the bytecode
        // frame and native/sysop trampoline always see an Object::Function.
        let mut dispatch_ptr = function;
        let mut effective_type_args = type_args;
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
        if let Some(carried) = carried_type_args {
            effective_type_args = carried;
        }

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
        let passthrough_entry = self
            .get_object(dispatch_ptr)
            .as_callable()
            .is_ok_and(|callee| callee.trace_passthrough);
        if !hooked_entry && !passthrough_entry {
            self.entry_reservation = None;
        }
        if !hooked_entry && passthrough_entry {
            let trace = CallTrace {
                telemetry: self.entry_trace.take().unwrap_or_default(),
                context: self.entry_trace_context.clone(),
                reservation: self.entry_reservation.take(),
            };
            self.hold_trace(self.frames.len(), trace);
        }
        if !hooked_entry && let Some(patch) = self.entry_trace_context.take() {
            self.root_context = self.root_context.with_patch(&patch);
            if let Some(state) = &mut self.telemetry {
                state.set_context(self.root_context.clone());
            }
        }
        match callable_kind {
            FunctionKind::Bytecode => {
                let callee = self
                    .frame_function_for(dispatch_ptr)
                    .expect("entry point must be a valid function frame");
                let locals = callee.real_local_count;
                let reserve = args
                    .len()
                    .checked_add(callee.frame_reserve())
                    .ok_or(VmPanic::StackOverflow)
                    .and_then(|additional| self.stack.try_reserve(additional));
                if let Err(panic) = reserve {
                    self.pending_entry_frame_panic = Some(panic);
                    return;
                }
                let hooked = hooked_entry;
                if hooked {
                    self.pending_trace_hooks.push(PendingTraceHook {
                        frame: self.frames.len(),
                        caller: None,
                        caller_pc: 0,
                        trace: self.entry_trace.take().map(|telemetry| CallTrace {
                            telemetry,
                            context: self.entry_trace_context.take(),
                            reservation: self.entry_reservation.take(),
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
                self.stack
                    .try_extend(args.iter().copied())
                    .expect("entry values were reserved");
                let room = callee
                    .bytecode
                    .compact
                    .as_ref()
                    .and_then(|code| {
                        Self::frame_room_within(self.stack.capacity(), self.stack.len(), code)
                    })
                    .expect("the entry frame's room was reserved");
                self.frames.push_reserved(Frame::Bytecode(BytecodeFrame {
                    function: dispatch_ptr,
                    instruction_ptr: 0,
                    locals_offset: StackIndex::from_raw(0),
                    type_args: effective_type_args.into(),
                    faulting_pc: 0,
                    telemetry: frame_telemetry,
                    context: self.root_context.clone(),
                    room,
                }));

                // Entry functions need the same frame-local pre-allocation as normal
                // bytecode calls now that INIT_LOCALS is gone from bytecode.
                self.stack
                    .try_resize(self.stack.len() + locals, Value::NULL)
                    .expect("entry locals were reserved");

                let stub = match self.get_object(dispatch_ptr) {
                    Object::Function(callee) => crate::compiled_rt::stub_start(callee),
                    _ => None,
                };
                if let Some(stub) = stub
                    && let Some(Frame::Bytecode(frame)) = self.frames.last_mut()
                {
                    frame.instruction_ptr = stub + crate::compiled_rt::stub::START;
                }
            }
            FunctionKind::Native(..) | FunctionKind::SysOp(_) => {
                if let Err(panic) = self.push_trampoline_frame(
                    dispatch_ptr,
                    args,
                    effective_type_args,
                    callable_kind,
                ) {
                    self.pending_entry_frame_panic = Some(panic);
                }
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
        callable_kind: FunctionKind,
    ) -> Result<(), VmPanic> {
        let additional = args
            .len()
            .checked_add(type_args.len())
            .ok_or(VmPanic::StackOverflow)?;
        self.stack.try_reserve(additional)?;
        let callee_global = self
            .global_index_for_function_ptr(function)
            .expect("entry point must be present in globals");
        let ntypeargs = u16::try_from(type_args.len()).expect("entry type args fit in u16");

        let (callee_name, return_type, throws_type) = match self.get_object(function) {
            Object::Function(f) => (f.name(), f.return_type.clone(), f.throws_type.clone()),
            other => unreachable!("expect function as entry point, got {other:?}"),
        };

        self.pending_call_type_args.clear();
        match callable_kind {
            FunctionKind::Native(..) => {
                for ty in type_args {
                    let ty_ptr = self.alloc_type(bex_vm_types::types::TypeValue::new(ty));
                    self.stack.try_push(Value::object(ty_ptr))?;
                }
                self.stack.try_extend(args.iter().copied())?;
            }
            FunctionKind::SysOp(_) => {
                self.stack.try_extend(args.iter().copied())?;
            }
            FunctionKind::Bytecode | FunctionKind::NativeUnresolved => {
                unreachable!("trampoline frame requires Native or SysOp")
            }
        }

        let instructions = match callable_kind {
            FunctionKind::Native(..) => vec![
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
        // The arguments (and a native's type arguments) are on the stack
        // before the first instruction, and the call takes all of them.
        let pushed_type_args = if matches!(callable_kind, FunctionKind::Native(..)) {
            usize::from(ntypeargs)
        } else {
            0
        };
        let arity = |_: GlobalIndex| Some(args.len());
        let ctx = bex_vm_types::stack_effect::StackContext {
            entry_depth: args.len() + pushed_type_args,
            global_arity: &arity,
            indirect_args: None,
        };
        let compact = bytecode
            .lower_verified(&ctx, 0, false)
            .expect("a trampoline's call takes the values it is entered with");
        // The frame starts under its entry values, which `max_stack` counts.
        self.stack
            .try_reserve(compact.frame_slots.saturating_sub(ctx.entry_depth))?;
        let room = Self::frame_room_within(
            self.stack.capacity(),
            self.stack.len() - ctx.entry_depth,
            &compact,
        )
        .ok_or(VmPanic::StackOverflow)?;
        bytecode.compact = Some(compact);

        let display_return_type = return_type.to_string();
        let entry_function = Function {
            identity: bex_vm_types::types::FunctionIdentity::Entry(callee_name.into_boxed_str()),
            source_file: String::new(),
            docstring: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            hotswap_redirect: bex_vm_types::types::HotswapRedirect::default(),
            compiled_body: None,
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
            native_key: None,
            body_meta: None,
            trace_passthrough: false,
            owner: HeapPtr::null(),
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
        self.frames.push_reserved(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: FrameTypeArgs::default(),
            faulting_pc: 0,
            telemetry: frame_telemetry,
            context: self.root_context.clone(),
            room,
        }));
        Ok(())
    }

    /// Enter a host-owned callable as a VM root by reproducing the normal
    /// indirect-call stack shape: arguments first, callable on top. The
    /// synthetic wrapper gives the yielded host sys-op a bytecode frame to
    /// resume into and a normal `Return` path after the host result arrives.
    fn push_host_closure_trampoline_frame(
        &mut self,
        closure: HeapPtr,
        args: &[Value],
    ) -> Result<(), VmPanic> {
        self.stack
            .try_reserve(args.len().checked_add(1).ok_or(VmPanic::StackOverflow)?)?;
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
        self.stack.try_extend(args.iter().copied())?;
        self.stack.try_push(Value::object(closure))?;

        let mut bytecode = bytecode::Bytecode {
            instructions: vec![Instruction::CallIndirect, Instruction::Return],
            ..bytecode::Bytecode::default()
        };
        // The arguments and the callable are on the stack before the first
        // instruction.
        let no_globals = |_: GlobalIndex| None;
        let ctx = bex_vm_types::stack_effect::StackContext {
            entry_depth: args.len() + 1,
            global_arity: &no_globals,
            indirect_args: Some(args.len()),
        };
        let compact = bytecode
            .lower_verified(&ctx, 0, false)
            .expect("a trampoline's call takes the values it is entered with");
        // The frame starts under its entry values, which `max_stack` counts.
        self.stack
            .try_reserve(compact.frame_slots.saturating_sub(ctx.entry_depth))?;
        let room = Self::frame_room_within(
            self.stack.capacity(),
            self.stack.len() - ctx.entry_depth,
            &compact,
        )
        .ok_or(VmPanic::StackOverflow)?;
        bytecode.compact = Some(compact);

        let display_return_type = return_type.to_string();
        let entry_function = Function {
            identity: bex_vm_types::types::FunctionIdentity::Entry("<host-callable>".into()),
            source_file: String::new(),
            docstring: None,
            arity: 0,
            real_local_count: 0,
            bytecode,
            kind: FunctionKind::Bytecode,
            telemetry_function_id: None,
            telemetry_registration: bex_vm_types::FunctionRegistration::default(),
            telemetry_policy_id: btel_types::TelemetryPolicyId::none(),
            hotswap_redirect: bex_vm_types::types::HotswapRedirect::default(),
            compiled_body: None,
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
            native_key: None,
            body_meta: None,
            trace_passthrough: false,
            owner: HeapPtr::null(),
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
        self.frames.push_reserved(Frame::Bytecode(BytecodeFrame {
            function: entry_ptr,
            instruction_ptr: 0,
            locals_offset: StackIndex::from_raw(0),
            type_args: FrameTypeArgs::default(),
            faulting_pc: 0,
            telemetry: frame_telemetry,
            context: self.root_context.clone(),
            room,
        }));
        Ok(())
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

    /// The foreign payload of the `@@rust_type` instance `value`, as a shared
    /// `Arc<T>`, so a native can keep it while it borrows `&mut BexVm` to
    /// allocate. Only a bare `@@rust_type` class's payload is foreign; a kind
    /// with a variant of its own is read through its class's view.
    pub fn foreign_payload<T: bex_vm_types::BexRustData>(
        &self,
        value: &Value,
    ) -> Result<Arc<T>, VmInternalError> {
        let payload = &self.as_rust_instance(value)?.payload;
        let mismatch = || VmInternalError::RustTypeError {
            expected: TypeId::of::<T>(),
            got: payload.payload_type_id(),
        };
        match payload {
            bex_vm_types::RustPayload::Foreign(data) => Arc::clone(data)
                .downcast_payload::<T>()
                .map_err(|_| mismatch()),
            bex_vm_types::RustPayload::DefaultHasher(_)
            | bex_vm_types::RustPayload::Sha256(_)
            | bex_vm_types::RustPayload::Aes256GcmSiv(_)
            | bex_vm_types::RustPayload::Aes128GcmSiv(_)
            | bex_vm_types::RustPayload::ChaCha20Poly1305(_)
            | bex_vm_types::RustPayload::XChaCha20Poly1305(_)
            | bex_vm_types::RustPayload::Regex(_)
            | bex_vm_types::RustPayload::CsvRecord(_)
            | bex_vm_types::RustPayload::Semaphore(_)
            | bex_vm_types::RustPayload::Permit(_)
            | bex_vm_types::RustPayload::Ticket(_)
            | bex_vm_types::RustPayload::CancelToken(_)
            | bex_vm_types::RustPayload::SpanId(_)
            | bex_vm_types::RustPayload::TraceOptions(_)
            | bex_vm_types::RustPayload::ReservedSpan(_)
            | bex_vm_types::RustPayload::File(_)
            | bex_vm_types::RustPayload::Glob(_)
            | bex_vm_types::RustPayload::OutputFormat(_)
            | bex_vm_types::RustPayload::Lock(_)
            | bex_vm_types::RustPayload::TcpStream(_)
            | bex_vm_types::RustPayload::TcpListener(_)
            | bex_vm_types::RustPayload::UdpSocket(_)
            | bex_vm_types::RustPayload::Xoshiro256PlusPlus(_)
            | bex_vm_types::RustPayload::ChaCha20(_)
            | bex_vm_types::RustPayload::InterfaceWitness(_)
            | bex_vm_types::RustPayload::CsvReader(_)
            | bex_vm_types::RustPayload::CsvWriter(_)
            | bex_vm_types::RustPayload::ClassBuilder(_)
            | bex_vm_types::RustPayload::ChildProcess(_)
            | bex_vm_types::RustPayload::ReadPipe(_)
            | bex_vm_types::RustPayload::WritePipe(_)
            | bex_vm_types::RustPayload::WebSocket(_)
            | bex_vm_types::RustPayload::HttpBody(_)
            | bex_vm_types::RustPayload::SseStream(_)
            | bex_vm_types::RustPayload::HttpServer(_)
            | bex_vm_types::RustPayload::TlsKeys(_)
            | bex_vm_types::RustPayload::HostValue(_)
            | bex_vm_types::RustPayload::Image(_)
            | bex_vm_types::RustPayload::Audio(_)
            | bex_vm_types::RustPayload::Pdf(_)
            | bex_vm_types::RustPayload::Video(_)
            | bex_vm_types::RustPayload::Prompt(_) => Err(mismatch()),
        }
    }

    /// The instance `value` names, or a type error.
    ///
    /// Used by generated glue code to construct `view::` structs.
    /// The `@@rust_type` instance `value` holds.
    pub fn as_rust_instance(
        &self,
        value: &Value,
    ) -> Result<&bex_vm_types::RustInstance, VmInternalError> {
        let mismatch = || VmInternalError::TypeError {
            expected: Type::Object(ObjectType::RustInstance),
            got: self.type_of(value),
        };
        let ptr = value.as_object_ptr().ok_or_else(mismatch)?;
        match self.get_object(ptr) {
            Object::RustInstance(instance) => Ok(instance),
            _ => Err(mismatch()),
        }
    }

    /// Allocate an instance of the `@@rust_type` class `class` at
    /// `class_type_args`, holding `payload`.
    pub fn alloc_rust_instance(
        &mut self,
        class: HeapPtr,
        class_type_args: bex_vm_types::ThinVec<bex_vm_types::RealizedTy>,
        payload: bex_vm_types::RustPayload,
    ) -> Value {
        Value::object(
            self.tlab
                .alloc(Object::RustInstance(bex_vm_types::RustInstance::new(
                    class,
                    class_type_args,
                    payload,
                ))),
        )
    }

    /// A media value, as an instance of its kind's wrapper class
    /// (`baml.media.Image`, …).
    pub fn alloc_media(&mut self, media: bex_vm_types::payload::Media) -> Value {
        let class = self.lang_object(media.kind.wrapper_class());
        self.alloc_rust_instance(
            class,
            bex_vm_types::ThinVec::new(),
            bex_vm_types::RustPayload::media(media),
        )
    }

    /// A rendered prompt, as an `ai.Prompt`.
    pub fn alloc_prompt(&mut self, prompt: bex_vm_types::payload::Prompt) -> Value {
        let class = self.lang_object(crate::lang_items::AI_PROMPT);
        self.alloc_rust_instance(
            class,
            bex_vm_types::ThinVec::new(),
            bex_vm_types::RustPayload::Prompt(prompt),
        )
    }

    pub fn as_instance(&self, value: &Value) -> Result<InstanceRef, VmInternalError> {
        value
            .as_object_ptr()
            .and_then(|ptr| self.get_instance(ptr))
            .ok_or_else(|| VmInternalError::TypeError {
                expected: Type::Object(ObjectType::Instance),
                got: self.type_of(value),
            })
    }

    pub fn as_tuple(&self, value: &Value) -> Result<&bex_vm_types::Tuple, VmInternalError> {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Tuple(tuple)) => Ok(tuple),
            _ => Err(VmInternalError::TypeError {
                expected: Type::Object(ObjectType::Tuple),
                got: self.type_of(value),
            }),
        }
    }

    /// The function a frame for `function_ptr` runs: the callable itself, or
    /// the function a closure, bound method or generic function wraps.
    fn frame_function_for(
        &mut self,
        function_ptr: HeapPtr,
    ) -> Result<&'static Function, VmInternalError> {
        // SAFETY: an entry point's callable, and the function it wraps, are
        // live for the whole call.
        let obj: &'static Object = unsafe { function_ptr.get() };
        let frame_function = match obj {
            Object::Function(function) => function.as_ref(),
            Object::Closure(closure) => {
                // SAFETY: closure.function points to a Function object with
                // appropriate lifetime guarantees.
                let func_obj = unsafe { closure.function.get() };
                match func_obj {
                    Object::Function(f) => f.as_ref(),
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
                    Object::Function(f) => f.as_ref(),
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
                    Object::Function(f) => f.as_ref(),
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

        Ok(frame_function)
    }

    /// Context of the frame `run_compact` is dispatching, which is the top
    /// frame: the same value [`Self::current_context`] finds by walking the
    /// frame stack.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn dispatch_frame_context(&self, frame_idx: usize) -> &Context {
        debug_assert_eq!(frame_idx + 1, self.frames.len());
        let Some(Frame::Bytecode(frame)) = self.frames.get(frame_idx) else {
            verifier_unreachable!()
        };
        &frame.context
    }

    /// Pop the returning bytecode frame and drop its owning fields in line
    /// (the rest are `Copy`). Through `Option<Frame>`'s drop glue, every
    /// return paid an out-of-line call even though a typical frame owns
    /// nothing that needs freeing.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn pop_finished_bytecode_frame(&mut self) {
        match self.frames.pop() {
            Some(Frame::Bytecode(BytecodeFrame {
                type_args, context, ..
            })) => {
                drop(type_args);
                drop(context);
            }
            other => drop(other),
        }
    }

    /// `pop_finished_bytecode_frame` for a returning frame whose type
    /// arguments allocate nothing and whose context is the version its
    /// caller holds: dropping the context then only releases a reference.
    /// Dropping the whole frame instead would bring the type arguments' and
    /// the native frame's drop glue into the fast path.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn pop_frame_sharing_caller_context(&mut self) {
        let popped = std::mem::ManuallyDrop::new(self.frames.pop());
        if let Some(Frame::Bytecode(frame)) = &*popped {
            debug_assert!(frame.type_args.owns_nothing());
            // SAFETY: the context is moved out of a frame that is never
            // dropped, so it is dropped exactly once.
            drop(unsafe { std::ptr::read(&raw const frame.context) });
        }
    }

    /// `locals_offset - 1` of the bytecode frame at `frame_idx`; see
    /// [`Self::local_at`].
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn frame_locals_base(&self, frame_idx: usize) -> usize {
        let Some(Frame::Bytecode(frame)) = self.frames.get(frame_idx) else {
            verifier_unreachable!()
        };
        frame.locals_offset.raw().wrapping_sub(1)
    }

    /// Stack index of local `slot` (1-based) for a frame whose
    /// `locals_offset - 1` is `locals_base`. Wrapping arithmetic keeps the
    /// result exact for a frame at stack index 0 (`locals_base == usize::MAX`).
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn local_at(locals_base: usize, slot: usize) -> StackIndex {
        debug_assert!(
            slot > 0,
            "local slot 0 is reserved and should never be materialized on stack"
        );
        StackIndex::from_raw(locals_base.wrapping_add(slot))
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
            // non-null for class-field type consistency). `_handle` is the
            // `baml.host.Value` around the host's reference to its exception.
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
                // reference to the originating host exception, which crosses
                // back to the host as that reference.
                let host_value = self.lang_object(crate::lang_items::BAML_HOST_VALUE);
                let handle_val = Value::object(self.tlab.alloc(Object::RustInstance(
                    bex_vm_types::RustInstance::new(
                        host_value,
                        bex_vm_types::ThinVec::new(),
                        bex_vm_types::RustPayload::HostValue(handle),
                    ),
                )));
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
        Value::object(self.tlab.alloc_instance(class_ptr, vec![nanos]))
    }

    pub(crate) fn alloc_error_value(&mut self, class: ErrorClass, fields: Vec<Value>) -> Value {
        let class_ptr = self.error_class_ptrs[class as usize];
        Value::object(self.tlab.alloc_instance(class_ptr, fields))
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

    /// Field `index` of the instance `value` names, as a value: the
    /// interpreter's field load. `thread_local` loads without ordering, for
    /// an instance escape analysis proved no other thread can reach.
    #[allow(clippy::inline_always, reason = "the generic field read's fast path")]
    #[inline(always)]
    pub(crate) fn load_instance_field(
        &mut self,
        value: Value,
        index: usize,
        thread_local: bool,
    ) -> Result<Value, VmError> {
        match self.shaped_load(value, index, thread_local) {
            Some(loaded) => Ok(loaded),
            None => self.load_instance_field_slow(value, index, thread_local),
        }
    }

    #[cold]
    #[inline(never)]
    fn load_instance_field_slow(
        &mut self,
        value: Value,
        index: usize,
        thread_local: bool,
    ) -> Result<Value, VmError> {
        let field = self
            .checked_field(value, index)?
            .load(load_order(thread_local));
        Ok(self.field_to_value(field))
    }

    /// Store `new_value` into field `index` of the instance `value` names. A
    /// value of the wrong kind for a raw slot is a type error.
    #[allow(clippy::inline_always, reason = "the generic field store's fast path")]
    #[inline(always)]
    pub(crate) fn store_instance_field(
        &mut self,
        value: Value,
        index: usize,
        new_value: Value,
        thread_local: bool,
    ) -> Result<(), VmError> {
        if self.shaped_store(value, index, new_value, thread_local) {
            return Ok(());
        }
        self.store_instance_field_slow(value, index, new_value, thread_local)
    }

    #[cold]
    #[inline(never)]
    fn store_instance_field_slow(
        &mut self,
        value: Value,
        index: usize,
        new_value: Value,
        thread_local: bool,
    ) -> Result<(), VmError> {
        self.checked_field(value, index)?
            .store(new_value, store_order(thread_local))
            .map_err(|mismatch| self.slot_kind_error(mismatch).into())
    }

    /// Field `index` of the instance `value` names, read by the field
    /// instruction whose site is `site` in `function`: one compare against
    /// the class the site remembers, else [`Self::shaped_load`].
    #[allow(clippy::inline_always, reason = "every interpreted field read")]
    #[inline(always)]
    fn site_load(
        &self,
        function: &Function,
        site: usize,
        value: Value,
        index: usize,
        thread_local: bool,
    ) -> Option<Value> {
        let (instance, class) = unsafe { InstanceRef::with_class_bits(value.as_object_ptr()?) }?;
        let entry = Self::field_site(function, site);
        if let Some(entry) = entry
            && let Some(code) = FieldSites::hit(entry.load(Ordering::Relaxed), class)
        {
            let bits = unsafe { instance.word(1 + index) }.load(load_order(thread_local));
            return ClassShape::decode(code, bits);
        }
        self.site_load_miss(entry, value, index, thread_local)
    }

    #[cold]
    #[inline(never)]
    fn site_load_miss(
        &self,
        entry: Option<&AtomicU64>,
        value: Value,
        index: usize,
        thread_local: bool,
    ) -> Option<Value> {
        self.remember_field_site(entry, value, index);
        self.shaped_load(value, index, thread_local)
    }

    /// Store `new_value` into field `index` by the field instruction whose
    /// site is `site` in `function`. `false`, having stored nothing, for
    /// anything the slow path must decide or report.
    #[allow(clippy::inline_always, reason = "every interpreted field write")]
    #[inline(always)]
    fn site_store(
        &self,
        function: &Function,
        site: usize,
        value: Value,
        index: usize,
        new_value: Value,
        thread_local: bool,
    ) -> bool {
        let Some((instance, class)) = value
            .as_object_ptr()
            .and_then(|ptr| unsafe { InstanceRef::with_class_bits(ptr) })
        else {
            return false;
        };
        let entry = Self::field_site(function, site);
        if let Some(entry) = entry
            && let Some(code) = FieldSites::hit(entry.load(Ordering::Relaxed), class)
        {
            let bits = match code {
                3 => Some(new_value.bits()),
                0 => new_value.as_int().map(i64::cast_unsigned),
                2 => new_value.as_bool().map(u64::from),
                _ => new_value.as_float().map(f64::to_bits),
            };
            let Some(bits) = bits else {
                return false;
            };
            unsafe { instance.word(1 + index) }.store(bits, store_order(thread_local));
            return true;
        }
        self.site_store_miss(entry, value, index, new_value, thread_local)
    }

    #[cold]
    #[inline(never)]
    fn site_store_miss(
        &self,
        entry: Option<&AtomicU64>,
        value: Value,
        index: usize,
        new_value: Value,
        thread_local: bool,
    ) -> bool {
        self.remember_field_site(entry, value, index);
        self.shaped_store(value, index, new_value, thread_local)
    }

    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn field_site(function: &Function, site: usize) -> Option<&AtomicU64> {
        function.bytecode.compact.as_ref()?.field_sites.get(site)
    }

    /// Point `entry` at the class of the instance `value` names when that
    /// class never moves, so no later class can reuse its address.
    fn remember_field_site(&self, entry: Option<&AtomicU64>, value: Value, index: usize) {
        let Some(entry) = entry else {
            return;
        };
        let Some(ptr) = value.as_object_ptr() else {
            return;
        };
        let Some((instance, class)) = (unsafe { InstanceRef::with_class_bits(ptr) }) else {
            return;
        };
        let Some((_, shape)) = (unsafe { self.shapes.shape(ptr) }) else {
            return;
        };
        if index < shape.field_count() && self.heap.is_compile_time_ptr(instance.class()) {
            FieldSites::remember(entry, class, shape.kind_code(index));
        }
    }

    /// Field `index` of the instance `value` names, read through its class's
    /// cached shape. `None` for anything the slow path must decide: an
    /// operand that is not an instance, a class too wide to pack, a field
    /// out of range, and a raw float only boxing makes a value.
    #[allow(clippy::inline_always, reason = "every interpreted field read")]
    #[inline(always)]
    fn shaped_load(&self, value: Value, index: usize, thread_local: bool) -> Option<Value> {
        let (instance, shape) = unsafe { self.shapes.shape(value.as_object_ptr()?) }?;
        if index >= shape.field_count() {
            return None;
        }
        let bits = unsafe { instance.word(1 + index) }.load(load_order(thread_local));
        shape.value_of(index, bits)
    }

    /// Store `new_value` into field `index` through the class's cached
    /// shape. `false`, having stored nothing, for anything the slow path
    /// must decide or report.
    #[allow(clippy::inline_always, reason = "every interpreted field write")]
    #[inline(always)]
    fn shaped_store(
        &self,
        value: Value,
        index: usize,
        new_value: Value,
        thread_local: bool,
    ) -> bool {
        let Some((instance, shape)) = value
            .as_object_ptr()
            .and_then(|ptr| unsafe { self.shapes.shape(ptr) })
        else {
            return false;
        };
        if index >= shape.field_count() {
            return false;
        }
        let kind = shape.kind(index);
        let bits = if kind == SlotKind::Value {
            Some(new_value.bits())
        } else if kind == SlotKind::Int {
            new_value.as_int().map(i64::cast_unsigned)
        } else if kind == SlotKind::Bool {
            new_value.as_bool().map(u64::from)
        } else {
            new_value.as_float().map(f64::to_bits)
        };
        let Some(bits) = bits else {
            return false;
        };
        unsafe { instance.word(1 + index) }.store(bits, store_order(thread_local));
        true
    }

    /// Field `index` of the instance `value` names.
    fn checked_field(&mut self, value: Value, index: usize) -> Result<FieldRef, VmError> {
        let Some(instance) = value.as_object_ptr().and_then(|ptr| self.get_instance(ptr)) else {
            return Err(self.not_an_instance(value));
        };
        instance
            .field(index)
            .ok_or_else(|| self.invalid_field_access_error(index, instance.field_len()))
    }

    #[cold]
    #[inline(never)]
    fn not_an_instance(&self, value: Value) -> VmError {
        VmInternalError::TypeError {
            expected: Type::Object(ObjectType::Instance),
            got: self.type_of(&value),
        }
        .into()
    }

    /// A store of a value of the wrong kind into a raw slot: refused, never
    /// stored.
    pub(crate) fn slot_kind_error(&self, mismatch: SlotKindMismatch) -> VmInternalError {
        let expected = match mismatch.kind {
            SlotKind::Int => Type::Int,
            SlotKind::Float => Type::Float,
            SlotKind::Bool => Type::Bool,
            SlotKind::Value => unreachable!("a value slot takes any value"),
        };
        VmInternalError::TypeError {
            expected,
            got: self.type_of(&mismatch.value),
        }
    }

    #[cold]
    #[inline(never)]
    fn invalid_field_access_error(&mut self, field_index: usize, field_count: usize) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::InvalidFieldAccess {
            field_index,
            field_count,
        }))
    }

    /// Destructure a virtual-dispatch interface operand into the `(name, input args)`
    /// pair the impl resolver selects on. Associated types are *outputs* of an impl,
    /// so they are deliberately not part of the key.
    /// The physical slot interface field `field_index` of the interface
    /// operand `iface_value` lives at in `receiver`, through the per-site
    /// cache. As for `VirtualCall`, only an answer from an immutable
    /// program-image impl is remembered, keyed by the site, the receiver's
    /// type and the interface instantiation; a hit allocates nothing.
    fn virtual_field_slot(
        &mut self,
        caller: Option<usize>,
        receiver: Value,
        iface_value: Value,
        field_index: usize,
    ) -> Result<usize, VmError> {
        let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
        let cache_key = {
            let mut scratch = None;
            match self.virtual_site_query(caller, receiver, iface_ptr, &mut scratch) {
                Some(query) => match self.static_virtual_field_cache.get(&query) {
                    Some(slot) => return Ok(*slot),
                    None => Some(query.to_key()),
                },
                None => None,
            }
        };
        let (iface_head, iface_args) = self.interface_operand_parts(iface_ptr);
        let (slot, is_static) = self.resolve_virtual_field_slot_and_origin(
            receiver,
            iface_head,
            iface_args,
            field_index,
        )?;
        if is_static && let Some(cache_key) = cache_key {
            self.static_virtual_field_cache.insert(cache_key, slot);
        }
        Ok(slot)
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
        &self,
        receiver: Value,
        iface_head: bex_vm_types::TypeHead,
        iface_args: &[bex_vm_types::RealizedTy],
        field_index: usize,
    ) -> Result<usize, VmError> {
        self.resolve_virtual_field_slot_and_origin(receiver, iface_head, iface_args, field_index)
            .map(|(slot, _)| slot)
    }

    /// [`Self::resolve_virtual_field_slot`], and whether the rule it read is
    /// an immutable program-image rule.
    fn resolve_virtual_field_slot_and_origin(
        &self,
        receiver: Value,
        iface_head: bex_vm_types::TypeHead,
        iface_args: &[bex_vm_types::RealizedTy],
        field_index: usize,
    ) -> Result<(usize, bool), VmError> {
        let self_ty =
            bex_vm_types::RealizedTy::from(self.value_concrete_ty(receiver).unwrap_or_else(|| {
                unreachable!(
                    "value of kind {:?} cannot be a virtual field-access receiver",
                    self.type_of(&receiver)
                )
            }));
        let slot = crate::package_baml::ImplResolver::for_value(self, receiver)
            .resolve_implements_rule(&self_ty, iface_head, iface_args)
            .and_then(|(rule, _bound_args)| {
                let slot = rule.field_links.get(field_index).copied()?;
                Some((slot as usize, rule.is_static()))
            });
        slot.ok_or_else(|| {
            VmInternalError::UnresolvedVirtualFieldAccess {
                interface: baml_type::HeadDisplay::head_display_name(&iface_head),
                field_index,
            }
            .into()
        })
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
        Value::object(self.tlab.alloc_instance(class_ptr, fields))
    }

    /// Unwinds error values (both thrown and panics).
    /// Name of the innermost frame's function, for attributing work started
    /// from the current execution point (e.g. a spawn's provenance label).
    /// Cheap single-frame variant of `Self::capture_stack_trace`.
    pub fn current_function_name(&self) -> Option<String> {
        let frame = self.top_frame()?;
        let func = self.callable_function(self.frame_callable(frame))?;
        Some(func.name())
    }

    /// The function the callable `callable` runs, for a frame that is named or
    /// located in a trace. `as_callable` alone does not see through a
    /// generic-function value, which is what a function held as a typed
    /// function value is: the callee of `reflect.call_any`, and so every tool
    /// an agent calls. A frame that could not be resolved used to drop out of
    /// the stack trace without a word, taking the line that threw with it.
    fn callable_function(&self, callable: HeapPtr) -> Option<&Function> {
        match self.get_object(callable) {
            Object::GenericFunction(generic) => {
                let authored = self.generic_function_authored_ptr(generic).ok()?;
                self.get_object(authored).as_function().ok()
            }
            object => object.as_callable().ok(),
        }
    }

    /// Whether program global `slot` still waits for its initializer: true
    /// only while `$init` runs ([`Self::pending_globals`]).
    pub(crate) fn global_is_pending(&self, slot: bex_vm_types::GlobalIndex) -> bool {
        self.pending_globals
            .as_ref()
            .is_some_and(|pending| pending.get(slot.raw()) == Some(&true))
    }

    /// The VM's current call stack as a traceback, outermost frame first:
    /// where execution stands when the VM yields to its embedder.
    pub fn stack_trace(&self) -> Vec<StackFrame> {
        self.capture_stack_trace()
    }

    fn capture_stack_trace(&self) -> Vec<StackFrame> {
        // The innermost (topmost) bytecode frame's live PC lives in `cur_pc`;
        // outer frames recorded their call-site PC in `faulting_pc` at call
        // time, and compiled frames their last reported instruction.
        let top_bc = self.live_bytecode_frame();
        self.stack_view()
            .into_iter()
            .filter_map(|frame| {
                if let FrameAt::Vm(idx) = frame
                    && self
                        .pending_trace_hooks
                        .iter()
                        .any(|hook| hook.frame == idx && hook.evaluating)
                {
                    return None;
                }
                let func = self.callable_function(self.frame_callable(frame))?;
                if !self.frame_is_bytecode(frame) {
                    return Some(StackFrame {
                        function_name: func.name(),
                        file_path: func.source_file.clone(),
                        error_line: 0,
                    });
                }
                let pc = self.frame_at_pc(frame, top_bc).unwrap_or(0);
                let error_line = if let Some(compact) = &func.bytecode.compact {
                    compact.source_line_for_pc(pc)
                } else {
                    func.bytecode.source_line_for_pc(pc)
                };
                Some(StackFrame {
                    function_name: func.name(),
                    file_path: func.source_file.clone(),
                    error_line,
                })
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

    /// The landing context and whether it came from a handled error, per BEP-042.
    fn in_flight_context(
        &mut self,
        carried: Option<Value>,
        exception_value: Value,
    ) -> Result<(InFlightContext, bool), VmError> {
        Ok(match carried {
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
        })
    }

    /// Point `cur_pc` at the call site the innermost bytecode frame recorded
    /// when it called a native, before raising an error that native's
    /// continuation produced. Between a callback's return and the
    /// continuation, `cur_pc` still holds the PC the callback last ran, which
    /// belongs to another function: unwinding from it, a `catch` directly
    /// around the native call would be missed.
    fn restore_native_call_site_pc(&mut self) {
        let caller_pc = (0..self.frames.len())
            .rev()
            .find_map(|depth| match &self.frames[depth] {
                Frame::Bytecode(caller) => Some(caller.faulting_pc),
                Frame::Native(_) => None,
            });
        if let Some(caller_pc) = caller_pc {
            self.cur_pc = caller_pc;
        }
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
        self.with_telemetry_unwind(|vm| {
            vm.unwind_exception_frames(frame_idx, function, thrown, entry)
        })
    }

    fn with_telemetry_unwind<T>(&mut self, unwind: impl FnOnce(&mut Self) -> T) -> T {
        // Only the innermost span the error escapes records it by default.
        let enclosing = self
            .telemetry
            .as_mut()
            .and_then(crate::telemetry::TelemetryState::begin_unwind);
        let result = unwind(self);
        if let Some(state) = self.telemetry.as_mut() {
            state.end_unwind(enclosing);
        }
        result
    }

    fn unwind_exception_frames(
        &mut self,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        thrown: VmThrown,
        entry: RaiseEntry,
    ) -> Result<(), VmError> {
        let exception_value = thrown.value;
        let telemetry_outcome = self.telemetry_outcome_for_exception(exception_value);

        // BEP-042: the context this error lands with. A caught error raised
        // again — `throw` of its binding, a catch's no-match fall-through, a
        // `defer` pad re-raising the in-flight error — and
        // one awaited from another task carry the context of their original
        // throw, trace and cause intact. The context travels with the error,
        // so two errors that happen to be equal values never share one. It is
        // the error's only while it describes this value: a binding reassigned
        // before its `throw` makes that a new throw. A new throw inside a
        // handler body is "during handling of" that handler's error, which
        // becomes its cause, unless it throws a conversion of that error
        // (`UnknownError.from`), which takes over the error's own trace and
        // cause.
        let (mut context, mut evidence) =
            self.exception_evidence(&thrown, entry, Some((*frame_idx, *function)))?;

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
            // The record a compiled callee left when it returned the error is
            // a running compiled frame: it has no handlers. Parked records are
            // reached through their VM frame, which delivers to them.
            if let Some(FrameAt::Record(record)) = self.top_frame()
                && self.is_zombie(record)
            {
                let frame = FrameAt::Record(record);
                innermost_bc = false;
                // SAFETY: See `load_function`.
                let frame_function = match unsafe { self.frame_function(frame) } {
                    Ok(frame_function) => frame_function,
                    Err(error) => {
                        if let Some(evidence) = evidence.take() {
                            self.error_not_caught(evidence, btel_records::UnwindResult::Aborted);
                        }
                        return Err(error.into());
                    }
                };
                let recorded = if self.frame_wants_error_capture(frame, frame_function) {
                    self.recorded_context(&mut context, exception_value)
                } else {
                    exception_value
                };
                let level = self.level_of(frame);
                self.complete_unwound_frame(
                    frame,
                    frame_function,
                    telemetry_outcome,
                    recorded,
                    evidence.as_mut(),
                );
                if let Some(evidence) = &mut evidence {
                    self.error_frame_unwound(evidence, level);
                }
                self.pop_zombie(record);
                continue;
            }
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
                self.frames.pop();
                continue; // try next outer frame
            }

            // From here, frame is guaranteed Bytecode.
            let Frame::Bytecode(frame) = frame else {
                unreachable!("non-Native frames already handled above");
            };
            let (frame_faulting_pc, frame_locals_offset) = (frame.faulting_pc, frame.locals_offset);
            // Running compiled bodies have no bytecode handlers.
            let compiled = self.compiled_at(depth);
            // Suspended bodies receive errors through the resume stub.
            if compiled == Some(crate::compiled_rt::CompiledAt::Suspended) {
                *function = self.unwind_suspended_compiled(
                    depth,
                    frame_locals_offset,
                    exception_value,
                    &context,
                    &mut evidence,
                )?;
                *frame_idx = depth;
                return Ok(());
            }

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
            // A running compiled body has left its prologue with the error.
            if compiled.is_none()
                && self
                    .pending_trace_hooks
                    .last()
                    .is_some_and(|hook| hook.frame == depth && hook.evaluating)
            {
                let cancelled = self.as_instance(&exception_value).is_ok_and(|instance| {
                    instance.class() == self.panic_class_ptrs[PanicClass::Cancelled as usize]
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
                    self.push_stack(Value::NULL)?;
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
            let handler_entry = if compiled.is_some() {
                None
            } else if let Some(compact) = &frame_function.bytecode.compact {
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
            let recorded = if self.frame_wants_error_capture(FrameAt::Vm(depth), frame_function) {
                self.recorded_context(&mut context, exception_value)
            } else {
                exception_value
            };
            self.complete_unwound_frame(
                FrameAt::Vm(depth),
                frame_function,
                telemetry_outcome,
                recorded,
                evidence.as_mut(),
            );
            if let Some(evidence) = &mut evidence {
                let level = self.level_of_vm(depth);
                self.error_frame_unwound(evidence, level);
            }
            if self.frames.len() <= 1 {
                if let Some(evidence) = evidence.take() {
                    self.error_not_caught(evidence, btel_records::UnwindResult::Unhandled);
                }
                // No frame is left to carry the error: the thread's own
                // completion does, as its `baml.errors.Context`.
                if self.telemetry.is_some() {
                    let recorded = self.recorded_context(&mut context, exception_value);
                    if let Some(state) = self.telemetry.as_mut() {
                        // SAFETY: unwinding runs under the heap permit, and
                        // `recorded` is the in-flight context or the thrown
                        // value, both rooted by the unwinder.
                        unsafe { state.record_thread_error(recorded) };
                    }
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

    /// BEP-042 cause chain: the innermost active handler's context, or null.
    /// Inner bytecode uses `cur_pc`; outer bytecode uses its saved call site,
    /// and compiled frames use their recorded cause.
    fn find_cause_context(&self) -> Value {
        self.handler_contexts::<false>()
            .first()
            .copied()
            .unwrap_or(Value::NULL)
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
    ) -> Result<usize, VmError> {
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
        self.extend_stack(
            mapping
                .into_iter()
                .map(|slot| slot.map_or(Value::OMITTED_ARG, |index| values[index])),
        )?;
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

    /// The frame a bound method curries from its receiver: the type arguments
    /// of the receiver's concrete type, which are the class generics of the
    /// method's owner (an instance's `class_type_args`, a list's element
    /// type for `baml.Array<T>`, a map's key and value for `baml.Map<K, V>`,
    /// a future's value and error for `baml.future.Future<T, E>`); a receiver
    /// whose concrete type takes no arguments curries none. Captured at
    /// `MakeBoundMethod` so the value is fully realized; installed as the
    /// callee's `frame.type_args` at `CallIndirect`, with a later explicit
    /// specialization (`xs.map<string, never>`) appended after them.
    ///
    /// Exact, not an approximation: `MakeBoundMethod`'s only callees are
    /// class-inherent methods, whose owner frame IS the class frame. An
    /// implements-block method referenced in value position binds through
    /// `MakeVirtualBoundMethod` (or a shim's `shim_rule_method`) instead,
    /// where the impl rule's `realize_frame` supplies the owner frame a
    /// receiver's type cannot express (blanket impls, adopted defaults).
    pub(crate) fn receiver_type_args(
        &self,
        receiver: Value,
    ) -> Result<Box<[bex_vm_types::RealizedTy]>, VmInternalError> {
        use bex_vm_types::ConcreteRealizedTy as Concrete;
        let Some(concrete) = self.value_concrete_ty(receiver) else {
            return Err(VmInternalError::TypeSubstitution {
                message: format!(
                    "a bound method's receiver of kind {:?} has no realized concrete type",
                    self.type_of(&receiver)
                ),
            });
        };
        Ok(match concrete {
            Concrete::Class(_, args) => args,
            Concrete::List(element) => Box::new([*element]),
            Concrete::Map { key, value } => Box::new([*key, *value]),
            Concrete::Future(value, error) => Box::new([*value, *error]),
            Concrete::Int
            | Concrete::Bigint
            | Concrete::Float
            | Concrete::String
            | Concrete::Bool
            | Concrete::Null
            | Concrete::Uint8Array
            | Concrete::Media(_)
            | Concrete::Enum(_)
            | Concrete::Tuple(_)
            | Concrete::Function { .. }
            | Concrete::Type
            | Concrete::Resource
            | Concrete::PromptAst
            | Concrete::Never => Box::new([]),
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
    /// `baml.host._call_host_value` in `sys_ops/.../io_generated.rs`):
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
        let ret_ty_ptr = self.alloc_type(bex_vm_types::types::TypeValue::new(ret_ty.clone()));
        let throws_ty_ptr = self.alloc_type(bex_vm_types::types::TypeValue::new(throws_ty.clone()));
        VmExecState::SysOp {
            operation: bex_vm_types::SysOp::BamlHostCallHostValue,
            args: vec![
                Value::object(closure_ptr),
                Value::object(args_array_ptr),
                Value::object(ret_ty_ptr),
                Value::object(throws_ty_ptr),
            ],
            return_type: ret_ty,
            throws_type: throws_ty,
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
        let (sys_op, arity, return_type, throws_type) = {
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
            // A sys-op threads no type arguments (see the dispatch sites), so
            // its declared types realize on their own.
            let realize = |template: &bex_vm_types::TyTemplate, what: &str| {
                template
                    .substitute(&[], self)
                    .map_err(|e| VmInternalError::TypeSubstitution {
                        message: format!(
                            "sys-op `{sys_op}` has a {what} type that does not realize: {e}"
                        ),
                    })
            };
            let return_type = realize(&f.return_type, "return")?;
            let throws_type = realize(&f.throws_type, "throws")?;
            (sys_op, f.arity, return_type, throws_type)
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
            return_type,
            throws_type,
        })
    }

    /// Enter a bytecode function or closure called through `CallIndirect` the
    /// way `CallExactArgs` enters its callee, when the site pushed exactly the
    /// callee's slots. Returns the entered function; the caller then switches
    /// dispatch to it. `None` leaves everything untouched for the general
    /// path, which handles compiled, hooked and pass-through callees, a
    /// pending trace or type lane, layouts that need remapping, and every
    /// other callable.
    ///
    /// Kept out of line: inlined into `run_compact`, it made every ordinary
    /// exact call about ten instructions slower.
    #[inline(never)]
    fn enter_indirect_bytecode_call(
        &mut self,
        callee_ptr: HeapPtr,
        frame_idx: usize,
        caller: &'static Function,
    ) -> Result<Option<&'static Function>, VmError> {
        if self.pending_call_trace.is_some() || !self.pending_call_type_args.is_empty() {
            return Ok(None);
        }
        // SAFETY: the active heap permit prevents collection during
        // frame/stack growth, as at `CallExactArgs`; a closure's function
        // lives at least as long as the closure.
        let (callee, callee_fn_ptr, captured_type_args) = match unsafe { callee_ptr.get() } {
            Object::Function(callee) => (callee, callee_ptr, &[][..]),
            Object::Closure(closure) => match unsafe { closure.function.get() } {
                Object::Function(callee) => {
                    (callee, closure.function, &closure.captured_type_args[..])
                }
                _ => return Ok(None),
            },
            _ => return Ok(None),
        };
        if !matches!(callee.kind, FunctionKind::Bytecode)
            || callee.compiled_body.is_some()
            || self.hotswap_redirects(callee)
            || callee.trace_passthrough
            || callee
                .bytecode
                .compact
                .as_ref()
                .is_none_or(|code| code.trace_hook_finish_pc.is_some())
            || Self::recorded_call_layout(caller, self.cur_pc)
                .is_some_and(|layout| !callee.argument_layout_is(layout))
        {
            return Ok(None);
        }

        let _popped_callee = self.stack.ensure_pop();
        // Eager ok_or constructs/drops VmInternalError even on success.
        #[expect(clippy::unnecessary_lazy_evaluations)]
        let locals_offset = StackIndex::from_raw(
            self.stack
                .len()
                .checked_sub(callee.arity)
                .ok_or_else(|| VmInternalError::NotEnoughItemsOnStack(callee.arity))?,
        );
        let Frame::Bytecode(caller_frame) = &mut self.frames[frame_idx] else {
            verifier_unreachable!()
        };
        caller_frame.faulting_pc = self.cur_pc;
        self.reserve_call_frame()?;
        let room = self.reserve_frame_room(callee)?;
        let context = self.dispatch_frame_context(frame_idx).clone();
        if let Some(telemetry) = &mut self.telemetry {
            telemetry.set_context(context.clone());
        }
        let frame_telemetry = if self.telemetry.is_some() {
            self.enter_indirect_bytecode_telemetry(
                frame_idx,
                callee,
                callee_fn_ptr,
                locals_offset,
                captured_type_args,
            )
        } else {
            None
        };
        // Written in place, as in the general path.
        self.frames.push_reserved_with(|| {
            Frame::Bytecode(BytecodeFrame {
                function: callee_ptr,
                instruction_ptr: 0,
                locals_offset,
                // A closure made in a generic context reads the type
                // arguments it captured.
                type_args: captured_type_args.to_vec().into(),
                faulting_pc: 0,
                telemetry: frame_telemetry,
                context,
                room,
            })
        });
        if callee.real_local_count != 0 {
            let new_len = self.stack.len() + callee.real_local_count;
            self.resize_stack(new_len, Value::NULL)?;
        }
        Ok(Some(callee))
    }

    /// `VirtualCall [nargs, ntypeargs, self_arg]` when its site cache already
    /// holds the callee, entered like a plain bytecode callee of
    /// [`Self::enter_indirect_bytecode_call`]: the frame is pushed here and
    /// dispatch continues in it, without the general call path or a
    /// type-argument lane. A generic impl's frame shares the cache entry's
    /// type arguments. Returns `None` with nothing changed when the call
    /// needs the general path: a miss, method type arguments, telemetry, a
    /// pending trace or lane, arguments the site remaps, type arguments that
    /// name runtime declarations, or a callee that is not a plain bytecode
    /// function.
    #[inline(never)]
    fn enter_cached_virtual_call(
        &mut self,
        [nargs, ntypeargs, self_arg]: [usize; 3],
        frame_idx: usize,
        caller: &'static Function,
        runtime_package: HeapPtr,
        return_pc: usize,
    ) -> Result<Option<&'static Function>, VmError> {
        if ntypeargs != 0
            || self.telemetry.is_some()
            || self.pending_call_trace.is_some()
            || !self.pending_call_type_args.is_empty()
        {
            return Ok(None);
        }
        // `[args..., iface_type, method_name]`, top last.
        let len = self.stack.len();
        let Some(args_offset) = len.checked_sub(nargs + 2) else {
            return Ok(None);
        };
        let receiver = self.stack[StackIndex::from_raw(args_offset + self_arg)];
        let Some(iface_ptr) = self.stack[StackIndex::from_raw(len - 2)].as_object_ptr() else {
            return Ok(None);
        };
        if !matches!(self.get_object(iface_ptr), Object::Type(_)) {
            return Ok(None);
        }
        let Some(identity) = self.static_cache_identity(caller, runtime_package, frame_idx) else {
            return Ok(None);
        };
        let pc = self.cur_pc;
        let receiver_addr = receiver
            .as_object_ptr()
            .map_or(0, |ptr| ptr.as_ptr() as usize);
        let iface_addr = iface_ptr.as_ptr() as usize;
        let slot = ((identity >> 4) ^ pc) & (VIRTUAL_CALL_MEMO_SLOTS - 1);
        let memo = &mut self.virtual_call_memo[slot];
        let same_site = memo.caller == identity && memo.pc == pc;
        let (callee_ptr, shared) = if same_site
            && memo.receiver == receiver_addr
            && memo.iface == iface_addr
        {
            memo.answered = true;
            let entry = memo.entry;
            // Checked when the memo was made, against the same entry.
            let Some((_, target)) = self.static_virtual_call_cache.get_index(entry) else {
                verifier_unreachable!()
            };
            (target.callee.ptr(), target.shared_frame_type_args.clone())
        } else {
            // A site that never used its memo keeps it, and counts the call.
            let remember = if same_site && !memo.answered {
                memo.passed_over += 1;
                memo.passed_over >= VIRTUAL_CALL_MEMO_RETRY
            } else {
                true
            };
            let mut scratch = None;
            let Some(query) =
                self.virtual_site_query(Some(identity), receiver, iface_ptr, &mut scratch)
            else {
                return Ok(None);
            };
            // Only an instance of a compile-time class keeps the type the
            // cache keyed for as long as it lives; a container's type is
            // read off it each time.
            let remember = remember && matches!(query.receiver, VirtualReceiverQuery::Class { .. });
            let Some((entry, _, target)) = self.static_virtual_call_cache.get_full(&query) else {
                return Ok(None);
            };
            if target.arg_count != Some(nargs)
                || (!target.frame_type_args.is_empty() && target.shared_frame_type_args.is_none())
                || !Self::enters_as_plain_bytecode(target.callee.ptr(), nargs)
            {
                return Ok(None);
            }
            let found = (target.callee.ptr(), target.shared_frame_type_args.clone());
            if remember {
                self.virtual_call_memo[slot] = VirtualCallMemo {
                    caller: identity,
                    pc,
                    receiver: receiver_addr,
                    iface: iface_addr,
                    entry,
                    answered: false,
                    passed_over: 0,
                };
            }
            found
        };
        let type_args = shared.map_or_else(FrameTypeArgs::default, FrameTypeArgs::shared);
        // SAFETY: the callee is live: collection drops cache entries whose
        // callee died and clears the memo. The active heap permit prevents
        // collection during frame/stack growth, as at `CallExactArgs`.
        let Object::Function(callee) = (unsafe { callee_ptr.get() }) else {
            verifier_unreachable!()
        };
        if self.hotswap_redirects(callee) {
            return Ok(None);
        }

        self.stack.truncate(len - 2);
        let Frame::Bytecode(caller_frame) = &mut self.frames[frame_idx] else {
            verifier_unreachable!()
        };
        caller_frame.instruction_ptr = return_pc;
        caller_frame.faulting_pc = self.cur_pc;
        self.reserve_call_frame()?;
        let room = self.reserve_frame_room(callee)?;
        let context = self.dispatch_frame_context(frame_idx).clone();
        self.frames.push_reserved_with(|| {
            Frame::Bytecode(BytecodeFrame {
                function: callee_ptr,
                instruction_ptr: 0,
                locals_offset: StackIndex::from_raw(args_offset),
                type_args,
                faulting_pc: 0,
                telemetry: None,
                context,
                room,
            })
        });
        if callee.real_local_count != 0 {
            let new_len = self.stack.len() + callee.real_local_count;
            self.resize_stack(new_len, Value::NULL)?;
        }
        Ok(Some(callee))
    }

    /// Whether `callee_ptr`, called with `nargs` arguments, is a plain
    /// bytecode function [`Self::enter_cached_virtual_call`] can enter
    /// itself: not native, compiled, hooked or pass-through.
    fn enters_as_plain_bytecode(callee_ptr: HeapPtr, nargs: usize) -> bool {
        // SAFETY: a cache hit's callee is live (collection drops entries
        // whose callee died).
        let Object::Function(callee) = (unsafe { callee_ptr.get() }) else {
            return false;
        };
        matches!(callee.kind, FunctionKind::Bytecode)
            && callee.compiled_body.is_none()
            && !callee.trace_passthrough
            && callee.arity == nargs
            && callee
                .bytecode
                .compact
                .as_ref()
                .is_some_and(|code| code.trace_hook_finish_pc.is_none())
    }

    /// `args` as a slice the callee frames of virtual-call cache hits can
    /// share ([`FrameTypeArgs::shared`]): when there are any, and every
    /// declaration they name is compile-time, so a frame holding them has
    /// nothing for a collection to visit.
    fn shareable_frame_type_args(
        &self,
        args: &[bex_vm_types::RealizedTy],
    ) -> Option<Arc<[bex_vm_types::RealizedTy]>> {
        if args.is_empty() {
            return None;
        }
        let mut runtime = false;
        for ty in args {
            ty.visit_heads(&mut |head| {
                runtime |= head.is_resolved() && !self.heap.is_compile_time_ptr(head.ptr());
            });
        }
        (!runtime).then(|| Arc::from(args))
    }

    /// Telemetry entry for a bytecode callee `CallIndirect` enters directly:
    /// what `execute_call_from_locals_offset` records for an untraced call
    /// that passes no type arguments.
    #[inline(never)]
    fn enter_indirect_bytecode_telemetry(
        &mut self,
        frame_idx: usize,
        callee: &Function,
        callee_fn_ptr: HeapPtr,
        locals_offset: StackIndex,
        carried: &[bex_vm_types::RealizedTy],
    ) -> Option<FrameTelemetry> {
        // As the general path records it, with this instruction as the call site.
        let caller = self.bytecode_caller(frame_idx);
        let actual_caller = self
            .visible_bytecode_caller(frame_idx)
            .and_then(|at| self.frame_identity(at));
        let caller_is_observed = self.frame_receipt(caller).is_some();
        let caller_pc = self.caller_pc(caller, frame_idx, self.cur_pc);
        let args = &self.stack.as_slice()
            [locals_offset.raw()..locals_offset.raw().saturating_add(callee.arity)];
        // SAFETY: arguments and callable remain live under the heap permit.
        self.telemetry.as_mut().and_then(|telemetry| unsafe {
            telemetry.enter_bytecode_with_trace(
                callee,
                callee_fn_ptr,
                actual_caller,
                caller_pc,
                caller_is_observed,
                args,
                None,
                CallTypeArgs {
                    carried,
                    passed: &[],
                },
                |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
            )
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
        let thread_local = std::mem::take(&mut self.thread_local_call);
        let passes_type_args = std::mem::take(&mut self.pending_call_passes_type_args);
        // Record the caller's call-site PC before descending. Once a callee
        // frame is pushed, this (now-outer) frame is no longer the innermost, so
        // its live PC must be persisted into `faulting_pc` for correct unwinding
        // and stack traces. `cur_pc` holds this call instruction's start.
        let call_site = self.cur_pc;

        // `compiled_caller_call` sets `cur_pc` to the call's instruction index.
        if let Some(Frame::Bytecode(bf)) = self.frames.get_mut(*frame_idx) {
            bf.faulting_pc = call_site;
        }

        // SAFETY: setup cannot collect. The body may collect after its frame
        // is rooted, so these borrows must not be used after the call.
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

        // The frame a function value carries, in the callee's De Bruijn
        // order: a bound method's receiver-realized class arguments followed
        // by its specialization (`xs.map` ⇒ `[T, U, E]`), a generic
        // function's instantiation (`foo<int>`), or a closure's snapshot of
        // its enclosing frame. A plain function carries none. A bytecode
        // callee installs it as `frame.type_args`; a native reads it through
        // `current_call_type_args()`, so both see the frame a direct call
        // would have seeded.
        let carried_type_args: &[bex_vm_types::RealizedTy] = match callee_object {
            Object::Closure(c) => &c.captured_type_args,
            Object::BoundMethod(bm) => &bm.type_args,
            Object::GenericFunction(gf) => &gf.type_args,
            _ => &[],
        };

        // The replacement runs with the type arguments and receiver the call
        // carries for the target, so select only after reading them.
        let callee_ptr = if self.hotswap_enabled {
            self.hotswap_entry(callee_ptr)
        } else {
            callee_ptr
        };
        // SAFETY: the swap table roots a replacement, and setup cannot collect.
        let callee_object: &'static Object = unsafe { callee_ptr.get() };

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
        if trace
            .as_ref()
            .and_then(|trace| trace.reservation.as_ref())
            .is_some_and(bex_vm_types::trace::ReservedSpanData::is_forwarded_claim)
            && !callee.trace_passthrough
        {
            return Err(self.trace_attachment_error(
                "reservation claim can only continue into a pass-through function",
            ));
        }
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
        match callee_kind {
            FunctionKind::Native(func_ptr, local) => {
                let func_ptr = match local {
                    Some(local) if thread_local => local,
                    _ => func_ptr,
                };
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

                // A native reads its type args through
                // `current_call_type_args()`. The direct-call path sets them
                // from LoadType operands; an indirect call through a value
                // carries them on the value (`let f = baml.json.decode<User>`
                // on a `GenericFunction`, `let g = baml.json.decode; g<User>`
                // on a closure, `xs.map` on a bound method whose receiver
                // realized `T` and whose specialization appended `U, E`).
                let restore_pending = !carried_type_args.is_empty();
                if restore_pending {
                    self.save_call_types();
                    self.pending_call_type_args = carried_type_args.to_vec();
                }
                let native_result = func(self, &args);
                if restore_pending {
                    let _ = self.restore_call_types();
                }
                self.settle();

                match native_result {
                    NativeCallResult::Done(v) => {
                        self.push_stack(v)?;
                    }
                    other => {
                        return self.finish_native_call(other, callee_ptr, frame_idx, function);
                    }
                }
            }

            FunctionKind::Bytecode => {
                self.reserve_call_frame()?;
                let room = self.reserve_frame_room(callee)?;
                // Push the new frame.
                let hooked = callee
                    .bytecode
                    .compact
                    .as_ref()
                    .is_some_and(|code| code.trace_hook_finish_pc.is_some());
                let caller_pc = if hooked {
                    self.caller_pc(self.bytecode_caller(*frame_idx), *frame_idx, call_site)
                } else {
                    0
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
                let frame_telemetry = if !hooked && self.telemetry.is_some() {
                    let caller = self.bytecode_caller(*frame_idx);
                    let actual_caller = self
                        .visible_bytecode_caller(*frame_idx)
                        .and_then(|at| self.frame_identity(at));
                    let caller_is_observed = self.frame_receipt(caller).is_some();
                    let caller_pc = self.caller_pc(caller, *frame_idx, call_site);
                    let args = &self.stack.as_slice()
                        [locals_offset.raw()..locals_offset.raw().saturating_add(arg_count)];
                    let type_args = CallTypeArgs {
                        carried: carried_type_args,
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
                if !hooked && callee.trace_passthrough {
                    self.hold_trace(self.frames.len(), trace.clone().unwrap_or_default());
                }
                if hooked {
                    self.pending_trace_hooks.push(PendingTraceHook {
                        frame: self.frames.len(),
                        caller: self
                            .visible_bytecode_caller(*frame_idx)
                            .map(|at| self.level_of(at)),
                        caller_pc,
                        trace,
                        evaluating: false,
                    });
                }
                // Construct after reserving capacity so the frame can be written
                // directly into the vector instead of moved through a temporary.
                self.frames.push_reserved_with(|| {
                    Frame::Bytecode(BytecodeFrame {
                        function: callee_ptr,
                        instruction_ptr: 0,
                        locals_offset,
                        type_args: carried_type_args.to_vec().into(),
                        faulting_pc: 0,
                        telemetry: frame_telemetry,
                        context,
                        room,
                    })
                });
                // Update frame_idx to point to the new frame.
                *frame_idx = self.frames.len() - 1;

                *function = callee;

                // A compiled body's prologue resolves a pending hook as the
                // bytecode prologue does.
                let compiled = crate::compiled_rt::compiled_fn(callee);
                match compiled {
                    None => {
                        let new_len = self.stack.len() + callee.real_local_count;
                        self.resize_stack(new_len, Value::NULL)?;
                    }
                    // A compiled caller is on the fiber already, so its callee
                    // runs in place; from bytecode the loop enters the fiber
                    // through the stub.
                    Some(body) if self.compiled.on_fiber => {
                        // Keep arguments rooted on the stack until `finish_bytecode_frame`.
                        let args: SmallVec<[Value; 4]> = (0..arg_count)
                            .map(|i| {
                                self.stack
                                    .get_at(StackIndex::from_raw(locals_offset.raw() + i))
                            })
                            .collect();
                        if let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] {
                            frame.instruction_ptr = crate::compiled_rt::RUNNING;
                        }
                        // The body reads type arguments from its frame.
                        if passes_type_args {
                            self.bind_passed_type_args(*frame_idx);
                        }
                        let outcome = self.stack_room().and_then(|()| self.run_entry(body, &args));
                        self.settle();
                        match outcome {
                            Ok(value) => {
                                self.push_stack(value)?;
                                if let Some(state) =
                                    self.finish_bytecode_frame(frame_idx, function, value)?
                                {
                                    return Ok(Some(state));
                                }
                            }
                            Err(error) => return Err(*error),
                        }
                    }
                    Some(_) => {
                        let start = crate::compiled_rt::stub_start(callee)
                            .expect("a compiled function has a stub");
                        if let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] {
                            frame.instruction_ptr = start + crate::compiled_rt::stub::START;
                        }
                    }
                }
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
                    carried_type_args.is_empty(),
                    "sysop dispatch received type args, which it cannot thread to the op",
                );
                let state = self.dispatch_sysop_yield(callee_fn_ptr)?;
                return Ok(Some(state));
            }

            FunctionKind::NativeUnresolved => {
                // This should never happen - native functions should be resolved
                // by attach_builtins() before the VM runs.
                let callee_name = callee.name();
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

    /// Complete a native call whose result is not a plain value: a thrown
    /// error, or a callback the native yields to. A yield pushes the native's
    /// continuation frame and dispatches the callback.
    fn finish_native_call(
        &mut self,
        native_result: NativeCallResult,
        callee_ptr: HeapPtr,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        match native_result {
            NativeCallResult::Done(v) => {
                self.push_stack(v)?;
                Ok(None)
            }
            NativeCallResult::Error(e) => Err(self.native_error_to_vm_error(e)),
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
                self.reserve_call_frame()?;
                self.frames.push_reserved(Frame::Native(NativeFrame {
                    function: callee_ptr,
                    continuation,
                    bytecode_caller: self.level_of(self.bytecode_caller(*frame_idx)),
                }));

                // If callee is a BoundMethod, insert receiver into args.
                let real_callee = self.resolve_bound_method_callee(callee, &mut callback_args)?;

                let arg_count = callback_args.len();
                let cb_locals = StackIndex::from_raw(self.stack.len());
                self.extend_stack(callback_args)?;

                let result = self.execute_call_from_locals_offset_with_type_args(
                    real_callee,
                    cb_locals,
                    arg_count,
                    CallOptions {
                        type_args: &callback_type_args,
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

                result
            }
        }
    }

    /// Run the native builtin a `Call` site names without the general call
    /// setup, when nothing about the call needs it: no `$trace`, no type lane
    /// of an enclosing call, and a site that pushed exactly the native's
    /// slots. The site's type operands, if any, become the type arguments the
    /// native reads through [`Self::current_call_type_args`], and leave the
    /// stack with its values. Returns [`NativeDirect::NotTaken`] without
    /// touching anything when the call needs the general path.
    ///
    /// Every check is in here, out of line: `run_compact` is sensitive to
    /// what its call arm holds, and code added there slowed unrelated
    /// instructions.
    #[inline(never)]
    fn call_native_direct(
        &mut self,
        callee_value: Value,
        thread_local: bool,
        ntypeargs: usize,
        frame_idx: &mut usize,
        function: &mut &'static Function,
        return_pc: usize,
    ) -> Result<NativeDirect, VmError> {
        let Some(callee_ptr) = callee_value.as_object_ptr() else {
            return Ok(NativeDirect::NotTaken);
        };
        // SAFETY: a function global lives for the whole program.
        let Object::Function(callee) = (unsafe { callee_ptr.get() }) else {
            return Ok(NativeDirect::NotTaken);
        };
        let FunctionKind::Native(native, local) = callee.kind else {
            return Ok(NativeDirect::NotTaken);
        };
        if self.pending_call_trace.is_some() || !self.pending_call_type_args.is_empty() {
            return Ok(NativeDirect::NotTaken);
        }
        if Self::recorded_call_layout(function, self.cur_pc)
            .is_some_and(|layout| !callee.argument_layout_is(layout))
        {
            return Ok(NativeDirect::NotTaken);
        }
        let operand_count = callee.arity + ntypeargs;
        let type_start = self
            .stack
            .len()
            .checked_sub(operand_count)
            .ok_or(VmInternalError::NotEnoughItemsOnStack(operand_count))?;
        self.thread_local_call = false;

        // The caller's return address and call site, as the general path
        // records them, for a yield or an unwind out of the native.
        let Frame::Bytecode(caller) = &mut self.frames[*frame_idx] else {
            verifier_unreachable!()
        };
        caller.instruction_ptr = return_pc;
        caller.faulting_pc = self.cur_pc;

        if ntypeargs != 0 {
            // The lane is empty here; reusing its allocation keeps a generic
            // builtin call (`xs.push(x)` passes `T`) from allocating.
            let mut type_args = std::mem::take(&mut self.pending_call_type_args);
            for slot in type_start..type_start + ntypeargs {
                let value = self.stack[StackIndex::from_raw(slot)];
                let ty = match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
                    Some(Object::Type(type_value)) => type_value.ty.clone(),
                    _ => self.type_operand_value(value)?.ty,
                };
                type_args.push(ty);
            }
            self.pending_call_type_args = type_args;
        }

        // As in the general path, a native keeps its own roots: the
        // arguments leave the stack before it runs.
        let args: SmallVec<[Value; 4]> = self.stack.as_slice()[type_start + ntypeargs..]
            .iter()
            .copied()
            .collect();
        self.stack.truncate(type_start);

        let func_ptr = match local {
            Some(local) if thread_local => local,
            _ => native,
        };
        // SAFETY: as in `execute_call_from_locals_offset`, the pointer was
        // cast from a `NativeFunction` in `attach_builtins`.
        let func = unsafe { std::mem::transmute::<*const (), NativeFunction>(func_ptr) };
        let native_result = func(self, &args);
        // A native reads its type arguments while it runs, never from a
        // continuation, exactly as the general path restores the lane here.
        self.pending_call_type_args.clear();
        self.settle();

        match native_result {
            NativeCallResult::Done(value) => {
                self.push_stack(value)?;
                Ok(NativeDirect::Ran(
                    self.should_early_yield().then_some(VmExecState::EarlyYield),
                ))
            }
            other => self
                .finish_native_call(other, callee_ptr, frame_idx, function)
                .map(NativeDirect::Ran),
        }
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
        // The caller's slice may borrow a frame, so copy before entering the callee.
        self.execute_call_from_locals_offset_with_owned_type_args(
            callee_ptr,
            locals_offset,
            arg_count,
            options.type_args.to_vec(),
            frame_idx,
            function,
        )
    }

    /// [`Self::execute_call_from_locals_offset_with_type_args`] for a caller
    /// that owns the type arguments: they become the pending lane, and the
    /// callee's frame, without a copy.
    fn execute_call_from_locals_offset_with_owned_type_args(
        &mut self,
        callee_ptr: HeapPtr,
        locals_offset: StackIndex,
        arg_count: usize,
        type_args: Vec<bex_vm_types::RealizedTy>,
        frame_idx: &mut usize,
        function: &mut &'static Function,
    ) -> Result<Option<VmExecState>, VmError> {
        let passes_type_args = !type_args.is_empty();
        self.save_call_types();
        self.pending_call_type_args = type_args;
        let frames_before = self.frames.len();
        self.pending_call_passes_type_args = passes_type_args;
        let result = self.execute_call_from_locals_offset(
            callee_ptr,
            locals_offset,
            arg_count,
            frame_idx,
            function,
        );
        // The pending lane is forwarded if an inline body collects. Caller
        // slices and the saved outer lane must not be read from Rust locals.
        //
        // A frame at `RUNNING` already has type arguments bound and is
        // waiting for the unwinder after an inline body failed.
        if !self.pending_call_type_args.is_empty()
            && self.frames.len() == frames_before + 1
            && *frame_idx == frames_before
            && let Some(Frame::Bytecode(frame)) = self.frames.get_mut(frames_before)
            && frame.instruction_ptr != crate::compiled_rt::RUNNING
        {
            if frame.type_args.is_empty() {
                // A callee that carries no frame of its own (a plain
                // function, not a closure or bound method) takes the lane
                // as its frame. The lane is restored just below, so nothing
                // reads it again.
                frame.type_args = std::mem::take(&mut self.pending_call_type_args).into();
            } else {
                frame
                    .type_args
                    .to_mut()
                    .extend_from_slice(&self.pending_call_type_args);
            }
        }
        let _ = self.restore_call_types();
        result
    }

    /// The type arguments a virtually dispatched callee's frame starts with:
    /// the resolved impl's frame, then the call's own method arguments.
    #[inline]
    fn seed_frame_type_args(
        frame: &[bex_vm_types::RealizedTy],
        method: Option<&TakenTypeArgs>,
    ) -> Vec<bex_vm_types::RealizedTy> {
        let method = method.map_or(&[][..], |method| &method.tys[..]);
        let mut type_args = Vec::with_capacity(frame.len() + method.len());
        type_args.extend_from_slice(frame);
        type_args.extend_from_slice(method);
        type_args
    }

    fn init_spread(
        &mut self,
        dest_value: Value,
        source_value: Value,
        field_copy_set: &bytecode::FieldCopySet,
    ) -> Result<(), VmError> {
        let mut copied_fields = Vec::with_capacity(field_copy_set.fields.len());
        for copy in &field_copy_set.fields {
            self.checked_field(dest_value, copy.dest)?;
            let value = self.load_instance_field(source_value, copy.source, false)?;
            copied_fields.push((copy.dest, value));
        }
        for (dest_field, value) in copied_fields {
            self.store_instance_field(dest_value, dest_field, value, false)?;
        }
        Ok(())
    }

    fn alloc_initialized_instance(
        &mut self,
        plan: &bytecode::ClassInitPlan,
    ) -> Result<Value, VmError> {
        let class_ptr = self.idx_to_ptr(plan.class_obj);
        if !matches!(self.get_object(class_ptr), Object::Class(_)) {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Class.into(),
                got: ObjectType::of(self.get_object(class_ptr)).into(),
            }
            .into());
        }
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
        let values: Vec<Value> = self.stack.drain(StackIndex::from_raw(base)..).collect();

        let instance = self.tlab.alloc_instance_zeroed(class_ptr, class_type_args);
        for (&field, value) in plan.fields.iter().zip(values) {
            instance
                .store_relaxed(field, value)
                .map_err(|mismatch| self.slot_kind_error(mismatch))?;
        }
        Ok(Value::object(self.tlab.instance_ptr(instance)))
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
        // SAFETY: forwarded to the caller.
        unsafe { self.load_callable(self.frames[frame_idx].function()) }
    }

    /// [`Self::load_function`] for the callable `ptr` a frame runs.
    ///
    /// # Safety
    ///
    /// As for [`Self::load_function`].
    #[inline]
    pub(crate) unsafe fn load_callable(
        &self,
        ptr: HeapPtr,
    ) -> Result<&'static Function, VmInternalError> {
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
    pub(crate) fn callable_function_identity(&self, ptr: HeapPtr) -> Option<HeapPtr> {
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
        self.end_manual_activation(frame_idx);
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
        frame: FrameAt,
        function: &Function,
        outcome: InvocationOutcome,
        value: Value,
        evidence: Option<&mut error_evidence::UnwindEvidence>,
    ) {
        match frame {
            FrameAt::Vm(frame_idx) => self.end_manual_activation(frame_idx),
            FrameAt::Record(record) => {
                if compiled_rt::record_ref(record).scoped.get() {
                    self.end_record_activation(record);
                }
            }
        }
        let telemetry = self.with_frame_receipt(frame, Option::take);
        let Some(telemetry) = telemetry else {
            self.restore_caller_context_at(frame);
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
        let context = self.frame_context(frame).cloned();
        let state = self
            .telemetry
            .as_mut()
            .expect("observed invocation has telemetry");
        if let Some(context) = context {
            state.set_context(context);
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
        self.restore_caller_context_at(frame);
    }

    /// [`Self::complete_bytecode_invocation`] for a frame record.
    fn complete_record_invocation(
        &mut self,
        frame: FrameAt,
        outcome: InvocationOutcome,
        value: Option<Value>,
    ) {
        let Some(telemetry) = self.with_frame_receipt(frame, Option::take) else {
            self.restore_caller_context_at(frame);
            return;
        };
        // SAFETY: the record roots its function through completion.
        let function = unsafe { self.frame_function(frame) }
            .expect("a frame record must retain its function through completion");
        let context = self.frame_context(frame).cloned();
        let state = self
            .telemetry
            .as_mut()
            .expect("observed invocation has telemetry");
        if let Some(context) = context {
            state.set_context(context);
        }
        // SAFETY: completion holds the heap permit and the result/error is live.
        unsafe {
            state.complete_invocation(telemetry, function, outcome, value);
        }
        self.restore_caller_context_at(frame);
    }

    /// The context `frame` runs in. A frame record inherits its VM frame's.
    fn frame_context(&self, frame: FrameAt) -> Option<&Context> {
        match frame {
            FrameAt::Vm(idx) => match &self.frames[idx] {
                Frame::Bytecode(frame) => Some(&frame.context),
                Frame::Native(_) => None,
            },
            FrameAt::Record(record) => Some(
                self.frames[..compiled_rt::record_ref(record).anchor()]
                    .iter()
                    .rev()
                    .find_map(|frame| match frame {
                        Frame::Bytecode(frame) => Some(&frame.context),
                        Frame::Native(_) => None,
                    })
                    .unwrap_or(&self.root_context),
            ),
        }
    }

    /// [`Self::restore_caller_context`] for any frame.
    fn restore_caller_context_at(&mut self, frame: FrameAt) {
        match frame {
            FrameAt::Vm(idx) => self.restore_caller_context(idx),
            FrameAt::Record(record) => {
                self.restore_caller_context(compiled_rt::record_ref(record).anchor());
            }
        }
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
        self.end_manual_activation(frame_idx);
        let state = self
            .telemetry
            .as_mut()
            .expect("observed invocation has telemetry");
        if let Frame::Bytecode(frame) = &self.frames[frame_idx] {
            state.select_context(&frame.context);
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

    fn frame_wants_error_capture(&self, frame: FrameAt, function: &Function) -> bool {
        let Some(state) = self.telemetry.as_ref() else {
            return false;
        };
        self.frame_receipt(frame)
            .is_some_and(|telemetry| state.wants_error_capture(&telemetry, function))
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
    pub fn complete_network_io(
        &mut self,
        span: btel_types::TelemetryId,
        at: ClockInstant,
        error: Option<&sys_types::network::NetworkError>,
    ) {
        if let Some(state) = self.telemetry.as_mut() {
            state.complete_network_io(span, at, error);
        }
    }

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

    /// Name a root future after its entry function, which may record no call
    /// of its own (a pass-through LLM function, or one hidden by its policy).
    fn name_root_thread(&mut self, callable: HeapPtr) {
        let metadata = self
            .callable_function_identity(callable)
            .and_then(|function| {
                // SAFETY: the entry callable roots its function under the heap permit.
                match unsafe { function.get() } {
                    Object::Function(function) => function.runtime_metadata(),
                    _ => None,
                }
            });
        if let (Some(state), Some(metadata)) = (&mut self.telemetry, metadata) {
            state.name_root_thread(&metadata.fqn);
        }
    }

    /// Name this thread's future for telemetry; call before its entry point.
    pub fn set_telemetry_thread_name(&mut self, name: &str) {
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.set_thread_name(name);
        }
    }

    /// Validate the nominal Context class without introducing a trace → baml package cycle.
    pub(crate) fn is_error_context(&self, value: Value) -> bool {
        self.as_instance(&value).is_ok_and(|instance| {
            self.error_class_ptrs
                .get(ErrorClass::Context as usize)
                .is_some_and(|class| *class == instance.class())
        })
    }

    /// How an escaping error ends an observed invocation: cancelled for a
    /// cancellation or `exit`, panicked for another panic, else errored.
    pub fn telemetry_outcome_for_exception(&self, value: Value) -> InvocationOutcome {
        let Some(ptr) = value.as_object_ptr() else {
            return InvocationOutcome::Errored;
        };
        let Some(class) = self.get_instance(ptr).map(InstanceRef::class) else {
            return InvocationOutcome::Errored;
        };
        let is = |panic: PanicClass| {
            self.panic_class_ptrs
                .get(panic as usize)
                .is_some_and(|panic_class| *panic_class == class)
        };
        // Exit ends the process on purpose: the frames it unwinds were
        // stopped, not failed.
        if is(PanicClass::Cancelled) || is(PanicClass::Exit) {
            InvocationOutcome::Cancelled
        } else if self.panic_class_ptrs.contains(&class) {
            InvocationOutcome::Panicked
        } else {
            InvocationOutcome::Errored
        }
    }

    /// The frame a call or wait at VM frame `frame_idx` belongs to: a native
    /// continuation resolves to its caller, a pending hook target to the
    /// target's caller, and a compiled frame to the innermost frame record
    /// above it.
    fn bytecode_caller(&self, frame_idx: usize) -> FrameAt {
        let at = match &self.frames[frame_idx] {
            Frame::Bytecode(_) => self.innermost_at(frame_idx),
            Frame::Native(frame) => self
                .frame_at_level(frame.bytecode_caller)
                .unwrap_or(FrameAt::Vm(frame_idx)),
        };
        if let FrameAt::Vm(index) = at
            && let Some(pending) = self
                .pending_trace_hooks
                .iter()
                .find(|pending| pending.frame == index && pending.evaluating)
        {
            return pending
                .caller
                .and_then(|level| self.frame_at_level(level))
                .unwrap_or(at);
        }
        at
    }

    /// A root entry's pending target is bookkeeping, not a visible caller of
    /// its hook. Keep the physical frame for execution without inventing a
    /// target-to-hook edge in the recorded call path.
    fn visible_bytecode_caller(&self, frame_idx: usize) -> Option<FrameAt> {
        let at = self.bytecode_caller(frame_idx);
        if let FrameAt::Vm(index) = at
            && let Some(pending) = self
                .pending_trace_hooks
                .iter()
                .find(|pending| pending.frame == index && pending.evaluating)
        {
            return pending.caller.and_then(|level| self.frame_at_level(level));
        }
        Some(at)
    }

    /// The call-path identity of the function `frame` runs.
    pub(crate) fn frame_identity(&self, frame: FrameAt) -> Option<HeapPtr> {
        self.callable_function_identity(self.frame_callable(frame))
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
        let owner = self.bytecode_caller(frame_idx);
        if self.frame_receipt(owner).is_some()
            || self
                .manual_activations
                .last()
                .is_some_and(|active| active.telemetry.is_some())
        {
            debug_assert!(self.pending_telemetry_wait.is_none());
            self.pending_telemetry_wait = Some((
                self.level_of(owner),
                self.telemetry
                    .as_ref()
                    .expect("observed wait has telemetry")
                    .clock()
                    .read(),
            ));
        }
    }

    /// The engine handled the [`VmExecState::Checkpoint`] this VM yielded
    /// without cancelling it: the `AwaitAny` it resumes takes its settled
    /// input.
    pub fn pass_checkpoint(&mut self) {
        self.checkpoint_passed = true;
    }

    /// Transfer the wait across a non-fallible engine parking scope. Its end
    /// clock is read before reacquisition, then the saved owner is charged.
    /// The owner is a frame level; see `BexVm::stack_view`.
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
        if frame_idx.is_some()
            && let Some(scope) = self
                .manual_activations
                .last()
                .and_then(|active| active.telemetry.as_ref())
        {
            scope.add_await(
                elapsed,
                self.telemetry
                    .as_ref()
                    .expect("active telemetry scope")
                    .clock(),
            );
        }
        if let Some(owner) = frame_idx.and_then(|level| self.frame_at_level(level)) {
            self.with_frame_receipt(owner, |receipt| {
                if let Some(telemetry) = receipt {
                    telemetry.add_await(elapsed);
                }
            });
        }
    }

    /// Hand the invocation timings this VM buffered to the runtime, before
    /// the execution scope they were written in ends. Out of line: `exec`
    /// holds the dispatch loop, whose register allocation shifts with any
    /// code added around it.
    #[cfg(not(target_arch = "wasm32"))]
    #[inline(never)]
    fn flush_telemetry_timing(&mut self) {
        if let Some(telemetry) = self.telemetry.as_mut() {
            telemetry.flush_timing();
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
            self.drop_telemetry();
        }
    }

    /// Stop observing this VM's execution, as when recording is off
    /// (`BAML_TELEMETRY=off`): release its telemetry state and every frame's.
    /// Frames remain executable. Execution then takes the same paths it
    /// takes with recording off, including the dispatch loop's fast path for
    /// plain calls and returns.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn disable_telemetry(&mut self) {
        if self.telemetry.is_some() {
            self.drop_telemetry();
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    fn drop_telemetry(&mut self) {
        self.telemetry.take().unwrap().abandon();
        self.pending_telemetry_wait = None;
        for frame in &mut self.frames {
            if let Frame::Bytecode(frame) = frame {
                frame.telemetry = None;
            }
        }
        for frame in self.stack_view() {
            if let FrameAt::Record(record) = frame {
                compiled_rt::record_ref(record).receipt.set(None);
            }
        }
        // An active `Span.run` scope observes through the same state.
        for active in &mut self.manual_activations {
            active.telemetry = None;
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
        for frame in self.stack_view() {
            self.with_frame_receipt(frame, |receipt| {
                if let Some(telemetry) = receipt {
                    telemetry.entered_at = at;
                }
            });
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
                .stack_view()
                .into_iter()
                .any(|frame| self.frame_receipt(frame).is_some())
            && self
                .panic_class_ptrs
                .get(PanicClass::Cancelled as usize)
                .is_some_and(|class| !class.is_null()))
        .then(|| self.panic_to_exception_value(VmPanic::Cancelled))
        .map(|error| self.error_context_value(error, &[], Value::NULL));
        for frame in self.stack_view().into_iter().rev() {
            match frame {
                FrameAt::Vm(frame_idx) => {
                    self.complete_bytecode_invocation(frame_idx, outcome, error);
                }
                FrameAt::Record(_) => self.complete_record_invocation(frame, outcome, error),
            }
        }
        let telemetry = self.telemetry.as_mut().unwrap();
        telemetry.complete_thread(outcome);
        #[cfg(not(target_arch = "wasm32"))]
        telemetry.flush_timing();
    }

    pub fn telemetry_clock(&self) -> Option<&Arc<btel_clock::ClockEpoch>> {
        self.telemetry.as_ref().map(TelemetryState::clock)
    }
    pub fn is_telemetry_root(&self) -> bool {
        self.telemetry
            .as_ref()
            .is_some_and(TelemetryState::is_root_thread)
    }

    /// Reserve value backing storage under the same budget as call frames.
    #[inline]
    fn reserve_stack(&mut self, additional: usize) -> Result<(), VmError> {
        self.stack
            .try_reserve(additional)
            .map_err(|panic| VmError::thrown_fresh(self.panic_to_exception_value(panic)))
    }

    /// Debug builds, before the dispatch loop runs the instruction at `pc`
    /// of `function` in frame `frame_idx`: record its opcode for the coverage
    /// test, and assert the live operand depth is the one
    /// [`bex_vm_types::Bytecode::verify_stack`] proved for it. A handler
    /// whose real stack effect differs from
    /// [`bex_vm_types::Bytecode::stack_effect`] fails at the next instruction
    /// its frame runs, however the instruction finished: in place, after a
    /// call returned, or after the engine resumed it.
    #[cfg(debug_assertions)]
    fn debug_check_dispatch(&self, frame_idx: usize, function: &Function, pc: usize, op: u8) {
        crate::opcode_coverage::record(op);
        let Some(Frame::Bytecode(frame)) = self.frames.get(frame_idx) else {
            return;
        };
        Self::debug_check_operand_depth(
            function,
            frame.locals_offset.raw(),
            self.stack.len(),
            self.stack.capacity(),
            pc,
        );
    }

    /// The depth half of [`Self::debug_check_dispatch`], for a frame whose
    /// locals start at `locals_offset` and a stack `len` values long with
    /// room for `capacity`. Also asserts that entering the frame reserved its
    /// whole operand bound, which the unchecked pushes rely on.
    #[cfg(debug_assertions)]
    fn debug_check_operand_depth(
        function: &Function,
        locals_offset: usize,
        len: usize,
        capacity: usize,
        pc: usize,
    ) {
        let Some(code) = function.bytecode.compact.as_ref() else {
            return;
        };
        // Past the instructions: a compiled body's resume stub, which keeps
        // its own stack.
        let Some(&depth) = code.operand_depths.get(pc) else {
            return;
        };
        let base = locals_offset + function.arity + function.real_local_count;
        assert!(
            locals_offset + function.arity + code.frame_slots <= capacity
                && code.frame_slots == function.real_local_count + code.max_stack,
            "a frame of `{}` runs without its operand bound reserved",
            function.name(),
        );
        assert!(
            depth != bex_vm_types::stack_effect::NO_DEPTH,
            "`{}` runs the instruction at {pc}, which its stack check found unreachable",
            function.name(),
        );
        assert_eq!(
            len.checked_sub(base),
            Some(depth as usize),
            "`{}` reaches the instruction at {pc} with an operand depth its stack check did not prove",
            function.name(),
        );
    }

    /// Push an operand with fallible backing growth.
    ///
    /// The common case writes into spare capacity behind one compare; only a
    /// push that must grow the stack takes the out-of-line budgeted path.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn push_stack(&mut self, value: Value) -> Result<(), VmError> {
        if self.stack.push_within_capacity(value) {
            Ok(())
        } else {
            self.push_stack_grow(value)
        }
    }

    /// [`Self::push_stack`] when the operand stack is full: grow it within the
    /// thread's stack budget, or raise the allocation failure.
    #[cold]
    #[inline(never)]
    fn push_stack_grow(&mut self, value: Value) -> Result<(), VmError> {
        self.stack
            .try_push(value)
            .map_err(|panic| VmError::thrown_fresh(self.panic_to_exception_value(panic)))
    }

    /// Reserve a complete argument sequence before changing the live stack.
    fn extend_stack(
        &mut self,
        values: impl IntoIterator<Item = Value, IntoIter: ExactSizeIterator>,
    ) -> Result<(), VmError> {
        self.stack
            .try_extend(values)
            .map_err(|panic| VmError::thrown_fresh(self.panic_to_exception_value(panic)))
    }

    /// Initialize already reserved locals, or fallibly grow value backing storage.
    fn resize_stack(&mut self, len: usize, value: Value) -> Result<(), VmError> {
        self.stack
            .try_resize(len, value)
            .map_err(|panic| self.panic_error(panic))
    }

    /// The error a refused stack or frame reservation raises. Out of line:
    /// reservations sit on every call's fast path.
    #[cold]
    #[inline(never)]
    pub(crate) fn panic_error(&mut self, panic: VmPanic) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(panic))
    }

    /// All frame growth is fallible and charged at the same storage boundary,
    /// including exact calls, indirect calls, entries, and native callbacks.
    #[inline]
    fn reserve_call_frame(&mut self) -> Result<(), VmError> {
        self.frames
            .reserve_one()
            .map_err(|panic| self.panic_error(panic))
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
        #[cfg(not(target_arch = "wasm32"))]
        self.flush_telemetry_timing();

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
        let handled = self.handle_external_thrown(thrown);
        #[cfg(not(target_arch = "wasm32"))]
        self.flush_telemetry_timing();
        handled
    }

    /// `try_handle_external_thrown` inside its telemetry scope.
    fn handle_external_thrown(&mut self, thrown: VmThrown) -> Result<(), VmError> {
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
        if let Some(panic) = self
            .pending_entry_frame_panic
            .take()
            .or_else(|| self.stack.take_pending_panic())
        {
            let value = self.panic_to_exception_value(panic);
            self.try_handle_external_exception(value)?;
        }
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
        let result = self.cmpop_values(op, left, right)?;
        self.push_stack(result)?;
        Ok(())
    }

    fn cmpop_values(&mut self, op: CmpOp, left: Value, right: Value) -> Result<Value, VmError> {
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
            left.as_int().map(|i| i as f64).or_else(|| left.as_float()),
            right
                .as_int()
                .map(|i| i as f64)
                .or_else(|| right.as_float()),
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

        Ok(result)
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
        let result = self.binop_values(op, left, right)?;
        self.push_stack(result)?;
        Ok(())
    }

    fn binop_values(&mut self, op: BinOp, left: Value, right: Value) -> Result<Value, VmError> {
        #[allow(clippy::cast_precision_loss)]
        let result = if let (Some(l), Some(r)) = (left.as_int(), right.as_int()) {
            match op {
                // Both `/` and `%` by zero throw DivisionByZero (the `%` guard
                // matches the specialized `ModInt` opcode — without it, `l % 0`
                // on this generic path would raw-Rust-panic).
                BinOp::Div | BinOp::Mod if r == 0 => {
                    return Err(self.division_by_zero(l, r));
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
            left.as_int().map(|i| i as f64).or_else(|| left.as_float()),
            right
                .as_int()
                .map(|i| i as f64)
                .or_else(|| right.as_float()),
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
            self.float_value(f)
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

        Ok(result)
    }

    /// Allocate the result of a `ConcatStrings`, interpreted or compiled.
    pub(crate) fn alloc_joined_string(&mut self, result: bex_vm_types::BexStr) -> Value {
        let value = Value::object(self.alloc_string(result));
        // One instruction can build a string of any size.
        self.settle();
        value
    }

    /// The strings `parts` joined left to right (`ConcatStrings`).
    pub(crate) fn join_strings(
        &self,
        parts: &[Value],
    ) -> Result<bex_vm_types::BexStr, VmInternalError> {
        let mut strings: smallvec::SmallVec<[&bex_vm_types::BexStr; 32]> =
            smallvec::SmallVec::with_capacity(parts.len());
        for part in parts {
            strings.push(self.as_string(part)?);
        }
        Ok(bex_vm_types::BexStr::concat_many(&strings))
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
                        self.push_stack(val)?;
                    }
                    NativeCallResult::Error(e) => match self.native_error_to_vm_error(e) {
                        VmError::Thrown(thrown) => {
                            self.restore_native_call_site_pc();
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
                        // Reuse the slot popped above; no allocation is needed.
                        self.frames.push_reserved(Frame::Native(NativeFrame {
                            function: native_fn_ptr,
                            continuation,
                            bytecode_caller: nf.bytecode_caller,
                        }));

                        let real_callee =
                            self.resolve_bound_method_callee(callee, &mut callback_args)?;
                        let arg_count = callback_args.len();
                        let cb_locals = StackIndex::from_raw(self.stack.len());
                        let ecflo_outcome = self.extend_stack(callback_args).and_then(|()| {
                            self.execute_call_from_locals_offset_with_type_args(
                                real_callee,
                                cb_locals,
                                arg_count,
                                CallOptions {
                                    type_args: &callback_type_args,
                                },
                                &mut frame_idx,
                                &mut function,
                            )
                        });

                        let ecflo_result = match ecflo_outcome {
                            Ok(result) => result,
                            Err(VmError::Thrown(thrown)) => {
                                self.restore_native_call_site_pc();
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
                    self.thread_local_call = false;
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

    // ── Opcode semantics shared by `run_compact` and `run_fast_path` ──────
    //
    // Each instruction the fast path runs is defined once, here, and both
    // loops call these. They differ only in what happens when a helper
    // declines: the fast path hands the instruction back unexecuted, and
    // the full dispatch raises the error or takes its general path.

    /// Target of a branch whose `offset` is relative to `next_pc`, the end
    /// of the branch instruction.
    #[allow(
        clippy::inline_always,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation
    )]
    #[inline(always)]
    fn branch_target(next_pc: usize, offset: i32) -> usize {
        (next_pc as i64 + i64::from(offset)) as usize
    }

    /// Whether `JumpIfFalse` jumps on `cond` (which it leaves on the stack).
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn jump_if_false_taken(cond: Value) -> bool {
        cond == Value::bool(false)
    }

    /// Whether `PopJumpIfFalse` / `PopJumpIfTrue` (`op`) jumps on `cond`.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn pop_jump_taken(op: bex_vm_types::bytecode::OpCode, cond: Value) -> bool {
        cond == Value::bool(op == bex_vm_types::bytecode::OpCode::PopJumpIfTrue)
    }

    /// Whether `JumpIfFalseOrPop` / `JumpIfTrueOrPop` / `JumpIfNotNullOrPop`
    /// (`op`) jumps on `cond`; when it does not, it pops `cond`.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn jump_or_pop_taken(op: bex_vm_types::bytecode::OpCode, cond: Value) -> bool {
        use bex_vm_types::bytecode::OpCode;
        match op {
            OpCode::JumpIfFalseOrPop => Self::jump_if_false_taken(cond),
            OpCode::JumpIfTrueOrPop => cond == Value::bool(true),
            _ => !cond.is_null(),
        }
    }

    /// `MulInt` of two ints, or `None` when the product leaves the int range.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn int_mul(l: i64, r: i64) -> Option<Value> {
        l.checked_mul(r).and_then(Value::try_int)
    }

    /// `CmpInt*` (`op`) on two `Int`-tagged values. Both carry the same tag
    /// bit, so the tagged words, read as signed, order like their ints (see
    /// `Value::tagged_int_add_checked`).
    #[allow(clippy::inline_always, clippy::cast_possible_wrap)]
    #[inline(always)]
    fn int_compare(op: bex_vm_types::bytecode::OpCode, l: Value, r: Value) -> bool {
        use bex_vm_types::bytecode::OpCode;
        let (l, r) = (l.bits() as i64, r.bits() as i64);
        match op {
            OpCode::CmpIntEq => l == r,
            OpCode::CmpIntNotEq => l != r,
            OpCode::CmpIntLt => l < r,
            OpCode::CmpIntLtEq => l <= r,
            OpCode::CmpIntGt => l > r,
            _ => {
                debug_assert_eq!(op, OpCode::CmpIntGtEq);
                l >= r
            }
        }
    }

    /// Truthiness of a value that needs no heap inspection: `false`, `null`,
    /// an omitted argument and zero are falsy. `None` for a heap object; see
    /// [`Self::is_truthy`].
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn scalar_truthiness(value: Value) -> Option<bool> {
        use bex_vm_types::ValueKind;
        match value.kind() {
            ValueKind::Null | ValueKind::OmittedArg => Some(false),
            ValueKind::Bool(b) => Some(b),
            ValueKind::Int(v) => Some(v != 0),
            // An inline float is a scalar too; a boxed one (NaN, inf, out of
            // range) is an object and `is_truthy` reads it.
            ValueKind::Float(f) => Some(f != 0.0),
            ValueKind::Object(_) => None,
        }
    }

    /// What `Not` / `Truthy` (`op`) push for an operand of `truthiness`.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn truthiness_result(op: bex_vm_types::bytecode::OpCode, truthiness: bool) -> Value {
        Value::bool(truthiness == (op == bex_vm_types::bytecode::OpCode::Truthy))
    }

    /// The shared object for variant `variant` of the enum at `enum_idx`, for
    /// `LoadVariant` in a static function (null `package`) naming a
    /// compile-time enum: those variants are permanent shared objects (see
    /// `Enum::shared_variants`). `None` sends the instruction to
    /// [`Self::variant_value`], which handles every other case.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn shared_variant(&self, package: HeapPtr, enum_idx: usize, variant: usize) -> Option<Value> {
        if !package.is_null() || enum_idx >= self.heap.compile_time_len() {
            return None;
        }
        let Object::Enum(enm) = self.get_object(self.heap.compile_time_ptr(enum_idx)) else {
            return None;
        };
        enm.shared_variants.get(variant).copied().map(Value::object)
    }

    /// Enter the plain bytecode callee of an exact call: record the caller's
    /// return PC and call-site PC, push the callee frame with the caller's
    /// context, and initialize the callee's locals to null.
    ///
    /// The caller is the top frame. A frame slot must already be reserved,
    /// `room` proves the stack holds `callee.frame_reserve()` more slots, and
    /// the arguments must be the top `callee.arity` stack values, starting at
    /// `locals_offset`.
    ///
    /// The frame is pushed with an empty context and the caller's written
    /// in after: with a cloned context live across the push's capacity
    /// assertion, unwinding needs a landing pad, which in the fast path made
    /// every entry save ten registers. The locals are filled with volatile
    /// stores for the same reason: LLVM turns a plain fill into a `memset`
    /// call.
    #[allow(clippy::inline_always, unsafe_code)]
    #[expect(
        clippy::too_many_arguments,
        reason = "inlined into the dispatch loops; the room is zero-sized"
    )]
    #[inline(always)]
    fn enter_exact_callee(
        &mut self,
        call_pc: usize,
        return_pc: usize,
        callee_ptr: HeapPtr,
        callee: &Function,
        locals_offset: StackIndex,
        telemetry: Option<FrameTelemetry>,
        room: frame_room::FrameRoom,
    ) {
        let Some(Frame::Bytecode(caller)) = self.frames.last_mut() else {
            verifier_unreachable!()
        };
        caller.instruction_ptr = return_pc;
        caller.faulting_pc = call_pc;
        self.frames.push_reserved_with(|| {
            Frame::Bytecode(BytecodeFrame {
                function: callee_ptr,
                instruction_ptr: 0,
                locals_offset,
                type_args: FrameTypeArgs::default(),
                faulting_pc: 0,
                telemetry,
                context: Context::default(),
                room,
            })
        });
        let [.., Frame::Bytecode(caller), Frame::Bytecode(frame)] = &mut self.frames[..] else {
            verifier_unreachable!()
        };
        let (base, len, capacity) = self.stack.raw_parts();
        debug_assert!(capacity - len >= callee.frame_reserve());
        // SAFETY: the slot holds `Context::default()`, which owns nothing,
        // so overwriting it without a drop leaks nothing. The locals fit in
        // the reserved capacity, and `Value` is `Copy`.
        unsafe {
            std::ptr::write(&raw mut frame.context, caller.context.clone());
            for slot in len..len + callee.real_local_count {
                base.add(slot).write_volatile(Value::NULL);
            }
            self.stack.set_len(len + callee.real_local_count);
        }
    }

    /// What telemetry records as the dispatching frame `frame_idx` makes an
    /// exact call, at the instruction starting at `call_pc`, to the bytecode
    /// function `callee`, whose arguments start at `locals_offset` and whose
    /// frame starts with `type_args`. Returns the callee frame's receipt.
    /// Both dispatch loops enter observed calls through this, so they record
    /// the same invocations; it is out of line so neither loop carries the
    /// producer code.
    #[inline(never)]
    #[allow(unsafe_code)]
    fn observe_exact_call(
        &mut self,
        frame_idx: usize,
        call_pc: usize,
        callee_ptr: HeapPtr,
        callee: &Function,
        locals_offset: StackIndex,
        type_args: &[bex_vm_types::RealizedTy],
    ) -> Option<FrameTelemetry> {
        // The dispatching frame is the top frame, so its context is
        // `current_context()` without the walk.
        debug_assert_eq!(frame_idx + 1, self.frames.len());
        let (Some(Frame::Bytecode(caller)), Some(telemetry)) =
            (self.frames.get(frame_idx), self.telemetry.as_mut())
        else {
            return None;
        };
        telemetry.select_context(&caller.context);
        let caller_telemetry = caller.telemetry;
        let caller_is_observed = caller_telemetry.is_some();
        // A frame this function entered resolves to its own function object,
        // so only other frames look their identity up.
        let actual_caller = if caller_telemetry.is_some_and(FrameTelemetry::own_identity) {
            debug_assert_eq!(
                self.callable_function_identity(caller.function),
                Some(caller.function)
            );
            Some(caller.function)
        } else {
            self.callable_function_identity(caller.function)
        };
        let caller_pc = u32::try_from(call_pc).unwrap_or(u32::MAX);
        let args = &self.stack.as_slice()
            [locals_offset.raw()..locals_offset.raw().saturating_add(callee.arity)];
        // SAFETY: arguments and callable remain live under the heap permit.
        let mut entered = self.telemetry.as_mut().and_then(|telemetry| unsafe {
            telemetry.enter_bytecode_with_trace(
                callee,
                callee_ptr,
                actual_caller,
                caller_pc,
                caller_is_observed,
                args,
                None,
                CallTypeArgs {
                    carried: &[],
                    passed: type_args,
                },
                |caller, callee| Self::register_call_path_functions(&self.heap, caller, callee),
            )
        });
        // The callee frame runs `callee_ptr`, a function object, and is
        // observed only when that function carries a telemetry id: exactly
        // when `callable_function_identity` resolves it to itself.
        if let Some(entered) = &mut entered {
            entered.set_own_identity();
        }
        entered
    }

    /// The fast path's exact call of the plain bytecode `callee` with
    /// telemetry on, from the dispatching frame `frame_idx` at the
    /// `CallExactArgs` spanning `call_pc..return_pc`: the full dispatch's arm
    /// for a non-generic callee (`observe_exact_call`, then the frame push).
    /// Returns where dispatch continues, in the callee. Out of line, so the
    /// fast path's telemetry-off call keeps its registers.
    #[inline(never)]
    #[expect(
        clippy::too_many_arguments,
        reason = "the fast path's frame entry, called with what it holds in registers"
    )]
    fn enter_observed_exact_callee(
        &mut self,
        frame_idx: usize,
        call_pc: usize,
        return_pc: usize,
        callee_ptr: HeapPtr,
        callee: &'static Function,
        locals_offset: StackIndex,
        room: frame_room::FrameRoom,
    ) -> DispatchState {
        let telemetry =
            self.observe_exact_call(frame_idx, call_pc, callee_ptr, callee, locals_offset, &[]);
        self.enter_exact_callee(
            call_pc,
            return_pc,
            callee_ptr,
            callee,
            locals_offset,
            telemetry,
            room,
        );
        self.resume_state(frame_idx + 1, callee)
    }

    /// The fast path's return from the frame `frame_idx` (running
    /// `function`) into its bytecode caller, with telemetry on: the full
    /// dispatch's `Return` arm for a frame that owns nothing but its
    /// caller's context. Returns where dispatch continues, in the caller.
    /// Out of line, so the fast path's telemetry-off return keeps its
    /// registers.
    #[inline(never)]
    fn return_observed_to_caller(
        &mut self,
        frame_idx: usize,
        function: &Function,
    ) -> DispatchState {
        let result = self.stack.get_at(self.stack.ensure_stack_top());
        let Some(Frame::Bytecode(returning)) = self.frames.get_mut(frame_idx) else {
            verifier_unreachable!()
        };
        let locals_offset = returning.locals_offset;
        match returning.telemetry.take() {
            // Ends by selecting the context of the nearest bytecode frame
            // below `frame_idx`: the caller's, which the `Return` arm
            // selects again after the pop.
            Some(telemetry) => self.complete_bytecode_invocation_with_function(
                frame_idx,
                function,
                telemetry,
                InvocationOutcome::Ok,
                Some(result),
            ),
            // `end_manual_activation` has nothing to end: the fast path
            // hands back a frame that scopes a trace. Selecting the caller's
            // context now is what the `Return` arm does after the pop.
            None => self.restore_caller_context(frame_idx),
        }
        self.leave_returning_frame_slots(locals_offset);
        self.pop_frame_sharing_caller_context();
        self.resume_top_bytecode_frame()
    }

    /// `resume_state` for the top frame, a bytecode frame running a
    /// function object, as the fast path checks before it returns into it.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn resume_top_bytecode_frame(&self) -> DispatchState {
        let frame_idx = self.frames.len() - 1;
        let Some(Frame::Bytecode(frame)) = self.frames.get(frame_idx) else {
            verifier_unreachable!()
        };
        // SAFETY: frames hold rooted function objects under the heap permit.
        let Object::Function(function) = (unsafe { frame.function.get() }) else {
            verifier_unreachable!()
        };
        self.resume_state(frame_idx, function)
    }

    /// Replace the returning frame's slots, from `locals_offset` up, with
    /// its result, the top stack value.
    #[allow(clippy::inline_always, unsafe_code)]
    #[inline(always)]
    fn leave_returning_frame_slots(&mut self, locals_offset: StackIndex) {
        let result = self.stack.get_at(self.stack.ensure_stack_top());
        // SAFETY: the compiler guarantees a result at or above the callee's
        // locals base. That base is therefore an existing initialized slot,
        // even for a zero-argument function with no locals. Value is Copy, so
        // discarded slots need no drop. No allocation or GC point occurs
        // between writing the result and shortening the stack to the caller's
        // new top.
        let consumed = self.stack.len() - locals_offset.raw();
        unsafe { self.stack.replace_top_n_dynamic(consumed, result) };
    }

    /// Where dispatch continues in the bytecode frame at `frame_idx`, which
    /// runs `function`.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    fn resume_state(&self, frame_idx: usize, function: &'static Function) -> DispatchState {
        let Some(Frame::Bytecode(frame)) = self.frames.get(frame_idx) else {
            verifier_unreachable!()
        };
        DispatchState {
            pc: frame.instruction_ptr,
            code: &function
                .bytecode
                .compact
                .as_ref()
                .expect("a bytecode frame's function has compact code")
                .code,
            function,
            frame_idx,
            locals_base: frame.locals_offset.raw().wrapping_sub(1),
            runtime_package: self.runtime_package_of(function),
        }
    }

    /// Run instructions from `state.pc` in the dispatch loop's fast path
    /// until one it does not handle, then return with `state` describing the
    /// frame and instruction the full dispatch continues with.
    ///
    /// The fast path covers what tight scalar loops and plain calls execute:
    /// local and constant loads and stores, instance field loads, enum variant
    /// constants (without `CALLS`), int arithmetic and comparisons,
    /// truthiness, branches, and, when `CALLS`, exact calls into and returns
    /// out of plain bytecode functions. It is
    /// small and out of line so the program counter, stack top, and locals
    /// base stay in registers; inside [`Self::run_compact`] they were
    /// reloaded from spill slots on every instruction. It never allocates on
    /// the VM heap, reaches a GC safepoint, runs native code, or polls for an
    /// early yield, so nothing can observe the stack length it keeps in a
    /// register; it publishes that length before returning, and before a
    /// call or return records telemetry, which reads the arguments and the
    /// result off the stack. It does not record `cur_pc`: nothing reads it
    /// during these instructions (a recorded call gets the call's pc
    /// passed), and the full dispatch records it for the instruction it runs
    /// next.
    ///
    /// The two variants trade entry cost against reach. Without `CALLS` the
    /// loop is a leaf that saves almost nothing on entry, which matters for
    /// code that alternates between it and the full dispatch (array and
    /// float operations, allocation). With `CALLS` it also switches frames,
    /// which needs more registers and a costlier entry, but keeps call-heavy
    /// code out of the full dispatch entirely.
    ///
    /// Each arm runs only its common case. When an instruction would grow
    /// the stack or frame stack, overflow, poll for an early yield, meet an
    /// operand it does not handle, or involve trace hooks, model usage,
    /// generic type arguments, or a native frame, the loop stops *before* that
    /// instruction, with no effect, and the full dispatch runs it. Every
    /// result is therefore what the full dispatch alone would produce.
    #[inline(never)]
    #[allow(
        unsafe_code,
        clippy::too_many_lines,
        clippy::cast_possible_wrap,
        clippy::cast_sign_loss,
        clippy::cast_possible_truncation,
        clippy::inline_always
    )]
    fn run_fast_path<const CALLS: bool>(&mut self, state: &mut DispatchState) {
        use bex_vm_types::bytecode::OpCode;

        #[inline(always)]
        fn operand_u32(code: &[u8], at: usize) -> usize {
            debug_assert!(at + 4 <= code.len());
            // SAFETY: the encoder emits every operand in full.
            u32::from_le(unsafe { code.as_ptr().add(at).cast::<u32>().read_unaligned() }) as usize
        }
        #[inline(always)]
        fn operand_u16(code: &[u8], at: usize) -> usize {
            debug_assert!(at + 2 <= code.len());
            // SAFETY: the encoder emits every operand in full.
            usize::from(u16::from_le(unsafe {
                code.as_ptr().add(at).cast::<u16>().read_unaligned()
            }))
        }
        #[inline(always)]
        fn operand_i32(code: &[u8], at: usize) -> i32 {
            debug_assert!(at + 4 <= code.len());
            // SAFETY: the encoder emits every operand in full.
            i32::from_le(unsafe { code.as_ptr().add(at).cast::<i32>().read_unaligned() })
        }
        // Branches: opcode and an i32 offset from the end of the instruction.
        const BRANCH_LEN: usize = 5;
        // `CallExactArgs`: opcode, u32 callee global, u16 type-arg count.
        const CALL_EXACT_LEN: usize = 7;

        let DispatchState {
            mut pc,
            mut code,
            mut function,
            mut frame_idx,
            mut locals_base,
            mut runtime_package,
        } = *state;
        let mut constants: &[Value] = &function.bytecode.resolved_constants;
        let (mut base, mut sp, mut cap) = self.stack.raw_parts();
        // SAFETY (every arm): the encoder validates each instruction's stack
        // effect and local slots, so operand reads stay within `0..sp` and
        // local slots within the frame. Pushes stay within the capacity the
        // frame reserved on entry (`Function::frame_reserve`), and nothing
        // below grows the stack, so `base` stays valid.
        unsafe {
            macro_rules! at {
                ($index:expr) => {
                    *base.add($index)
                };
            }
            macro_rules! local {
                ($slot:expr) => {
                    *base.add(Self::local_at(locals_base, $slot).raw())
                };
            }
            loop {
                let op: OpCode = std::mem::transmute(*code.get_unchecked(pc));
                #[cfg(debug_assertions)]
                {
                    crate::opcode_coverage::record(op as u8);
                    Self::debug_check_operand_depth(
                        function,
                        locals_base.wrapping_add(1),
                        sp,
                        cap,
                        pc,
                    );
                }
                match op {
                    OpCode::LoadNull | OpCode::LoadTrue | OpCode::LoadFalse => {
                        at!(sp) = match op {
                            OpCode::LoadNull => Value::NULL,
                            OpCode::LoadTrue => Value::bool(true),
                            _ => Value::bool(false),
                        };
                        sp += 1;
                        pc += 1;
                    }
                    OpCode::LoadIntSmall => {
                        let value = *code.get_unchecked(pc + 1) as i8;
                        at!(sp) = Value::int(i64::from(value));
                        sp += 1;
                        pc += 2;
                    }
                    OpCode::LoadConst => {
                        let Some(&value) = constants.get(operand_u32(code, pc + 1)) else {
                            break;
                        };
                        at!(sp) = value;
                        sp += 1;
                        pc += 5;
                    }
                    OpCode::LoadVar => {
                        at!(sp) = local!(operand_u32(code, pc + 1));
                        sp += 1;
                        pc += 5;
                    }
                    OpCode::LoadVar2 => {
                        at!(sp) = local!(operand_u32(code, pc + 1));
                        at!(sp + 1) = local!(operand_u32(code, pc + 5));
                        sp += 2;
                        pc += 9;
                    }
                    OpCode::StoreVar => {
                        sp -= 1;
                        local!(operand_u32(code, pc + 1)) = at!(sp);
                        pc += 5;
                    }
                    OpCode::StoreVar2 => {
                        local!(operand_u32(code, pc + 1)) = at!(sp - 1);
                        local!(operand_u32(code, pc + 5)) = at!(sp - 2);
                        sp -= 2;
                        pc += 9;
                    }
                    OpCode::StoreVarLoadVar => {
                        local!(operand_u32(code, pc + 1)) = at!(sp - 1);
                        pc += 5;
                    }
                    OpCode::Pop => {
                        sp -= operand_u32(code, pc + 1);
                        pc += 5;
                    }
                    OpCode::Copy => {
                        at!(sp) = at!(sp - 1 - operand_u32(code, pc + 1));
                        sp += 1;
                        pc += 5;
                    }
                    OpCode::LoadField | OpCode::LoadFieldLocal => {
                        let Some(value) = self.site_load(
                            function,
                            operand_u16(code, pc + 5),
                            at!(sp - 1),
                            operand_u32(code, pc + 1),
                            op == OpCode::LoadFieldLocal,
                        ) else {
                            break;
                        };
                        at!(sp - 1) = value;
                        pc += 7;
                    }
                    OpCode::AddLocalSmall => {
                        let addend = Value::int(i64::from(*code.get_unchecked(pc + 9) as i8));
                        let Some(sum) = Value::tagged_int_add_checked(
                            local!(operand_u32(code, pc + 5)),
                            addend,
                        ) else {
                            break;
                        };
                        local!(operand_u32(code, pc + 1)) = sum;
                        pc += 10;
                    }
                    OpCode::LoadVarField | OpCode::LoadVarFieldLocal => {
                        let Some(value) = self.site_load(
                            function,
                            operand_u16(code, pc + 9),
                            local!(operand_u32(code, pc + 1)),
                            operand_u32(code, pc + 5),
                            op == OpCode::LoadVarFieldLocal,
                        ) else {
                            break;
                        };
                        at!(sp) = value;
                        sp += 1;
                        pc += 11;
                    }
                    // Only in the leaf variant: in the one that switches
                    // frames this arm slowed call-heavy code. There it stops,
                    // and the full dispatch re-enters the leaf variant here.
                    OpCode::LoadVariant if !CALLS => {
                        let Some(value) = self.shared_variant(
                            runtime_package,
                            operand_u32(code, pc + 1),
                            operand_u32(code, pc + 5),
                        ) else {
                            break;
                        };
                        at!(sp) = value;
                        sp += 1;
                        pc += 9;
                    }
                    OpCode::AddInt | OpCode::SubInt => {
                        let (l, r) = (at!(sp - 2), at!(sp - 1));
                        let result = if op == OpCode::AddInt {
                            Value::tagged_int_add_checked(l, r)
                        } else {
                            Value::tagged_int_sub_checked(l, r)
                        };
                        let Some(result) = result else { break };
                        sp -= 1;
                        at!(sp - 1) = result;
                        pc += 1;
                    }
                    OpCode::MulInt => {
                        let (Some(l), Some(r)) = (at!(sp - 2).as_int(), at!(sp - 1).as_int())
                        else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(result) = Self::int_mul(l, r) else {
                            break;
                        };
                        sp -= 1;
                        at!(sp - 1) = result;
                        pc += 1;
                    }
                    OpCode::CmpIntEq
                    | OpCode::CmpIntNotEq
                    | OpCode::CmpIntLt
                    | OpCode::CmpIntLtEq
                    | OpCode::CmpIntGt
                    | OpCode::CmpIntGtEq => {
                        let result = Self::int_compare(op, at!(sp - 2), at!(sp - 1));
                        sp -= 1;
                        at!(sp - 1) = Value::bool(result);
                        pc += 1;
                    }
                    OpCode::Not | OpCode::Truthy => {
                        let Some(truthiness) = Self::scalar_truthiness(at!(sp - 1)) else {
                            break;
                        };
                        at!(sp - 1) = Self::truthiness_result(op, truthiness);
                        pc += 1;
                    }
                    // A branch that ticks the early-yield interval leaves the
                    // tick that would elapse it to the full dispatch, which
                    // polls.
                    OpCode::Jump => {
                        if !self.early_yield.tick_unless_due() {
                            break;
                        }
                        pc = Self::branch_target(pc + BRANCH_LEN, operand_i32(code, pc + 1));
                    }
                    OpCode::PopJumpIfFalse | OpCode::PopJumpIfTrue => {
                        if !self.early_yield.tick_unless_due() {
                            break;
                        }
                        sp -= 1;
                        pc = if Self::pop_jump_taken(op, at!(sp)) {
                            Self::branch_target(pc + BRANCH_LEN, operand_i32(code, pc + 1))
                        } else {
                            pc + BRANCH_LEN
                        };
                    }
                    OpCode::JumpIfLocalsCmp | OpCode::JumpIfLocalConstCmp => {
                        let cmp: OpCode = std::mem::transmute(*code.get_unchecked(pc + 1));
                        let l = local!(operand_u32(code, pc + 2));
                        let rhs = operand_u32(code, pc + 6);
                        let r = if op == OpCode::JumpIfLocalsCmp {
                            local!(rhs)
                        } else {
                            let Some(&constant) = constants.get(rhs) else {
                                break;
                            };
                            constant
                        };
                        if !self.early_yield.tick_unless_due() {
                            break;
                        }
                        pc = if Self::int_compare(cmp, l, r) {
                            Self::branch_target(pc + 14, operand_i32(code, pc + 10))
                        } else {
                            pc + 14
                        };
                    }
                    OpCode::JumpIfFalse => {
                        pc = if Self::jump_if_false_taken(at!(sp - 1)) {
                            Self::branch_target(pc + BRANCH_LEN, operand_i32(code, pc + 1))
                        } else {
                            pc + BRANCH_LEN
                        };
                    }
                    OpCode::JumpIfFalseOrPop
                    | OpCode::JumpIfTrueOrPop
                    | OpCode::JumpIfNotNullOrPop => {
                        if !self.early_yield.tick_unless_due() {
                            break;
                        }
                        if Self::jump_or_pop_taken(op, at!(sp - 1)) {
                            pc = Self::branch_target(pc + BRANCH_LEN, operand_i32(code, pc + 1));
                        } else {
                            sp -= 1;
                            pc += BRANCH_LEN;
                        }
                    }
                    // `run_compact`'s `CallExactArgs` arm, for a plain,
                    // non-generic bytecode callee. Both enter the callee
                    // through `enter_exact_callee`, and with telemetry on
                    // record the call through `observe_exact_call`.
                    OpCode::CallExactArgs if CALLS => {
                        if self.pending_call_trace.is_some()
                            || !self.pending_call_type_args.is_empty()
                            || operand_u16(code, pc + 5) != 0
                        {
                            break;
                        }
                        let callee_value = self.load_global_in(
                            runtime_package,
                            bex_vm_types::GlobalIndex::from_raw(operand_u32(code, pc + 1)),
                        );
                        let Some(callee_ptr) = callee_value.as_object_ptr() else {
                            break;
                        };
                        let Object::Function(callee) = callee_ptr.get() else {
                            break;
                        };
                        let Some(callee_code) = callee.bytecode.compact.as_ref() else {
                            break;
                        };
                        let Some(room) = Self::frame_room_within(cap, sp, callee_code) else {
                            break;
                        };
                        if !matches!(callee.kind, FunctionKind::Bytecode)
                            || callee.compiled_body.is_some()
                            || self.hotswap_redirects(callee)
                            || sp < callee.arity
                            || self.frames.len() == self.frames.capacity()
                            // Last check, so a handed-back call has not ticked.
                            || !self.early_yield.tick_unless_due()
                        {
                            break;
                        }
                        self.thread_local_call = false;
                        let locals_offset = sp - callee.arity;
                        self.stack.set_len(sp);
                        if self.telemetry.is_some() {
                            // Everything the loop keeps in registers is
                            // reloaded after this call, so none of it lives
                            // across the call in a callee-saved register.
                            DispatchState {
                                pc,
                                code,
                                function,
                                frame_idx,
                                locals_base,
                                runtime_package,
                            } = self.enter_observed_exact_callee(
                                frame_idx,
                                pc,
                                pc + CALL_EXACT_LEN,
                                callee_ptr,
                                callee,
                                StackIndex::from_raw(locals_offset),
                                room,
                            );
                            (base, sp, cap) = self.stack.raw_parts();
                        } else {
                            self.enter_exact_callee(
                                pc,
                                pc + CALL_EXACT_LEN,
                                callee_ptr,
                                callee,
                                StackIndex::from_raw(locals_offset),
                                None,
                                room,
                            );
                            sp = self.stack.len();
                            frame_idx += 1;
                            function = callee;
                            runtime_package = self.runtime_package_of(function);
                            code = &callee_code.code;
                            locals_base = locals_offset.wrapping_sub(1);
                            pc = 0;
                        }
                        constants = &function.bytecode.resolved_constants;
                    }
                    // `run_compact`'s `Return` arm, back into a plain bytecode
                    // caller, with no model usage or trace scope to settle.
                    // Both share `leave_returning_frame_slots` and
                    // `resume_state`, and with telemetry on it runs out of
                    // line in `return_observed_to_caller`; only the frame pop
                    // differs (see below).
                    OpCode::Return if CALLS => {
                        if frame_idx == 0
                            || self.trace_scope_frame == frame_idx
                            || matches!(
                                function.body_meta,
                                Some(bex_vm_types::FunctionMeta::ModelInvoke)
                            )
                        {
                            break;
                        }
                        let (Some(Frame::Bytecode(caller)), Some(Frame::Bytecode(returning))) =
                            (self.frames.get(frame_idx - 1), self.frames.get(frame_idx))
                        else {
                            break;
                        };
                        // Only a frame that owns nothing but its caller's
                        // context is popped here; see the pop below. Its
                        // receipt is `Copy`.
                        let observed = self.telemetry.is_some();
                        if (returning.telemetry.is_some() && !observed)
                            || !returning.type_args.owns_nothing()
                            || !returning.context.same_version(&caller.context)
                        {
                            break;
                        }
                        if !matches!(caller.function.get(), Object::Function(_))
                            || !self.early_yield.tick_unless_due()
                        {
                            break;
                        }
                        let returned_locals = returning.locals_offset;
                        self.stack.set_len(sp);
                        // Both ways, everything the loop keeps in registers
                        // is reloaded after the frame is popped, so none of
                        // it lives across a call in a callee-saved register:
                        // the telemetry call, or the drop of the context's
                        // last reference (which the caller's makes
                        // impossible, but the compiler cannot know).
                        DispatchState {
                            pc,
                            code,
                            function,
                            frame_idx,
                            locals_base,
                            runtime_package,
                        } = if observed {
                            self.return_observed_to_caller(frame_idx, function)
                        } else {
                            self.leave_returning_frame_slots(returned_locals);
                            // KEEP IN SYNC with `pop_finished_bytecode_frame`,
                            // which the `Return` arm uses: the same pop, minus
                            // the drop of the type arguments, which checked
                            // above allocate nothing (the other fields are
                            // `Copy`).
                            self.pop_frame_sharing_caller_context();
                            self.resume_top_bytecode_frame()
                        };
                        (base, sp, cap) = self.stack.raw_parts();
                        constants = &function.bytecode.resolved_constants;
                    }
                    _ => break,
                }
                #[cfg(feature = "kperf")]
                {
                    self.op_count += 1;
                    crate::op_mix::record(op as u8);
                }
            }
            self.stack.set_len(sp);
        }
        *state = DispatchState {
            pc,
            code,
            function,
            frame_idx,
            locals_base,
            runtime_package,
        };
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
        // Each reader is one unaligned load: assembling the value from single
        // bytes left LLVM emitting one `ldrb` per byte on some arms.
        #[inline(always)]
        unsafe fn read_u32_unchecked(code: &[u8], pc: &mut usize) -> u32 {
            unsafe {
                let p = *pc;
                debug_assert!(p + 4 <= code.len());
                *pc = p + 4;
                u32::from_le(code.as_ptr().add(p).cast::<u32>().read_unaligned())
            }
        }

        #[inline(always)]
        unsafe fn read_u16_unchecked(code: &[u8], pc: &mut usize) -> u16 {
            unsafe {
                let p = *pc;
                debug_assert!(p + 2 <= code.len());
                *pc = p + 2;
                u16::from_le(code.as_ptr().add(p).cast::<u16>().read_unaligned())
            }
        }

        #[inline(always)]
        unsafe fn read_i32_unchecked(code: &[u8], pc: &mut usize) -> i32 {
            unsafe {
                let p = *pc;
                debug_assert!(p + 4 <= code.len());
                *pc = p + 4;
                i32::from_le(code.as_ptr().add(p).cast::<i32>().read_unaligned())
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

        // An operand push of the running instruction, into the capacity its
        // frame reserved on entry (`Function::frame_reserve`): no capacity
        // check. The load-time stack check proves the bound.
        macro_rules! push_operand {
            ($value:expr) => {{
                let value = $value;
                // SAFETY: see `EvalStack::push_reserved`.
                unsafe { self.stack.push_reserved(value) };
            }};
        }

        // The current function's runtime package, derived once per function
        // switch rather than on every instruction that addresses an operand.
        let mut runtime_package = self.runtime_package_of(function);
        // The current frame's `locals_offset - 1`, so local `slot` (1-based)
        // lives at stack index `locals_base + slot`. Locals never move while a
        // frame runs, so this stays in a register instead of re-reading the
        // frame on every local access; it is refreshed wherever dispatch
        // switches frames without returning.
        let mut locals_base = self.frame_locals_base(*frame_idx);

        loop {
            // Run straight-line scalar code in the register-resident fast
            // path; it stops before the first instruction it does not run,
            // which the match below then executes.
            // SAFETY: PC is always in bounds (bytecode invariant).
            #[allow(unsafe_code)]
            let mut next = unsafe { *code.get_unchecked(*pc) };
            if FAST_PATH_OPS[usize::from(next)] && self.dispatch_fast_path {
                let mut state = DispatchState {
                    pc: *pc,
                    code,
                    function,
                    frame_idx: *frame_idx,
                    locals_base,
                    runtime_package,
                };
                // Straight-line code runs in the variant without calls: it
                // is a leaf, so entering it is cheap for code that keeps
                // switching between it and the match below. A call or
                // return, reached directly or where that variant stops,
                // enters the variant that switches frames, which then keeps
                // running call-heavy code without coming back here.
                let is_frame_switch =
                    |byte: u8| byte == OpCode::CallExactArgs as u8 || byte == OpCode::Return as u8;
                if !is_frame_switch(next) {
                    self.run_fast_path::<false>(&mut state);
                    *pc = state.pc;
                    next = unsafe { *code.get_unchecked(*pc) };
                }
                if is_frame_switch(next) {
                    self.run_fast_path::<true>(&mut state);
                    // It may have entered or left frames; it stops at an
                    // instruction boundary of whichever frame is on top.
                    *pc = state.pc;
                    *code = state.code;
                    *function = state.function;
                    *frame_idx = state.frame_idx;
                    *dispatch_frame_idx = state.frame_idx;
                    locals_base = state.locals_base;
                    runtime_package = state.runtime_package;
                }
            }

            // Read opcode byte and advance PC past it.
            // SAFETY: PC is always in bounds (bytecode invariant).
            #[allow(unsafe_code)]
            let op_byte = unsafe { *code.get_unchecked(*pc) };
            *pc += 1;
            #[cfg(feature = "kperf")]
            {
                self.op_count += 1;
                crate::op_mix::record(op_byte);
            }

            // Record the innermost frame's current instruction start cheaply (one
            // flat field store), instead of writing the frame's `faulting_pc` every
            // op (which needs a bounds-checked index + enum match + store). Outer
            // frames record their call-site PC at call time; read sites resolve the
            // innermost frame from `cur_pc`.
            self.cur_pc = *pc - 1;
            #[cfg(debug_assertions)]
            self.debug_check_dispatch(*frame_idx, function, *pc - 1, op_byte);

            // SAFETY: OpCode is #[repr(u8)] and the compact bytecode is produced by our
            // own encoder which only emits valid opcode bytes.
            #[allow(unsafe_code)]
            let op: OpCode = unsafe { std::mem::transmute(op_byte) };

            // Tagged-int comparisons skip untagging by comparing bits directly
            // (see `Value::tagged_int_add_checked` for the encoding rationale; the
            // shift-left-by-1 preserves signed ordering between operands that
            // share the same tag bit). Float comparisons read two floats (inline
            // or boxed) and apply the operator through `float_order` — BAML's total
            // float order, so these stay identical to the `exec_cmpop` fallback and
            // to `baml.ops.Compare for float`. Both operands are guaranteed Float by
            // the bytecode encoder, so the `else` arms are unreachable.
            macro_rules! cmp_float_op {
                ($op:expr) => {{
                    let Some(r) = self
                        .stack
                        .get_at(self.stack.ensure_slot_from_top(0))
                        .as_float()
                    else {
                        std::hint::unreachable_unchecked()
                    };
                    let Some(l) = self
                        .stack
                        .get_at(self.stack.ensure_slot_from_top(1))
                        .as_float()
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
                        push_operand!(Value::NULL);
                    }
                    OpCode::LoadTrue => {
                        push_operand!(Value::bool(true));
                    }
                    OpCode::LoadFalse => {
                        push_operand!(Value::bool(false));
                    }
                    OpCode::LoadIntSmall => {
                        let val = { read_i8_unchecked(code, pc) };
                        push_operand!(Value::int(i64::from(val)));
                    }

                    // ── LoadConst ─────────────────────────────────────────────────
                    OpCode::LoadConst => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let val = function.bytecode.resolved_constants[idx];
                        push_operand!(val);
                    }

                    // ── LoadVar / StoreVar ────────────────────────────────────────
                    OpCode::LoadVar => {
                        let slot = { read_u32_unchecked(code, pc) as usize };
                        let value = self.stack.get_at(Self::local_at(locals_base, slot));
                        push_operand!(value);
                    }

                    OpCode::StoreVar => {
                        let slot = { read_u32_unchecked(code, pc) as usize };
                        let value = self.stack.ensure_pop();
                        self.store_local_value(Self::local_at(locals_base, slot), value);
                    }

                    OpCode::ArrayIterNext | OpCode::ArrayIterNextLocal => {
                        let iterator = { read_u32_unchecked(code, pc) as usize };
                        let index = { read_u32_unchecked(code, pc) as usize };
                        let iterator = Self::local_at(locals_base, iterator);
                        let index = Self::local_at(locals_base, index);
                        let array = self.stack.get_at(iterator);
                        let Some(i) = self.stack.get_at(index).as_int() else {
                            return Err(VmInternalError::TypeError {
                                expected: bex_vm_types::types::Type::Int,
                                got: self.type_of(&self.stack.get_at(index)),
                            }
                            .into());
                        };
                        let ptr = self.as_object_ptr(array, ObjectType::Array)?;
                        // SAFETY (`_LOCAL`): see `ContainerLenLocal`.
                        let element = match self.get_object(ptr) {
                            Object::Array(arr) => {
                                let guard = if op == OpCode::ArrayIterNextLocal {
                                    arr.lock_thread_local()
                                } else {
                                    arr.lock()
                                };
                                usize::try_from(i).ok().and_then(|i| guard.get(i).copied())
                            }
                            other => {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Array.into(),
                                    got: ObjectType::of(other).into(),
                                }
                                .into());
                            }
                        };
                        let item = match element {
                            Some(element) => {
                                self.stack.set_at(index, Value::int(i + 1));
                                element
                            }
                            None => {
                                let done = self.lang_object(crate::lang_items::BAML_ITER_DONE);
                                Value::object(self.alloc_instance(done, Vec::new()))
                            }
                        };
                        push_operand!(item);
                    }

                    // ── Operand-movement superinstructions (CPython-style) ────────
                    // LoadVar2(a, b) == `LoadVar(a); LoadVar(b)`: push both locals.
                    OpCode::LoadVar2 => {
                        let a = { read_u32_unchecked(code, pc) as usize };
                        let b = { read_u32_unchecked(code, pc) as usize };
                        let va = self.stack.get_at(Self::local_at(locals_base, a));
                        let vb = self.stack.get_at(Self::local_at(locals_base, b));
                        push_operand!(va);
                        push_operand!(vb);
                    }
                    // StoreVar2(a, b) == `StoreVar(a); StoreVar(b)`: pop TOS into
                    // local[a], then pop into local[b].
                    OpCode::StoreVar2 => {
                        let a = { read_u32_unchecked(code, pc) as usize };
                        let b = { read_u32_unchecked(code, pc) as usize };
                        let sa = Self::local_at(locals_base, a);
                        let sb = Self::local_at(locals_base, b);
                        let va = self.stack.ensure_pop();
                        let vb = self.stack.ensure_pop();
                        self.store_local_value(sa, va);
                        self.store_local_value(sb, vb);
                    }

                    OpCode::StoreVarLoadVar => {
                        let slot = { read_u32_unchecked(code, pc) as usize };
                        let local_var_index = Self::local_at(locals_base, slot);
                        let value_slot = self.stack.ensure_slot_from_top(0);
                        let value = self.stack[value_slot];
                        self.store_local_value(local_var_index, value);
                    }

                    // ── Local-operand superinstructions ───────────────────────────
                    // AddLocalSmall(dst, src, k) == `LoadVar(src); LoadIntSmall(k);
                    // AddInt; StoreVar(dst)`, overflow included: that sequence
                    // also leaves the stack as it found it when it raises.
                    OpCode::AddLocalSmall => {
                        let dst = { read_u32_unchecked(code, pc) as usize };
                        let src = { read_u32_unchecked(code, pc) as usize };
                        let r = Value::int(i64::from(read_i8_unchecked(code, pc)));
                        let l = self.stack.get_at(Self::local_at(locals_base, src));
                        let Some(sum) = Value::tagged_int_add_checked(l, r) else {
                            return Err(self.tagged_int_overflow(l, '+', r));
                        };
                        self.store_local_value(Self::local_at(locals_base, dst), sum);
                    }
                    // LoadVarField(slot, i) == `LoadVar(slot); LoadField(i)`.
                    OpCode::LoadVarField | OpCode::LoadVarFieldLocal => {
                        let slot = { read_u32_unchecked(code, pc) as usize };
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let site = { read_u16_unchecked(code, pc) as usize };
                        let instance = self.stack.get_at(Self::local_at(locals_base, slot));
                        let thread_local = op == OpCode::LoadVarFieldLocal;
                        let value =
                            match self.site_load(function, site, instance, idx, thread_local) {
                                Some(value) => value,
                                None => self.load_instance_field(instance, idx, thread_local)?,
                            };
                        push_operand!(value);
                    }

                    // ── LoadGlobal / StoreGlobal ──────────────────────────────────
                    OpCode::LoadGlobal => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        // During `$init`, a global whose initializer has not run
                        // yet holds a placeholder: reading it is an
                        // initialization-order failure, reported as one.
                        if runtime_package.is_null() && self.global_is_pending(global_idx) {
                            return Err(
                                VmInternalError::GlobalReadBeforeInit { slot: global_idx }.into()
                            );
                        }
                        let value = self.load_global_in(runtime_package, global_idx);
                        push_operand!(value);
                    }

                    OpCode::StoreGlobal => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let value = self.stack.ensure_pop();
                        // Only valid during `$init`; post-init globals are frozen in `Arc<[Value]>`
                        // and a write here is a VM internal error.
                        self.store_global_in(runtime_package, global_idx, value)?;
                        if runtime_package.is_null()
                            && let Some(pending) = self.pending_globals.as_mut()
                            && let Some(slot) = pending.get_mut(raw as usize)
                        {
                            *slot = false;
                        }
                    }

                    // ── LoadField / StoreField / InitField ────────────────────────
                    // `_LOCAL`: the instance is thread-local (escape analysis),
                    // so the slot needs no ordering.
                    OpCode::LoadField | OpCode::LoadFieldLocal => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let site = { read_u16_unchecked(code, pc) as usize };
                        let top = self.stack.ensure_pop();
                        let thread_local = op == OpCode::LoadFieldLocal;
                        let value = match self.site_load(function, site, top, idx, thread_local) {
                            Some(value) => value,
                            None => self.load_instance_field(top, idx, thread_local)?,
                        };
                        push_operand!(value);
                    }

                    OpCode::StoreField | OpCode::StoreFieldLocal => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let site = { read_u16_unchecked(code, pc) as usize };
                        let new_value = self.stack.ensure_pop();
                        let instance_value = self.stack.ensure_pop();
                        let thread_local = op == OpCode::StoreFieldLocal;
                        if !self.site_store(
                            function,
                            site,
                            instance_value,
                            idx,
                            new_value,
                            thread_local,
                        ) {
                            self.store_instance_field(
                                instance_value,
                                idx,
                                new_value,
                                thread_local,
                            )?;
                        }
                    }

                    OpCode::LoadTupleField => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let top = self.stack.ensure_pop();
                        let obj_ptr = self.as_object_ptr(top, ObjectType::Tuple)?;
                        let load_result = {
                            let Object::Tuple(tuple) = self.get_object(obj_ptr) else {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Tuple.into(),
                                    got: ObjectType::of(self.get_object(obj_ptr)).into(),
                                }
                                .into());
                            };
                            tuple.try_load_slot(idx).ok_or_else(|| tuple.len())
                        };
                        let value = match load_result {
                            Ok(value) => value,
                            Err(length) => {
                                return Err(self.invalid_field_access_error(idx, length));
                            }
                        };
                        push_operand!(value);
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
                        let receiver = self.stack.ensure_pop();
                        let caller =
                            self.static_cache_identity(function, runtime_package, *frame_idx);
                        let slot =
                            self.virtual_field_slot(caller, receiver, iface_value, field_index)?;
                        let value = self.load_instance_field(receiver, slot, false)?;
                        push_operand!(value);
                    }

                    OpCode::VirtualStoreField => {
                        let field_index = { read_u32_unchecked(code, pc) as usize };
                        let iface_value = self.stack.ensure_pop();
                        let new_value = self.stack.ensure_pop();
                        let receiver = self.stack.ensure_pop();
                        let caller =
                            self.static_cache_identity(function, runtime_package, *frame_idx);
                        let slot =
                            self.virtual_field_slot(caller, receiver, iface_value, field_index)?;
                        self.store_instance_field(receiver, slot, new_value, false)?;
                    }

                    OpCode::InitField => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let new_value = self.stack.ensure_pop();
                        let instance_value = self.stack.ensure_pop();
                        self.store_instance_field(instance_value, idx, new_value, false)?;
                        push_operand!(instance_value);
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
                        push_operand!(value);
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
                        push_operand!(Value::object(array_index));
                    }

                    OpCode::AllocTuple => {
                        let size = { read_u32_unchecked(code, pc) as usize };
                        let tuple_ty = self.ensure_pop_type()?;
                        let drain_range = StackIndex::from_raw(self.stack.len() - size)..;
                        let elements: Vec<Value> = self.stack.drain(drain_range).collect();
                        let tuple = Self::tuple_at_type(tuple_ty, elements)?;
                        let tuple_index = self.tlab.alloc_tuple(tuple);
                        self.stack.push(Value::object(tuple_index));
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
                        push_operand!(Value::object(obj_index));
                    }

                    OpCode::ConcatStrings => {
                        let n = { read_u32_unchecked(code, pc) as usize };
                        let first = StackIndex::from_raw(self.stack.len() - n);
                        let result = self.join_strings(&self.stack[first..])?;
                        // SAFETY: the encoder validates the instruction's stack effect.
                        self.stack.truncate_suffix(n);
                        let value = self.alloc_joined_string(result);
                        push_operand!(value);
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

                        if !matches!(self.get_object(class_ptr), Object::Class(_)) {
                            return Err(VmInternalError::TypeError {
                                expected: ObjectType::Class.into(),
                                got: ObjectType::of(self.get_object(class_ptr)).into(),
                            }
                            .into());
                        }
                        let instance = self.tlab.alloc_instance_zeroed(class_ptr, class_type_args);
                        let instance_ptr = self.tlab.instance_ptr(instance);
                        push_operand!(Value::object(instance_ptr));
                    }

                    OpCode::InitInstance => {
                        let plan_idx = { read_u32_unchecked(code, pc) as usize };
                        let instance = self.alloc_initialized_instance(
                            &function.bytecode.class_init_plans[plan_idx],
                        )?;
                        push_operand!(instance);
                    }

                    OpCode::AllocVariant => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let enum_ptr =
                            self.object_in(runtime_package, ObjectIndex::from_raw(raw as usize));
                        let variant = self.stack.ensure_pop();
                        let (variant_count, shared) = {
                            let Object::Enum(enm) = self.get_object(enum_ptr) else {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Enum.into(),
                                    got: ObjectType::of(self.get_object(enum_ptr)).into(),
                                }
                                .into());
                            };
                            #[allow(clippy::cast_sign_loss, clippy::cast_possible_truncation)]
                            let shared = variant
                                .as_int()
                                .and_then(|index| enm.shared_variants.get(index as usize))
                                .copied();
                            (enm.variants.len(), shared)
                        };
                        // A compile-time enum's variants are permanent shared
                        // objects (see `Enum::shared_variants`).
                        if let Some(variant_ptr) = shared {
                            push_operand!(Value::object(variant_ptr));
                            continue;
                        }
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
                        push_operand!(Value::object(variant_ptr));
                    }

                    // `Status.X` in one step: `LoadConst` + `AllocVariant`.
                    OpCode::LoadVariant => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let variant = { read_u32_unchecked(code, pc) };
                        let value = self.variant_value(
                            runtime_package,
                            ObjectIndex::from_raw(raw as usize),
                            Value::int(i64::from(variant)),
                        )?;
                        push_operand!(value);
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
                        let callee_value = self.load_global_in(runtime_package, callee);
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
                            // A compiled body that spawns may be a frame record
                            // above this frame; see `begin_telemetry_wait`.
                            let caller = self.bytecode_caller(*frame_idx);
                            let actual_caller = self.frame_identity(caller);
                            let caller_pc = self.caller_pc(caller, *frame_idx, self.cur_pc);
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
                        push_operand!(value);
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
                                        // `trace.span`'s own defaults, as in
                                        // `TraceHookMode::config`.
                                        inputs: if op == OpCode::TraceHookEmptySpan {
                                            Some(false)
                                        } else {
                                            (op == OpCode::TraceHookRich
                                                || op == OpCode::TraceHookSpan)
                                                .then_some(true)
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
                                    reservation: None,
                                }
                            });
                        self.finish_trace_hook_config(*frame_idx, returned)?;
                    }
                    OpCode::EndTraceHook => {
                        let result = self.stack.ensure_pop();
                        self.finish_trace_hook(*frame_idx, result)?;
                    }
                    // Passing `pc` to an outlined call prevents register allocation
                    // across dispatch, so recover it from the parked frame.
                    OpCode::ResumeCompiled => {
                        let outcome = self.resume_compiled(frame_idx, function);
                        // The fiber may have replaced the frame at this depth
                        // with another function's, so the cached code follows.
                        *code = &function.bytecode.compact.as_ref().unwrap().code;
                        if let Some(Frame::Bytecode(frame)) = self.frames.get(*frame_idx) {
                            *pc = frame.instruction_ptr;
                        }
                        return outcome;
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
                                        | OpCode::CallLocal
                                        | OpCode::CallIndirect
                                        | OpCode::VirtualCall
                                )
                            ),
                            "SetCallTrace must be followed by a call instruction"
                        );
                        let value = self.stack.ensure_pop();
                        self.pending_call_trace = Some(self.trace_config(value)?);
                    }
                    OpCode::Call
                    | OpCode::CallExactArgs
                    | OpCode::CallHooked
                    | OpCode::CallLocal => {
                        // The general call path takes the builtin's twin.
                        self.thread_local_call = op == OpCode::CallLocal;
                        let raw = read_u32_unchecked(code, pc);
                        let ntypeargs = usize::from(read_u16_unchecked(code, pc));

                        let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let callee_value = self.load_global_in(runtime_package, callee_global);

                        // `run_fast_path` runs the plain case of this exact
                        // call itself (non-generic bytecode callee) and hands
                        // every other case back here. A step added to this
                        // path that applies to plain calls must go into
                        // `enter_exact_callee` or `observe_exact_call`, which
                        // both share, or become one of the fast path's
                        // hand-back guards.
                        if op == OpCode::CallExactArgs
                            && self.pending_call_trace.is_none()
                            && self.pending_call_type_args.is_empty()
                        {
                            let callee_ptr =
                                self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                            // SAFETY: the active heap permit prevents collection
                            // during frame/stack growth, just as in general call
                            // setup. Reload from rooted frames after any yield.
                            let callee_object: &'static Object = callee_ptr.get();
                            // A generic callee's type operands sit below its
                            // values. Plain static ones go straight into the
                            // new frame; any other operand takes the general
                            // path, which carries runtime declarations along.
                            // A swapped callee takes the general path too, which
                            // selects its replacement; checked first, before the
                            // type operands are taken.
                            if let Object::Function(callee) = callee_object
                                && matches!(callee.kind, FunctionKind::Bytecode)
                                && !self.hotswap_redirects(callee)
                                && let Some(type_args) = if ntypeargs == 0 {
                                    Some(Vec::new())
                                } else {
                                    self.take_plain_static_type_args(ntypeargs, callee.arity)
                                }
                            {
                                // `call_specialize` routes compiled callees through the general call path.
                                debug_assert!(callee.compiled_body.is_none());
                                // Eager ok_or constructs/drops VmInternalError
                                // even on success; release assembly retains a
                                // destructor call for that large enum.
                                #[expect(clippy::unnecessary_lazy_evaluations)]
                                let locals_offset = StackIndex::from_raw(
                                    self.stack.len().checked_sub(callee.arity).ok_or_else(
                                        || VmInternalError::NotEnoughItemsOnStack(callee.arity),
                                    )?,
                                );
                                self.reserve_call_frame()?;
                                let room = self.reserve_frame_room(callee)?;
                                let frame_telemetry = if self.telemetry.is_some() {
                                    self.observe_exact_call(
                                        *frame_idx,
                                        self.cur_pc,
                                        callee_ptr,
                                        callee,
                                        locals_offset,
                                        &type_args,
                                    )
                                } else {
                                    None
                                };
                                // The fast path enters plain callees the same way.
                                self.enter_exact_callee(
                                    self.cur_pc,
                                    *pc,
                                    callee_ptr,
                                    callee,
                                    locals_offset,
                                    frame_telemetry,
                                    room,
                                );
                                // Attached after the push so the frame of a
                                // non-generic call is built exactly as before.
                                if !type_args.is_empty()
                                    && let Some(Frame::Bytecode(frame)) = self.frames.last_mut()
                                {
                                    frame.type_args = type_args.into();
                                }
                                *frame_idx = self.frames.len() - 1;
                                *function = callee;
                                runtime_package = self.runtime_package_of(function);
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
                                locals_base = locals_offset.raw().wrapping_sub(1);
                                continue;
                            }
                        }

                        // A native builtin skips the general setup: no callee
                        // unwrapping, no layout remap, no copy of its type
                        // arguments. It returns to the engine loop as the
                        // general path does: continuing here instead keeps the
                        // whole loop state live across the call, and the
                        // dispatch loop then spills `self` on every
                        // instruction.
                        if let NativeDirect::Ran(state) = self.call_native_direct(
                            callee_value,
                            op == OpCode::CallLocal,
                            ntypeargs,
                            frame_idx,
                            function,
                            *pc,
                        )? {
                            return Ok(state);
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

                        let result = if ntypeargs == 0 && self.pending_call_type_args.is_empty() {
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
                            self.execute_call_from_locals_offset_with_owned_type_args(
                                callee_ptr,
                                locals_offset,
                                arg_count,
                                type_args.tys,
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

                        // A site whose cache knows the callee enters it as
                        // `CallIndirect` enters a plain bytecode function.
                        if let Some(callee) = self.enter_cached_virtual_call(
                            [nargs, ntypeargs, self_arg],
                            *frame_idx,
                            function,
                            runtime_package,
                            *pc,
                        )? {
                            *frame_idx = self.frames.len() - 1;
                            *function = callee;
                            runtime_package = self.runtime_package_of(function);
                            if self.should_early_yield() {
                                return Ok(Some(VmExecState::EarlyYield));
                            }
                            *pc = 0;
                            *code = &callee.bytecode.compact.as_ref().unwrap().code;
                            *dispatch_frame_idx = *frame_idx;
                            locals_base = self.frame_locals_base(*frame_idx);
                            continue;
                        }

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
                        let iface_ptr = self.as_object_ptr(iface_value, ObjectType::Type)?;
                        let caller =
                            self.static_cache_identity(function, runtime_package, *frame_idx);
                        // Probe the per-site cache with a borrowed query: the
                        // receiver's and the interface's type arguments are read
                        // in place, so a hit allocates nothing beyond the callee
                        // frame's own copy of its type arguments. A miss carries
                        // the owned key out for the insert.
                        let probe = {
                            let mut scratch = None;
                            match self.virtual_site_query(caller, receiver, iface_ptr, &mut scratch)
                            {
                                Some(query) => match self.static_virtual_call_cache.get(&query) {
                                    Some(target) => Ok((
                                        target.callee.ptr(),
                                        Self::seed_frame_type_args(
                                            &target.frame_type_args,
                                            method_type_args.as_ref(),
                                        ),
                                        target.arg_count,
                                    )),
                                    None => Err(Some(query.to_key())),
                                },
                                None => Err(None),
                            }
                        };
                        let (callee_ptr, type_args, known_arg_count) = match probe {
                            Ok(hit) => hit,
                            Err(cache_key) => {
                                let method_name = self.as_string(&method_value)?.to_string();
                                // The interface's input args select among a type's
                                // impls of the same interface at several
                                // instantiations. Associated types are outputs,
                                // not part of the resolver key.
                                let (iface_qtn, iface_args) =
                                    self.interface_operand_parts(iface_ptr);
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
                                    .resolve_implementation(&self_ty, iface_qtn, iface_args)
                                    .ok_or_else(|| VmInternalError::UnresolvedVirtualCall {
                                        method: method_name.clone(),
                                    })?;
                                let (callee, frame) = resolver
                                    .implementation_method(&implementation, method_name.as_str())?;
                                let cacheable = implementation.is_static();
                                // The receiver's class-level slots are realized by the
                                // resolver off `Self` as head-carrying types, so
                                // `reflect.Type.of<T>()` in an impl or default-method
                                // body dereferences the caller's own declaration rather
                                // than deriving a fresh identity for it.
                                let type_args =
                                    Self::seed_frame_type_args(&frame, method_type_args.as_ref());
                                if cacheable && let Some(cache_key) = cache_key {
                                    let arg_count =
                                        match Self::recorded_call_layout(function, self.cur_pc) {
                                            None => Some(nargs),
                                            Some(layout) => self
                                                .callee_params(callee)?
                                                .layout_is(layout)
                                                .then(|| layout.len()),
                                        };
                                    let shared_frame_type_args =
                                        self.shareable_frame_type_args(&frame);
                                    self.static_virtual_call_cache.insert(
                                        cache_key,
                                        StaticVirtualCallTarget {
                                            callee: Weak::new(callee),
                                            frame_type_args: frame,
                                            shared_frame_type_args,
                                            arg_count,
                                        },
                                    );
                                }
                                (callee, type_args, None)
                            }
                        };
                        let nargs = match known_arg_count {
                            Some(count) => count,
                            None => match Self::recorded_call_layout(function, self.cur_pc) {
                                Some(layout) => self.remap_call_arguments(layout, callee_ptr)?,
                                None => nargs,
                            },
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
                        {
                            self.execute_call_from_locals_offset(
                                callee_ptr,
                                locals_offset,
                                nargs,
                                frame_idx,
                                function,
                            )
                        } else {
                            self.execute_call_from_locals_offset_with_owned_type_args(
                                callee_ptr,
                                locals_offset,
                                nargs,
                                type_args,
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
                        // Bytecode functions and closures stay in compact
                        // dispatch, as at `CallExactArgs`.
                        if let Some(callee) =
                            self.enter_indirect_bytecode_call(callee_ptr, *frame_idx, function)?
                        {
                            *frame_idx = self.frames.len() - 1;
                            *function = callee;
                            runtime_package = self.runtime_package_of(function);
                            if self.should_early_yield() {
                                return Ok(Some(VmExecState::EarlyYield));
                            }
                            *pc = 0;
                            *code = &callee.bytecode.compact.as_ref().unwrap().code;
                            *dispatch_frame_idx = *frame_idx;
                            locals_base = self.frame_locals_base(*frame_idx);
                            continue;
                        }
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
                            self.stack
                                .try_insert(args_offset, receiver)
                                .map_err(|panic| {
                                    VmError::thrown_fresh(self.panic_to_exception_value(panic))
                                })?;
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
                    // `run_fast_path` runs the plain case of this return
                    // itself (bytecode caller, no model usage, trace scope or
                    // owned frame state) and hands every other case back
                    // here. A step added here that applies to plain returns
                    // must go into the helpers both share
                    // (`leave_returning_frame_slots`, `resume_state`), into
                    // the fast path's telemetry return
                    // (`return_observed_to_caller`), or
                    // become one of the fast path's hand-back guards.
                    OpCode::Return => {
                        let result = self.stack.get_at(self.stack.ensure_stack_top());

                        let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                            unreachable!()
                        };
                        let locals_offset = bf.locals_offset;
                        let frame_telemetry = bf.telemetry.take();

                        // Charged before the frame completes, so an invoke that
                        // is a span carries its own usage.
                        if matches!(
                            function.body_meta,
                            Some(bex_vm_types::FunctionMeta::ModelInvoke)
                        ) {
                            self.charge_model_usage(result);
                        }
                        if let Some(frame_telemetry) = frame_telemetry {
                            self.complete_bytecode_invocation_with_function(
                                *frame_idx,
                                function,
                                frame_telemetry,
                                InvocationOutcome::Ok,
                                Some(result),
                            );
                        } else {
                            self.end_manual_activation(*frame_idx);
                        }

                        // The fast path's `Return` shares this step and the
                        // caller's `resume_state` below.
                        self.leave_returning_frame_slots(locals_offset);
                        // KEEP IN SYNC with the fast path's `Return`, which
                        // pops a frame that owns nothing without the drops.
                        self.pop_finished_bytecode_frame();
                        if self.telemetry.is_some() {
                            let context = self.current_context().clone();
                            if let Some(telemetry) = &mut self.telemetry {
                                telemetry.set_context(context);
                            }
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
                            let caller_function = match caller.function.get() {
                                Object::Function(caller_fn) => caller_fn,
                                _ => self.load_function(*frame_idx)?,
                            };
                            let next = self.resume_state(*frame_idx, caller_function);
                            *pc = next.pc;
                            *code = next.code;
                            *function = next.function;
                            locals_base = next.locals_base;
                            runtime_package = next.runtime_package;
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
                        let checkpoint_passed = std::mem::take(&mut self.checkpoint_passed);
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
                            // A settled input ends the wait, not the suspension
                            // point: the engine delivers the thread's own
                            // cancellation first. Without this, a task that
                            // reached the opcode after both its cancellation and
                            // an input's settlement would take the input.
                            Some(_) if !checkpoint_passed => {
                                *pc -= AWAIT_ANY_OPCODE_LEN;
                                return Ok(Some(VmExecState::Checkpoint));
                            }
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
                        *pc = Self::branch_target(*pc, offset);
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                    }

                    OpCode::PopJumpIfFalse | OpCode::PopJumpIfTrue => {
                        let offset = read_i32_unchecked(code, pc);
                        let cond = self.stack.ensure_pop();
                        if Self::pop_jump_taken(op, cond) {
                            *pc = Self::branch_target(*pc, offset);
                        }
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                    }

                    // JumpIfLocalsCmp(cmp, a, b) == `LoadVar2(a, b); CmpInt*;
                    // PopJumpIfTrue`, with a `PopJumpIfFalse` stored as the
                    // negated comparison; the const form loads constant `b`.
                    OpCode::JumpIfLocalsCmp | OpCode::JumpIfLocalConstCmp => {
                        let cmp: OpCode = std::mem::transmute(*code.get_unchecked(*pc));
                        *pc += 1;
                        let a = { read_u32_unchecked(code, pc) as usize };
                        let b = { read_u32_unchecked(code, pc) as usize };
                        let offset = read_i32_unchecked(code, pc);
                        let l = self.stack.get_at(Self::local_at(locals_base, a));
                        let r = if op == OpCode::JumpIfLocalsCmp {
                            self.stack.get_at(Self::local_at(locals_base, b))
                        } else {
                            function.bytecode.resolved_constants[b]
                        };
                        if Self::int_compare(cmp, l, r) {
                            *pc = Self::branch_target(*pc, offset);
                        }
                        if self.should_early_yield() {
                            return Ok(Some(VmExecState::EarlyYield));
                        }
                    }

                    OpCode::JumpIfFalse => {
                        let offset = read_i32_unchecked(code, pc);
                        let top_slot = self.stack.ensure_stack_top();
                        let cond = self.stack[top_slot];
                        if Self::jump_if_false_taken(cond) {
                            *pc = Self::branch_target(*pc, offset);
                        }
                    }

                    OpCode::JumpIfFalseOrPop
                    | OpCode::JumpIfTrueOrPop
                    | OpCode::JumpIfNotNullOrPop => {
                        let offset = read_i32_unchecked(code, pc);
                        let cond = self.stack[self.stack.ensure_stack_top()];
                        if Self::jump_or_pop_taken(op, cond) {
                            *pc = Self::branch_target(*pc, offset);
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
                        let discriminant = variant_index as i64;
                        push_operand!(Value::int(discriminant));
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
                        push_operand!(Value::bool(result));
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
                        push_operand!(Value::bool(matched));
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
                        push_operand!(Value::int(arm.map_or(-1, i64::from)));
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
                        push_operand!(Value::object(ptr));
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
                        push_operand!(Value::object(ptr));
                    }

                    // ── LoadType ──────────────────────────────────────────────────
                    OpCode::LoadType => {
                        let idx = { read_u32_unchecked(code, pc) as usize };
                        let identity =
                            self.static_cache_identity(function, runtime_package, *frame_idx);
                        let cached = identity
                            .and_then(|identity| self.cached_load_type(identity, idx, *frame_idx));
                        let value = match cached {
                            Some(ty) => Value::object(ty),
                            None => self.load_type_uncached(function, identity, idx, *frame_idx)?,
                        };
                        push_operand!(value);
                    }

                    // ── `let type T extends I = …` bound check ──────────────────
                    OpCode::SatisfiesBound => {
                        let bound = self.stack.ensure_pop();
                        let value = self.stack.ensure_pop();
                        let satisfied = self.satisfies_bound(value, bound)?;
                        push_operand!(Value::bool(satisfied));
                    }

                    // ── Lexical runtime type binding ────────────────────────────
                    OpCode::BindType => {
                        let slot = read_u32_unchecked(code, pc) as usize;
                        let value = self.stack.ensure_pop();
                        let type_value = self.type_operand_value(value)?;
                        let Frame::Bytecode(frame) = &mut self.frames[*frame_idx] else {
                            unreachable!("compact bytecode runs in a bytecode frame")
                        };
                        let type_args = frame.type_args.to_mut();
                        type_args.resize(slot + 1, bex_vm_types::RealizedTy::unknown());
                        type_args[slot] = type_value.ty;
                    }
                    // ── Package.current() ───────────────────────────────────────
                    OpCode::LoadCurrentPackage => {
                        let ordinal = read_u32_unchecked(code, pc) as usize;
                        let value =
                            crate::package_reflect::reflect::current_package_value(self, ordinal);
                        push_operand!(value);
                    }

                    // ── MakeBoundMethod ───────────────────────────────────────────
                    OpCode::MakeBoundMethod => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let receiver = self.stack.ensure_pop();
                        let callee_value = self.load_global_in(runtime_package, global_idx);
                        let function_ptr =
                            self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                        // Curry the receiver's type args (→ `Self`) into the value
                        // now, so the bound method is fully realized and the
                        // `CallIndirect` that invokes it needs no type-arg operands.
                        // A later explicit method specialization (`b.m<int>`)
                        // appends its arguments in MakeGenericFunctionFromValue.
                        let type_args = self.receiver_type_args(receiver)?;
                        let bound = Object::BoundMethod(BoundMethod {
                            function: function_ptr,
                            receiver,
                            type_args,
                        });
                        let ptr = self.tlab.alloc(bound);
                        push_operand!(Value::object(ptr));
                    }

                    // ── MakeGenericBoundMethod ────────────────────────────────────
                    // The bound method whose frame the reference site supplies:
                    // an interface's default body bound to `self` (`default.m`
                    // as a value), whose `[Self, interface args..]` frame the
                    // receiver's class args cannot express.
                    // Stack (top last): `[type_args…, receiver]`.
                    OpCode::MakeGenericBoundMethod => {
                        let raw = { read_u32_unchecked(code, pc) };
                        let global_idx = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                        let ntypeargs = { read_u16_unchecked(code, pc) as usize };
                        let receiver = self.stack.ensure_pop();
                        let type_args = self.pop_type_args(ntypeargs)?;
                        let callee_value = self.load_global_in(runtime_package, global_idx);
                        let function_ptr =
                            self.as_object_ptr(callee_value, FunctionType::Callable.into())?;
                        let bound = Object::BoundMethod(BoundMethod {
                            function: function_ptr,
                            receiver,
                            type_args: type_args.tys.into_boxed_slice(),
                        });
                        let ptr = self.tlab.alloc(bound);
                        push_operand!(Value::object(ptr));
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
                        push_operand!(Value::object(ptr));
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
                        push_operand!(Value::object(ptr));
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
                            runtime_package,
                        });
                        let ptr = self.tlab.alloc(gf);
                        push_operand!(Value::object(ptr));
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
                        // A bound method must retain its receiver as well as its
                        // class/impl frame. Specialize a fresh value so aliases of
                        // the original method keep their own generic environment.
                        if let Object::BoundMethod(method) = self.get_object(callable_ptr) {
                            let mut method = method.clone();
                            let mut carried = method.type_args.into_vec();
                            carried.extend(type_args.tys);
                            method.type_args = carried.into_boxed_slice();
                            let ptr = self.tlab.alloc(Object::BoundMethod(method));
                            push_operand!(Value::object(ptr));
                            continue;
                        }
                        // `GenericFunction` does not reach here: TIR rejects
                        // type args on an already-specialized value.
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
                                push_operand!(Value::object(ptr));
                            }
                            None => push_operand!(callable),
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
                        push_operand!(cell.load());
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
                        push_operand!(cell.load());
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
                        push_operand!(closure.captures[idx]);
                    }

                    // ── Array / Map element ops ───────────────────────────────────
                    OpCode::ContainerLen | OpCode::ContainerLenLocal => {
                        // SAFETY (`thread_local`): the emitter selects the
                        // `_LOCAL` opcodes only where escape analysis proved
                        // no other thread can reach the container.
                        let thread_local = op == OpCode::ContainerLenLocal;
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
                            Object::Array(arr) if thread_local => {
                                arr.lock_thread_local().len() as i64
                            }
                            Object::Uint8Array(bytes) if thread_local => {
                                bytes.lock_thread_local().len() as i64
                            }
                            Object::Map(map) if thread_local => {
                                map.lock_thread_local().len() as i64
                            }
                            Object::Array(arr) => arr.len() as i64,
                            Object::Uint8Array(bytes) => bytes.len() as i64,
                            Object::Tuple(tuple) => tuple.len() as i64,
                            Object::Map(map) => map.len() as i64,
                            Object::String(s) => s.byte_len() as i64,
                            other => {
                                return Err(VmInternalError::TypeError {
                                    expected: ObjectType::Array.into(),
                                    got: ObjectType::of(other).into(),
                                }
                                .into());
                            }
                        };
                        push_operand!(Value::int(len));
                    }

                    OpCode::LoadArrayElement | OpCode::LoadArrayElementLocal => {
                        // SAFETY (`thread_local`): see `ContainerLenLocal`.
                        let thread_local = op == OpCode::LoadArrayElementLocal;
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
                                    let guard = if thread_local {
                                        arr.lock_thread_local()
                                    } else {
                                        arr.lock()
                                    };
                                    let len = guard.len();
                                    match crate::array_index::resolve_index(i, len) {
                                        Some(idx) => Ok(guard[idx]),
                                        None => Err((i, len)),
                                    }
                                }
                                Object::Uint8Array(bytes) => {
                                    let guard = if thread_local {
                                        bytes.lock_thread_local()
                                    } else {
                                        bytes.lock()
                                    };
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
                        push_operand!(element);
                    }

                    OpCode::LoadMapElement => {
                        unreachable!("map indexing must be lowered through baml.Map.index");
                    }

                    OpCode::StoreArrayElement | OpCode::StoreArrayElementLocal => {
                        // SAFETY (`thread_local`): see `ContainerLenLocal`.
                        let thread_local = op == OpCode::StoreArrayElementLocal;
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
                                    let debt = self.tlab.alloc_debt();
                                    let mut guard = if thread_local {
                                        arr.lock_mut_thread_local(debt)
                                    } else {
                                        arr.lock_mut(debt)
                                    };
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
                                    let debt = self.tlab.alloc_debt();
                                    let mut guard = if thread_local {
                                        bytes.lock_mut_thread_local(debt)
                                    } else {
                                        bytes.lock_mut(debt)
                                    };
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
                        let Some(v) = Self::int_mul(l, r) else {
                            return Err(self.integer_overflow(format!("{l} * {r} overflows int")));
                        };
                        push_operand!(v);
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
                        push_operand!(v);
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
                        push_operand!(v);
                    }

                    // ── Specialized float arithmetic (skip type dispatch) ─────────
                    OpCode::AddFloat => {
                        let Some(r) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(l) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let v = self.float_value(l + r);
                        push_operand!(v);
                    }
                    OpCode::SubFloat => {
                        let Some(r) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(l) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let v = self.float_value(l - r);
                        push_operand!(v);
                    }
                    OpCode::MulFloat => {
                        let Some(r) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(l) = self.stack.ensure_pop().as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let v = self.float_value(l * r);
                        push_operand!(v);
                    }
                    OpCode::DivFloat => {
                        // IEEE-754: a zero divisor yields `±inf`, and `0.0 / 0.0`
                        // yields `NaN`. Unlike `int` and `bigint`, `float` has
                        // values for those results, so there is nothing to panic
                        // about.
                        let right_v = self.stack.ensure_pop();
                        let left_v = self.stack.ensure_pop();
                        let Some(r) = right_v.as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let Some(l) = left_v.as_float() else {
                            std::hint::unreachable_unchecked()
                        };
                        let v = self.float_value(l / r);
                        push_operand!(v);
                    }

                    // ── Specialized int comparison (skip type dispatch) ───────────
                    OpCode::CmpIntEq
                    | OpCode::CmpIntNotEq
                    | OpCode::CmpIntLt
                    | OpCode::CmpIntLtEq
                    | OpCode::CmpIntGt
                    | OpCode::CmpIntGtEq => {
                        let r = self.stack.get_at(self.stack.ensure_slot_from_top(0));
                        let l = self.stack.get_at(self.stack.ensure_slot_from_top(1));
                        // SAFETY: the encoder validates two comparison operands.
                        self.stack
                            .replace_top_n::<2>(Value::bool(Self::int_compare(op, l, r)));
                    }

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
                        push_operand!(Value::bool(result));
                    }
                    OpCode::CmpBigintNotEq => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let result = self.bigint_cmp(CmpOp::NotEq, l, r);
                        push_operand!(Value::bool(result));
                    }
                    OpCode::CmpBigintLt => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let result = self.bigint_cmp(CmpOp::Lt, l, r);
                        push_operand!(Value::bool(result));
                    }
                    OpCode::CmpBigintLtEq => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let result = self.bigint_cmp(CmpOp::LtEq, l, r);
                        push_operand!(Value::bool(result));
                    }
                    OpCode::CmpBigintGt => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let result = self.bigint_cmp(CmpOp::Gt, l, r);
                        push_operand!(Value::bool(result));
                    }
                    OpCode::CmpBigintGtEq => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let result = self.bigint_cmp(CmpOp::GtEq, l, r);
                        push_operand!(Value::bool(result));
                    }

                    // ── Specialized bigint arithmetic (skip type dispatch) ────────
                    OpCode::AddBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Add, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::SubBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Sub, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::MulBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Mul, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::DivBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Div, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::ModBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Mod, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::BitAndBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::BitAnd, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::BitOrBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::BitOr, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::BitXorBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::BitXor, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::ShlBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Shl, l, r)?;
                        push_operand!(value);
                    }
                    OpCode::ShrBigint => {
                        let r = self.pop_bigint_operand();
                        let l = self.pop_bigint_operand();
                        let value = self.bigint_binop(BinOp::Shr, l, r)?;
                        push_operand!(value);
                    }

                    // ── Expanded unary ────────────────────────────────────────────
                    // `!` negates TRUTHINESS (B-1563) - `!0` and `if (0)`
                    // agree, closing B-1071's asymmetry.
                    OpCode::Not | OpCode::Truthy => {
                        let val = self.stack.get_at(self.stack.ensure_stack_top());
                        let truthy = self.is_truthy(val);
                        // SAFETY: the encoder validates one input value.
                        self.stack
                            .replace_top_n::<1>(Self::truthiness_result(op, truthy));
                    }
                    OpCode::IsNull => {
                        let val = self.stack.get_at(self.stack.ensure_stack_top());
                        // SAFETY: the encoder validates one input value.
                        self.stack.replace_top_n::<1>(Value::bool(val.is_null()));
                    }
                    OpCode::IsNotNull => {
                        let val = self.stack.get_at(self.stack.ensure_stack_top());
                        // SAFETY: the encoder validates one input value.
                        self.stack.replace_top_n::<1>(Value::bool(!val.is_null()));
                    }
                    // `==`: decide the pair in place when no driver is needed,
                    // otherwise call the driver exactly as `Call` would.
                    OpCode::CallEquals => {
                        let raw = read_u32_unchecked(code, pc);
                        let b = self.stack.get_at(self.stack.ensure_slot_from_top(0));
                        let a = self.stack.get_at(self.stack.ensure_slot_from_top(1));
                        if let Some(equal) = crate::package_baml::equals_without_driver(self, a, b)
                        {
                            // SAFETY: the encoder validates two comparison operands.
                            self.stack.replace_top_n::<2>(Value::bool(equal));
                        } else {
                            let callee_global = bex_vm_types::GlobalIndex::from_raw(raw as usize);
                            let callee_value = self.load_global_in(runtime_package, callee_global);
                            let (callee_ptr, arg_count) =
                                self.resolve_callable_target(callee_value)?;
                            let locals_offset = StackIndex::from_raw(
                                self.stack
                                    .len()
                                    .checked_sub(arg_count)
                                    .ok_or(VmInternalError::NotEnoughItemsOnStack(arg_count))?,
                            );
                            let Frame::Bytecode(bf) = &mut self.frames[*frame_idx] else {
                                verifier_unreachable!()
                            };
                            bf.instruction_ptr = *pc;
                            return if self.pending_call_type_args.is_empty() {
                                self.execute_call_from_locals_offset(
                                    callee_ptr,
                                    locals_offset,
                                    arg_count,
                                    frame_idx,
                                    function,
                                )
                            } else {
                                let type_args = self.take_type_args_below_values(0, arg_count)?;
                                self.execute_call_from_locals_offset_with_owned_type_args(
                                    callee_ptr,
                                    locals_offset,
                                    arg_count,
                                    type_args.tys,
                                    frame_idx,
                                    function,
                                )
                            };
                        }
                    }
                    OpCode::Neg => {
                        let val = self.stack.ensure_pop();
                        if let Some(n) = val.as_int() {
                            // Negating INT_MIN = -2^62 yields 2^62, which fits i64
                            // (INT_MIN != i64::MIN) but not i63, so the range-check
                            // in try_int catches it.
                            match Value::try_int(n.wrapping_neg()) {
                                Some(v) => push_operand!(v),
                                None => {
                                    return Err(
                                        self.integer_overflow(format!("-({n}) overflows int"))
                                    );
                                }
                            }
                        } else if let Some(n) = val.as_float() {
                            let v = self.float_value(-n);
                            push_operand!(v);
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
                                    push_operand!(result);
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
                            let func_obj = self.callable_function(bf.function);
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
    fn visit_roots(&mut self, visitor: &mut dyn RootVisitor) {
        let Self {
            frames,
            // Holds only a refused frame allocation, which names nothing on
            // the heap (asserted below).
            pending_entry_frame_panic,
            stack,
            op_count: _,
            cur_pc: _,
            heap: _,
            hotswap_enabled: _,
            early_yield: _,
            dispatch_fast_path: _,
            // Its cached cursor is layout, not a pointer; see `relocated`.
            tlab: _,
            globals,
            pending_globals: _,
            // The shared tables name compile-time objects only (the static
            // packages and their declarations), which never move.
            packages: _,
            error_class_ptrs: _,
            panic_class_ptrs: _,
            stdlib_heads: _,
            // Rooted by the engine's own `DynDispatchRoot` holder.
            dynamic_dispatch: _,
            // Its keys are forwarded by the engine's own `MonitorRoot` holder.
            monitors: _,
            thread_id: _,
            telemetry,
            trace_scope: _,
            manual_activations: _,
            pending_telemetry_wait: _,
            context_transfers,
            argv: _,
            pending_call_trace: _,
            thread_local_call: _,
            pending_trace_hooks: _,
            hotswap_prepared,
            held_traces: _,
            trace_scope_frame: _,
            entry_reservation: _,
            inherited_hook_suppression: _,
            pending_host_trace: _,
            entry_trace: _,
            entry_trace_context: _,
            root_context: _,
            pending_call_type_args,
            compiled,
            impl_cache,
            json_decode_cache,
            checkpoint_passed: _,
            pending_call_passes_type_args: _,
            static_load_type_cache,
            static_virtual_call_cache,
            virtual_call_memo,
            static_virtual_field_cache,
            enum_equals_memo: _,
            shapes: _,
        } = self;
        if let Some(prepared) = hotswap_prepared {
            prepared.visit_roots(visitor);
        }
        debug_assert!(
            matches!(
                pending_entry_frame_panic,
                None | Some(VmPanic::StackOverflow | VmPanic::AllocFailure { .. })
            ),
            "a pending entry-frame panic is a refused allocation, with no heap roots"
        );

        // Stack values. Frame locals live in the stack at the frame's
        // `locals_offset`, so they are covered here.
        for value in &mut *stack {
            value.visit_roots(visitor);
        }

        // A `$init` VM owns the globals it is populating, and is the only
        // thing that roots them. Once `$init` finishes they are frozen into
        // the engine's `SharedGlobals`, a permit holder of its own.
        match globals {
            VmGlobals::Owned(pool) => pool.0.visit_roots(visitor),
            VmGlobals::Shared(_) => {}
        }

        // Native generic calls have no bytecode frame while the builtin is
        // executing, so this is the only owner of their realized type heads.
        // A runtime declaration bound to T must remain live across any GC the
        // native performs or triggers through a continuation.
        pending_call_type_args.visit_roots(visitor);

        // The static caches are keyed by a function's address, tagged in the
        // low bit when it is a runtime function (a compile-time function is
        // keyed by its payload address and never moves). A key is a weak reference:
        // the entry goes when its function does, and follows it otherwise.
        let old_load_type_cache = std::mem::take(static_load_type_cache);
        for ((identity, index), mut site) in old_load_type_cache {
            let Some(identity) = forward_cache_identity(identity, visitor) else {
                continue;
            };
            let live = match &mut site {
                LoadTypeSite::Realized(ty) => ty.visit(visitor),
                // Each binding is weak, its argument list and object alike.
                // The site itself names nothing, so it stays even when every
                // binding went.
                LoadTypeSite::Generic(bindings) => {
                    bindings.retain(|(args, ty)| {
                        forward_weak_heads(args, visitor) && ty.visit(visitor)
                    });
                    true
                }
                LoadTypeSite::Projection => true,
            };
            if live {
                static_load_type_cache.insert((identity, index), site);
            }
        }
        // The virtual-site caches are weak, key and value alike: an entry
        // names a callee, an interface and the declarations in the receiver's
        // and the call's type arguments, and must keep none of them alive.
        // A key hashes its caller's address, which moves with a runtime
        // function, and a dead entry must go, so the tables are rebuilt.
        // The memo names objects by address, which a collection may move or
        // free.
        virtual_call_memo.fill(VirtualCallMemo::EMPTY);
        let old_virtual_call_cache = std::mem::take(static_virtual_call_cache);
        for (mut key, mut target) in old_virtual_call_cache {
            if key.forward(visitor)
                && target.callee.visit(visitor)
                && forward_weak_heads(&mut target.frame_type_args, visitor)
            {
                static_virtual_call_cache.insert(key, target);
            }
        }
        let old_virtual_field_cache = std::mem::take(static_virtual_field_cache);
        for (mut key, slot) in old_virtual_field_cache {
            if key.forward(visitor) {
                static_virtual_field_cache.insert(key, slot);
            }
        }
        // Resolver answers, held weakly like the static caches above.
        impl_cache.get_mut().visit_weak(visitor);
        // Decode lookups hold pointers too, but they are refilled in one
        // decode call, so a collection drops them rather than forwarding
        // them. Before the frames, so a suspended decode's class plans are
        // its own when they are repointed.
        json_decode_cache.clear();

        for ContextTransfer { handling, target } in context_transfers.iter_mut() {
            handling.visit_roots(visitor);
            target.visit_roots(visitor);
        }

        compiled.visit_roots(visitor);

        for frame in &mut *frames {
            frame.visit_roots(visitor);
        }
        if let Some(telemetry) = telemetry {
            telemetry.visit_roots(visitor);
        }
    }

    fn relocated(&mut self) {
        // The GC has reset the heap's TLAB cursor (`gen0_next_chunk`) and
        // swapped semispaces, so this VM's cached `alloc_ptr`/`alloc_limit`
        // now point into a region the heap will hand out to other VMs as a
        // fresh chunk. Drop them so the next allocation refills from the
        // post-GC cursor.
        self.tlab.invalidate();
        self.shapes.clear();
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

/// The ordering of a read of an instance word: none needed when escape
/// analysis proved the instance thread-local.
pub(crate) const fn load_order(local: bool) -> Ordering {
    if local {
        Ordering::Relaxed
    } else {
        Ordering::Acquire
    }
}

/// [`load_order`] for a write.
pub(crate) const fn store_order(local: bool) -> Ordering {
    if local {
        Ordering::Relaxed
    } else {
        Ordering::Release
    }
}
