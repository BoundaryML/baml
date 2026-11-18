//! The free functions of `baml`.

use super::{Elem, NativeBaml, Natives, Scope, Thrown, TypeId, json};

impl NativeBaml for Natives {
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
}
