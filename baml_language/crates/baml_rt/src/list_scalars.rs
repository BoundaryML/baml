//! Scalar operations required at typed list boundaries.
use crate::{
    R,
    lists::List,
    strings::{Cx, Str},
};
pub fn itrunc(cx: &mut Cx, value: f64) -> R<i64> {
    // Same truncation, i63 interval, and messages as package_baml::float.
    let value = value.trunc();
    if value.is_nan() {
        return Err(crate::list_errors::invalid_argument(
            cx,
            "float.itrunc: cannot convert NaN to int".into(),
        ));
    }
    if !(-4_611_686_018_427_387_904.0..4_611_686_018_427_387_904.0).contains(&value) {
        return Err(crate::list_errors::invalid_argument(
            cx,
            format!("float.itrunc: {value} is out of int range"),
        ));
    }
    Ok(value as i64)
}
pub fn from_code_points(cx: &mut Cx, values: List<i64>) -> R<Str> {
    let mut text = String::with_capacity(values.len(cx) as usize);
    for i in 0..values.len(cx) {
        let value = values.get(cx, i)?;
        let code=u32::try_from(value).ok().and_then(char::from_u32).ok_or_else(||crate::list_errors::invalid_argument(cx,format!("string.from_code_points: value {value} at index {i} is not a valid Unicode code point (must be in [0, 0x10FFFF] and not a surrogate)")))?;
        text.push(code);
    }
    cx.string(&text)
}
