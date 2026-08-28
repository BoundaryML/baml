//! Baml virtual machine.
//!
//! This crate implements a stack based virtual machine similar to the `CPython`
//! VM or Lox VM from [Crafting Interpreters](https://craftinginterpreters.com/).
//!
//! Main entry point is `Vm::exec` (in `bex_vm` crate) which runs the VM cycle:
//! 1. Decode Instruction.
//! 2. Execute Instruction.
//! 3. Increment instruction pointer and repeat loop.
//!
//! The instructions that the VM runs are defined in [`Instruction`] enum.

pub mod bytecode;
pub mod errors;
pub mod head_walk;
pub mod heap_ptr;
pub mod identity;
pub mod indexable;
pub mod lazy_biased_mutex;
pub mod link;
pub mod relink;
mod roots;
pub mod runtime_compile;
pub mod task_group;
pub mod type_head;
pub mod types;
pub mod unit;

pub use bex_str::BexStr;
pub use bytecode::{BinOp, Bytecode, CmpOp, Instruction, JumpTableData, UnaryOp};
pub use heap_ptr::HeapPtr;
pub use indexable::{
    GlobalIndex, GlobalPool, ObjectIndex, ObjectPool, SharedGlobals, StackIndex, VmGlobals,
};
pub use link::LinkError;
pub use roots::{PermitProof, RootHaver, WriteBarrier};
pub use runtime_compile::{
    ArtifactKind, RuntimeCompileArtifact, RuntimeCompileArtifactSlot, RuntimeCompileDiagnostic,
    RuntimeCompileMode, RuntimeCompileRequest, RuntimeDiagnosticSeverity, RuntimeMountedClass,
    RuntimeMountedEnum, RuntimeMountedFieldAttrs, RuntimeMountedVariantAttrs, RuntimePackageMount,
    RuntimeSessionCompileArtifact, RuntimeSessionCompileRequest, RuntimeSessionInitializer,
    RuntimeSessionStep, RuntimeSessionStepKind, RuntimeSourceSpan, RuntimeTypeMount,
    SessionContract, SessionEvalLease, SessionVisibleKind, SessionVisibleSymbol,
};
pub use task_group::{TaskGroupInner, TaskGroupPermit, TaskGroupTicket};
pub use type_head::TypeHead;

// ── The runtime's instantiation of the `baml_type` family ────────────────────
//
// The runtime carries the *same* types the compiler does, at a different head:
// a [`TypeHead`] (tag identity + pointer) instead of a `TypeName` that must be
// looked up in a package map. These aliases name that instantiation, so runtime
// code writes `bex_vm_types::RuntimeTy` where compiler code writes
// `baml_type::RuntimeTy`.
//
// The shadowing is deliberate. A module that needs both forms gets a name
// collision and must say which it means — which is exactly the place where
// mixing a compiled type with a loaded one would otherwise pass silently.

/// [`baml_type::RuntimeTy`] at the runtime's head. Declaration-facing: keeps
/// type variables and projections for reflection and dispatch.
pub type RuntimeTy = baml_type::RuntimeTy<TypeHead>;

/// [`baml_type::RealizedTy`] at the runtime's head. Value-facing: no type
/// variables, so it is what a live value's type is.
pub type RealizedTy = baml_type::RealizedTy<TypeHead>;

/// [`baml_type::TyTemplate`] at the runtime's head. Signature-facing: leaves may
/// be positional `TypeArgRef`s awaiting a frame's type arguments.
pub type TyTemplate = baml_type::TyTemplate<TypeHead>;

/// [`baml_type::RuntimeInterface`] at the runtime's head.
pub type RuntimeInterface = baml_type::RuntimeInterface<TypeHead>;

/// [`baml_type::Interface`] at the runtime's head. An interface *instantiation*
/// — which interface, at which args and associated bindings.
pub type Interface = baml_type::Interface<TypeHead>;

/// [`baml_type::Ty`] at the runtime's head. The algebra's own spelling — the
/// widest member, admitting every position the others rule out.
pub type Ty = baml_type::Ty<TypeHead>;

/// [`baml_type::ConcreteRealizedTy`] at the runtime's head. What a live value's
/// type narrows to once it is known to name no abstract position.
pub type ConcreteRealizedTy = baml_type::ConcreteRealizedTy<TypeHead>;

/// [`baml_type::RealizedFunctionParamTy`] at the runtime's head.
pub type RealizedFunctionParamTy = baml_type::RealizedFunctionParamTy<TypeHead>;

// ── Crossing between the two heads ───────────────────────────────────────────
//
// Emit is the one legitimate producer of runtime types without a heap: it mints
// each head's *identity* from the declaration's name and leaves the pointer
// unfilled, for the loader to bind. The reverse direction is
// [`TypeHead::to_name`], which needs a live heap and so can fail.

/// Mint unresolved runtime heads for a compiled signature type.
#[must_use]
pub fn anchor_template(ty: &baml_type::TyTemplate) -> TyTemplate {
    ty.map_heads(&mut TypeHead::of_name)
}

/// Mint unresolved runtime heads for a compiled declaration-facing type.
#[must_use]
pub fn anchor_runtime_ty(ty: &baml_type::RuntimeTy) -> RuntimeTy {
    ty.map_heads(&mut TypeHead::of_name)
}

/// Mint unresolved runtime heads for a compiled value-facing type.
#[must_use]
pub fn anchor_realized(ty: &baml_type::RealizedTy) -> RealizedTy {
    ty.map_heads(&mut TypeHead::of_name)
}

/// Mint unresolved runtime heads for a compiled interface bound.
#[must_use]
pub fn anchor_interface(interface: &baml_type::RuntimeInterface) -> RuntimeInterface {
    interface.map_heads(&mut TypeHead::of_name)
}

/// A head that could not be named when converting a type out of the VM.
///
/// Carries the tag rather than a stand-in name: a boundary that cannot say what
/// a type *is* must fail rather than hand the host something that looks like a
/// real type but names nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UnnameableHead(pub baml_type::typetag::TypeTag);

impl std::fmt::Display for UnnameableHead {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(
            f,
            "type head #{} names no loaded declaration",
            self.0.as_i64()
        )
    }
}

impl std::error::Error for UnnameableHead {}

/// Re-anchor a runtime type onto names, for handing it outside the VM.
///
/// A [`TypeHead`] is a live pointer into this process's heap, so it is
/// meaningless in an FFI payload or a file. Every boundary out of the VM
/// converts through here — see [`TypeHead::declared_name`].
pub fn name_headed(ty: &RuntimeTy) -> Result<baml_type::RuntimeTy, UnnameableHead> {
    ty.try_map_heads(&mut |head| head.declared_name().ok_or(UnnameableHead(head.tag())))
}

/// [`name_headed`] for a realized type.
pub fn name_headed_realized(ty: &RealizedTy) -> Result<baml_type::RealizedTy, UnnameableHead> {
    ty.try_map_heads(&mut |head| head.declared_name().ok_or(UnnameableHead(head.tag())))
}
pub use types::{
    ArrayContainer, ArrayReadGuard, ArrayWriteGuard, AtomicValueSlot, BoundMethod, CaptureCategory,
    CaptureOption, Class, ClassField, CleanupLatch, ClientBuildMeta, ClientBuildType, CollectorRef,
    ConstValue, DeclarationName, Enum, EnumVariant, Function, FunctionCaptureProps, FunctionKind,
    FunctionMeta, FunctionOrigin, Future, FutureRead, GenericFunction, HostClosure, Instance,
    LlmOperationCapabilities, LockedContainer, LockedReadGuard, LockedWriteGuard, MapContainer,
    MapReadGuard, MapWriteGuard, MediaValue, Object, ObjectType, PanicClass, Program, PromptAst,
    RetryPolicyMeta, SysOp, SysOpErrorCategory, SysOpPanicCategory, Uint8ArrayContainer,
    Uint8ArrayReadGuard, Uint8ArrayWriteGuard, UnscheduledFuture, Value, ValueKind, Variant,
    format_float, sys_op_for_path, type_tags,
};
pub use unit::{
    CompilationUnit, ExportTable, GenericFnKey, InitTail, LocalRef, ProgramImplRuleFrag,
    ProgramMethodImplFrag, ProgramPackageFrag, Symbol, SymbolKind,
};

/// Used to check if the VM should yield early.
///
/// ## Why do we need this?
///
/// #### On multi-threaded targets
///
/// A thread that is running an infinite loop (or that otherwise just doesn't yield for a long time)
/// would prevent all other threads from continuing whenever they are waiting for a GC to complete,
/// since GCs require all threads to be parked.
///
/// #### On single-threaded targets (WASM)
///
/// A thread that is running an infinite loop (or that otherwise just doesn't yield for a long time)
/// would prevent the VM from running any other instructions, since the VM is single-threaded.
/// This may eventually crash the program since the GC cannot run, resulting in unpreventable heap growth.
///
/// #### Solution
///
/// [`EarlyYieldCheck`] increments a counter every time it is called (should be called for every __*control flow* instruction__).
/// - For single-threaded targets, it always returns `true` after a certain number of increments.
/// - For multi-threaded targets, it checks an atomic flag every `N` increments. If the flag is set, it returns `true`.
///   The flag should be set by another thread that wants to park the VM.
pub struct EarlyYieldCheck {
    counter: u64,
    interval: u64,
    /// Only used in non-WASM targets, since WASM currently doesn't support threads.
    /// If another thread wants us to park (e.g. for a GC) they will set this to true.
    #[cfg(not(target_arch = "wasm32"))]
    park_requested: ::std::sync::Arc<::std::sync::atomic::AtomicBool>,
}

/// Default poll interval: ~32M instructions (~1.5s at typical IPC).
pub const EARLY_YIELD_INTERVAL: u64 = 1 << 25;

impl EarlyYieldCheck {
    #[cfg(target_arch = "wasm32")]
    #[expect(clippy::new_without_default)]
    pub const fn new() -> Self {
        Self {
            counter: EARLY_YIELD_INTERVAL,
            interval: EARLY_YIELD_INTERVAL,
        }
    }
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new(park_requested: ::std::sync::Arc<::std::sync::atomic::AtomicBool>) -> Self {
        Self {
            counter: EARLY_YIELD_INTERVAL,
            interval: EARLY_YIELD_INTERVAL,
            park_requested,
        }
    }
    /// Create with a custom interval (for testing).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn with_interval(
        park_requested: ::std::sync::Arc<::std::sync::atomic::AtomicBool>,
        interval: u64,
    ) -> Self {
        assert!(
            interval > 0,
            "early-yield interval must be greater than zero"
        );
        Self {
            counter: interval,
            interval,
            park_requested,
        }
    }
    /// Decrement and return true if we should yield.
    ///
    /// Checks every ~32M calls (~1.5s at typical IPC). GC parks at async
    /// yield points anyway; this is just a fallback for tight compute loops.
    ///
    /// Counts down to zero so the check is a single `subs` + `b.ne` on ARM —
    /// the subtraction sets the zero flag, no separate compare needed.
    #[allow(clippy::inline_always)]
    #[inline(always)]
    pub fn should_early_yield(&mut self) -> bool {
        self.counter -= 1;
        if self.counter != 0 {
            return false;
        }
        self.counter = self.interval;

        #[cfg(target_arch = "wasm32")]
        {
            self.counter > (1 << 16)
        }
        #[cfg(not(target_arch = "wasm32"))]
        {
            self.park_requested
                .load(::core::sync::atomic::Ordering::Relaxed)
        }
    }
    pub const fn reset(&mut self) {
        self.counter = self.interval;
    }
}

#[cfg(all(test, not(target_arch = "wasm32")))]
mod tests {
    use ::std::sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    };

    use super::EarlyYieldCheck;

    /// Small interval for fast tests.
    const TEST_INTERVAL: u64 = 1 << 10;
    const POLL_WINDOW: u64 = TEST_INTERVAL + 100;

    #[test]
    fn flag_false_never_yields() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut check = EarlyYieldCheck::with_interval(flag, TEST_INTERVAL);
        for _ in 0..10_000 {
            assert!(!check.should_early_yield());
        }
    }

    #[test]
    fn flag_true_yields_within_poll_window() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut check = EarlyYieldCheck::with_interval(flag, TEST_INTERVAL);
        let mut yielded_at = None;
        for i in 0..POLL_WINDOW {
            if check.should_early_yield() {
                yielded_at = Some(i);
                break;
            }
        }
        assert!(
            yielded_at.is_some(),
            "should have yielded within {POLL_WINDOW} increments when flag was set"
        );
    }

    #[test]
    fn flag_set_mid_execution_is_observed() {
        let flag = Arc::new(AtomicBool::new(false));
        let mut check = EarlyYieldCheck::with_interval(Arc::clone(&flag), TEST_INTERVAL);

        for _ in 0..2_000 {
            assert!(!check.should_early_yield());
        }

        flag.store(true, Ordering::Relaxed);

        let mut yielded = false;
        for _ in 0..POLL_WINDOW {
            if check.should_early_yield() {
                yielded = true;
                break;
            }
        }
        assert!(
            yielded,
            "yield should fire within one poll window after the flag flips"
        );
    }

    #[test]
    fn reset_clears_counter() {
        let flag = Arc::new(AtomicBool::new(true));
        let mut check = EarlyYieldCheck::with_interval(Arc::clone(&flag), TEST_INTERVAL);

        let mut saw_yield = false;
        for _ in 0..POLL_WINDOW {
            if check.should_early_yield() {
                saw_yield = true;
                break;
            }
        }
        assert!(saw_yield, "precondition: counter should have yielded once");

        check.reset();
        flag.store(false, Ordering::Relaxed);

        for _ in 0..2_000 {
            assert!(!check.should_early_yield());
        }
    }

    #[test]
    fn reset_is_public() {
        let _: fn(&mut EarlyYieldCheck) = EarlyYieldCheck::reset;
    }
}
