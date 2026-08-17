//! Executable oracles for incremental runtime class builders, pending
//! composites, recursive group freezing, and structured diagnostics.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn self_recursive_employee_renders_and_parses() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let employee = reflect.class.builder("Employee")
            let pending = employee.type()
            employee.field("name", type.of<string>().meta(
                alias = "full_name",
                description = "the employee's display name",
            ))
            employee.field("manager", pending.optional())
            let employee_t = employee.build()
            let prompt = Extract$render_prompt<unreflect(employee_t.as_type())>().text()
            let value = Extract$parse<unreflect(employee_t.as_type())>(
                `{"full_name":"Ada","manager":{"full_name":"Grace","manager":null}}`,
            )
            let manager = reflect.class.get_field<unknown>(value, "manager")
            let manager_name = reflect.class.get_field<string>(manager, "name")
            return prompt
                + "\n<RESULT>" + manager_name
                + "|" + (pending.resolved() == employee_t.as_type()).to_string()
                + "|" + baml.json.encode(value)
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("a self-recursive runtime class should render and parse")
    else {
        panic!("expected a string result")
    };
    assert!(
        result.contains("Employee"),
        "class missing from prompt: {result}"
    );
    assert!(
        result.ends_with(
            r#"<RESULT>Grace|true|{"name":"Ada","manager":{"name":"Grace","manager":null}}"#,
        ),
        "recursive parse landing changed: {result}"
    );
}

#[tokio::test]
async fn mutually_recursive_group_freezes_together_and_is_idempotent() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let left = reflect.class.builder("Left")
            let right = reflect.class.builder("Right")
            let left_p = left.type()
            let right_p = right.type()
            let lefts_p = left_p.array()
            let either_p = left_p.union([type.of<string>()])
            let pending_starts_null = left_p.resolved() == null

            left.field("right", right_p.optional())
            right.field("lefts", lefts_p)
            right.field("either", either_p)

            let left_t = left.build()
            let right_t = right.build()
            let left_again = left.build()
            let right_again = right.build()
            let right_fields = right_t.fields()
            return pending_starts_null.to_string()
                + "|" + (left_t.as_type() == left_again.as_type()).to_string()
                + "|" + (right_t.as_type() == right_again.as_type()).to_string()
                + "|" + (left_p.resolved() == left_t.as_type()).to_string()
                + "|" + (right_p.resolved() == right_t.as_type()).to_string()
                + "|" + (lefts_p.resolved() == right_fields[0].type).to_string()
                + "|" + (either_p.resolved() == right_fields[1].type).to_string()
        }
        "#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "true|true|true|true|true|true|true".into()
        ))
    );
}

#[tokio::test]
async fn frozen_mutation_and_unresolved_call_name_the_builder() {
    let output = baml_test!(
        r##"
        client TestClient = openai.ResponsesClient.new(
            model = "gpt-4o-mini",
            api_key = "test-key",
            base_url = "http://localhost:1234",
        );

        function Extract<T>() -> T {
            client: TestClient
            prompt: `${ctx.output_format}`
        }

        function main() -> string {
            let frozen = reflect.class.builder("FrozenEmployee")
            frozen.field("name", type.of<string>())
            frozen.build()
            let frozen_error = frozen.field("age", type.of<int>()) catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].message
            }

            let unresolved = reflect.class.builder("UnbuiltTenant")
            let erased: unknown = unresolved.type()
            let pending_error = Extract$render_prompt<unreflect(erased)>() catch (e) {
                baml.reflect.errors.CompilationError => e.diagnostics[0].message
            }

            let left = "missing frozen error"
            if frozen_error is string {
                left = frozen_error
            }
            let right = "missing pending error"
            if pending_error is string {
                right = pending_error
            }
            return left + "|" + right
        }
        "##
    );

    let BexExternalValue::String(result) = output
        .result
        .expect("both builder errors should be catchable CompilationError values")
    else {
        panic!("expected a string result")
    };
    assert!(result.contains("`FrozenEmployee` is frozen"), "{result}");
    assert!(result.contains("`UnbuiltTenant`"), "{result}");
    assert!(result.contains("call build()"), "{result}");
}

#[tokio::test]
async fn duplicate_field_structured_diagnostic_has_a_null_span() {
    let output = baml_test!(
        r#"
        function main() -> string {
            let builder = reflect.class.builder("Duplicate")
            builder.field("name", type.of<string>())
            let result = builder.field("name", type.of<int>()) catch (e) {
                baml.reflect.errors.CompilationError => {
                    let span = e.diagnostics[0].span
                    return e.diagnostics[0].code
                        + "|" + (span == null).to_string()
                }
            }
            return "duplicate did not throw"
        }
    "#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String("E0012|true".into()))
    );
}

#[tokio::test]
#[should_panic(expected = "[E0001]")]
async fn pending_type_is_not_a_static_subtype_of_type() {
    let _ = baml_test!(
        r#"
        function accepts_type(value: type) -> bool { true }

        function main() -> bool {
            let builder = reflect.class.builder("StillPending")
            accepts_type(builder.type())
        }
        "#
    );
}

#[tokio::test]
#[should_panic(expected = "[E0001]")]
async fn unreflect_runtime_escape_does_not_accept_ordinary_values() {
    let _ = baml_test!(
        r#"
        function generic<T>() -> bool { true }

        function main() -> bool {
            generic<unreflect(42)>()
        }
        "#
    );
}
