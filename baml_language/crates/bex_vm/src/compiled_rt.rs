//! Runtime surface shared by the interpreter and natively compiled BAML
//! functions.
//!
//! A BAML function compiled to Rust (see `Function::native_body`) must produce
//! exactly the values and exactly the exceptions the bytecode interpreter
//! produces for the same program. Both share the semantic primitives that
//! decide those: `Value::try_int` and the tagged checked arithmetic, index
//! resolution, the generic `binop`/`cmpop`, and the cold constructors of every
//! panic. The dispatch arms in `run_compact` keep their own hand-tuned shape:
//! routing them through these methods measurably slowed the interpreter (one
//! giant function whose register allocation depends on every arm), so the
//! methods here compose the same primitives for generated code instead.
//!
//! Every method takes operands as values rather than reading the eval stack,
//! because compiled code keeps its locals in Rust variables and has no eval
//! stack region of its own. Errors are the interpreter's own `VmError`: a
//! thrown BAML exception (`VmError::Thrown`) built through
//! [`BexVm::panic_to_exception_value`], or an internal error for an operand the
//! type checker should have ruled out.
//!
//! Nothing here reads run-loop state (`pc`, the current frame index, the
//! current function). Operations that do — `LoadType`, calls, `Await` — are
//! not part of this surface.

use bex_heap::TlabHolder;
use bex_vm_types::{
    BinOp, CmpOp, HeapPtr, Object, ObjectType, RealizedTy, StackIndex, UnaryOp, Value,
    types::{Closure, ConstValue, Function},
};

use crate::{
    errors::{ThrowKind, VmError, VmInternalError, VmPanic, VmThrown},
    indexable::EvalStackTrait,
    package_baml::{NativeCallResult, NativeFunction},
    vm::{BexVm, VmExecState, value_as_float},
};

/// A natively compiled BAML function body. Errors are the VM's own: a thrown
/// BAML exception keeps its throw kind and context end to end, and an internal
/// error keeps its own variant. Boxed so the `Result` stays two words wide;
/// the box is paid only on throw.
pub type CompiledBody = fn(&mut BexVm, &[Value]) -> Result<Value, Box<VmError>>;

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
pub enum Step {
    /// The body finished with this value.
    Return(Value),
    /// Call `callee` through the VM's call path (bytecode, an LLM function, a
    /// sys op, a builtin that runs callbacks, a closure, a compiled function
    /// that can suspend) with the `argc` arguments the body pushed
    /// ([`BexVm::push_arg`]).
    Call {
        callee: HeapPtr,
        argc: u32,
        /// Type arguments, for a generic callee.
        type_args: TypeArgs,
    },
    /// `await future`, run by the interpreter's own `Await` on this frame.
    Await(Value),
    /// `baml.future.__await_any(futures)`: the index of the first settled one.
    AwaitAny(Value),
    /// `spawn plan`: the spawned task's future.
    Spawn(Value),
    /// Let the scheduler run (a GC, another task) and continue.
    Yield,
}

/// The type arguments of a [`Step::Call`]: inline for the one or two a
/// generic call has, so a suspending call allocates nothing for them.
pub type TypeArgs = smallvec::SmallVec<[RealizedTy; 2]>;

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
/// Its `faulting_pc`, which only bytecode reads as a position, holds the
/// source line the body last reported.
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
}

/// The `instruction_ptr` of a compiled frame whose body is running on the
/// Rust stack; see [`stub`].
pub(crate) const RUNNING: usize = usize::MAX;

/// The Rust stack a compiled call leaves for what runs above it: the
/// callee's own frame (a body with thousands of locals takes tens of KiB),
/// the builtins it calls, the unwinder. Less than this and the call raises
/// `StackOverflow` instead of overflowing the thread's stack, whose size (a
/// 2 MiB tokio worker, an 8 MiB main thread, a host's own thread) the VM does
/// not choose.
#[cfg(not(target_arch = "wasm32"))]
const STACK_RED_ZONE: usize = 256 << 10;

/// The lowest stack address compiled code on this thread may reach before a
/// call raises `StackOverflow`: the thread's stack limit plus
/// [`STACK_RED_ZONE`]. Read once per `exec`, so a compiled call compares one
/// address instead of looking the thread's stack up. 0 when unknown.
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

impl BexVm {
    // ── Arithmetic ────────────────────────────────────────────────────────────

    /// The generic binary operator: total over every operand pair the
    /// specialized opcodes accept, including `bigint`, the `bigint`/`int` mix,
    /// mixed `int`/`float`, and string concatenation for `+`.
    #[inline]
    pub fn binop(&mut self, op: BinOp, left: Value, right: Value) -> Result<Value, VmError> {
        self.binop_values(op, left, right)
    }

    /// The generic comparison operator, total over the same operand pairs as
    /// [`Self::binop`] plus strings, byte arrays, enum variants, and `type`
    /// values.
    #[inline]
    pub fn cmpop(&mut self, op: CmpOp, left: Value, right: Value) -> Result<Value, VmError> {
        self.cmpop_values(op, left, right)
    }

    /// `int + int`, overflow-checked into the i63 range. Operands are
    /// `Int`-tagged values; the hot path neither untags nor retags.
    #[inline]
    pub fn add_int(&mut self, l: Value, r: Value) -> Result<Value, VmError> {
        match Value::tagged_int_add_checked(l, r) {
            Some(v) => Ok(v),
            None => Err(self.tagged_int_overflow(l, '+', r)),
        }
    }

    /// `int - int`, overflow-checked into the i63 range.
    #[inline]
    pub fn sub_int(&mut self, l: Value, r: Value) -> Result<Value, VmError> {
        match Value::tagged_int_sub_checked(l, r) {
            Some(v) => Ok(v),
            None => Err(self.tagged_int_overflow(l, '-', r)),
        }
    }

    /// `int * int` on raw operands, overflow-checked.
    #[inline]
    pub fn mul_int(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        self.int_arith_result(l.checked_mul(r), l, '*', r)
    }

    /// `int / int` on raw operands. A zero divisor throws `DivisionByZero`;
    /// `INT_MIN / -1` throws `IntegerOverflow`.
    #[inline]
    pub fn div_int(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        if r == 0 {
            return Err(self.division_by_zero(l, r));
        }
        self.finish_int(l / r, l, '/', r)
    }

    /// `int % int` on raw operands. A zero divisor throws `DivisionByZero`.
    #[inline]
    pub fn mod_int(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        if r == 0 {
            return Err(self.division_by_zero(l, r));
        }
        self.finish_int(l % r, l, '%', r)
    }

    /// `int << int` with the i63 semantics constant folding uses.
    #[inline]
    pub fn shl_int(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        self.int_shl(l, r)
    }

    /// `int >> int` with the i63 semantics constant folding uses.
    #[inline]
    pub fn shr_int(&mut self, l: i64, r: i64) -> Result<Value, VmError> {
        self.int_shr(l, r)
    }

    /// Box a float result. Floats live on the heap behind a `HeapPtr`; compiled
    /// code keeps raw `f64` locals and boxes only when a value crosses into
    /// `Value` form.
    #[inline]
    pub fn float_value(&mut self, f: f64) -> Value {
        Value::object(self.alloc_float(f))
    }

    /// Unbox a heap float. `None` for any other value, including ints.
    #[inline]
    #[must_use]
    pub fn float_of(value: Value) -> Option<f64> {
        value_as_float(value)
    }

    /// Unary minus over `int`, `float`, and `bigint`.
    pub fn neg(&mut self, val: Value) -> Result<Value, VmError> {
        if let Some(n) = val.as_int() {
            // Negating INT_MIN = -2^62 yields 2^62, which fits i64
            // (INT_MIN != i64::MIN) but not i63, so the range-check
            // in try_int catches it.
            return match Value::try_int(n.wrapping_neg()) {
                Some(v) => Ok(v),
                None => Err(self.integer_overflow(format!("-({n}) overflows int"))),
            };
        }
        if let Some(n) = value_as_float(val) {
            return Ok(self.float_value(-n));
        }
        if let Some(ptr) = val.as_object_ptr() {
            // Bigint negation. Compute the negated value into an owned `Arc`
            // first so the immutable `get_object` borrow is released before
            // the `&mut self` `alloc_bigint`.
            let negated = match self.get_object(ptr) {
                Object::Bigint(bi) => Some(std::sync::Arc::new(-bi.as_ref().clone())),
                _ => None,
            };
            if let Some(arc) = negated {
                return self.alloc_bigint(arc);
            }
        }
        Err(VmInternalError::CannotApplyUnaryOp {
            op: UnaryOp::Neg,
            value: self.type_of(&val),
        }
        .into())
    }

    /// Build a catchable `baml.panics.DivisionByZero` throw for `l / 0`.
    #[cold]
    #[inline(never)]
    pub fn division_by_zero(&mut self, l: i64, r: i64) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::DivisionByZero {
            left: Value::int(l),
            right: Value::int(r),
        }))
    }

    // ── Containers ────────────────────────────────────────────────────────────

    /// `.length()` of an array, byte array, map, or string (in bytes).
    pub fn container_len(&self, container: Value) -> Result<Value, VmError> {
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
            Object::String(s) => s.byte_len() as i64,
            other => {
                return Err(VmInternalError::TypeError {
                    expected: ObjectType::Array.into(),
                    got: ObjectType::of(other).into(),
                }
                .into());
            }
        };
        Ok(Value::int(len))
    }

    /// `array[index]` for arrays and byte arrays. A negative index counts from
    /// the end; one that still lands outside throws `IndexOutOfBounds` with
    /// the original index.
    pub fn load_array_element(&mut self, array: Value, index: Value) -> Result<Value, VmError> {
        let array_ptr = self.as_object_ptr(array, ObjectType::Array)?;
        let Some(i) = index.as_int() else {
            return Err(VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Int,
                got: self.type_of(&index),
            }
            .into());
        };
        // Acquire the array's read lock for the duration of the bounds-check
        // + element load so it stays atomic against a racing `push`/grow. The
        // guard drops at the end of the inner scope before any `&mut self`
        // call.
        let load_result: Result<Value, (i64, usize)> = {
            match self.get_object(array_ptr) {
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
        match load_result {
            Ok(v) => Ok(v),
            Err((index, length)) => Err(self.index_out_of_bounds(index, length)),
        }
    }

    /// `array[index] = value` for arrays and byte arrays, with the same index
    /// semantics as [`Self::load_array_element`]. Applies the generational
    /// write barrier.
    pub fn store_array_element(
        &mut self,
        array: Value,
        index: Value,
        new_value: Value,
    ) -> Result<(), VmError> {
        let array_ptr = self.as_object_ptr(array, ObjectType::Array)?;
        let Some(i) = index.as_int() else {
            return Err(VmInternalError::TypeError {
                expected: bex_vm_types::types::Type::Int,
                got: self.type_of(&index),
            }
            .into());
        };
        let new_value_u8: Option<u8> = new_value.as_int().map(|v| (v.cast_unsigned() & 0xFF) as u8);
        let store_result: Result<(), (i64, usize)> = {
            match self.get_object(array_ptr) {
                Object::Array(arr) => {
                    let mut guard = arr.lock_mut(self.tlab.alloc_debt());
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
                    let mut guard = bytes.lock_mut(self.tlab.alloc_debt());
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
        if let Err((index, length)) = store_result {
            return Err(self.index_out_of_bounds(index, length));
        }
        self.heap.write_barrier(array_ptr, new_value);
        Ok(())
    }

    /// Build a catchable `baml.panics.IndexOutOfBounds` throw.
    #[cold]
    #[inline(never)]
    pub fn index_out_of_bounds(&mut self, index: i64, length: usize) -> VmError {
        VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::IndexOutOfBounds { index, length }),
        )
    }

    // ── Instances ─────────────────────────────────────────────────────────────

    /// Read field `idx` of a class instance.
    pub fn load_field(&mut self, instance: Value, idx: usize) -> Result<Value, VmError> {
        let obj_ptr = self.as_object_ptr(instance, ObjectType::Instance)?;
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
        match load_result {
            Ok(value) => Ok(value),
            Err(length) => Err(self.invalid_field_access_error(idx, length)),
        }
    }

    /// Write field `idx` of a class instance, applying the generational write
    /// barrier.
    pub fn store_field(
        &mut self,
        instance: Value,
        idx: usize,
        new_value: Value,
    ) -> Result<(), VmError> {
        let obj_ptr = self.as_object_ptr(instance, ObjectType::Instance)?;
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
        Ok(())
    }

    // ── Closures ──────────────────────────────────────────────────────────────

    /// Allocate a closure over `function` with already-evaluated capture cells
    /// and already-resolved type arguments.
    pub fn make_closure(
        &mut self,
        function: HeapPtr,
        captures: Vec<Value>,
        captured_type_args: Box<[RealizedTy]>,
    ) -> Value {
        let closure = Object::Closure(Closure {
            function,
            captures: captures.into_boxed_slice(),
            captured_type_args,
        });
        Value::object(self.tlab.alloc(closure))
    }

    // ── Raw-operand arithmetic ───────────────────────────────────────────────
    //
    // Compiled code keeps statically typed `int` locals as `i64` and `float`
    // locals as `f64`; these are the checked operators over those, with the
    // interpreter's exact overflow and divide-by-zero behaviour and messages.

    /// `l + r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn add_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        let v = l.wrapping_add(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.integer_overflow(format!("{l} + {r} overflows int")))
        }
    }

    /// `l - r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn sub_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        let v = l.wrapping_sub(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.integer_overflow(format!("{l} - {r} overflows int")))
        }
    }

    /// `l * r` on raw ints, overflow-checked.
    #[inline]
    pub fn mul_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        match l.checked_mul(r).filter(|v| Value::try_int(*v).is_some()) {
            Some(v) => Ok(v),
            None => Err(self.integer_overflow(format!("{l} * {r} overflows int"))),
        }
    }

    /// `l / r` on raw ints: `DivisionByZero` for a zero divisor,
    /// `IntegerOverflow` for `INT_MIN / -1`.
    #[inline]
    pub fn div_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        if r == 0 {
            return Err(self.division_by_zero(l, r));
        }
        let v = l / r;
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.integer_overflow(format!("{l} / {r} overflows int")))
        }
    }

    /// `l % r` on raw ints: `DivisionByZero` for a zero divisor.
    #[inline]
    pub fn mod_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        if r == 0 {
            return Err(self.division_by_zero(l, r));
        }
        Ok(l % r)
    }

    /// `l << r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shl_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        self.int_shl(l, r)?
            .as_int()
            .ok_or_else(|| checker_hole("int"))
    }

    /// `l >> r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shr_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        self.int_shr(l, r)?
            .as_int()
            .ok_or_else(|| checker_hole("int"))
    }

    /// `-n` on a raw int: `IntegerOverflow` for `INT_MIN`.
    #[inline]
    pub fn neg_i64(&mut self, n: i64) -> Result<i64, VmError> {
        let v = n.wrapping_neg();
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.integer_overflow(format!("-({n}) overflows int")))
        }
    }

    /// `!value` with BAML truthiness.
    #[inline]
    #[must_use]
    pub fn not(&self, value: Value) -> bool {
        !self.is_truthy(value)
    }

    // ── Enums and classes ─────────────────────────────────────────────────────

    /// The variant index of an enum value.
    pub fn discriminant(&self, value: Value) -> Result<i64, VmError> {
        let Some(ptr) = value.as_object_ptr() else {
            return Err(VmInternalError::TypeError {
                expected: ObjectType::Variant.into(),
                got: self.type_of(&value),
            }
            .into());
        };
        match self.get_object(ptr) {
            #[allow(clippy::cast_possible_wrap)]
            Object::Variant(variant) => Ok(variant.index as i64),
            other => Err(VmInternalError::TypeError {
                expected: ObjectType::Variant.into(),
                got: ObjectType::of(other).into(),
            }
            .into()),
        }
    }

    /// `value is C` for a non-generic class `C`: class identity, which is
    /// exactly what the interpreter's type test decides for such a class.
    #[inline]
    #[must_use]
    pub fn is_instance_of(&self, value: Value, class: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Instance(instance)) => instance.class == class,
            _ => false,
        }
    }

    /// `value is E` for an enum `E`.
    #[inline]
    #[must_use]
    pub fn is_variant_of(&self, value: Value, enm: HeapPtr) -> bool {
        match value.as_object_ptr().map(|ptr| self.get_object(ptr)) {
            Some(Object::Variant(variant)) => variant.enm == enm,
            _ => false,
        }
    }

    /// The type of instances of the non-generic class `class`, as a type
    /// argument or an array's element type.
    #[must_use]
    pub fn class_type(&self, class: HeapPtr) -> RealizedTy {
        let Object::Class(declaration) = self.get_object(class) else {
            unreachable!("compiled code names classes by their class object");
        };
        RealizedTy::Class(
            bex_vm_types::TypeHead::new(class, declaration.type_tag),
            Box::new([]),
        )
    }

    /// Allocate an instance of a non-generic class with its fields in slot
    /// order.
    #[inline]
    pub fn new_instance(&mut self, class: HeapPtr, fields: Vec<Value>) -> Value {
        Value::object(self.alloc_instance(class, fields))
    }

    /// Allocate an array of `element_ty`.
    #[inline]
    pub fn new_array(&mut self, element_ty: RealizedTy, values: Vec<Value>) -> Value {
        Value::object(self.alloc_array(element_ty, values))
    }

    /// Allocate a string from a literal.
    #[inline]
    pub fn string_value(&mut self, text: &str) -> Value {
        Value::object(self.alloc_string(text))
    }

    // ── Calls ─────────────────────────────────────────────────────────────────

    /// Call a VM-owned builtin synchronously. A builtin that yields (to run a
    /// callback) cannot be called from compiled code; the compiler admits only
    /// builtins that never do.
    pub fn call_builtin(
        &mut self,
        builtin: NativeFunction,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let outcome = builtin(self, args);
        self.settle();
        match outcome {
            NativeCallResult::Done(value) => Ok(value),
            NativeCallResult::Error(error) => Err(Box::new(self.native_error_to_vm_error(error))),
            NativeCallResult::YieldToCall { .. } => Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "a builtin yielded inside compiled code".to_string(),
                }
                .into(),
            )),
        }
    }

    /// [`Self::call_builtin`] for a generic builtin (`Array.push`,
    /// `json.to_string`, …), with its type arguments where the interpreter
    /// puts them for a native callee: `current_call_type_args()`.
    pub fn call_builtin_typed(
        &mut self,
        builtin: NativeFunction,
        type_args: &[RealizedTy],
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        // Reuse the buffer's capacity: a `push` in a loop allocates nothing
        // here after the first call.
        let previous = std::mem::take(&mut self.pending_call_type_args);
        let mut buffer = std::mem::take(&mut self.compiled_type_args);
        buffer.clear();
        buffer.extend_from_slice(type_args);
        self.pending_call_type_args = buffer;
        let result = self.call_builtin(builtin, args);
        self.compiled_type_args = std::mem::replace(&mut self.pending_call_type_args, previous);
        result
    }

    /// Size the slot region of the compiled function whose frame count is
    /// `depth` to `slots` (a resumable body's arguments arrive in the first
    /// ones). Called once, on start; a resumable body's region then sits at
    /// the top of the stack whenever it suspends.
    pub fn reserve_slots(&mut self, depth: usize, slots: usize) {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body reserves slots in its own frame");
        };
        let end = frame.locals_offset.raw() + slots;
        self.stack.resize(end, Value::NULL);
    }

    /// Slot `index` of the compiled function whose frame count is `depth`.
    #[inline]
    #[must_use]
    pub fn slot(&self, depth: usize, index: usize) -> Value {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body reads its own frame");
        };
        self.stack
            .get_at(StackIndex::from_raw(frame.locals_offset.raw() + index))
    }

    /// Write slot `index` of the compiled function whose frame count is
    /// `depth`; see [`Self::slot`].
    #[inline]
    pub fn set_slot(&mut self, depth: usize, index: usize, value: Value) {
        let crate::vm::Frame::Bytecode(frame) = &self.frames[depth - 1] else {
            unreachable!("a compiled body writes its own frame");
        };
        let at = frame.locals_offset.raw() + index;
        self.stack.0[at] = value;
    }

    /// Push an argument of the call a resumable body is about to request
    /// ([`Step::Call`]); arguments sit above the body's slots, rooted.
    #[inline]
    pub fn push_arg(&mut self, value: Value) {
        self.stack.push(value);
    }

    /// The value of `future` when it has settled with one and awaiting it
    /// would not poll for a yield: `await`'s fast path, with no suspension.
    /// `None` sends the body through the interpreter's own `Await`
    /// ([`Step::Await`]), which raises, parks or yields exactly as bytecode
    /// does. Counts the `Await`'s control-flow check, as bytecode does.
    #[inline]
    pub fn ready(&mut self, future: Value) -> Option<Value> {
        if self.early_yield.is_due() {
            return None;
        }
        let value = match self.get_object(future.as_object_ptr()?) {
            Object::Future(future) => match future.read() {
                bex_vm_types::FutureRead::Ready(value) => value,
                _ => return None,
            },
            _ => return None,
        };
        self.early_yield.tick();
        Some(value)
    }

    /// Whether a resumable body should yield ([`Step::Yield`]) at a loop back
    /// edge: the interpreter's control-flow check.
    #[inline]
    pub fn should_yield(&mut self) -> bool {
        self.should_early_yield()
    }

    /// The frame count with the running compiled function's frame on top.
    /// A compiled body reads it on entry and hands it to [`Self::catch`].
    #[inline]
    #[must_use]
    pub fn frame_depth(&self) -> usize {
        self.frames.len()
    }

    /// Record `line` as the source position of the compiled function whose
    /// frame count is `depth` (see [`Self::frame_depth`]). Compiled code calls
    /// this only when an error leaves the function, lands in its `catch`, or
    /// it suspends, so stack traces name the statement that raised, at no
    /// cost otherwise. A compiled frame keeps its line in `faulting_pc`; see
    /// `stub`.
    #[cold]
    pub fn note_line(&mut self, depth: usize, line: u32) {
        if let Some(crate::vm::Frame::Bytecode(frame)) = depth
            .checked_sub(1)
            .and_then(|top| self.frames.get_mut(top))
        {
            frame.faulting_pc = line as usize;
        }
    }

    /// Land a thrown error in a compiled `catch`; see
    /// `BexVm::land_compiled_catch`. Returns the caught value and its
    /// `baml.errors.Context`; an internal error keeps propagating.
    #[inline]
    pub fn catch(
        &mut self,
        error: Box<VmError>,
        depth: usize,
    ) -> Result<(Value, Value), Box<VmError>> {
        self.land_compiled_catch(error, depth)
    }

    /// `unreachable` reached in compiled code: the catchable
    /// `baml.panics.Unreachable` bytecode's `Unreachable` throws.
    #[cold]
    #[inline(never)]
    pub fn unreachable_panic(&mut self) -> Box<VmError> {
        Box::new(VmError::thrown_fresh(
            self.panic_to_exception_value(VmPanic::Unreachable),
        ))
    }

    /// Call a compiled function from compiled code.
    ///
    /// Every call gets a frame, as in bytecode: it counts toward the depth
    /// limit (a catchable `StackOverflow`, never a native stack overflow),
    /// names the callee in stack traces, and roots the callee. With telemetry
    /// off that frame is all the call costs: a struct push, then a direct
    /// Rust call LLVM can inline. With telemetry on the call goes through the
    /// VM's call path, so records, call paths and `$trace` behave exactly as
    /// for a bytecode call.
    ///
    /// On `Err` the callee's frame stays pushed, as the VM's call path leaves
    /// it: the unwinder pops it if the error escapes, [`Self::catch`] if a
    /// compiled handler lands it.
    #[inline]
    pub fn call_compiled(
        &mut self,
        callee: HeapPtr,
        body: CompiledBody,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        if self.frames.len() >= crate::vm::MAX_FRAMES || stack_address() < self.stack_floor {
            return Err(Box::new(VmError::thrown_fresh(
                self.panic_to_exception_value(VmPanic::StackOverflow),
            )));
        }
        if self.telemetry.is_some() {
            return self.call_compiled_observed(callee, args);
        }
        let context = self.current_context().clone();
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.frames.push(crate::vm::Frame::Bytecode(
            crate::vm::BytecodeFrame::compiled(callee, locals_offset, context),
        ));
        let result = body(self, args);
        if result.is_ok() {
            self.frames.pop();
            self.stack.truncate(locals_offset.raw());
        }
        result
    }

    /// [`Self::call_compiled`] through the VM's call path, for telemetry.
    #[cold]
    #[inline(never)]
    #[allow(unsafe_code)]
    fn call_compiled_observed(
        &mut self,
        callee: HeapPtr,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let top = self.frames.len() - 1;
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.stack.extend(args.iter().copied());
        let mut frame_idx = top;
        // SAFETY: the running compiled frame's function object lives in the
        // compile-time heap, which is never collected; see `load_function`.
        let mut function = unsafe {
            self.load_function(top)
                .map_err(|error| Box::new(VmError::InternalError(error)))?
        };
        let outcome = self.compiled_caller_call(
            callee,
            locals_offset,
            args.len(),
            &[],
            &mut frame_idx,
            &mut function,
        );
        match outcome {
            // The callee ran to completion inside the call path (its body is
            // compiled), so its result is on the stack. An `EarlyYield`
            // request is left armed for the enclosing loop.
            Ok(None | Some(VmExecState::EarlyYield)) if frame_idx == top => {
                Ok(self.stack.ensure_pop())
            }
            Ok(_) => Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "a compiled call reached a callee without a compiled body".to_string(),
                }
                .into(),
            )),
            Err(error) => Err(Box::new(error)),
        }
    }

    // ── Type tests ────────────────────────────────────────────────────────────

    /// `value is T` against a type constant from the function's constant pool,
    /// with the caller's resolved generic type arguments (empty when the
    /// enclosing function is not generic).
    pub fn value_is_type(
        &self,
        type_args: &[RealizedTy],
        value: Value,
        raw_const: &ConstValue,
        resolved_const: Value,
    ) -> Result<bool, VmInternalError> {
        self.value_matches_type_constant_with_type_args(type_args, value, raw_const, resolved_const)
    }
}
