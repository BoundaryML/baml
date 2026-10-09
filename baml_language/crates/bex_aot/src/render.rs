//! `to_string()` for every value a native program can hold: the VM's
//! `baml._to_string_default` structural walk (`package_baml/root.rs`,
//! `render_to_sink`).
//!
//! Rules, in the VM's words:
//!
//! - `int` decimal, `bool` `true`/`false`, `null` → `null`, `float` via
//!   [`bex_lang::float::format`].
//! - A `string` prints bare at the top level and in Rust `{:?}` form (quoted,
//!   escaped) when nested inside an array or class.
//! - Arrays print `[a, b]`, elements nested; the empty array is `[]`.
//! - A class prints `Name { f: v, g: w }` with the fields in declaration order
//!   and the *unqualified* class name; a class with no fields prints just
//!   `Name`. The class is a handle, which renders as its pointee.
//! - `T | null` renders the inner value or `null`.
//!
//! Classes with a user `to_string` override are the generated code's concern:
//! its `ToBaml` impl calls the override.

use crate::{Int63, Str, float, handle::Shared};

/// Structural `to_string`. Implementors write into `out`; [`ToBaml::to_baml`]
/// is the top-level entry (`nested == false`).
pub trait ToBaml {
    /// Append this value's rendering. `nested` is true inside an array or
    /// class, where strings are quoted.
    fn render(&self, out: &mut String, nested: bool);

    /// `value.to_string()`.
    fn to_baml(&self) -> Str {
        Str::from(to_std(self))
    }
}

/// `value.to_string()` as a Rust `String`.
pub fn to_std<T: ToBaml + ?Sized>(value: &T) -> String {
    let mut out = String::new();
    value.render(&mut out, false);
    out
}

/// `value.to_string()`: the `baml._to_string_default<T>` entry point.
pub fn to_string<T: ToBaml + ?Sized>(value: &T) -> Str {
    value.to_baml()
}

/// Render a class instance for a generated `ToBaml` impl: `Name` when it has
/// no fields, otherwise `Name { f: v, g: w }` with every field rendered nested.
pub fn class(out: &mut String, name: &str, fields: &[(&str, &dyn ToBaml)]) {
    out.push_str(name);
    if fields.is_empty() {
        return;
    }
    out.push_str(" { ");
    for (i, (field, value)) in fields.iter().enumerate() {
        if i != 0 {
            out.push_str(", ");
        }
        out.push_str(field);
        out.push_str(": ");
        value.render(out, true);
    }
    out.push_str(" }");
}

impl ToBaml for Int63 {
    fn render(&self, out: &mut String, _nested: bool) {
        use std::fmt::Write;
        let _ = write!(out, "{}", self.get());
    }
}

impl ToBaml for bool {
    fn render(&self, out: &mut String, _nested: bool) {
        out.push_str(if *self { "true" } else { "false" });
    }
}

impl ToBaml for f64 {
    fn render(&self, out: &mut String, _nested: bool) {
        out.push_str(&float::format(*self));
    }
}

/// `null`.
impl ToBaml for () {
    fn render(&self, out: &mut String, _nested: bool) {
        out.push_str("null");
    }
}

impl ToBaml for Str {
    fn render(&self, out: &mut String, nested: bool) {
        self.as_str().render(out, nested);
    }
}

impl ToBaml for str {
    fn render(&self, out: &mut String, nested: bool) {
        if nested {
            use std::fmt::Write;
            let _ = write!(out, "{self:?}");
        } else {
            out.push_str(self);
        }
    }
}

impl<T: ToBaml> ToBaml for Option<T> {
    fn render(&self, out: &mut String, nested: bool) {
        match self {
            Some(value) => value.render(out, nested),
            None => out.push_str("null"),
        }
    }
}

impl<T: ToBaml> ToBaml for Shared<T> {
    fn render(&self, out: &mut String, nested: bool) {
        self.borrow().render(out, nested);
    }
}

impl<T: ToBaml> ToBaml for Vec<T> {
    fn render(&self, out: &mut String, nested: bool) {
        self.as_slice().render(out, nested);
    }
}

impl<T: ToBaml> ToBaml for [T] {
    fn render(&self, out: &mut String, _nested: bool) {
        out.push('[');
        for (i, item) in self.iter().enumerate() {
            if i != 0 {
                out.push_str(", ");
            }
            item.render(out, true);
        }
        out.push(']');
    }
}

impl<T: ToBaml + ?Sized> ToBaml for &T {
    fn render(&self, out: &mut String, nested: bool) {
        (**self).render(out, nested);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{array, handle::shared, string::from_literal};

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    struct Point {
        x: Int63,
        y: f64,
        label: Str,
    }

    impl ToBaml for Point {
        fn render(&self, out: &mut String, _nested: bool) {
            class(
                out,
                "Point",
                &[("x", &self.x), ("y", &self.y), ("label", &self.label)],
            );
        }
    }

    struct Empty;

    impl ToBaml for Empty {
        fn render(&self, out: &mut String, _nested: bool) {
            class(out, "Empty", &[]);
        }
    }

    struct Node {
        children: Shared<Vec<Shared<Node>>>,
        tag: Option<Str>,
        next: Option<Shared<Node>>,
    }

    impl ToBaml for Node {
        fn render(&self, out: &mut String, _nested: bool) {
            class(
                out,
                "Node",
                &[
                    ("children", &self.children),
                    ("tag", &self.tag),
                    ("next", &self.next),
                ],
            );
        }
    }

    #[test]
    fn scalars_render_like_the_vm() {
        assert_eq!(to_std(&int(-42)), "-42");
        assert_eq!(to_std(&true), "true");
        assert_eq!(to_std(&false), "false");
        assert_eq!(to_std(&1.0), "1.0");
        assert_eq!(to_std(&f64::NAN), "NaN");
        assert_eq!(to_std(&()), "null");
        assert_eq!(to_std(&from_literal("plain \"text\"")), "plain \"text\"");
        assert_eq!(to_string(&from_literal("x")).as_str(), "x");
        assert_eq!(to_std(&None::<Int63>), "null");
        assert_eq!(to_std(&Some(int(3))), "3");
        assert_eq!(to_std(&Some(from_literal("bare"))), "bare");
    }

    #[test]
    fn arrays_quote_nested_strings() {
        let strings = array::new(vec![from_literal("a"), from_literal("b\n\"c\"")]);
        assert_eq!(to_std(&strings), r#"["a", "b\n\"c\""]"#);
        let ints = array::new(vec![int(1), int(2)]);
        assert_eq!(to_std(&ints), "[1, 2]");
        let empty: Shared<Vec<Int63>> = array::new(Vec::new());
        assert_eq!(to_std(&empty), "[]");
        let nested = array::new(vec![
            array::new(vec![1.5, f64::INFINITY]),
            array::new(vec![]),
        ]);
        assert_eq!(to_std(&nested), "[[1.5, Infinity], []]");
        let optional = array::new(vec![Some(from_literal("s")), None]);
        assert_eq!(to_std(&optional), r#"["s", null]"#);
        let bools = vec![true, false];
        assert_eq!(to_std(&bools), "[true, false]");
    }

    #[test]
    fn classes_render_fields_in_order_and_quote_strings() {
        let point = shared(Point {
            x: int(1),
            y: 2.0,
            label: from_literal("origin"),
        });
        assert_eq!(to_std(&point), r#"Point { x: 1, y: 2.0, label: "origin" }"#);
        assert_eq!(to_std(&Empty), "Empty");
        assert_eq!(to_std(&array::new(vec![shared(Empty)])), "[Empty]");
        let points = array::new(vec![point]);
        assert_eq!(
            to_std(&points),
            r#"[Point { x: 1, y: 2.0, label: "origin" }]"#
        );
    }

    #[test]
    fn nested_classes_and_nulls() {
        let leaf = shared(Node {
            children: array::new(Vec::new()),
            tag: Some(from_literal("leaf")),
            next: None,
        });
        let root = Node {
            children: array::new(vec![leaf.clone()]),
            tag: None,
            next: Some(leaf),
        };
        assert_eq!(
            to_std(&root),
            r#"Node { children: [Node { children: [], tag: "leaf", next: null }], tag: null, next: Node { children: [], tag: "leaf", next: null } }"#
        );
    }
}
