//! Host-only coverage for BEP-066 runtime package compilation.
//!
//! Language-observable package behavior lives in the native BAML corpus under
//! `root.runtime_package_compile_phase1`. These tests remain in Rust because
//! they inspect host logger state or the `BexExternalValue` throw layout.

use std::sync::Arc;

use bex_engine::{
    BexEngine, BexExternalValue, EngineError, FunctionCallContextBuilder,
    logger::{TraceLogDrainReport, TraceLogger},
};
use sys_native::SysOpsExt;

const SUCCESSFUL_INIT_SOURCE: &str = r####"
function main() -> bool {
  let pkg = reflect.Package.compile({ "schema.baml": `
client InitClient = openai.ResponsesClient.new(
    model = "unused-network-free-init-check",
    api_key = "unused",
);
function init_ready() -> bool {
  InitClient != null
}
class Ready { value string }
` })
  let init_ready = pkg.get_function<() -> bool>("root.init_ready")
    ?? throw "missing init_ready"
  pkg.get_class("root.Ready") != null && init_ready()
}
"####;

const REJECTED_INIT_SOURCE: &str = r####"
function main() -> null {
  reflect.Package.compile({ "schema.baml": `
client InitClient = openai.ResponsesClient.new(
    model = "unused-network-free-init-check",
    api_key = "unused",
);
function Bad() -> int { "wrong" }
class Broken { value MissingType }
`, "tag.baml": "function sql(body: (x: int) -> baml.TaggedString) -> string { \"ok\" }\nfunction Demo() -> string { sql`hi ${1}` }\n" })
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

function main() -> string {
  let skill = reflect.Package.compile(
    { "skill.baml": `
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
` },
    packages = { "app": reflect.Package.current() },
  )
  let run = skill.get_function<(AgentState) -> AgentAction>("root.Run")
    ?? throw "missing root.Run"
  run(AgentState { goal: "ship", history: [] }).summary
}
"####;

async fn run_main_with_logs(
    source: &str,
) -> (Result<BexExternalValue, EngineError>, TraceLogDrainReport) {
    let program = baml_db::testing::compile_source(source);
    let engine = Arc::new(
        BexEngine::new_with_runtime_compiler(
            program,
            Arc::new(sys_native::SysOps::native()),
            Vec::new(),
            bex_project::runtime_compiler(),
        )
        .expect("runtime-package test engine"),
    );
    let logs = TraceLogger::bounded(16);
    let result = engine
        .call_function(
            "user.main",
            Vec::new(),
            FunctionCallContextBuilder::new(sys_types::CallId::next())
                .with_logger(logs.clone())
                .build(),
            true,
        )
        .await;
    (result, logs.drain_rendered_logs())
}

fn strip_union(mut value: &BexExternalValue) -> &BexExternalValue {
    while let BexExternalValue::Union { value: inner, .. } = value {
        value = inner;
    }
    value
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
    assert_eq!(class_name, "reflect.errors.CompilationError");
    let Some(BexExternalValue::Array { items, .. }) = fields.get("diagnostics") else {
        panic!("CompilationError did not contain diagnostics: {fields:?}")
    };
    assert!(
        items.iter().any(|item| matches!(
            item,
            BexExternalValue::Instance { fields, .. }
                if matches!(fields.get("code"), Some(BexExternalValue::String(code)) if code.as_str() == "E0002")
        )),
        "the original unresolved-type diagnostic disappeared: {items:?}"
    );
    let diagnostic = items
        .iter()
        .find_map(|item| match item {
            BexExternalValue::Instance {
                class_name, fields, ..
            } if class_name == "reflect.Diagnostic"
                && matches!(fields.get("headline"), Some(BexExternalValue::String(message)) if message.as_str() == "mismatched types") =>
            {
                Some(fields)
            }
            _ => None,
        })
        .expect("structured type-mismatch diagnostic");
    assert_eq!(
        diagnostic.get("message"),
        Some(&BexExternalValue::String(
            "mismatched types: expected `int`, found `\"wrong\"`".into()
        ))
    );
    assert_eq!(
        diagnostic.get("severity").map(strip_union),
        Some(&BexExternalValue::String("error".into()))
    );
    assert_eq!(
        diagnostic.get("phase").map(strip_union),
        Some(&BexExternalValue::String("type".into()))
    );
    assert_eq!(
        diagnostic.get("headline"),
        Some(&BexExternalValue::String("mismatched types".into()))
    );
    assert_eq!(
        diagnostic.get("primary_label"),
        Some(&BexExternalValue::String(
            "expected `int`, found `\"wrong\"`".into()
        ))
    );
    let Some(BexExternalValue::Instance { fields: span, .. }) = diagnostic.get("span") else {
        panic!("diagnostic did not retain its primary span: {diagnostic:?}")
    };
    assert_eq!(
        span.get("file"),
        Some(&BexExternalValue::String("schema.baml".into()))
    );
    let Some(BexExternalValue::Array {
        items: headline_highlights,
        ..
    }) = diagnostic.get("message_highlights")
    else {
        panic!("diagnostic did not expose headline highlights: {diagnostic:?}")
    };
    assert!(headline_highlights.is_empty());

    let Some(BexExternalValue::Array {
        items: annotations, ..
    }) = diagnostic.get("annotations")
    else {
        panic!("diagnostic did not expose annotations: {diagnostic:?}")
    };
    let [
        BexExternalValue::Instance {
            class_name,
            fields: annotation,
            ..
        },
    ] = annotations.as_slice()
    else {
        panic!("expected one primary annotation: {annotations:?}")
    };
    assert_eq!(class_name, "reflect.DiagnosticAnnotation");
    assert_eq!(
        annotation.get("is_primary"),
        Some(&BexExternalValue::Bool(true))
    );
    assert_eq!(annotation.get("message"), diagnostic.get("primary_label"));
    let Some(BexExternalValue::Array {
        items: label_highlights,
        ..
    }) = annotation.get("message_highlights")
    else {
        panic!("annotation did not expose label highlights: {annotation:?}")
    };
    assert_eq!(label_highlights.len(), 2);
    for highlight in label_highlights {
        let BexExternalValue::Instance {
            class_name, fields, ..
        } = highlight
        else {
            panic!("highlight was not structured: {highlight:?}")
        };
        assert_eq!(class_name, "reflect.DiagnosticHighlight");
        assert_eq!(
            fields.get("kind").map(strip_union),
            Some(&BexExternalValue::String("type_expression".into()))
        );
    }

    let related_diagnostic = items
        .iter()
        .find_map(|item| match item {
            BexExternalValue::Instance { fields, .. }
                if matches!(fields.get("message"), Some(BexExternalValue::String(message)) if message.contains("not a tagged-string function")) =>
            {
                Some(fields)
            }
            _ => None,
        })
        .expect("diagnostic carrying related source information");
    let Some(BexExternalValue::Array { items: related, .. }) =
        related_diagnostic.get("related_info")
    else {
        panic!("diagnostic did not expose related information: {related_diagnostic:?}")
    };
    let [
        BexExternalValue::Instance {
            class_name,
            fields: related,
            ..
        },
    ] = related.as_slice()
    else {
        panic!("expected one related location: {related:?}")
    };
    assert_eq!(class_name, "reflect.DiagnosticRelatedInfo");
    let Some(BexExternalValue::Instance {
        fields: related_span,
        ..
    }) = related.get("span")
    else {
        panic!("related location did not retain a span: {related:?}")
    };
    assert_eq!(
        related_span.get("file"),
        Some(&BexExternalValue::String("tag.baml".into()))
    );
    assert!(matches!(
        related.get("message"),
        Some(BexExternalValue::String(message))
            if message.contains("add a `//baml:tagged_string` marker")
    ));
    assert_eq!(related.get("file_path"), Some(&BexExternalValue::Null));
    let Some(BexExternalValue::Array {
        items: related_highlights,
        ..
    }) = related.get("message_highlights")
    else {
        panic!("related information did not retain highlights: {related:?}")
    };
    let [
        BexExternalValue::Instance {
            class_name,
            fields: related_highlight,
            ..
        },
    ] = related_highlights.as_slice()
    else {
        panic!("expected one related-information highlight: {related_highlights:?}")
    };
    assert_eq!(class_name, "reflect.DiagnosticHighlight");
    assert_eq!(
        related_highlight.get("kind").map(strip_union),
        Some(&BexExternalValue::String("code".into()))
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
