//! Runtime operations and frame support for BAML compiled to Rust. Generated
//! bodies share the interpreter's semantic primitives and error types; the
//! interpreter keeps its own dispatch arms, including container and field access,
//! to preserve its performance. See `baml_compiler2_native/DESIGN.txt` for the
//! compilation, rooting, and suspension protocols.

mod arith;
mod call;
mod cell;
mod heap;
mod program;
mod types;
mod r#virtual;

/// What lets compiled code collect garbage in place: the engine's view of
/// every other mutator. Collecting moves objects, so `vm`'s stack (where
/// compiled code roots its live values first) is a root and is forwarded.
pub trait CompiledCollector: Send + Sync {
    /// Collect now if every other mutator is parked and a collection is due;
    /// whether it collected.
    fn collect_in_place(&self, vm: &mut crate::BexVm) -> bool;
}

/// Whether `BAML_GC_STRESS=1` makes every compiled GC check collect, so a
/// value compiled code failed to root is caught at once.
#[must_use]
pub fn gc_stress() -> bool {
    static STRESS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STRESS.get_or_init(|| std::env::var_os("BAML_GC_STRESS").is_some_and(|value| value == "1"))
}

/// The literal in a `RealizedTy::Literal` built by generated code.
pub use baml_base::Literal;
/// The freshness of a `RealizedTy::Literal` built by generated code.
pub use baml_type::Freshness;
/// Severity of a log emitted by generated code.
pub use btel_records::LogLevel;
#[cfg(test)]
pub(crate) use call::FORCE_CALL_FUNNEL;

use bex_vm_types::{HeapPtr, Value, types::Function};
pub use r#virtual::VirtualSite;

use crate::{
    errors::{ThrowKind, VmError, VmInternalError, VmThrown},
    vm::{BexVm, value_as_float},
};

/// A natively compiled BAML function body. Errors are the VM's own: a thrown
/// BAML exception keeps its throw kind and context end to end, and an internal
/// error keeps its own variant. Boxed so the `Result` stays two words wide;
/// the box is paid only on throw.
pub type CompiledBody = fn(&mut BexVm, &[Value]) -> Result<Value, Box<VmError>>;

const _: () =
    assert!(std::mem::size_of::<Result<Value, Box<VmError>>>() == 2 * std::mem::size_of::<usize>());

/// A natively compiled body that can suspend: a state machine the VM drives.
///
/// It keeps its arguments, every local live across a suspension and the point
/// it suspended at in its frame's stack slots (`slot`/`set_slot`), which the GC
/// scans and moves, and returns a [`Step`]: finished, or a request the VM
/// carries out before calling the body again with the outcome.
pub type ResumableBody = fn(&mut BexVm, Resume) -> Result<Step, Box<VmError>>;

/// How a [`ResumableBody`] is entered. A resumed body reads the point it
/// suspended at from its own slot; this says what the request produced.
pub enum Resume {
    /// The body starts.
    Start,
    /// The call or await returned this value.
    Value(Value),
    /// The call or await raised this error; the body lands it in its catch
    /// or returns it.
    Thrown(Box<VmError>),
    /// The body yielded at a loop back edge and continues.
    Yielded,
}

/// What a [`ResumableBody`] asks of the VM. The body records its resume point
/// in its frame before returning a request.
#[derive(Clone, Copy)]
pub enum Step {
    /// The body finished with this value.
    Return(Value),
    /// Call `callee` through the VM's call path (bytecode, an LLM function, a
    /// sys op, a builtin that runs callbacks, a closure, a compiled function
    /// that can suspend) with the `argc` arguments the body pushed
    /// ([`BexVm::push_arg`]) and the type arguments it pushed
    /// ([`BexVm::push_type_arg`]). Every step a body returns is moved out of
    /// it, so this stays small: two words.
    Call { callee: HeapPtr, argc: u32 },
    /// `await future`, run by the interpreter's own `Await` on this frame.
    Await(Value),
    /// `baml.future.__await_any(futures)`: the index of the first settled one.
    AwaitAny(Value),
    /// `spawn plan`: the spawned task's future.
    Spawn(Value),
    /// Let the scheduler run (a GC, another task) and continue.
    Yield,
}

/// A compiled body, by whether it can suspend.
#[derive(Clone, Copy)]
pub enum CompiledFn {
    /// Runs to completion inside the call that enters it.
    Inline(CompiledBody),
    /// Driven by the VM; see [`ResumableBody`].
    Resumable(ResumableBody),
}

/// One compiled function in a generated module's table.
#[derive(Clone, Copy)]
pub struct CompiledEntry {
    /// Fingerprint of the serialized program this body was compiled from.
    pub program_fingerprint: [u8; 32],
    /// Object index of the bytecode `Function` this body replaces: the
    /// identity the generated code bakes in for callees and classes.
    pub object: u32,
    /// That function's link name, checked at attach.
    pub name: &'static str,
    /// That function's arity, checked at attach.
    pub arity: usize,
    /// The body.
    pub body: CompiledFn,
    /// The stack slots the body keeps in its frame; see
    /// [`bex_vm_types::types::NativeBody::slots`].
    pub slots: u32,
    /// The body's cause slot, when it has a `catch`; see
    /// [`bex_vm_types::types::NativeBody::cause_slot`].
    pub cause_slot: Option<u32>,
}

impl CompiledEntry {
    pub(crate) fn erase(self) -> bex_vm_types::types::NativeBody {
        let (ptr, resumable) = match self.body {
            CompiledFn::Inline(body) => (body as *const (), false),
            CompiledFn::Resumable(body) => (body as *const (), true),
        };
        // SAFETY: `ptr` is cast from the body type `resumable` names.
        #[allow(unsafe_code)]
        unsafe {
            bex_vm_types::types::NativeBody::new(ptr, resumable, self.slots, self.cause_slot)
        }
    }
}

/// Where a compiled frame stands, read off its `instruction_ptr`.
///
/// A compiled frame keeps the bytecode frame's layout and adds nothing to it.
/// Its position is one of two things: [`RUNNING`], while its body is on the
/// Rust stack, or an offset into [`STUB`], the code appended to its function's
/// bytecode at load (`prepare_compact_code`): where the interpreter resumes it.
/// Its `faulting_pc` holds the index of the instruction of its function's
/// bytecode equivalent to where the body last reported, which a reader
/// translates to a compact PC (`BexVm::compiled_pc`) and then reads as a
/// bytecode frame's.
pub(crate) mod stub {
    use bex_vm_types::bytecode::OpCode;

    /// The code appended to every compiled function. `ResumeCompiled` hands
    /// the frame back to its body; the suspending instructions before three
    /// of them are the interpreter's own, so `await`, `await_any` and `spawn`
    /// in compiled code keep their exact semantics, telemetry and
    /// cancellation by running as bytecode on the compiled frame.
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
    /// Not started: the body begins here (an entry point or a resumable call).
    pub(crate) const START: usize = 0;
    /// Suspended on a call; its result is on the stack when it resumes (or an
    /// error is delivered).
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

/// The Rust stack a compiled call leaves for what runs above it: the
/// callee's own frame (a body with thousands of locals takes tens of KiB),
/// the builtins it calls, the unwinder. Less than this and the call
/// continues on a fresh stack segment ([`STACK_SEGMENT`]) instead of
/// overflowing the thread's stack, whose size (a 2 MiB tokio worker, an
/// 8 MiB main thread, a host's own thread) the VM does not choose.
#[cfg(not(target_arch = "wasm32"))]
const STACK_RED_ZONE: usize = 256 << 10;

/// The stack segment a compiled call continues on below the floor; see
/// `BexVm::with_stack_room`. Mapped lazily, so only the pages the frames
/// touch cost memory; big enough that the recursion the frame limit admits
/// (256 frames of tens of KiB) fits in a couple.
#[cfg(not(target_arch = "wasm32"))]
pub(crate) const STACK_SEGMENT: usize = 8 << 20;

/// The lowest stack address compiled code may reach on the stack it is
/// running on (the thread's, or a segment a call grew) before the next call
/// moves to a fresh segment: that stack's limit plus [`STACK_RED_ZONE`]. Read
/// once per `exec` and once per segment, so a compiled call compares one
/// address instead of looking the stack up. 0 when unknown.
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

/// An address in the caller's stack frame: where the stack is now.
#[inline]
fn stack_address() -> usize {
    let marker = 0u8;
    std::ptr::from_ref(&marker) as usize
}

/// Where `function`'s [`stub::CODE`] starts, for a function with a compiled
/// body.
#[inline]
pub(crate) fn stub_start(function: &Function) -> Option<usize> {
    function.native_body?;
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

/// `throw value` from compiled code: a fresh throw, whose
/// `baml.errors.Context` is built where it lands.
#[must_use]
pub fn throw(value: Value) -> Box<VmError> {
    Box::new(VmError::Thrown(VmThrown::fresh(value)))
}

/// A caught error raised again with the context it was caught with, so it
/// keeps its original trace and cause, as bytecode's rethrow does.
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

/// A value the checker typed `int`, unboxed. A non-int here is a checker hole,
/// reported as the internal error the interpreter's generic paths raise.
#[inline]
pub fn expect_int(value: Value) -> Result<i64, Box<VmError>> {
    value.as_int().ok_or_else(|| Box::new(checker_hole("int")))
}

/// A value the checker typed `bool`, unboxed; see [`expect_int`].
#[inline]
pub fn expect_bool(value: Value) -> Result<bool, Box<VmError>> {
    value
        .as_bool()
        .ok_or_else(|| Box::new(checker_hole("bool")))
}

/// A value the checker typed `float`, unboxed; see [`expect_int`].
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
pub(crate) fn native_body_fn(function: &Function) -> Option<CompiledFn> {
    let body = function.native_body?;
    // SAFETY: a `NativeBody` is built only through its `unsafe` constructor,
    // whose contract is that `resumable` names the type the pointer was cast
    // from. `transmute` because Rust has no `as` cast from `*const ()` to a fn
    // pointer.
    Some(unsafe {
        if body.resumable() {
            CompiledFn::Resumable(std::mem::transmute::<*const (), ResumableBody>(body.ptr()))
        } else {
            CompiledFn::Inline(std::mem::transmute::<*const (), CompiledBody>(body.ptr()))
        }
    })
}
