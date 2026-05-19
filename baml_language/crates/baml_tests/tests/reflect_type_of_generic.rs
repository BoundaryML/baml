//! Phase 5 runtime tests for `reflect.type_of<T>()` inside generic functions.
//!
//! These tests verify:
//! - `reflect.type_of<T>()` returns the concrete `Ty` passed at the call site.
//! - Composite templates (`T[]`, `map<string, T>`) substitute correctly.
//! - Type args forward through a generic-to-generic call chain (`fwd<T>` calling `described_type<T>`).
//! - Closures capturing enclosing type params correctly carry `T` after the outer frame returns.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const PRELUDE: &str = r#"
class User { name string }
"#;

// ─── Bare-T tests ─────────────────────────────────────────────────────────────

/// `describe<T>()` returns `reflect.type_of<T>().to_string()` at the call site.
#[tokio::test]
async fn describe_user_returns_user() {
    let source = format!(
        r#"
        {PRELUDE}
        function describe<T>() -> string {{
            reflect.type_of<T>().to_string()
        }}
        function main() -> string {{
            describe<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    insta::assert_snapshot!(output.bytecode, @r"
    function describe() -> string {
        load_type #0
        call baml.TypeValue.to_string
        return
    }

    function main() -> string {
        load_type User
        call user.describe
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User".to_string()))
    );
}

#[tokio::test]
async fn describe_int_returns_int() {
    let source = r#"
        function describe<T>() -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> string {
            describe<int>()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

#[tokio::test]
async fn describe_string_returns_string() {
    let source = r#"
        function describe<T>() -> string {
            reflect.type_of<T>().to_string()
        }
        function main() -> string {
            describe<string>()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("string".to_string()))
    );
}

// ─── Composite template tests ─────────────────────────────────────────────────

/// `array_of<T>()` returns `reflect.type_of<T[]>()`.
#[tokio::test]
async fn array_of_user_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function array_of<T>() -> type {{
            reflect.type_of<T[]>()
        }}
        function main() -> string {{
            array_of<User>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User[]".to_string()))
    );
}

#[tokio::test]
async fn array_of_int_to_string() {
    let source = r#"
        function array_of<T>() -> type {
            reflect.type_of<T[]>()
        }
        function main() -> string {
            array_of<int>().to_string()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int[]".to_string()))
    );
}

// ─── Equality tests ───────────────────────────────────────────────────────────

/// `described_type<User>() == reflect.type_of<User>()` should be `true`.
#[tokio::test]
async fn described_type_eq_concrete() {
    let source = format!(
        r#"
        {PRELUDE}
        function described_type<T>() -> type {{
            reflect.type_of<T>()
        }}
        function main() -> bool {{
            described_type<User>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

/// `described_type<int>() == reflect.type_of<User>()` should be `false`.
#[tokio::test]
async fn described_type_neq_different() {
    let source = format!(
        r#"
        {PRELUDE}
        function described_type<T>() -> type {{
            reflect.type_of<T>()
        }}
        function main() -> bool {{
            described_type<int>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// ─── Closure captures type args ───────────────────────────────────────────────

/// A generic function that returns a zero-arg lambda. The lambda captures `T`
/// from the enclosing generic context and returns `reflect.type_of<T>().to_string()`.
/// After the outer call returns, the captured type arg should still be correct.
#[tokio::test]
async fn closure_captures_type_arg_user() {
    let source = format!(
        r#"
        {PRELUDE}
        function make_describer<T>() -> () -> string {{
            return () -> string {{ reflect.type_of<T>().to_string() }}
        }}
        function main() -> string {{
            let f = make_describer<User>();
            f()
        }}
        "#
    );
    let output = baml_test!(&source);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> string {
        load_type User
        call user.make_describer
        call_indirect
        return
    }

    function make_describer() -> () -> string throws never {
        load_type #0
        make_closure .<lambda(make_describer, 0)>, captures=0, ntypeargs=1
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User".to_string()))
    );
}

#[tokio::test]
async fn closure_captures_type_arg_int() {
    let source = r#"
        function make_describer<T>() -> () -> string {
            return () -> string { reflect.type_of<T>().to_string() }
        }
        function main() -> string {
            let f = make_describer<int>();
            f()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

// ─── Forwarding chain tests ───────────────────────────────────────────────────

/// `fwd<T>()` calls `described_type<T>()` which calls `reflect.type_of<T>()`.
/// Result should equal a direct `reflect.type_of<User>()`.
#[tokio::test]
async fn fwd_user_eq_reflect_type_of_user() {
    let source = format!(
        r#"
        {PRELUDE}
        function described_type<T>() -> type {{
            reflect.type_of<T>()
        }}
        function fwd<T>() -> type {{
            described_type<T>()
        }}
        function main() -> bool {{
            fwd<User>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn fwd_int_neq_user() {
    let source = format!(
        r#"
        {PRELUDE}
        function described_type<T>() -> type {{
            reflect.type_of<T>()
        }}
        function fwd<T>() -> type {{
            described_type<T>()
        }}
        function main() -> bool {{
            fwd<int>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

/// Three levels of explicit-arg forwarding: the original `T` should still
/// resolve correctly at the deepest call site.
#[tokio::test]
async fn three_level_forwarding_eq_concrete() {
    let source = format!(
        r#"
        {PRELUDE}
        function level1<T>() -> type {{
            reflect.type_of<T>()
        }}
        function level2<T>() -> type {{
            level1<T>()
        }}
        function level3<T>() -> type {{
            level2<T>()
        }}
        function main() -> bool {{
            level3<User>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn three_level_forwarding_to_string() {
    let source = r#"
        function level1<T>() -> string {
            reflect.type_of<T>().to_string()
        }
        function level2<T>() -> string {
            level1<T>()
        }
        function level3<T>() -> string {
            level2<T>()
        }
        function main() -> string {
            level3<int>()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

// ─── Closure capturing composite type ─────────────────────────────────────────

/// Closure body uses a composite template (`T[]`) — exercises Phase 5's
/// `TypeArgRef` substitution under a captured type-arg vector.
#[tokio::test]
async fn closure_captures_type_arg_array_of_user() {
    let source = format!(
        r#"
        {PRELUDE}
        function make_array_describer<T>() -> () -> string {{
            return () -> string {{ reflect.type_of<T[]>().to_string() }}
        }}
        function main() -> string {{
            let f = make_array_describer<User>();
            f()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User[]".to_string()))
    );
}

#[tokio::test]
async fn closure_captures_type_arg_optional_of_int() {
    let source = r#"
        function make_opt_describer<T>() -> () -> string {
            return () -> string { reflect.type_of<T?>().to_string() }
        }
        function main() -> string {
            let f = make_opt_describer<int>();
            f()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int?".to_string()))
    );
}

// ─── Depth-3 method-call paths (Phase 9) ──────────────────────────────────────
//
// These tests exercise multi-segment method-call paths like `holder.box.describe()`
// where the receiver-prefix is itself a parametric class. Phase 9 wires
// `path_segment_types` so the MIR Phase-8 prepend reads `type(holder.box) = Box<int>`
// (segment len-2 = 1) instead of `type(holder) = Holder` (segment 0).

/// The plan's reproducer: `holder.box.describe()` where `Holder` has a field
/// `box: Box<int>` and `Box<T>::describe` returns `reflect.type_of<T>().to_string()`.
/// Without Phase 9 the receiver type is read from `path_root_types` which holds
/// `Holder` (no class type-args), and the inner `reflect.type_of<T>()` returns
/// garbage. With Phase 9 the prefix `holder.box` resolves to `Box<int>` and the
/// class type-arg `T = int` flows through to the method frame.
#[tokio::test]
async fn type_of_depth3_path_in_method() {
    let output = baml_test!(
        r#"
        class Box<T> {
            value T
            function describe(self) -> string {
                reflect.type_of<T>().to_string()
            }
        }
        class Holder {
            box Box<int>
        }
        function main() -> string {
            let h: Holder = Holder { box: Box<int> { value: 42 } };
            h.box.describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

/// Depth-3 self-rooted path inside a method (`self.inner.name()`). `Outer<T>`'s
/// `T` is bound to `int`; `Outer<int>.inner` has type `Inner<int>`; `Inner<U>`'s
/// method `name` reads `U` which must resolve to `int` via the prefix's class
/// type-args. Proves the prefix lookup picks `Inner`'s `U` binding (not
/// `Outer`'s `T` directly), so Phase-8 method dispatch composes with Phase-9.
#[tokio::test]
async fn type_of_depth3_picks_inner_type_param() {
    let output = baml_test!(
        r#"
        class Inner<U> {
            v U
            function name(self) -> string {
                reflect.type_of<U>().to_string()
            }
        }
        class Outer<T> {
            inner Inner<T>
            function outer_describe(self) -> string {
                self.inner.name()
            }
        }
        function main() -> string {
            let o: Outer<int> = Outer<int> { inner: Inner<int> { v: 7 } };
            o.outer_describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

/// Same shape but parametric `Holder<T>` instantiated with `string`, exercising
/// `self.field.method()` chains where the enclosing class's `T` flows into the
/// inner field's class type-args.
#[tokio::test]
async fn type_of_self_field_method_depth3() {
    let output = baml_test!(
        r#"
        class Inner<U> {
            v U
            function show(self) -> string {
                reflect.type_of<U>().to_string()
            }
        }
        class Holder<T> {
            inner Inner<T>
            function delegate(self) -> string {
                self.inner.show()
            }
        }
        function main() -> string {
            let h: Holder<string> = Holder<string> { inner: Inner<string> { v: "x" } };
            h.delegate()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("string".to_string()))
    );
}

/// 4-segment path where the parametric class is at depth-2, not the root or
/// the immediate prefix. Proves the prefix lookup uses `segments.len() - 2`,
/// not a fixed offset.
#[tokio::test]
async fn type_of_depth4_path_picks_prefix_class_args() {
    let output = baml_test!(
        r#"
        class Leaf<T> {
            v T
            function describe(self) -> string {
                reflect.type_of<T>().to_string()
            }
        }
        class Mid {
            leaf Leaf<int>
        }
        class Root {
            mid Mid
        }
        function main() -> string {
            let r: Root = Root { mid: Mid { leaf: Leaf<int> { v: 1 } } };
            r.mid.leaf.describe()
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}
