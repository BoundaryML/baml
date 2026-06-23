use bex_vm_types::{Object, ObjectType, Value};

use super::{BamlNamespaceMath, PackageBamlImpl};
use crate::{
    BexVm,
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
        sorted.sort_by(|left, right| left.total_cmp(right));
        let mid = sorted.len() / 2;
        if sorted.len() % 2 == 1 {
            Ok(sorted[mid])
        } else {
            Ok((sorted[mid - 1] + sorted[mid]) / 2.0)
        }
    }
}
