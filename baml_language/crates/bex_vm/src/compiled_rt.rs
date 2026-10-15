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
    errors::{ThrowKind, VmError, VmInternalError, VmPanic, VmRustFnError, VmThrown},
    indexable::EvalStackTrait,
    package_baml::{NativeCallResult, NativeFunction},
    vm::{BexVm, VmExecState, value_as_float},
};

/// `throw value` from compiled code: a fresh throw, whose
/// `baml.errors.Context` the unwinder builds when it leaves the frame.
#[must_use]
pub fn throw(value: Value) -> Box<VmError> {
    Box::new(VmError::Thrown(VmThrown::fresh(value)))
}

/// A caught value raised again from compiled code. Compiled code never builds
/// a context, so the rethrow carries none and the unwinder builds a fresh
/// one; see the design notes on this v0 deviation.
#[must_use]
pub fn rethrow(value: Value) -> Box<VmError> {
    Box::new(VmError::Thrown(VmThrown {
        value,
        throw_kind: ThrowKind::Rethrow,
        context: None,
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

/// The exception value inside an error a compiled `catch` can handle.
///
/// A thrown BAML value is catchable; an internal error is not and keeps
/// propagating, exactly as the interpreter's unwinder treats the two.
// The box is the error type compiled bodies hold; taking it by value avoids
// a re-box on the propagating path.
#[allow(clippy::boxed_local)]
pub fn caught(error: Box<VmError>) -> Result<Value, Box<VmError>> {
    match *error {
        VmError::Thrown(thrown) => Ok(thrown.value),
        other => Err(Box::new(other)),
    }
}

/// The compiled body attached to `function`, as a callable pointer.
#[inline]
#[allow(unsafe_code)]
pub(crate) fn native_body_fn(function: &Function) -> Option<NativeFunction> {
    let body = function.native_body?;
    // SAFETY: `NativeBody` is only ever constructed from a `NativeFunction`
    // (see `attach_native_bodies`), so casting back to that type is sound.
    // `transmute` because Rust has no `as` cast from `*const ()` to a fn
    // pointer.
    Some(unsafe { std::mem::transmute::<*const (), NativeFunction>(body.0) })
}

/// Convert a compiled function body's result into what the VM's native call
/// path expects.
///
/// Compiled bodies return `Result<Value, Box<VmError>>` so that `?` composes
/// with every helper here at no cost (the std `From<T> for Box<T>`), and so the
/// `Result` stays two words wide. The call path only understands
/// [`NativeCallResult`], whose error type has no room for an attached
/// `baml.errors.Context`; a thrown value crosses with its throw kind and the
/// unwinder rebuilds the context with the compiled frame on the stack.
#[must_use]
pub fn into_native_result(result: Result<Value, Box<VmError>>) -> NativeCallResult {
    match result {
        Ok(value) => NativeCallResult::Done(value),
        Err(error) => NativeCallResult::Error(match *error {
            VmError::Thrown(thrown) => VmRustFnError::Thrown {
                value: thrown.value,
                throw_kind: thrown.throw_kind,
            },
            VmError::InternalError(internal) => VmRustFnError::InternalError(internal),
            VmError::ThrownUnhandled { .. } | VmError::TracedInternalError { .. } => {
                VmRustFnError::InternalError(VmInternalError::CompiledBody {
                    message: "compiled body produced an already-unwound error".to_string(),
                })
            }
        }),
    }
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
        Ok(Self::int_of(self.int_shl(l, r)?))
    }

    /// `l >> r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shr_i64(&mut self, l: i64, r: i64) -> Result<i64, VmError> {
        Ok(Self::int_of(self.int_shr(l, r)?))
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

    /// The raw int inside an `Int`-tagged value. The type checker proved the
    /// tag; a mismatch is a compiler bug, reported in debug builds.
    #[inline]
    #[must_use]
    pub fn int_of(value: Value) -> i64 {
        debug_assert!(value.is_int(), "compiled code read a non-int as int");
        value.as_int().unwrap_or(0)
    }

    /// The raw bool inside a `Bool` value; see [`Self::int_of`].
    #[inline]
    #[must_use]
    pub fn bool_of(value: Value) -> bool {
        debug_assert!(
            value.as_bool().is_some(),
            "compiled code read a non-bool as bool"
        );
        value.as_bool().unwrap_or(false)
    }

    /// The raw float inside a heap float; see [`Self::int_of`].
    #[inline]
    #[must_use]
    pub fn f64_of(value: Value) -> f64 {
        debug_assert!(
            value_as_float(value).is_some(),
            "compiled code read a non-float as float"
        );
        value_as_float(value).unwrap_or(0.0)
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

    /// Call a function that has a compiled body through the VM's own call
    /// path, so the call is observed exactly as a bytecode call would be:
    /// frame, telemetry record, call path. Used for calls to *observed*
    /// functions from compiled code; folded functions are called directly.
    #[allow(unsafe_code)]
    pub fn call_native_body(
        &mut self,
        callee: HeapPtr,
        args: &[Value],
    ) -> Result<Value, Box<VmError>> {
        let Some(top) = self.frames.len().checked_sub(1) else {
            return Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "call_native_body with no frame".to_string(),
                }
                .into(),
            ));
        };
        let locals_offset = StackIndex::from_raw(self.stack.len());
        self.stack.extend(args.iter().copied());
        let mut frame_idx = top;
        // SAFETY: the running compiled frame's function object lives in the
        // compile-time heap, which is never collected; see `load_function`.
        let mut function = unsafe {
            self.load_function(top)
                .map_err(|error| Box::new(VmError::InternalError(error)))?
        };
        let outcome = self.execute_call_from_locals_offset(
            callee,
            locals_offset,
            args.len(),
            &mut frame_idx,
            &mut function,
        );
        match outcome {
            // The callee's frame was pushed and completed inside the funnel
            // (its body is native), so its result is on the stack. An
            // `EarlyYield` request is left armed for the enclosing loop.
            Ok(None | Some(VmExecState::EarlyYield)) if frame_idx == top => {
                Ok(self.stack.ensure_pop())
            }
            Ok(_) => Err(Box::new(
                VmInternalError::CompiledBody {
                    message: "call_native_body callee has no compiled body".to_string(),
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
