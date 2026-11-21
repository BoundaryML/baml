//! The free functions of `baml`.

use super::{
    Arr, ArrRef, Elem, Float, Int, NativeBaml, Natives, Root, Scope, Slot, Thrown, TypeId, json,
};
use crate::layout::{TYPE_BIGINT, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_STRING};

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
            TYPE_BOX_INT | TYPE_BOX_FLOAT | TYPE_STRING | TYPE_BIGINT
        )
    }

    /// Sort `arr` in place by the order of its elements, and return it. The
    /// sort is stable. The call names the type of the elements, where any
    /// other names the type of its argument: the words of an `int[]` and of
    /// a `float[]` do not say which they are.
    fn _rust_sort(types: [TypeId; 1], arr: usize) -> usize {
        // SAFETY: the compiler passes an array, which nothing else reads
        // while a native that does not allocate runs.
        let words = unsafe { super::array::elements(arr) };
        match types[0].0 as usize {
            TYPE_BOX_INT => words.sort_by_key(|word| word.cast_signed()),
            TYPE_BOX_FLOAT => words
                .sort_by(|a, b| float_order(f64::from_bits(*a as u64), f64::from_bits(*b as u64))),
            // SAFETY: every element of a `string[]` is a live string.
            TYPE_STRING => words
                .sort_by(|a, b| unsafe { crate::string::text(*a).cmp(crate::string::text(*b)) }),
            TYPE_BIGINT => {
                // Each value is read out of its object once.
                // SAFETY: every element of a `bigint[]` is a live `bigint`.
                let mut keyed: Vec<_> = words
                    .iter()
                    .map(|word| (unsafe { crate::bigint::value(*word) }, *word))
                    .collect();
                keyed.sort_by(|a, b| a.0.cmp(&b.0));
                for (word, (_, sorted)) in words.iter_mut().zip(keyed) {
                    *word = sorted;
                }
            }
            other => unreachable!("_rust_sort is compiled for no element type with id {other}"),
        }
        arr
    }

    /// The sum of `values`, wrapping as `+` does.
    fn _sum_int(values: ArrRef<'_, Int>) -> i64 {
        values
            .as_slice()
            .iter()
            .fold(0, |sum, value| sum.wrapping_add(*value))
    }

    fn _sum_float(values: ArrRef<'_, Float>) -> f64 {
        values.as_slice().iter().sum()
    }

    /// The middle of `values` in the order of `float`, which ranks NaN
    /// above every number, or the mean of the two in the middle.
    fn _median_float<'s>(
        scope: &mut Scope<'s>,
        values: Root<'s, Arr<Float>>,
    ) -> Result<f64, Thrown<'s>> {
        let mut sorted = values.get(scope).as_slice().to_vec();
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
