//! BEP-066 constructor-correctness regression tests (N-1, K-2, and I-9).

use baml_compiler_diagnostics::Severity;
use baml_project::{collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn compile_errors(source: &str) -> Vec<(String, String)> {
    collect_diagnostics(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.message))
        .collect()
}

#[tokio::test]
async fn reserved_words_are_rejected_as_constructor_names() {
    let output = baml_test!(
        r#"
function main() -> string {
  let class_name = reflect.class.new("type", {}) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
  }
  let field_name = reflect.class.new("ValidClass", {
    "test": type.of<string>(),
  }) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
  }
  let enum_name = reflect.enum.new("class", []) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
  }
  let variant_name = reflect.enum.new("ValidEnum", ["function"]) catch (e) {
    reflect.errors.CompilationError => e.diagnostics[0].code + "|" + e.diagnostics[0].message
  }

  let class_result = if class_name is string { class_name } else { "class name accepted" }
  let field_result = if field_name is string { field_name } else { "field name accepted" }
  let enum_result = if enum_name is string { enum_name } else { "enum name accepted" }
  let variant_result = if variant_name is string { variant_name } else { "variant name accepted" }
  class_result + "\n" + field_result + "\n" + enum_result + "\n" + variant_result
}
"#
    );

    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0010|invalid class name `type`\n\
             E0010|invalid field name `ValidClass.test`\n\
             E0010|invalid enum name `class`\n\
             E0010|invalid enum variant name `ValidEnum.function`"
                .into()
        ))
    );
}

#[test]
fn enum_constructor_has_its_precise_declared_kind() {
    let errors = compile_errors(
        r#"
function make_enum() -> reflect.enum.Type {
  reflect.enum.new("PreciseEnum", [])
}
"#,
    );

    assert!(
        errors.is_empty(),
        "reflect.enum.new must return reflect.enum.Type, got: {errors:#?}"
    );
}

#[test]
fn both_removed_reader_spellings_name_their_replacements() {
    let errors = compile_errors(
        r#"
function old_type_reader() -> type {
  reflect.type_of<int>()
}

function old_value_reader(value: unknown) -> type {
  reflect.type_of_value(value)
}
"#,
    );

    for (old, replacement) in [
        ("reflect.type_of", "type.of"),
        ("reflect.type_of_value", "type.of_value"),
    ] {
        let matching = errors
            .iter()
            .filter(|(_, message)| message.contains(old))
            .collect::<Vec<_>>();
        assert_eq!(
            matching.len(),
            1,
            "expected exactly one diagnostic naming removed reader {old}, got: {errors:#?}"
        );
        assert!(
            matching[0].1.contains(replacement),
            "diagnostic for {old} must name replacement {replacement}: {:?}",
            matching[0]
        );
    }
}
