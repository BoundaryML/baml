//! Diagnostic consistency oracles: the static checker and runtime
//! reflection agree on codes and messages.

use baml_compiler_diagnostics::Severity;
use baml_tests::{
    baml_test,
    stdlib_prefix::{check_user_files, setup_test_db},
};
use bex_engine::BexExternalValue;

fn diagnostics(source: &str) -> Vec<(String, String)> {
    check_user_files(&setup_test_db(source))
        .into_iter()
        .filter(|diagnostic| diagnostic.severity == Severity::Error)
        .map(|diagnostic| (diagnostic.code().to_string(), diagnostic.message))
        .collect()
}

#[test]
fn m1_bare_computed_generic_argument_diagnostic_names_unreflect() {
    let rows = diagnostics(
        r#"
function id<T>(value: T) -> T { value }
function main() -> unknown {
  let runtime_t = reflect.Type.of<string>()
  id<runtime_t>("x")
}
"#,
    );
    let diagnostic = rows
        .iter()
        .find(|(code, _)| code == "E0002")
        .expect("bare computed argument should be rejected as E0002");
    assert!(
        diagnostic.1.contains("unreflect"),
        "diagnostic must name the required `unreflect` spelling: {diagnostic:?}"
    );
}

#[tokio::test]
async fn e1_duplicate_serialized_key_matches_static_code_and_message() {
    let static_rows = diagnostics(
        r#"
class Collision {
  wire string
  internal int @alias("wire")
}
"#,
    );
    let static_diagnostic = static_rows
        .iter()
        .find(|(code, _)| code == "E0149")
        .cloned()
        .expect("static duplicate-alias diagnostic");

    let output = baml_test!(
        r#"
function main() -> string {
  let result = reflect.class.new("Collision", {
    "wire": reflect.Type.of<string>(),
    "internal": reflect.Type.of<int>().meta(alias = "wire"),
  }) catch (e) {
    reflect.errors.CompilationError => {
      e.diagnostics[0].code + "|" + e.diagnostics[0].message
    }
  }
  if result is string { result } else { "did not throw" }
}
"#
    );
    let Ok(BexExternalValue::String(runtime)) = output.result else {
        panic!("expected runtime diagnostic, got {:?}", output.result)
    };
    let (runtime_code, runtime_message) = runtime
        .split_once('|')
        .expect("runtime result should contain code and message");
    assert_eq!(runtime_code, static_diagnostic.0);
    assert_eq!(runtime_message, static_diagnostic.1);
}

#[tokio::test]
async fn e5_structured_diagnostics_have_null_spans() {
    let output = baml_test!(
        r#"
function main() -> string {
  let zero_result = reflect.union.new([]) catch (e) {
    reflect.errors.CompilationError { diagnostics } => {
      diagnostics[0].code + ":" + (diagnostics[0].span == null).to_string()
    },
    _ => "wrong-zero-error",
  }
  let zero = if zero_result is string { zero_result } else { "zero did not throw" }

  let builder = reflect.class.builder("Duplicate")
  builder.field("x", reflect.Type.of<string>())
  let duplicate_result = builder.field("x", reflect.Type.of<int>()) catch (e) {
    reflect.errors.CompilationError { diagnostics } => {
      diagnostics[0].code + ":" + (diagnostics[0].span == null).to_string()
    },
    _ => "wrong-builder-error",
  }
  let duplicate = if duplicate_result is string {
    duplicate_result
  } else {
    "duplicate did not throw"
  }

  let package_result = reflect.Package.compile({ "broken.baml": "class {" }) catch (e) {
    reflect.errors.CompilationError { diagnostics } => {
      (diagnostics[0].span != null).to_string()
    },
    _ => "wrong-package-error",
  }
  let package_span = if package_result is string {
    package_result
  } else {
    "package did not throw"
  }

  zero + "|" + duplicate + "|" + package_span
}
"#
    );
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            "E0160:true|E0012:true|true".into()
        ))
    );
}
