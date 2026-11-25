//! The free functions of `baml`.

use super::{
    Arr, ArrRef, Elem, Float, Int, NativeBaml, Natives, Root, Scope, Slot, Thrown, TypeId, json,
};
use crate::layout::{TAG_BIGINT, TAG_BOX_FLOAT, TAG_BOX_INT, TAG_STRING};

impl NativeBaml for Natives {
    /// What a `from_json` nothing overrides returns: the value read by its
    /// fields, with declared names, each field by its own `from_json`.
    async fn _from_json_structural_default<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        j: Elem<'s>,
    ) -> Result<usize, Thrown<'s>> {
        json::decode::to(scope, types[0], j, false, true).await
    }

    /// What a `to_json` nothing overrides returns: `baml.json.from` with
    /// declared names and every field.
    async fn _to_json_default<'s>(
        scope: &mut Scope<'s>,
        types: [TypeId; 1],
        value: Elem<'s>,
    ) -> Result<usize, Thrown<'s>> {
        json::to_json_value(scope, types[0], value, false, false).await
    }

    /// The `eq` of a class that writes none: its fields, compared.
    async fn _equals_structural_default<'s>(
        scope: &mut Scope<'s>,
        a_type: TypeId,
        a: Elem<'s>,
        b_type: TypeId,
        b: Elem<'s>,
    ) -> Result<bool, Thrown<'s>> {
        super::structural::equals(scope, (a, a_type), (b, b_type)).await
    }

    /// `value` without its fraction, as the nearest `int`: an end of the
    /// 64-bit range for a value past it, and zero for NaN.
    fn _trunc_to_int(value: f64) -> i64 {
        #[expect(
            clippy::cast_possible_truncation,
            reason = "the conversion saturates, which is what is asked for"
        )]
        let truncated = value as i64;
        truncated
    }

    /// Whether `_rust_sort` sorts arrays of the element type: an `int`, a
    /// `float`, a `string` or a `bigint`.
    fn _is_primitive_array(types: [TypeId; 1], _arr: ArrRef<'_, Slot>) -> bool {
        matches!(
            types[0].0 as usize,
            TAG_BOX_INT | TAG_BOX_FLOAT | TAG_STRING | TAG_BIGINT
        )
    }

    /// Sort `arr` in place by the order of its elements, and return it. The
    /// sort is stable. The call names the type of the elements, where any
    /// other names the type of its argument.
    fn _rust_sort(types: [TypeId; 1], arr: usize) -> usize {
        // SAFETY: the compiler passes an array.
        let mut elements = unsafe { crate::array::values(arr) };
        match types[0].0 as usize {
            TAG_BOX_INT => elements.sort_by_key(|element| element.payload.cast_signed()),
            TAG_BOX_FLOAT => elements.sort_by(|a, b| {
                float_order(
                    f64::from_bits(a.payload as u64),
                    f64::from_bits(b.payload as u64),
                )
            }),
            // SAFETY: every element of a `string[]` is a live string.
            TAG_STRING => elements.sort_by(|a, b| unsafe {
                crate::string::text(a.payload).cmp(crate::string::text(b.payload))
            }),
            TAG_BIGINT => {
                // Each value is read out of its object once.
                // SAFETY: every element of a `bigint[]` is a live `bigint`.
                let mut keyed: Vec<_> = elements
                    .iter()
                    .map(|element| (unsafe { crate::bigint::value(element.payload) }, *element))
                    .collect();
                keyed.sort_by(|a, b| a.0.cmp(&b.0));
                for (element, (_, sorted)) in elements.iter_mut().zip(keyed) {
                    *element = sorted;
                }
            }
            other => unreachable!("_rust_sort is compiled for no element type with id {other}"),
        }
        // SAFETY: the array has the elements just read.
        unsafe { crate::array::put(arr, 0, &elements) };
        arr
    }

    /// The sum of `values`, wrapping as `+` does.
    fn _sum_int(values: ArrRef<'_, Int>) -> i64 {
        values.iter().fold(0, i64::wrapping_add)
    }

    fn _sum_float(values: ArrRef<'_, Float>) -> f64 {
        values.iter().sum()
    }

    /// The middle of `values` in the order of `float`, which ranks NaN
    /// above every number, or the mean of the two in the middle.
    fn _median_float<'s>(
        scope: &mut Scope<'s>,
        values: Root<'s, Arr<Float>>,
    ) -> Result<f64, Thrown<'s>> {
        let mut sorted: Vec<f64> = values.get(scope).iter().collect();
        if sorted.is_empty() {
            return Err(
                scope.invalid_argument("float[].median: cannot take the median of an empty array")
            );
        }
        sorted.sort_by(|a, b| float_order(*a, *b));
        let middle = sorted.len() / 2;
        Ok(if sorted.len() % 2 == 1 {
            sorted[middle]
        } else {
            f64::midpoint(sorted[middle - 1], sorted[middle])
        })
    }
}

/// The order of `float`: that of the numbers, with the two zeros equal,
/// NaN above every number and equal to itself.
pub(super) fn float_order(a: f64, b: f64) -> std::cmp::Ordering {
    a.partial_cmp(&b)
        .unwrap_or_else(|| a.is_nan().cmp(&b.is_nan()))
}
