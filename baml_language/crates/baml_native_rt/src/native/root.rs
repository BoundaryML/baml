//! The free functions of `baml`.

use super::{
    Arr, ArrRef, Elem, Float, Int, NativeBaml, Natives, Root, Scope, Slot, Thrown, TypeId, json,
};
use crate::layout::{TYPE_BIGINT, TYPE_BOX_FLOAT, TYPE_BOX_INT, TYPE_STRING};

/// The values of an `int` on the interpreter, which keeps one in 63 bits.
const INTERPRETED_INT: std::ops::RangeInclusive<i64> = -(1 << 62)..=(1 << 62) - 1;

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

    /// The sum of `values`, or a panic as soon as the running total leaves
    /// the range of an `int` on the interpreter, which is a bit narrower
    /// than a native one. A total past that range is one only native code
    /// could hold, and a total past 64 bits one nothing holds, so neither
    /// is returned as if it were the sum.
    fn _sum_int<'s>(scope: &mut Scope<'s>, values: Root<'s, Arr<Int>>) -> Result<i64, Thrown<'s>> {
        let slice = values.get(scope).as_slice();
        // One pass nothing branches in gives the wrapping sum and the bits
        // any value sets, a negative one counted by its complement. No
        // value is larger than two to the number of those bits, so when
        // that many times the count stays in the range, no running total
        // leaves it, and the wrapping sum is the sum.
        let (wrapped, bits) = slice.iter().fold((0_i64, 0_i64), |(sum, bits), value| {
            (sum.wrapping_add(*value), bits | (value ^ (value >> 63)))
        });
        let width = i64::BITS - bits.leading_zeros();
        let within = width < 62
            && (slice.len() as u64)
                .checked_mul(1 << width)
                .is_some_and(|bound| bound <= INTERPRETED_INT.end().cast_unsigned());
        if within {
            return Ok(wrapped);
        }
        let mut sum: i64 = 0;
        let mut overflow = None;
        for value in slice {
            match sum.checked_add(*value) {
                Some(total) if INTERPRETED_INT.contains(&total) => sum = total,
                _ => {
                    overflow = Some(*value);
                    break;
                }
            }
        }
        match overflow {
            Some(value) => Err(scope.integer_overflow(&format!("{sum} + {value} overflows int"))),
            None => Ok(sum),
        }
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
