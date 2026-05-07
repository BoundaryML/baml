//! Phase 4 runtime tests for `reflect.type_of<T>()` with concrete type arguments.
//!
//! These tests verify:
//! - `reflect.type_of<User>()` returns an `Object::Type` with `.to_string() == "User"`.
//! - Structural equality: two `type` values for the same type are `==`.
//! - Structural inequality: `type` values for different types are `!=`.
//! - Composite concrete types: `User[]`, `map<string, User>`, `User?`.
//! - Primitive types: `int`, `string`, `bool`.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const PRELUDE: &str = r#"
class User { name string }
class Container<T> { value T }
enum Color { Red, Green, Blue }
"#;

// ─── Basic to_string tests ────────────────────────────────────────────────────

#[tokio::test]
async fn type_of_class_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<User>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    insta::assert_snapshot!(output.bytecode, @r"
    function main() -> string {
        load_type User
        call baml.TypeValue.to_string
        return
    }
    ");
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User".to_string()))
    );
}

#[tokio::test]
async fn type_of_int_to_string() {
    let source = r#"
        function main() -> string {
            reflect.type_of<int>().to_string()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("int".to_string()))
    );
}

#[tokio::test]
async fn type_of_string_to_string() {
    let source = r#"
        function main() -> string {
            reflect.type_of<string>().to_string()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("string".to_string()))
    );
}

#[tokio::test]
async fn type_of_bool_to_string() {
    let source = r#"
        function main() -> string {
            reflect.type_of<bool>().to_string()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("bool".to_string()))
    );
}

#[tokio::test]
async fn type_of_array_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<User[]>().to_string()
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
async fn type_of_optional_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<User?>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("User?".to_string()))
    );
}

#[tokio::test]
async fn type_of_map_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<map<string, User>>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("map<string, User>".to_string()))
    );
}

#[tokio::test]
async fn type_of_generic_class_runs_without_crash() {
    // Phase 7: class type-args are now first-class in `Ty`, so the rendered
    // name includes the type argument: "Container<User>".
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<Container<User>>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Container<User>".to_string()))
    );
}

#[tokio::test]
async fn type_of_generic_class_to_string_matches_source() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<Container<User>>().to_string() == "Container<User>"
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_map_eq() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<map<string, User>>() == reflect.type_of<map<string, User>>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_generic_class_eq() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<Container<User>>() == reflect.type_of<Container<User>>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_generic_class_neq_different_arg() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<Container<User>>() == reflect.type_of<Container<int>>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// ─── Phase 7: parametric class forwarding via generic bodies ─────────────────

#[tokio::test]
async fn wrap_in_container_to_string() {
    // Validates Phase 5 ↔ Phase 7 composition: TypeArgRef substitution inside
    // a generic body produces a Ty::Class with resolved type args.
    let source = format!(
        r#"
        {PRELUDE}
        function wrap_in_container<T>() -> type {{
            reflect.type_of<Container<T>>()
        }}
        function main() -> string {{
            wrap_in_container<int>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Container<int>".to_string()))
    );
}

// ─── Structural equality ──────────────────────────────────────────────────────

#[tokio::test]
async fn type_of_same_class_eq() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<User>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_different_class_neq() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<User>() == reflect.type_of<int>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn type_of_assign_and_compare() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            let a: type = reflect.type_of<User>();
            let b: type = reflect.type_of<User>();
            a == b
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

// ─── Enum and union coverage ──────────────────────────────────────────────────

#[tokio::test]
async fn type_of_enum_to_string() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> string {{
            reflect.type_of<Color>().to_string()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("Color".to_string()))
    );
}

#[tokio::test]
async fn type_of_enum_eq() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<Color>() == reflect.type_of<Color>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_enum_neq_class() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> bool {{
            reflect.type_of<Color>() == reflect.type_of<User>()
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

#[tokio::test]
async fn type_of_union_eq() {
    let source = r#"
        function main() -> bool {
            reflect.type_of<int | string>() == reflect.type_of<int | string>()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn type_of_union_neq_different_member() {
    let source = r#"
        function main() -> bool {
            reflect.type_of<int | string>() == reflect.type_of<int | bool>()
        }
    "#;
    let output = baml_test!(source);
    assert_eq!(output.result, Ok(BexExternalValue::Bool(false)));
}

// ─── Stable identity via .to_string() ────────────────────────────────────────
//
// `type` values cannot yet be used as `map<type, V>` keys because BAML has no
// interface system to constrain K to "hashable".  In the meantime,
// `to_string()` returns a fully-qualified, package-prefixed name so user code
// can use the result as a stable identity key in `map<string, V>`.

#[tokio::test]
async fn type_of_to_string_works_as_map_key() {
    let source = format!(
        r#"
        {PRELUDE}
        function main() -> int {{
            let m: map<string, int> = {{}};
            m[reflect.type_of<User>().to_string()] = 1;
            m[reflect.type_of<int>().to_string()] = 2;
            m[reflect.type_of<User>().to_string()]
        }}
        "#
    );
    let output = baml_test!(&source);
    assert_eq!(output.result, Ok(BexExternalValue::Int(1)));
}
