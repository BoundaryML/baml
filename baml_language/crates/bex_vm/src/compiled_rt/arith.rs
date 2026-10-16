//! Arithmetic over tagged values and over the raw `i64`/`f64` operands
//! compiled code keeps for statically typed locals.

use bex_heap::TlabHolder;
use bex_vm_types::{BinOp, CmpOp, Object, UnaryOp, Value};

use super::checker_hole;
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::{BexVm, value_as_float},
};

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
}
