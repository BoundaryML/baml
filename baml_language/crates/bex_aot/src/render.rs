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
//! A class with its own `baml.ToString` is honored at every depth, as the
//! VM's `render_to_string_honoring_overrides` does, in three passes:
//!
//! 1. [`ToBaml::collect`] walks the value pre-order and records every object
//!    whose class overrides `to_string` ([`Overridable::OVERRIDES`]), in
//!    render order, without descending into it (its override owns the
//!    subtree). No user code runs, so the borrows the walk takes are safe.
//! 2. [`to_string`] calls each override in that order, under one frame (the
//!    VM's native continuation frame), with no borrow held.
//! 3. The structural walk renders the value over its current state and
//!    splices each override's result in at its pre-order position, matched
//!    by object identity ([`Overridable`]'s `render` through the handle).
//!
//! Passes 1 and 3 visit children in the same order (array elements, map
//! key then value per entry, class fields in declaration order), so the
//! next pending object is a pointer compare, as on the VM. An override
//! that mutates the value is seen by pass 3; an object no longer at its
//! position renders structurally and its result is dropped, as the VM's
//! counter does.

use std::cell::RefCell;

use crate::{Int63, Str, Thrown, depth, float, handle::Shared};

/// An override call recorded by pass 1: the object's identity and the call
/// pass 2 makes.
pub struct Pending {
    id: *const (),
    call: Box<dyn FnOnce() -> Result<Str, Thrown>>,
}

/// Structural `to_string`. Implementors write into `out`; [`to_string`] is
/// the top-level entry (`nested == false`).
pub trait ToBaml {
    /// Append this value's rendering. `nested` is true inside an array or
    /// class, where strings are quoted.
    fn render(&self, out: &mut String, nested: bool);

    /// Pass 1: record the override-bearing objects below this value, in
    /// render order. A leaf records nothing.
    fn collect(&self, _out: &mut Vec<Pending>) {}

    /// The structural rendering alone, no overrides: `string.from` on a
    /// value with no override-bearing object below it.
    fn to_baml(&self) -> Str {
        Str::from(to_std(self))
    }
}

/// What a class does when rendered through its handle: a class with its own
/// `to_string` is recorded in pass 1 and spliced in pass 3; every other
/// class renders structurally. The generated code implements this for
/// every class; arrays take the defaults.
pub trait Overridable: ToBaml + Sized {
    /// Whether the class provides its own `to_string`.
    const OVERRIDES: bool = false;

    /// Pass 2: the class's own `to_string` on this object.
    fn call_override(_this: &Shared<Self>) -> Result<Str, Thrown> {
        unreachable!("call_override on a class without a `to_string` override")
    }
}

impl<T: ToBaml> Overridable for Vec<T> {}

thread_local! {
    /// Pass 3's override results, in pre-order, with the index of the next
    /// one to splice. Empty outside pass 3, when a structural render of an
    /// override-bearing object (no results pending) renders it structurally.
    static SPLICE: RefCell<(Vec<(*const (), String)>, usize)> = const { RefCell::new((Vec::new(), 0)) };
}

/// Pass 3 at an override-bearing object: splice its result if it is the
/// next pending object.
fn splice(id: *const (), out: &mut String) -> bool {
    SPLICE.with(|state| {
        let mut state = state.borrow_mut();
        let (results, next) = &mut *state;
        match results.get(*next) {
            Some((pending, result)) if *pending == id => {
                out.push_str(result);
                *next += 1;
                true
            }
            _ => false,
        }
    })
}

/// `value.to_string()` as a Rust `String`, structurally.
pub fn to_std<T: ToBaml + ?Sized>(value: &T) -> String {
    let mut out = String::new();
    value.render(&mut out, false);
    out
}

/// `value.to_string()`: the `baml._to_string_default<T>` entry point,
/// honoring every `to_string` override below `value` (the module doc).
/// Throws what an override throws, `StackOverflow` included.
pub fn to_string<T: ToBaml + ?Sized>(value: &T) -> Result<Str, Thrown> {
    let mut pending = Vec::new();
    value.collect(&mut pending);
    if pending.is_empty() {
        return Ok(value.to_baml());
    }
    // Pass 2, under the frame the VM's continuation holds while the
    // overrides run.
    let results = {
        let _frame = depth::Guard::enter()?;
        let mut results = Vec::with_capacity(pending.len());
        for Pending { id, call } in pending {
            results.push((id, call()?.as_str().to_owned()));
        }
        results
    };
    // Pass 3. Nothing below runs user code, so no nested `to_string` can
    // disturb the pending results.
    SPLICE.with(|state| *state.borrow_mut() = (results, 0));
    let mut out = String::new();
    value.render(&mut out, false);
    SPLICE.with(|state| *state.borrow_mut() = (Vec::new(), 0));
    Ok(Str::from(out))
}

/// Pass 1 for a generated class: its fields in declaration order.
pub fn collect_fields(out: &mut Vec<Pending>, fields: &[&dyn ToBaml]) {
    for field in fields {
        field.collect(out);
    }
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

    fn collect(&self, out: &mut Vec<Pending>) {
        if let Some(value) = self {
            value.collect(out);
        }
    }
}

/// Through the handle: an override-bearing object is recorded in pass 1
/// and spliced in pass 3 when it is the next pending one; anything else
/// renders its pointee.
impl<T: Overridable + 'static> ToBaml for Shared<T> {
    fn render(&self, out: &mut String, nested: bool) {
        if T::OVERRIDES && splice(self.id(), out) {
            return;
        }
        self.borrow().render(out, nested);
    }

    fn collect(&self, out: &mut Vec<Pending>) {
        if T::OVERRIDES {
            let this = self.clone();
            out.push(Pending {
                id: self.id(),
                call: Box::new(move || T::call_override(&this)),
            });
        } else {
            self.borrow().collect(out);
        }
    }
}

impl<T: ToBaml> ToBaml for Vec<T> {
    fn render(&self, out: &mut String, nested: bool) {
        self.as_slice().render(out, nested);
    }

    fn collect(&self, out: &mut Vec<Pending>) {
        self.as_slice().collect(out);
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

    fn collect(&self, out: &mut Vec<Pending>) {
        for item in self {
            item.collect(out);
        }
    }
}

impl<T: ToBaml + ?Sized> ToBaml for &T {
    fn render(&self, out: &mut String, nested: bool) {
        (**self).render(out, nested);
    }

    fn collect(&self, out: &mut Vec<Pending>) {
        (**self).collect(out);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{array, handle::shared, string::from_literal};

    /// A class with its own `to_string` that counts its calls.
    struct Counter {
        calls: Shared<Vec<Int63>>,
    }
    crate::mem::trace_fields!(Counter, calls);

    impl ToBaml for Counter {
        fn render(&self, out: &mut String, _nested: bool) {
            class(out, "Counter", &[("calls", &self.calls)]);
        }

        fn collect(&self, out: &mut Vec<Pending>) {
            collect_fields(out, &[&self.calls]);
        }
    }

    impl Overridable for Counter {
        const OVERRIDES: bool = true;

        fn call_override(this: &Shared<Self>) -> Result<Str, Thrown> {
            let calls = this.borrow().calls.clone();
            let n = calls.borrow().len();
            calls.borrow_mut().push(int(i64::try_from(n).unwrap()));
            Ok(from_literal("<c>"))
        }
    }

    #[test]
    fn overrides_are_collected_in_order_called_once_each_and_spliced_bare() {
        let c = shared(Counter {
            calls: array::new(vec![]),
        });
        let list = array::new(vec![c.clone(), c.clone()]);
        // Nested, the override's result is spliced bare; it ran once per
        // position, and the mutation it made is visible afterwards.
        assert_eq!(to_string(&list).unwrap().as_str(), "[<c>, <c>]");
        assert_eq!(c.borrow().calls.borrow().len(), 2);
        // At the top level through the handle it is spliced too.
        assert_eq!(to_string(&c).unwrap().as_str(), "<c>");
        // The structural rendering alone ignores overrides.
        assert_eq!(
            to_std(&list),
            "[Counter { calls: [0, 1, 2] }, Counter { calls: [0, 1, 2] }]"
        );
    }

    fn int(value: i64) -> Int63 {
        Int63::new(value).unwrap()
    }

    struct Point {
        x: Int63,
        y: f64,
        label: Str,
    }
    crate::mem::trace_fields!(Point, x, y, label);

    impl Overridable for Point {}

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
    crate::mem::trace_fields!(Empty);

    impl Overridable for Empty {}

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
    crate::mem::trace_fields!(Node, children, tag, next);

    impl Overridable for Node {}

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
        assert_eq!(to_string(&from_literal("x")).unwrap().as_str(), "x");
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
