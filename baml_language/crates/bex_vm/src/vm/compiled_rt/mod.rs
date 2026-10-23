//! Runtime operations and frame support for BAML compiled to Rust.

mod arith;
mod call;
#[cfg(test)]
pub(crate) use call::{FORCE_CALL_FUNNEL, FORCE_VM_FRAMES};
mod cell;
mod fiber;
mod frame;
pub(crate) use frame::attach_compiled_bodies;
pub use frame::{CompiledBodies, CompiledBodyTable};
pub(in crate::vm) use frame::{append_passed_type_args, validate_compiled_program};
pub(crate) use state::CompiledState;
mod heap;
mod program;
mod record;
pub(crate) use record::{FrameAt, RecordChain, record_ref};
pub use record::{FrameRecord, RecordResult};
mod state;
mod trace;
mod types;
mod r#virtual;

#[cfg(test)]
pub(crate) mod tests;

/// `BAML_GC_STRESS=1` forces safepoints and collections.
#[must_use]
pub fn gc_stress() -> bool {
    static STRESS: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *STRESS.get_or_init(|| baml_env::os_var("BAML_GC_STRESS").is_some_and(|value| value == "1"))
}

/// The heap's own heuristic, or a major collection at every safepoint under
/// `BAML_GC_STRESS=1`.
#[inline]
#[must_use]
pub fn collection_due(heap: &bex_heap::BexHeap) -> Option<bex_heap::CollectionLevel> {
    if gc_stress() {
        return Some(bex_heap::CollectionLevel::Major);
    }
    heap.should_collect()
}

pub use baml_base::Literal;
pub use baml_type::Freshness;
use bex_vm_types::{HeapPtr, Value, types::Function};
pub use btel_records::LogLevel;
pub(crate) use fiber::{Fiber, Request};
pub use trace::TraceHookMode;

use crate::{
    errors::{ThrowKind, VmError, VmInternalError, VmThrown},
    vm::{BexVm, value_as_float},
};

/// Bodies run on their VM's fiber and can suspend anywhere in their call chain.
pub type CompiledFn = fn(&mut BexVm, &FrameRecord, &[Value]) -> Result<Value, Box<VmError>>;

/// `BAML_COMPILED_FRAMES=1` disables direct record calls.
#[must_use]
pub fn frames_forced() -> bool {
    static FORCED: std::sync::OnceLock<bool> = std::sync::OnceLock::new();
    *FORCED
        .get_or_init(|| baml_env::os_var("BAML_COMPILED_FRAMES").is_some_and(|value| value == "1"))
}

#[cfg(target_pointer_width = "64")]
const _: () =
    assert!(std::mem::size_of::<Result<Value, Box<VmError>>>() == 2 * std::mem::size_of::<usize>());

/// What a body asks the VM to do while it waits on the fiber; see [`BexVm::suspend`].
pub enum Step {
    /// Call `callee` with arguments pushed by [`BexVm::push_arg`] and [`BexVm::push_type_arg`].
    Call { callee: HeapPtr, argc: u32 },
    /// Await a future on this frame.
    Await(Value),
    /// `baml.future.__await_any(futures)`: the index of the first settled one.
    AwaitAny(Value),
    /// `spawn plan`: the spawned task's future.
    Spawn(Value),
    /// Deliver a `log.*` event.
    Log {
        level: LogLevel,
        event_name: Option<std::sync::Arc<str>>,
        data: Value,
        source_location: Option<crate::vm::VmEventSourceLocation>,
    },
    /// Let the engine collect, park, or cancel before continuing.
    Yield,
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
    pub body: CompiledFn,
    /// Frame slot count; see [`bex_vm_types::types::CompiledBody::slots`].
    pub slots: u32,
    /// Catch context slot; see [`bex_vm_types::types::CompiledBody::cause_slot`].
    pub cause_slot: Option<u32>,
    /// Whether the body needs a VM frame; see
    /// [`bex_vm_types::types::CompiledBody::framed`].
    pub framed: bool,
}

impl CompiledEntry {
    pub(crate) fn erase(self) -> bex_vm_types::types::CompiledBody {
        // SAFETY: `ptr` is cast from a `CompiledFn`.
        #[allow(unsafe_code)]
        unsafe {
            bex_vm_types::types::CompiledBody::new(
                self.body as *const (),
                self.slots,
                self.cause_slot,
                self.framed,
            )
        }
    }
}

/// Compiled frame positions: `instruction_ptr` holds [`RUNNING`] or an offset into
/// [`stub::CODE`].
/// `faulting_pc` holds an original bytecode instruction index, translated by
/// `BexVm::frame_pc`.
pub(crate) mod stub {
    use bex_vm_types::bytecode::OpCode;

    /// Resume stubs appended to compiled functions for calls, yields, awaits, and spawns.
    pub(crate) const CODE: [OpCode; 10] = [
        OpCode::ResumeCompiled, // START
        OpCode::ResumeCompiled, // AFTER_CALL
        OpCode::ResumeCompiled, // AFTER_YIELD
        OpCode::Await,          // AWAIT
        OpCode::ResumeCompiled,
        OpCode::AwaitAny, // AWAIT_ANY
        OpCode::ResumeCompiled,
        OpCode::Spawn, // SPAWN
        OpCode::ResumeCompiled,
        OpCode::ResumeCompiled, // AFTER_LOG
    ];
    /// Entry point for a body that has not started.
    pub(crate) const START: usize = 0;
    /// Call resume point, where the stack holds the result or an error is delivered.
    pub(crate) const AFTER_CALL: usize = 1;
    /// Suspended on a yield; the engine pushes nothing.
    pub(crate) const AFTER_YIELD: usize = 2;
    pub(crate) const AWAIT: usize = 3;
    pub(crate) const AWAIT_ANY: usize = 5;
    pub(crate) const SPAWN: usize = 7;
    /// Suspended on a log; the engine pushes a null to discard on resume.
    pub(crate) const AFTER_LOG: usize = 9;

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
        assert!(matches!(CODE[AFTER_LOG], OpCode::ResumeCompiled));
    };
}

/// Running fiber frames have no bytecode instruction pointer.
pub(crate) const RUNNING: usize = usize::MAX;

#[inline]
pub(crate) fn stub_start(function: &Function) -> Option<usize> {
    function.compiled_body?;
    Some(function.bytecode.compact.as_ref()?.code.len() - stub::CODE.len())
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CompiledAt {
    /// Running on the Rust stack, or not yet started: an error leaves it.
    Running,
    /// Suspended on a request: an error is delivered to it.
    Suspended,
}

#[inline]
pub(crate) fn compiled_at(instruction_ptr: usize, function: &Function) -> Option<CompiledAt> {
    if instruction_ptr == RUNNING {
        return Some(CompiledAt::Running);
    }
    Some(
        if instruction_ptr.checked_sub(stub_start(function)?)? == stub::START {
            CompiledAt::Running
        } else {
            CompiledAt::Suspended
        },
    )
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

/// The runtime type tag of a value, as `TypeTag` and `IsTypeTag` read it.
#[inline]
#[must_use]
pub fn type_tag(value: Value) -> i64 {
    crate::vm::value_type_tag(value)
}

#[inline]
pub fn expect_int(value: Value) -> Result<i64, Box<VmError>> {
    value.as_int().ok_or_else(|| Box::new(checker_hole("int")))
}

#[inline]
pub fn expect_bool(value: Value) -> Result<bool, Box<VmError>> {
    value
        .as_bool()
        .ok_or_else(|| Box::new(checker_hole("bool")))
}

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

#[inline]
#[allow(unsafe_code)]
pub(crate) fn compiled_fn(function: &Function) -> Option<CompiledFn> {
    let body = function.compiled_body?;
    // SAFETY: `CompiledEntry::erase` cast the pointer from a `CompiledFn`.
    Some(unsafe { std::mem::transmute::<*const (), CompiledFn>(body.ptr()) })
}
