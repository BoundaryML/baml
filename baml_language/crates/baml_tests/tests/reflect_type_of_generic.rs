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
        load_type 0
        call baml.TypeValue.to_string
        return
    }

    function main() -> string {
        load_type 0
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
        load_type 0
        call user.make_describer
        call_indirect
        return
    }

    function make_describer() -> () -> string throws never {
        load_type 0
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
