//! BEP-066 slice 6: runtime package compilation and Scenario 5.

use std::sync::Arc;

use baml_tests::baml_test;
use bex_engine::{
    BexEngine, BexExternalValue, CaptureDefaults, EngineError, FunctionCallContextBuilder,
    value_capture::{TraceCaptureConfig, TraceCaptureProducer, TraceLogDrainReport},
};
use sys_native::SysOpsExt;

const SCENARIO_SOURCE: &str = r####"
client<llm> TestClient {
  provider openai
  options {
    model "unused-network-free-companions"
    api_key "unused"
  }
}

function Extract<T>(document: string) -> T {
  client TestClient
  prompt #"Extract the document using this schema:\n{{ ctx.output_format }}"#
}

function main() -> string throws unknown {
  let source = #"
class ExtractedRecord {
  account string
  amount int
}
"#
  let pkg = reflect.Package.compile({ "schema.baml": source })
  let record_t = pkg.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
  let document_text = #"{"account":"AC-1","amount":42}"#
  let record = Extract$parse<unreflect(record_t.as_type())>(document_text)
  json.encode(record)
}

function rendered_schema() -> string throws unknown {
  let pkg = reflect.Package.compile({
    "schema.baml": "class ExtractedRecord { account string amount int }"
  })
  let record_t = pkg.get_class("root.ExtractedRecord") ?? throw "missing ExtractedRecord"
  Extract$render_prompt<unreflect(record_t.as_type())>("sample document").text()
}

function mint_properties() -> bool throws unknown {
  let files = { "schema.baml": "class ExtractedRecord { account string amount int }" }
  let first = reflect.Package.compile(files)
  let second = reflect.Package.compile(files)
  let a = first.get_class("root.ExtractedRecord") ?? throw "missing first"
  let a_again = first.get_class("root.ExtractedRecord") ?? throw "missing first again"
  let b = second.get_class("root.ExtractedRecord") ?? throw "missing second"
  a.as_type() == a_again.as_type() && a.as_type() != b.as_type()
}

function package_survives_gc() -> bool throws unknown {
  let pkg = reflect.Package.compile({
    "schema.baml": "class ExtractedRecord { account string amount int }"
  })
  let before = pkg.get_class("root.ExtractedRecord") ?? throw "missing before GC"
  baml.sys.collect_garbage()
  let after = pkg.get_class("root.ExtractedRecord") ?? throw "missing after GC"
  before.as_type() == after.as_type()
}

function namespace_and_dependency_mounts() -> bool throws unknown {
  let base = reflect.Package.compile({
    "ns_models/base.baml": "class Base { id string }"
  })
  let child = reflect.Package.compile(
    { "child.baml": "class Child { base dep.models.Base }" },
    packages = { "dep": base },
  )
  base.get_class("root.models.Base") != null &&
    child.get_class("root.Child") != null &&
    child.diagnostics().length() == 0
}
"####;

const SUCCESSFUL_INIT_SOURCE: &str = r####"
function main() -> bool throws unknown {
  let pkg = reflect.Package.compile({ "schema.baml": #"
let initialized = init_marker();
function init_marker() -> int {
  log.info("runtime init ran")
  1
}
class Ready { value string }
"# })
  pkg.get_class("root.Ready") != null
}
"####;

const REJECTED_INIT_SOURCE: &str = r####"
function main() -> null throws unknown {
  reflect.Package.compile({ "schema.baml": #"
let initialized = init_marker();
function init_marker() -> int {
  log.info("rejected init must not run")
  1
}
class Broken { value MissingType }
"# })
  null
}
"####;

async fn run_main_with_logs(
    source: &str,
) -> (Result<BexExternalValue, EngineError>, TraceLogDrainReport) {
    let program = baml_project::testing::compile_source(source);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("runtime-package test engine"),
    );
    let logs = TraceCaptureProducer::new(TraceCaptureConfig::logs_only(16));
    let result = engine
        .call_function(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_capture_defaults(CaptureDefaults {
                    values_enabled: false,
                    logs_enabled: true,
                })
                .with_value_capture(logs.clone())
                .build(),
            true,
        )
        .await;
    (result, logs.drain_rendered_logs())
}

#[tokio::test]
async fn scenario_5_compiles_parses_and_encodes_runtime_schema() {
    let output = baml_test!(SCENARIO_SOURCE);
    assert_eq!(
        output.result,
        Ok(BexExternalValue::String(
            r#"{"account":"AC-1","amount":42}"#.into()
        ))
    );
}

#[tokio::test]
async fn exact_runtime_types_do_not_regress_static_generic_json_calls() {
    let output = baml_test!(
        r#"
class User {
  name string
  age int
}

function main() -> string throws unknown {
  let original = User { name: "Ada", age: 30 }
  let encoded = original.to_json()
  let decoded = User.from_json(encoded)
  decoded.name
}
"#
    );
    assert_eq!(output.result, Ok(BexExternalValue::String("Ada".into())));
}

#[tokio::test]
async fn render_prompt_uses_runtime_package_schema() {
    let output = baml_test!(baml: SCENARIO_SOURCE, entry: "rendered_schema");
    let Ok(BexExternalValue::String(prompt)) = output.result else {
        panic!("expected rendered prompt, got {:?}", output.result)
    };
    assert!(prompt.contains("account"), "{prompt}");
    assert!(prompt.contains("amount"), "{prompt}");
}

#[tokio::test]
async fn package_mints_are_created_once_and_compiles_are_generative() {
    let output = baml_test!(baml: SCENARIO_SOURCE, entry: "mint_properties");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_package_and_mint_survive_major_collection() {
    let output = baml_test!(baml: SCENARIO_SOURCE, entry: "package_survives_gc");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn runtime_compile_honors_namespaces_and_mounted_dependencies() {
    let output = baml_test!(baml: SCENARIO_SOURCE, entry: "namespace_and_dependency_mounts");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn successful_compile_runs_init_before_returning_package() {
    let (result, report) = run_main_with_logs(SUCCESSFUL_INIT_SOURCE).await;
    assert_eq!(result, Ok(BexExternalValue::Bool(true)));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.logs.len(), 1);
    assert_eq!(report.logs[0].body, "runtime init ran");
}

#[tokio::test]
async fn rejected_compile_returns_real_diagnostic_without_running_init() {
    let (result, report) = run_main_with_logs(REJECTED_INIT_SOURCE).await;
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.logs.is_empty(), "rejected candidate emitted logs");

    let Err(EngineError::UnhandledThrow { value, .. }) = result else {
        panic!("expected CompilationError throw, got {result:?}")
    };
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = *value
    else {
        panic!("CompilationError throw was not an instance: {value:?}")
    };
    assert_eq!(class_name, "baml.reflect.errors.CompilationError");
    let Some(BexExternalValue::Array { items, .. }) = fields.get("diagnostics") else {
        panic!("CompilationError did not contain diagnostics: {fields:?}")
    };
    let diagnostic = items
        .iter()
        .find_map(|item| match item {
            BexExternalValue::Instance { fields, .. }
                if matches!(fields.get("span"), Some(BexExternalValue::Instance { .. })) =>
            {
                Some(fields)
            }
            _ => None,
        })
        .expect("compiler diagnostic with a submitted-source span");
    let Some(BexExternalValue::String(code)) = diagnostic.get("code") else {
        panic!("diagnostic did not contain a code: {diagnostic:?}")
    };
    assert_eq!(code.as_str(), "E0002");
    let Some(BexExternalValue::Instance { fields: span, .. }) = diagnostic.get("span") else {
        unreachable!()
    };
    assert_eq!(
        span.get("file"),
        Some(&BexExternalValue::String("schema.baml".into()))
    );
}
