use bex_vm_types::{Object, ObjectType, Value};

use super::{BamlNamespaceMath, PackageBamlImpl};
use crate::{
    BexVm, VmPanic,
    errors::{VmBamlError, VmRustFnError},
};

/// Returns a human-readable runtime type name for diagnostics.
fn value_type_name(vm: &BexVm, value: Value) -> String {
    if value.is_null() {
        return "null".to_string();
    }
    if value.as_int().is_some() {
        return "int".to_string();
    }
    if value.as_bool().is_some() {
        return "bool".to_string();
    }
    if value.is_omitted() {
        return "omitted".to_string();
    }
    if let Some(ptr) = value.as_object_ptr() {
        return ObjectType::of(vm.get_object(ptr)).to_string();
    }
    "unknown".to_string()
}

/// Extracts a float from a validated `float[]` element.
///
/// `baml.math.sum/mean/median` are all declared on `float[]`, so by the time
/// execution reaches this native path each element must be a boxed float. Any
/// other tag means an invariant was violated upstream.
fn expect_float(vm: &BexVm, value: Value, fn_name: &str, index: usize) -> f64 {
    let Some(ptr) = value.as_object_ptr() else {
        unreachable!(
            "{fn_name}: expected float at index {index}, got {}",
            value_type_name(vm, value)
        );
    };
    match vm.get_object(ptr) {
        Object::Float(float) => *float,
        _ => unreachable!(
            "{fn_name}: expected float at index {index}, got {}",
            value_type_name(vm, value)
        ),
    }
}

/// Extracts an `i64` from a validated `int[]` element.
///
/// `int[].sum()` reaches this native path only after the type system has proven
/// every element is an `int` (ints are unboxed tagged values, so no heap read is
/// needed). Any other tag means an invariant was violated upstream.
fn expect_int(value: Value, fn_name: &str, index: usize) -> i64 {
    value
        .as_int()
        .unwrap_or_else(|| unreachable!("{fn_name}: expected int at index {index}"))
}

impl BamlNamespaceMath for PackageBamlImpl {
    #[allow(clippy::cast_possible_truncation)]
    fn trunc(value: f64) -> i64 {
        value as i64
    }

    /// Returns the arithmetic sum of all values in `values`.
    fn sum(vm: &BexVm, values: &[Value]) -> f64 {
        values
            .iter()
            .enumerate()
            .map(|(index, value)| expect_float(vm, *value, "math.sum", index))
            .sum()
    }

    /// Returns the arithmetic sum of all values in an `int[]`.
    ///
    /// Accumulates left-to-right from `0`, checking the running total against
    /// the `int` (i63) range at each step so overflow raises `IntegerOverflow`
    /// exactly like repeated `+` would. Both `acc` and each element are already
    /// in the i63 range, so the intermediate i64 add never wraps; the meaningful
    /// bound is the tighter i63 range check. Internal helper backing
    /// `int[].sum()`; the empty array sums to `0`.
    fn _sum_int(values: &[Value]) -> Result<i64, VmRustFnError> {
        let mut acc: i64 = 0;
        for (index, value) in values.iter().enumerate() {
            let x = expect_int(*value, "math._sum_int", index);
            match acc.checked_add(x) {
                Some(v) if (Value::INT_MIN..=Value::INT_MAX).contains(&v) => acc = v,
                _ => {
                    return Err(VmPanic::IntegerOverflow {
                        message: format!("{acc} + {x} overflows int"),
                    }
                    .into());
                }
            }
        }
        Ok(acc)
    }

    /// Returns the arithmetic mean of `values`.
    ///
    /// Throws `InvalidArgument` when `values` is empty.
    #[allow(clippy::cast_precision_loss)]
    fn mean(vm: &BexVm, values: &[Value]) -> Result<f64, VmRustFnError> {
        if values.is_empty() {
            return Err(VmBamlError::InvalidArgument {
                message: "math.mean: cannot take the mean of an empty array".to_string(),
            }
            .into());
        }
        let n = values.len() as f64;
        Ok(Self::sum(vm, values) / n)
    }

    /// Returns the median value of `values`.
    ///
    /// Sorting uses `f64::total_cmp` to match BAML's total float ordering.
    /// Throws `InvalidArgument` when `values` is empty.
    fn median(vm: &BexVm, values: &[Value]) -> Result<f64, VmRustFnError> {
        if values.is_empty() {
            return Err(VmBamlError::InvalidArgument {
                message: "math.median: cannot take the median of an empty array".to_string(),
            }
            .into());
        }
        let mut sorted: Vec<f64> = values
            .iter()
            .enumerate()
            .map(|(index, value)| expect_float(vm, *value, "math.median", index))
            .collect();
        sorted.sort_by(f64::total_cmp);
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            Ok(sorted[mid])
        } else {
            Ok(f64::midpoint(sorted[mid - 1], sorted[mid]))
        }
    }
}
