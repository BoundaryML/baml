//! `baml.Float`.

use super::{NativeBamlFloat, Natives, Scope, Thrown};

/// The `int`s a `float` converts to: `-2^62 <= value < 2^62`.
const MIN: f64 = -4_611_686_018_427_387_904.0;
const MAX_PLUS_ONE: f64 = 4_611_686_018_427_387_904.0;

fn to_int<'s>(scope: &mut Scope<'s>, value: f64, operation: &str) -> Result<i64, Thrown<'s>> {
    if value.is_nan() {
        return Err(
            scope.invalid_argument(&format!("float.{operation}: cannot convert NaN to int"))
        );
    }
    if !(MIN..MAX_PLUS_ONE).contains(&value) {
        return Err(
            scope.invalid_argument(&format!("float.{operation}: {value} is out of int range"))
        );
    }
    #[expect(
        clippy::cast_possible_truncation,
        reason = "the value is whole and in range"
    )]
    Ok(value as i64)
}

impl NativeBamlFloat for Natives {
    fn sqrt(float: f64) -> f64 {
        float.sqrt()
    }

    fn itrunc<'s>(scope: &mut Scope<'s>, float: f64) -> Result<i64, Thrown<'s>> {
        to_int(scope, float.trunc(), "itrunc")
    }
}
