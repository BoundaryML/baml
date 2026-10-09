//! How the engine prints the fields of an uncaught throw
//! (`BexExternalValue::render_readable`), for the values a native program
//! can put in a thrown class.
//!
//! This is not `to_string`: a string is always quoted (`{:?}`), a float
//! prints through `f64`'s `Display` with `.0` appended to an integral
//! finite value, a class prints its *fully qualified* name with no space
//! inside the braces (`user.Inner {n: 1}`), a map quotes its keys as the
//! strings the engine converts them to, and an enum is its variant's name.

use crate::{BigInt, Int63, Str, handle::Shared, map::Map};

/// A value rendered as the engine prints it inside an uncaught throw.
pub trait Readable {
    /// The rendering.
    fn readable(&self) -> String;
}

impl Readable for Int63 {
    fn readable(&self) -> String {
        self.get().to_string()
    }
}

impl Readable for bool {
    fn readable(&self) -> String {
        self.to_string()
    }
}

impl Readable for f64 {
    fn readable(&self) -> String {
        let text = self.to_string();
        if text.contains('.') || !self.is_finite() {
            text
        } else {
            format!("{text}.0")
        }
    }
}

impl Readable for BigInt {
    fn readable(&self) -> String {
        self.to_string()
    }
}

impl Readable for Str {
    fn readable(&self) -> String {
        format!("{self:?}")
    }
}

impl Readable for () {
    fn readable(&self) -> String {
        "null".to_string()
    }
}

impl<T: Readable> Readable for Option<T> {
    fn readable(&self) -> String {
        match self {
            Some(value) => value.readable(),
            None => "null".to_string(),
        }
    }
}

impl<T: Readable> Readable for Shared<T> {
    fn readable(&self) -> String {
        self.borrow().readable()
    }
}

impl<T: Readable> Readable for Vec<T> {
    fn readable(&self) -> String {
        let items: Vec<String> = self.iter().map(Readable::readable).collect();
        format!("[{}]", items.join(", "))
    }
}

/// A map key as the engine converts it: `to_string`, then quoted.
pub trait ReadableKey {
    /// The key's text before quoting.
    fn key_text(&self) -> String;
}

impl ReadableKey for Str {
    fn key_text(&self) -> String {
        self.to_string()
    }
}

impl ReadableKey for Int63 {
    fn key_text(&self) -> String {
        self.get().to_string()
    }
}

impl ReadableKey for bool {
    fn key_text(&self) -> String {
        self.to_string()
    }
}

impl<K: ReadableKey, V: Readable> Readable for Map<K, V> {
    fn readable(&self) -> String {
        let entries: Vec<String> = self
            .handle()
            .borrow()
            .iter()
            .map(|(key, value)| format!("{:?}: {}", key.key_text(), value.readable()))
            .collect();
        format!("{{{}}}", entries.join(", "))
    }
}

/// Render a class instance for a generated [`Readable`] impl, for one
/// nested inside another thrown object's field: `fqn {f: v, ..}`, see
/// [`bex_lang::render_object`].
pub fn class(fqn: &str, fields: &[(&str, String)]) -> String {
    bex_lang::render_object(fqn, fields)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::shared;

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    #[test]
    fn scalars_render_like_the_engine() {
        assert_eq!(int(-7).readable(), "-7");
        assert_eq!(true.readable(), "true");
        assert_eq!(1.0_f64.readable(), "1.0");
        assert_eq!(2.5_f64.readable(), "2.5");
        assert_eq!(f64::INFINITY.readable(), "inf");
        assert_eq!(f64::NAN.readable(), "NaN");
        assert_eq!(1e21_f64.readable(), "1000000000000000000000.0");
        assert_eq!(Str::from("a \"b\"".to_string()).readable(), r#""a \"b\"""#);
        assert_eq!(().readable(), "null");
        assert_eq!(None::<Int63>.readable(), "null");
        assert_eq!(Some(int(3)).readable(), "3");
        assert_eq!(
            crate::bigint::lit("-12345678901234567890").readable(),
            "-12345678901234567890"
        );
    }

    #[test]
    fn containers_render_like_the_engine() {
        let array = shared(vec![Str::from("x".to_string()), Str::from("y".to_string())]);
        assert_eq!(array.readable(), r#"["x", "y"]"#);
        assert_eq!(shared(Vec::<Int63>::new()).readable(), "[]");
        let map = crate::map::new(vec![(int(1), true), (int(2), false)]);
        assert_eq!(map.readable(), r#"{"1": true, "2": false}"#);
        let map = crate::map::new(vec![(Str::from("k".to_string()), int(1))]);
        assert_eq!(map.readable(), r#"{"k": 1}"#);
        assert_eq!(
            class("user.Inner", &[("n", "1".into())]),
            "user.Inner {n: 1}"
        );
        assert_eq!(class("user.Empty", &[]), "user.Empty {}");
    }
}
