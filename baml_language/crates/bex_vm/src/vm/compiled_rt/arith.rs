//! Arithmetic over tagged values and unboxed numeric locals.

use bex_heap::TlabHolder;
use bex_vm_types::{BinOp, CmpOp, Object, UnaryOp, Value};

use super::expect_int;
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::{BexVm, value_as_float},
};

impl BexVm {
    #[inline]
    pub fn binop(&mut self, op: BinOp, left: Value, right: Value) -> Result<Value, Box<VmError>> {
        self.binop_values(op, left, right).map_err(Box::new)
    }

    #[inline]
    pub fn cmpop(&mut self, op: CmpOp, left: Value, right: Value) -> Result<Value, Box<VmError>> {
        self.cmpop_values(op, left, right).map_err(Box::new)
    }

    /// `ConcatStrings`: the strings `parts` joined left to right.
    pub fn concat_strings(&mut self, parts: &[Value]) -> Result<Value, Box<VmError>> {
        self.concat_parts(parts).map_err(|e| Box::new(e.into()))
    }

    #[inline]
    pub fn float_value(&mut self, f: f64) -> Value {
        Value::object(self.alloc_float(f))
    }

    pub fn neg(&mut self, val: Value) -> Result<Value, Box<VmError>> {
        if let Some(n) = val.as_int() {
            return self.neg_i64(n).map(Value::int);
        }
        if let Some(n) = value_as_float(val) {
            return Ok(self.float_value(-n));
        }
        if let Some(ptr) = val.as_object_ptr() {
            let negated = match self.get_object(ptr) {
                Object::Bigint(bi) => Some(std::sync::Arc::new(-bi.as_ref().clone())),
                _ => None,
            };
            if let Some(arc) = negated {
                return self.alloc_bigint(arc).map_err(Box::new);
            }
        }
        Err(VmInternalError::CannotApplyUnaryOp {
            op: UnaryOp::Neg,
            value: self.type_of(&val),
        }
        .into())
    }

    #[cold]
    #[inline(never)]
    pub(crate) fn division_by_zero(&mut self, l: i64, r: i64) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::DivisionByZero {
            left: Value::int(l),
            right: Value::int(r),
        }))
    }

    #[cold]
    #[inline(never)]
    fn division_by_zero_boxed(&mut self, l: i64, r: i64) -> Box<VmError> {
        Box::new(self.division_by_zero(l, r))
    }

    /// The `IntegerOverflow` throw for `l op r`, boxed. Kept out of line so
    /// the checked helpers below are a few instructions and inline into
    /// compiled bodies: formatting the message and boxing the error would
    /// otherwise spill the operands and set up a frame on the hot path.
    #[allow(
        clippy::unnecessary_box_returns,
        reason = "boxing out of line keeps the allocation off the inlined fast path"
    )]
    #[cold]
    #[inline(never)]
    fn int_overflow(&mut self, l: i64, op: char, r: i64) -> Box<VmError> {
        Box::new(self.integer_overflow(format!("{l} {op} {r} overflows int")))
    }

    /// `l + r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn add_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        let v = l.wrapping_add(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.int_overflow(l, '+', r))
        }
    }

    /// `l - r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn sub_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        let v = l.wrapping_sub(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.int_overflow(l, '-', r))
        }
    }

    /// `l * r` on raw ints, overflow-checked: the i64 product must not
    /// overflow and must fit the i63 range. Both checks are evaluated and
    /// combined without short-circuiting, so the fast path takes one branch.
    #[inline]
    pub fn mul_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        let (v, overflowed) = l.overflowing_mul(r);
        if !overflowed & Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.int_overflow(l, '*', r))
        }
    }

    /// `l / r` on raw ints: `DivisionByZero` for a zero divisor,
    /// `IntegerOverflow` for `INT_MIN / -1`.
    #[inline]
    pub fn div_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(self.division_by_zero_boxed(l, r));
        }
        let v = l / r;
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.int_overflow(l, '/', r))
        }
    }

    /// `l % r` on raw ints: `DivisionByZero` for a zero divisor.
    #[inline]
    pub fn mod_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(self.division_by_zero_boxed(l, r));
        }
        Ok(l % r)
    }

    /// `l << r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shl_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        expect_int(self.int_shl(l, r)?)
    }

    /// `l >> r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shr_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        expect_int(self.int_shr(l, r)?)
    }

    #[allow(
        clippy::unnecessary_box_returns,
        reason = "boxing out of line keeps the allocation off the inlined fast path"
    )]
    #[cold]
    #[inline(never)]
    fn neg_overflow(&mut self, n: i64) -> Box<VmError> {
        Box::new(self.integer_overflow(format!("-({n}) overflows int")))
    }

    /// `-n` on a raw int: `IntegerOverflow` for `INT_MIN`.
    #[inline]
    pub fn neg_i64(&mut self, n: i64) -> Result<i64, Box<VmError>> {
        let v = n.wrapping_neg();
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(self.neg_overflow(n))
        }
    }

    /// `!value` with BAML truthiness.
    #[inline]
    #[must_use]
    pub fn not(&self, value: Value) -> bool {
        !self.is_truthy(value)
    }
}
