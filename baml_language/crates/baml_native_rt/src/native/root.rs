//! The free functions of `baml`.

use super::{NativeBaml, Natives};

impl NativeBaml for Natives {
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
}
