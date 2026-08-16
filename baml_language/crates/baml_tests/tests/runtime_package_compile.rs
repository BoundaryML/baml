//! BEP-066 Scenario 5: runtime package compilation.

use std::sync::Arc;

use baml_tests::baml_test;
use bex_engine::{
    BexEngine, BexExternalValue, CaptureDefaults, EngineError, FunctionCallContextBuilder,
    value_capture::{TraceCaptureConfig, TraceCaptureProducer, TraceLogDrainReport},
};
use sys_native::SysOpsExt;

const SCENARIO_SOURCE: &str = r####"
client TestClient = openai.ResponsesClient.new(
    model = "unused-network-free-companions",
    api_key = "unused",
);

function Extract<T>(document: string) -> T {
  client: TestClient
  prompt: `Extract the document using this schema:\n${ctx.output_format}`
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

function mounted_runtime_interface_and_return_types_stay_hidden() -> bool throws unknown {
  let runtime_minted = reflect.class.new("RuntimeMinted", {
    "value": type.of<string>(),
  })
  let app = reflect.Package.current().with_types({
    "RuntimeMinted": runtime_minted,
  })
  let dependency = reflect.Package.compile({
    "dependency.baml": #"
interface CarriesRuntimeMint {
  item app.RuntimeMinted
}

function make_runtime_minted() -> app.RuntimeMinted {
  app.RuntimeMinted { value: "ok" }
}
"#
  }, packages = { "app": app })
  let consumer = reflect.Package.compile(
    { "consumer.baml": "function ready() -> bool { true }" },
    packages = { "dependency": dependency },
  )
  consumer.diagnostics().length() == 0
}
"####;

const SUCCESSFUL_INIT_SOURCE: &str = r####"
function main() -> bool throws unknown {
  let pkg = reflect.Package.compile({ "schema.baml": #"
client InitClient = openai.ResponsesClient.new(
    model = "unused-network-free-init-check",
    api_key = "unused",
);
function init_ready() -> bool {
  InitClient != null
}
class Ready { value string }
"# })
  let init_ready = pkg.get_function<() -> bool>("root.init_ready")
    ?? throw "missing init_ready"
  pkg.get_class("root.Ready") != null && init_ready()
}
"####;

const REJECTED_INIT_SOURCE: &str = r####"
function main() -> null throws unknown {
  reflect.Package.compile({ "schema.baml": #"
client InitClient = openai.ResponsesClient.new(
    model = "unused-network-free-init-check",
    api_key = "unused",
);
class Broken { value MissingType }
"# })
  null
}
"####;

const SCENARIO_6_SOURCE: &str = r####"
class AgentState {
  goal string
  history string[]
}

interface AgentAction {
  summary string
}

function Plan(state: AgentState) -> string {
  log.info("LIVE_PLAN:" + state.goal)
  "planned " + state.goal
}

function main() -> string throws unknown {
  let skill_source = #"
class PlanThenAct {
  summary string
  steps string[]
  implements app.AgentAction {}
}

function Run(state: app.AgentState) -> PlanThenAct {
  PlanThenAct {
    summary: app.Plan(state),
    steps: [],
  }
}
"#
  let skill = reflect.Package.compile(
    { "skill.baml": skill_source },
    packages = { "app": reflect.Package.current() },
  )
  let run = skill.get_function<(AgentState) -> AgentAction>("root.Run")
    ?? throw "missing root.Run"
  let action = run(AgentState { goal: "ship", history: [] })
  let functions = skill.functions()
  if (functions.get("root.Run") == null) { throw "root.Run not enumerated" } else { null }
  action.summary
}

function absent_function_is_null() -> bool throws unknown {
  let pkg = reflect.Package.compile({
    "main.baml": "function Present(value: string) -> string { value }"
  })
  pkg.get_function<(string) -> string>("root.Missing") == null
}

function mismatched_function_contract() -> null throws unknown {
  let pkg = reflect.Package.compile({
    "main.baml": "function Present(value: string) -> string { value }"
  })
  let _ = pkg.get_function<(int) -> string>("root.Present")
  null
}

function alias_order_and_reserved_names() -> bool throws unknown {
  let root_package = reflect.Package.current()
  let generated = reflect.Package.compile(
    { "main.baml": #"
function Read(state: app.AgentState) -> string {
  app.Plan(state) + ":" + baml.Array.length(["o", "k"]).to_string()
}
"# },
    packages = { "z_last": root_package, "app": root_package },
  )
  let read = generated.get_function<(AgentState) -> string>("root.Read")
    ?? throw "missing root.Read"
  let ordered = read(AgentState { goal: "ordered", history: [] }) == "planned ordered:2"

  let rejected = false
  let _ = reflect.Package.compile(
    { "main.baml": "function main() -> int { 1 }" },
    packages = { "baml": root_package },
  ) catch (e) {
    baml.reflect.errors.CompilationError => { rejected = true },
    _ => throw e,
  }
  ordered && rejected
}

test "enumerated package test" {
  assert.equal(1, 1)
}

function enumerated_test_runs() -> bool throws unknown {
  let tests = reflect.Package.current().tests()
  let run = tests.get("root::enumerated package test") ?? throw "test not enumerated"
  run()
  tests.length() == 1
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

/// The compiler-built stdlib remains the host image's immutable dispatch
/// world: a runtime-compiled generic bound must resolve the same stdlib impl
/// rule and inherited default method as statically compiled code.
#[tokio::test]
async fn runtime_compiled_code_dispatches_through_static_stdlib_impls() {
    let output = baml_test!(
        r####"
function compare_hot<T extends baml.ops.Compare>(value: T, n: int) -> int throws never {
  let count = 0
  for (let i = 0; i < n; i += 1) {
    if value <= value { count += 1 } else { count -= 1 }
  }
  count
}

function main() -> bool throws unknown {
  let package = reflect.Package.compile({ "dispatch.baml": #"
function compare_hot<T extends baml.ops.Compare>(value: T, n: int) -> int throws never {
  let count = 0
  for (let i = 0; i < n; i += 1) {
    if value <= value { count += 1 } else { count -= 1 }
  }
  count
}
function run(n: int) -> int throws never { compare_hot<int>(7, n) }
"# })
  let run = package.get_function<(int) -> int>("root.run") ?? throw "missing run"
  let before_gc = run(100)
  baml.sys.collect_garbage()
  compare_hot<int>(7, 100) == before_gc && before_gc == run(100)
}
"####
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
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
async fn mounted_runtime_types_do_not_leak_into_phantom_stub_diagnostics() {
    let output = baml_test!(
        baml: SCENARIO_SOURCE,
        entry: "mounted_runtime_interface_and_return_types_stay_hidden"
    );
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn successful_compile_runs_init_before_returning_package() {
    let (result, report) = run_main_with_logs(SUCCESSFUL_INIT_SOURCE).await;
    assert_eq!(result, Ok(BexExternalValue::Bool(true)));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert!(report.logs.is_empty());
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

#[tokio::test]
async fn scenario_6_extracts_and_calls_the_live_aliased_function() {
    let (result, report) = run_main_with_logs(SCENARIO_6_SOURCE).await;
    assert_eq!(result, Ok(BexExternalValue::String("planned ship".into())));
    assert!(report.failures.is_empty(), "{:?}", report.failures);
    assert_eq!(report.logs.len(), 1);
    assert_eq!(report.logs[0].body, "LIVE_PLAN:ship");
}

#[tokio::test]
async fn get_function_absence_is_null() {
    let output = baml_test!(baml: SCENARIO_6_SOURCE, entry: "absent_function_is_null");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn get_function_mismatch_throws_compiler_subtyping_diagnostic() {
    let output = baml_test!(baml: SCENARIO_6_SOURCE, entry: "mismatched_function_contract");
    let Err(EngineError::UnhandledThrow { value, .. }) = output.result else {
        panic!("expected CompilationError, got {:?}", output.result)
    };
    let BexExternalValue::Instance {
        class_name, fields, ..
    } = *value
    else {
        panic!("expected CompilationError instance")
    };
    assert_eq!(class_name, "baml.reflect.errors.CompilationError");
    let Some(BexExternalValue::Array { items, .. }) = fields.get("diagnostics") else {
        panic!("missing diagnostics: {fields:?}")
    };
    assert!(items.iter().any(|item| matches!(
        item,
        BexExternalValue::Instance { fields, .. }
            if fields.get("code") == Some(&BexExternalValue::String("E0001".into()))
                && matches!(fields.get("message"), Some(BexExternalValue::String(message)) if message.contains("not a subtype"))
    )));
}

#[tokio::test]
async fn alias_maps_are_order_independent_and_cannot_shadow_stdlib() {
    let output = baml_test!(baml: SCENARIO_6_SOURCE, entry: "alias_order_and_reserved_names");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}

#[tokio::test]
async fn package_tests_enumerate_invocable_zero_arg_functions() {
    let output = baml_test!(baml: SCENARIO_6_SOURCE, entry: "enumerated_test_runs");
    assert_eq!(output.result, Ok(BexExternalValue::Bool(true)));
}
