//! BEP-066 M-1/M-6/E-1/E-5 diagnostic consistency oracles.

use baml_compiler_diagnostics::Severity;
use baml_project::{collect_diagnostics, testing::setup_test_db};
use baml_tests::baml_test;
use bex_engine::BexExternalValue;

fn diagnostics(source: &str) -> Vec<(String, String)> {
    collect_diagnostics(&setup_test_db(source))
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
  let runtime_t = type.of<string>()
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

#[test]
fn indirect_runtime_checked_call_is_rejected_before_emission() {
    let rows = diagnostics(
        r#"
function main() -> int {
  type T = unreflect(type.of<string>());
  let indirect = (value: T) => { 1 }
  indirect("checked at runtime")
}
"#,
    );
    assert!(
        rows.iter().any(|(code, message)| {
            code == "E0010"
                && message == "runtime-checked arguments are not supported on indirect calls"
        }),
        "expected the indirect-call runtime-type diagnostic, got {rows:?}"
    );
}

#[test]
fn optional_indirect_runtime_checked_call_is_rejected_before_emission() {
    let rows = diagnostics(
        r#"
function main() -> int? {
  type T = unreflect(type.of<string>());
  let indirect: ((value: T) -> int throws never)? = (value: T) => { 1 }
  indirect?.("checked at runtime")
}
"#,
    );
    assert!(
        rows.iter().any(|(code, message)| {
            code == "E0010"
                && message == "runtime-checked arguments are not supported on indirect calls"
        }),
        "expected the optional indirect-call runtime-type diagnostic, got {rows:?}"
    );
}

#[tokio::test]
async fn m6_runtime_bound_diagnostic_matches_static_oracle_before_call() {
    let static_rows = diagnostics(
        r#"
interface Named { name string }
function bounded<T extends Named>() -> int { 1 }
function main() -> int { bounded<string>() }
"#,
    );
    let static_diagnostic = static_rows
        .iter()
        .find(|(code, _)| code == "E0001")
        .cloned()
        .expect("static bound violation should produce E0001");

    let output = baml_test!(
        r#"
interface Named { name string }
function bounded<T extends Named>() -> int {
  baml.sys.panic("callee body entered")
}
function main() -> string {
  let result = bounded<unreflect(type.of<string>())>() catch (e) {
    reflect.errors.CompilationError => {
      e.diagnostics[0].code + "|" + e.diagnostics[0].message
    }
  }
  let diagnostic = if result is string { result } else { "bound accepted" }
  diagnostic
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
    "wire": type.of<string>(),
    "internal": type.of<int>().meta(alias = "wire"),
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
function main() -> string throws unknown {
  let zero_result = reflect.union.new([]) catch (e) {
    reflect.errors.CompilationError { diagnostics } => {
      diagnostics[0].code + ":" + (diagnostics[0].span == null).to_string()
    },
    _ => "wrong-zero-error",
  }
  let zero = if zero_result is string { zero_result } else { "zero did not throw" }

  let builder = reflect.class.builder("Duplicate")
  builder.field("x", type.of<string>())
  let duplicate_result = builder.field("x", type.of<int>()) catch (e) {
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
