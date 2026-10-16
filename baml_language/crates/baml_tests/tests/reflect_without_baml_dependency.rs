//! Runs `reflect`'s BAML-bodied stdlib methods end to end.
//!
//! `baml` depends on `reflect`, never the reverse, so the implementations
//! `baml` declares are not visible inside `reflect`'s bodies. A `reflect` body
//! that leans on one anyway is not caught by the stdlib diagnostics check
//! (`stdlib_compiles_cleanly_under_its_declared_graph`):
//!
//! - a `for ... in` over a list type-checks, but code generation needs `baml`'s
//!   `Iterable` implementation and fails to compile every program;
//! - `to_string()` on a `reflect.Type` binds to `baml.ToString`'s default body
//!   instead of the `reflect.Type` override and crashes the VM.
//!
//! This program compiles the stdlib and calls each `AnyClass` body (which
//! iterate lists and render a `reflect.Type` inside `reflect`), `Concrete.type()`,
//! and the error path of `Enum.get_value`, so either hazard fails here.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

const PROGRAM: &str = r#"
    class Point {
        x int
        label string
    }

    enum Color {
        Red
    }

    function main() -> string {
        let point = Point { x: 1, label: "a" };
        let p: reflect.AnyClass = point;
        let names = "";
        for (let field in p.list_fields()) {
            names = names + field.name + ";";
        }
        let label = p.get<string>("label") catch (e) {
            _ => "type mismatch",
        };
        let invalid = reflect.Enum.get_value(1) catch (e) {
            reflect.errors.InvalidArgument => e.message,
        };
        [
            names,
            p.name(),
            `${p.has_field("x")}`,
            `${p.has_field("z")}`,
            p.get_field("x")?.name ?? "missing",
            label ?? "null",
            p.type().to_string(),
            Point.type().to_string(),
            `${int.type() == reflect.Type.of<int>()}`,
            reflect.Enum.get_value(Color.Red),
            invalid,
        ].join("|")
    }
"#;

#[tokio::test]
async fn reflect_bodies_compile_and_run_without_a_baml_dependency() {
    let output = baml_test!(PROGRAM);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            [
                "x;label;",
                "Point",
                "true",
                "false",
                "x",
                "a",
                "Point",
                "Point",
                "true",
                "Red",
                "reflect.Enum.get_value expects an enum value, got int",
            ]
            .join("|")
            .into()
        ))
    );
}
