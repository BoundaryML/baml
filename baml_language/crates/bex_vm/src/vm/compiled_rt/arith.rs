//! Arithmetic over tagged values and unboxed numeric locals.

use bex_heap::TlabHolder;
use bex_vm_types::{BinOp, CmpOp, Object, UnaryOp, Value};

use super::checker_hole;
use crate::{
    errors::{VmError, VmInternalError, VmPanic},
    vm::{BexVm, value_as_float},
};

impl BexVm {
    /// Apply a binary operator to tagged values, including mixed numeric types and string
    /// concatenation.
    #[inline]
    pub fn binop(&mut self, op: BinOp, left: Value, right: Value) -> Result<Value, Box<VmError>> {
        self.binop_values(op, left, right).map_err(Box::new)
    }

    /// Compare numeric values, strings, byte arrays, enum variants, or types.
    #[inline]
    pub fn cmpop(&mut self, op: CmpOp, left: Value, right: Value) -> Result<Value, Box<VmError>> {
        self.cmpop_values(op, left, right).map_err(Box::new)
    }

    /// Add tagged integers with i63 overflow checking.
    #[inline]
    pub fn add_int(&mut self, l: Value, r: Value) -> Result<Value, Box<VmError>> {
        match Value::tagged_int_add_checked(l, r) {
            Some(v) => Ok(v),
            None => Err(Box::new(self.tagged_int_overflow(l, '+', r))),
        }
    }

    /// Box a float for use as a tagged value.
    #[inline]
    pub fn float_value(&mut self, f: f64) -> Value {
        Value::object(self.alloc_float(f))
    }

    /// Unary minus over `int`, `float`, and `bigint`.
    pub fn neg(&mut self, val: Value) -> Result<Value, Box<VmError>> {
        if let Some(n) = val.as_int() {
            // Negating -2^62 fits i64 but exceeds the i63 range.
            return match Value::try_int(n.wrapping_neg()) {
                Some(v) => Ok(v),
                None => Err(Box::new(
                    self.integer_overflow(format!("-({n}) overflows int")),
                )),
            };
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

    /// Build a catchable `baml.panics.DivisionByZero` throw for `l / 0`.
    #[cold]
    #[inline(never)]
    pub fn division_by_zero(&mut self, l: i64, r: i64) -> VmError {
        VmError::thrown_fresh(self.panic_to_exception_value(VmPanic::DivisionByZero {
            left: Value::int(l),
            right: Value::int(r),
        }))
    }

    /// `l + r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn add_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        let v = l.wrapping_add(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(Box::new(
                self.integer_overflow(format!("{l} + {r} overflows int")),
            ))
        }
    }

    /// `l - r` on raw ints, overflow-checked into the i63 range.
    #[inline]
    pub fn sub_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        let v = l.wrapping_sub(r);
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(Box::new(
                self.integer_overflow(format!("{l} - {r} overflows int")),
            ))
        }
    }

    /// `l * r` on raw ints, overflow-checked.
    #[inline]
    pub fn mul_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        match l.checked_mul(r).filter(|v| Value::try_int(*v).is_some()) {
            Some(v) => Ok(v),
            None => Err(Box::new(
                self.integer_overflow(format!("{l} * {r} overflows int")),
            )),
        }
    }

    /// `l / r` on raw ints: `DivisionByZero` for a zero divisor,
    /// `IntegerOverflow` for `INT_MIN / -1`.
    #[inline]
    pub fn div_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(Box::new(self.division_by_zero(l, r)));
        }
        let v = l / r;
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(Box::new(
                self.integer_overflow(format!("{l} / {r} overflows int")),
            ))
        }
    }

    /// `l % r` on raw ints: `DivisionByZero` for a zero divisor.
    #[inline]
    pub fn mod_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        if r == 0 {
            return Err(Box::new(self.division_by_zero(l, r)));
        }
        Ok(l % r)
    }

    /// `l << r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shl_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        self.int_shl(l, r)?
            .as_int()
            .ok_or_else(|| Box::new(checker_hole("int")))
    }

    /// `l >> r` on raw ints with the i63 semantics constant folding uses.
    #[inline]
    pub fn shr_i64(&mut self, l: i64, r: i64) -> Result<i64, Box<VmError>> {
        self.int_shr(l, r)?
            .as_int()
            .ok_or_else(|| Box::new(checker_hole("int")))
    }

    /// `-n` on a raw int: `IntegerOverflow` for `INT_MIN`.
    #[inline]
    pub fn neg_i64(&mut self, n: i64) -> Result<i64, Box<VmError>> {
        let v = n.wrapping_neg();
        if Value::try_int(v).is_some() {
            Ok(v)
        } else {
            Err(Box::new(
                self.integer_overflow(format!("-({n}) overflows int")),
            ))
        }
    }

    /// `!value` with BAML truthiness.
    #[inline]
    #[must_use]
    pub fn not(&self, value: Value) -> bool {
        !self.is_truthy(value)
    }
}
