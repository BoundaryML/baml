//! Runtime operations and frame support for BAML compiled to Rust.

mod arith;
mod call;
mod cell;
mod frame;
pub(crate) use frame::attach_compiled_bodies;
pub use frame::{CompiledBodies, CompiledBodyTable, FramePosition, PendingLog};
pub use state::CompiledState;
pub(crate) use state::SavedCallTypes;
mod heap;
mod program;
mod state;
mod types;
mod r#virtual;

#[cfg(test)]
pub(crate) mod tests;

/// Coordinates in-place collection with the other mutators.
pub trait CompiledCollector: Send + Sync {
    /// Collect and forward `vm` roots if due and all other mutators are parked, returning
    /// whether collection ran.
    fn collect_in_place(&self, vm: &mut crate::BexVm) -> bool;
}

/// Whether `BAML_GC_STRESS=1` forces collection at every compiled GC check.
#[must_use]
pub fn gc_stress() -> bool {
    static STRESS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STRESS.get_or_init(|| baml_env::os_var("BAML_GC_STRESS").is_some_and(|value| value == "1"))
}

/// The literal in a `RealizedTy::Literal` built by generated code.
pub use baml_base::Literal;
/// The freshness of a `RealizedTy::Literal` built by generated code.
pub use baml_type::Freshness;
use bex_vm_types::{HeapPtr, Value, types::Function};
/// Severity of a log emitted by generated code.
pub use btel_records::LogLevel;
#[cfg(test)]
pub(crate) use call::FORCE_CALL_FUNNEL;
pub use r#virtual::VirtualSite;

use crate::{
    errors::{ThrowKind, VmError, VmInternalError, VmThrown},
    vm::{BexVm, value_as_float},
};

/// A compiled BAML function body that runs without suspending.
pub type InlineBody = fn(&mut BexVm, &[Value]) -> Result<Value, Box<VmError>>;

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(std::mem::size_of::<Result<Value, Box<VmError>>>() == 2 * std::mem::size_of::<usize>());

/// A compiled state machine whose arguments, live locals, and resume point reside in GC-scanned
/// frame slots.
pub type ResumableBody = fn(&mut BexVm, Resume) -> Result<Step, Box<VmError>>;

/// The outcome delivered to a [`ResumableBody`] at its saved resume point.
pub enum Resume {
    /// The body starts.
    Start,
    /// The call or await returned this value.
    Value(Value),
    /// The call or await raised this error.
    Thrown(Box<VmError>),
    /// The body yielded at a loop back edge and continues.
    Yielded,
}

/// A VM request whose resume point the body must save in its frame before returning.
#[derive(Clone, Copy)]
pub enum Step {
    /// The body finished with this value.
    Return(Value),
    /// Call `callee` with arguments pushed by [`BexVm::push_arg`] and [`BexVm::push_type_arg`].
    Call { callee: HeapPtr, argc: u32 },
    /// Await a future on this frame.
    Await(Value),
    /// `baml.future.__await_any(futures)`: the index of the first settled one.
    AwaitAny(Value),
    /// `spawn plan`: the spawned task's future.
    Spawn(Value),
    /// Yield to the scheduler before continuing.
    Yield,
}

/// A compiled body, by whether it can suspend.
#[derive(Clone, Copy)]
pub enum CompiledFn {
    /// Runs to completion inside the call that enters it.
    Inline(InlineBody),
    /// Driven by the VM; see [`ResumableBody`].
    Resumable(ResumableBody),
}

/// One compiled function in a generated module's table.
#[derive(Clone, Copy)]
pub struct CompiledEntry {
    /// Object index of the bytecode function this body replaces.
    pub object: u32,
    /// That function's link name, checked at attach.
    pub name: &'static str,
    /// That function's arity, checked at attach.
    pub arity: usize,
    /// The body.
    pub body: CompiledFn,
    /// Frame slot count; see [`bex_vm_types::types::CompiledBody::slots`].
    pub slots: u32,
    /// Catch context slot; see [`bex_vm_types::types::CompiledBody::cause_slot`].
    pub cause_slot: Option<u32>,
}

impl CompiledEntry {
    pub(crate) fn erase(self) -> bex_vm_types::types::CompiledBody {
        let (ptr, resumable) = match self.body {
            CompiledFn::Inline(body) => (body as *const (), false),
            CompiledFn::Resumable(body) => (body as *const (), true),
        };
        // SAFETY: `ptr` is cast from the body type `resumable` names.
        #[allow(unsafe_code)]
        unsafe {
            bex_vm_types::types::CompiledBody::new(ptr, resumable, self.slots, self.cause_slot)
        }
    }
}

/// Compiled frame positions: `instruction_ptr` holds [`RUNNING`] or an offset into
/// [`stub::CODE`].
/// `faulting_pc` holds an original bytecode instruction index, translated by
/// `BexVm::compiled_pc`.
pub(crate) mod stub {
    use bex_vm_types::bytecode::OpCode;

    /// Resume stubs appended to compiled functions for calls, yields, awaits, and spawns.
    pub(crate) const CODE: [OpCode; 9] = [
        OpCode::ResumeCompiled, // START
        OpCode::ResumeCompiled, // AFTER_CALL
        OpCode::ResumeCompiled, // AFTER_YIELD
        OpCode::Await,          // AWAIT
        OpCode::ResumeCompiled,
        OpCode::AwaitAny, // AWAIT_ANY
        OpCode::ResumeCompiled,
        OpCode::Spawn, // SPAWN
        OpCode::ResumeCompiled,
    ];
    /// Entry point for a body that has not started.
    pub(crate) const START: usize = 0;
    /// Call resume point, where the stack holds the result or an error is delivered.
    pub(crate) const AFTER_CALL: usize = 1;
    /// Suspended on a yield.
    pub(crate) const AFTER_YIELD: usize = 2;
    pub(crate) const AWAIT: usize = 3;
    pub(crate) const AWAIT_ANY: usize = 5;
    pub(crate) const SPAWN: usize = 7;

    const _: () = {
        assert!(matches!(CODE[START], OpCode::ResumeCompiled));
        assert!(matches!(CODE[AFTER_CALL], OpCode::ResumeCompiled));
        assert!(matches!(CODE[AFTER_YIELD], OpCode::ResumeCompiled));
        assert!(matches!(CODE[AWAIT], OpCode::Await));
        assert!(matches!(CODE[AWAIT_ANY], OpCode::AwaitAny));
        assert!(matches!(CODE[SPAWN], OpCode::Spawn));
        assert!(matches!(CODE[AWAIT + 1], OpCode::ResumeCompiled));
        assert!(matches!(CODE[AWAIT_ANY + 1], OpCode::ResumeCompiled));
        assert!(matches!(CODE[SPAWN + 1], OpCode::ResumeCompiled));
    };
}

/// The `instruction_ptr` of a compiled frame whose body is running on the
/// Rust stack; see [`stub`].
pub(crate) const RUNNING: usize = usize::MAX;

/// Stack space reserved for a compiled callee, its builtins, and unwinding.
#[cfg(not(target_arch = "wasm32"))]
const STACK_RED_ZONE: usize = 256 << 10;

/// Size of the lazily mapped stack segment used by `BexVm::with_stack_room`.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const STACK_SEGMENT: usize = 8 << 20;

/// Stack limit plus [`STACK_RED_ZONE`], or zero if the limit is unknown.
pub(crate) fn stack_floor() -> usize {
    #[cfg(not(target_arch = "wasm32"))]
    {
        stacker::remaining_stack().map_or(0, |left| {
            stack_address()
                .saturating_sub(left)
                .saturating_add(STACK_RED_ZONE)
        })
    }
    #[cfg(target_arch = "wasm32")]
    {
        0
    }
}

#[inline]
fn stack_address() -> usize {
    let marker = 0u8;
    std::ptr::from_ref(&marker) as usize
}

/// Where `function`'s [`stub::CODE`] starts, for a function with a compiled
/// body.
#[inline]
pub(crate) fn stub_start(function: &Function) -> Option<usize> {
    function.compiled_body?;
    Some(function.bytecode.compact.as_ref()?.code.len() - stub::CODE.len())
}

/// Where the body of a compiled frame stands; see [`stub`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompiledAt {
    /// Running on the Rust stack, or not yet started: an error leaves it.
    Running,
    /// Suspended on a request: an error is delivered to it.
    Suspended,
}

/// Where the compiled body of a frame at `instruction_ptr` running `function`
/// stands; `None` when the frame runs bytecode.
#[inline]
pub(crate) fn compiled_at(instruction_ptr: usize, function: &Function) -> Option<CompiledAt> {
    if instruction_ptr == RUNNING {
        return Some(CompiledAt::Running);
    }
    match instruction_ptr.checked_sub(stub_start(function)?)? {
        stub::START => Some(CompiledAt::Running),
        _ => Some(CompiledAt::Suspended),
    }
}

/// Create a fresh throw whose context is built when it lands.
#[must_use]
pub fn throw(value: Value) -> Box<VmError> {
    Box::new(VmError::Thrown(VmThrown::fresh(value)))
}

/// Rethrow a caught error with its original context.
#[must_use]
pub fn rethrow(value: Value, context: Value) -> Box<VmError> {
    Box::new(VmError::Thrown(VmThrown {
        value,
        throw_kind: ThrowKind::Rethrow,
        context: (!context.is_null()).then_some(context),
    }))
}

/// Control reached a block the checker proved unreachable.
#[must_use]
pub fn unreachable() -> Box<VmError> {
    Box::new(
        VmInternalError::CompiledBody {
            message: "compiled body reached an unreachable block".to_string(),
        }
        .into(),
    )
}

/// The runtime type tag of a value, as `TypeTag` and `IsTypeTag` read it.
#[inline]
#[must_use]
pub fn type_tag(value: Value) -> i64 {
    crate::vm::value_type_tag(value)
}

/// Unbox an `int`, reporting an internal error if the checker assigned the wrong type.
#[inline]
pub fn expect_int(value: Value) -> Result<i64, Box<VmError>> {
    value.as_int().ok_or_else(|| Box::new(checker_hole("int")))
}

/// Unbox a `bool`, reporting an internal error if the checker assigned the wrong type.
#[inline]
pub fn expect_bool(value: Value) -> Result<bool, Box<VmError>> {
    value
        .as_bool()
        .ok_or_else(|| Box::new(checker_hole("bool")))
}

/// Unbox a `float`, reporting an internal error if the checker assigned the wrong type.
#[inline]
pub fn expect_f64(value: Value) -> Result<f64, Box<VmError>> {
    value_as_float(value).ok_or_else(|| Box::new(checker_hole("float")))
}

#[cold]
#[inline(never)]
fn checker_hole(expected: &str) -> VmError {
    VmInternalError::CompiledBody {
        message: format!("compiled code read a non-{expected} value typed {expected}"),
    }
    .into()
}

/// The callable a function value refers to, for a [`Step::Call`].
pub fn callee_ptr(value: Value) -> Result<HeapPtr, Box<VmError>> {
    value
        .as_object_ptr()
        .ok_or_else(|| Box::new(checker_hole("function")))
}

/// The compiled body attached to `function`.
#[inline]
#[allow(unsafe_code)]
pub(crate) fn compiled_fn(function: &Function) -> Option<CompiledFn> {
    let body = function.compiled_body?;
    // SAFETY: `CompiledBody::new` requires `resumable` to match the original function pointer
    // type.
    Some(unsafe {
        if body.resumable() {
            CompiledFn::Resumable(std::mem::transmute::<*const (), ResumableBody>(body.ptr()))
        } else {
            CompiledFn::Inline(std::mem::transmute::<*const (), InlineBody>(body.ptr()))
        }
    })
}
