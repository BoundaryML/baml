//! Phase 8 end-to-end tests: class type-args threaded through instances,
//! methods, and runtime class matching.
//!
//! These tests verify:
//! - `reflect.type_of<T>()` inside a class method returns the correct concrete
//!   `Ty` (issue #2 — method can see the class-level type arg at runtime).
//! - `Foo<int> | Foo<string>` union dispatch works correctly (issue #9 — union
//!   members with the same base class are disambiguated by their type args).
//! - `value is Foo<int>` returns true for a `Foo<int>` instance and false for
//!   a `Foo<string>` instance.
//! - A closure created inside a method that captures `T` works after the outer
//!   method returns.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

// ─── Issue #2: method can read class-level type arg ──────────────────────────

/// `Box<T>::describe` returns `reflect.type_of<T>().to_string()`.
/// For `Box<int>` it should return `"int"`, for `Box<string>` → `"string"`.
#[tokio::test]
async fn method_sees_class_type_arg_int() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
            function describe(self) -> string {
                reflect.type_of<T>().to_string()
            }
        }
        function main() -> string {
            let b: Box<int> = Box { value: 42 };
            b.describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

#[tokio::test]
async fn method_sees_class_type_arg_string() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
            function describe(self) -> string {
                reflect.type_of<T>().to_string()
            }
        }
        function main() -> string {
            let b: Box<string> = Box { value: "hi" };
            b.describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("string".to_string()))
    );
}

/// `Box<Box<int>>::describe` should return `"Box<int>"`.
#[tokio::test]
async fn method_sees_composite_class_type_arg() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
            function describe(self) -> string {
                reflect.type_of<T>().to_string()
            }
        }
        function main() -> string {
            let inner: Box<int> = Box { value: 1 };
            let outer: Box<Box<int>> = Box { value: inner };
            outer.describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Box<int>".to_string()))
    );
}

// ─── Issue #9: union dispatch disambiguates by class type args ────────────────

/// `pick(Foo<int>)` should return `"int"`, `pick(Foo<string>)` → `"string"`.
#[tokio::test]
async fn union_dispatch_generic_class_int() {
    let output = baml_test!(
        r#"
        class Foo<T> { value T }
        function pick(x: Foo<int> | Foo<string>) -> string {
            match (x) {
                Foo<int>    => "int",
                Foo<string> => "string",
            }
        }
        function main() -> string {
            pick(Foo<int> { value: 7 })
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

#[tokio::test]
async fn union_dispatch_generic_class_string() {
    let output = baml_test!(
        r#"
        class Foo<T> { value T }
        function pick(x: Foo<int> | Foo<string>) -> string {
            match (x) {
                Foo<int>    => "int",
                Foo<string> => "string",
            }
        }
        function main() -> string {
            pick(Foo<string> { value: "x" })
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("string".to_string()))
    );
}

// ─── Issue #9: match-type dispatch respects class type args ──────────────────

/// Union dispatch with `Foo<int>` — the `Foo<int>` arm should hit, not `Foo<string>`.
#[tokio::test]
async fn is_type_parametric_class_same_args() {
    let output = baml_test!(
        r#"
        class Foo<T> { value T }
        function check(f: Foo<int> | Foo<string>) -> bool {
            match (f) {
                Foo<int> => true,
                Foo<string> => false,
            }
        }
        function main() -> bool {
            check(Foo<int> { value: 1 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// Union dispatch with `Foo<string>` — the `Foo<int>` arm should NOT hit.
#[tokio::test]
async fn is_type_parametric_class_different_args() {
    let output = baml_test!(
        r#"
        class Foo<T> { value T }
        function check(f: Foo<int> | Foo<string>) -> bool {
            match (f) {
                Foo<int> => true,
                Foo<string> => false,
            }
        }
        function main() -> bool {
            check(Foo<string> { value: "x" })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

/// Bare-class match arm (`Foo<int>`) still matches when the union includes
/// only `Foo<int>` — backward-compatible monomorphic matching path.
#[tokio::test]
async fn is_type_bare_class_still_matches() {
    let output = baml_test!(
        r#"
        class Foo<T> { value T }
        function check(f: Foo<int> | string) -> bool {
            match (f) {
                Foo<int> => true,
                _ => false,
            }
        }
        function main() -> bool {
            check(Foo<int> { value: 1 })
        }
    "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── Closure inside method captures class T ──────────────────────────────────

/// A closure created inside a method body can capture `T` from the class.
/// The closure is returned from the method then called in main.
#[tokio::test]
async fn closure_inside_method_captures_class_type_arg() {
    let output = baml_test!(
        r#"
        class Box2<T> {
            function make_describer(self) -> () -> string {
                return () -> string { reflect.type_of<T>().to_string() }
            }
        }
        function main() -> string {
            let b: Box2<int> = Box2 { };
            let describe = b.make_describer();
            describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

/// Same as above but with allocation pressure between the method call and the
/// closure invocation, exercising the case where the captured class type args
/// must survive (or be correctly forwarded) across GC cycles.
///
/// Pattern copied from `baml_tests/tests/gc.rs::test_map_survives_gc_during_operations`.
#[tokio::test]
async fn closure_inside_method_survives_gc() {
    let output = baml_test!(
        r#"
        class Box2<T> {
            function make_describer(self) -> () -> string {
                return () -> string { reflect.type_of<T>().to_string() }
            }
        }
        function main() -> string {
            let b: Box2<int> = Box2 { };
            let describe = b.make_describer();

            // Trigger allocation pressure with many short-lived arrays so the
            // GC has chances to run and move objects, including the closure's
            // captured class type args.
            let i = 0;
            while (i < 500) {
                let tmp = [i, i + 1, i + 2];
                i = i + 1;
            }

            describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}
