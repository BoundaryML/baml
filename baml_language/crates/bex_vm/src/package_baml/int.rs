use bex_vm_types::Value;

use super::{BamlClassInt, PackageBamlImpl};
use crate::errors::{VmBamlError, VmRustFnError};

impl BamlClassInt for PackageBamlImpl {
    fn to_json(int: i64) -> Value {
        Value::Int(int)
    }
    // ── Comparisons / clamping ────────────────────────────────────────────────

    fn abs(int: i64) -> Result<i64, VmRustFnError> {
        int.checked_abs().ok_or_else(|| {
            VmBamlError::InvalidArgument {
                message: "int.abs: cannot represent the absolute value of int.min_value() (would overflow)".to_string(),
            }
            .into()
        })
    }

    fn min(int: i64, other: i64) -> i64 {
        std::cmp::min(int, other)
    }

    fn max(int: i64, other: i64) -> i64 {
        std::cmp::max(int, other)
    }

    fn clamp(int: i64, min: i64, max: i64) -> i64 {
        // Two-step (clamp) so we do not panic when min > max — Rust's
        // `i64::clamp` debug-asserts `min <= max`. The behavior matches:
        // first cap at max, then floor at min.
        let v = std::cmp::min(int, max);
        std::cmp::max(v, min)
    }

    // ── Bit operations ────────────────────────────────────────────────────────

    fn leading_zeros(int: i64) -> i64 {
        i64::from(int.leading_zeros())
    }

    fn leading_ones(int: i64) -> i64 {
        i64::from(int.leading_ones())
    }

    fn trailing_zeros(int: i64) -> i64 {
        i64::from(int.trailing_zeros())
    }

    fn trailing_ones(int: i64) -> i64 {
        i64::from(int.trailing_ones())
    }

    fn count_zeros(int: i64) -> i64 {
        i64::from(int.count_zeros())
    }

    fn count_ones(int: i64) -> i64 {
        i64::from(int.count_ones())
    }

    // Note: `max_value()` and `min_value()` are implemented directly in
    // `int.baml` as BAML literal expressions — no Rust trampoline needed.
}
