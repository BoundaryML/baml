//! Arithmetic over tagged values and unboxed numeric locals.

use baml_type::IntShiftError;
use bex_vm_types::{BinOp, CmpOp, Object, UnaryOp, Value};

use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::BexVm,
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
        let result = self.join_strings(parts).map_err(|e| Box::new(e.into()))?;
        Ok(self.alloc_joined_string(result))
    }

    /// The `Value` of a float: inline when it can be, boxed otherwise. See
    /// [`bex_heap::Tlab::float_value`]. Compiled bodies keep float locals as
    /// raw `f64` and box them through here.
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn float_value(&mut self, f: f64) -> Value {
        self.tlab.float_value(f)
    }

    /// The `Value` of an int: inline when it can be, boxed otherwise. See
    /// [`bex_heap::Tlab::int_value`].
    #[inline(always)]
    #[allow(clippy::inline_always)]
    pub fn int_value(&mut self, i: i64) -> Value {
        self.tlab.int_value(i)
    }

    pub fn neg(&mut self, val: Value) -> Result<Value, Box<VmError>> {
        if let Some(n) = val.as_int() {
            let n = self.neg_i64(n)?;
            return Ok(self.int_value(n));
        }
        if let Some(n) = val.as_float() {
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
        let left = self.int_value(l);
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::DivisionByZero {
            left,
            right: Value::int(r),
        }))
    }

    #[cold]
    #[inline(never)]
    fn division_by_zero_boxed(&mut self, l: i64, r: i64) -> Box<VmError> {
        Box::new(self.division_by_zero(l, r))
    }

    // Int arithmetic wraps (two's complement). The add, sub, mul and neg
    // helpers keep the fallible signature every compiled binary op shares.

    /// `l + r` on raw ints, wrapping.
    #[inline]
    pub fn add_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        Ok(l.wrapping_add(r))
    }

    /// `l - r` on raw ints, wrapping.
    #[inline]
    pub fn sub_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        Ok(l.wrapping_sub(r))
    }

    /// `l * r` on raw ints, wrapping.
    #[inline]
    pub fn mul_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        Ok(l.wrapping_mul(r))
    }

    /// `l / r` on raw ints: `DivisionByZero` for a zero divisor; `MIN / -1`
    /// wraps to `MIN`.
    #[inline]
    pub fn div_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(self.division_by_zero_boxed(l, r));
        }
        Ok(l.wrapping_div(r))
    }

    /// `l % r` on raw ints: `DivisionByZero` for a zero divisor; `MIN % -1`
    /// is `0`.
    #[inline]
    pub fn mod_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(self.division_by_zero_boxed(l, r));
        }
        Ok(l.wrapping_rem(r))
    }

    /// `l << r` on raw ints with the semantics constant folding uses.
    #[inline]
    pub fn shl_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        baml_type::int::shl(l, r)
            .map_err(|IntShiftError::NegativeCount(count)| Box::new(self.negative_bit_shift(count)))
    }

    /// `l >> r` on raw ints with the semantics constant folding uses.
    #[inline]
    pub fn shr_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        baml_type::int::shr(l, r)
            .map_err(|IntShiftError::NegativeCount(count)| Box::new(self.negative_bit_shift(count)))
    }

    /// `-n` on a raw int, wrapping: `-MIN` is `MIN`.
    #[inline]
    pub fn neg_i64(&mut self, n: i64) -> Result<i64, Box<VmError>> {
        Ok(n.wrapping_neg())
    }

    /// `!value` with BAML truthiness.
    #[inline]
    #[must_use]
    pub fn not(&self, value: Value) -> bool {
        !self.is_truthy(value)
    }
}
