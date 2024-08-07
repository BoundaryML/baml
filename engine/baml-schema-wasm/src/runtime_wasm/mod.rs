pub mod generator;
pub mod runtime_prompt;

use crate::runtime_wasm::runtime_prompt::WasmPrompt;
use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
use baml_runtime::internal_core::configuration::GeneratorOutputType;
use baml_runtime::InternalRuntimeInterface;
use baml_runtime::{
    internal::llm_client::LLMResponse, BamlRuntime, DiagnosticsError, IRHelper, RenderedPrompt,
};
use baml_types::{BamlMap, BamlValue};
use internal_baml_codegen::version_check::GeneratorType;
use internal_baml_codegen::version_check::{check_version, VersionCheckMode};
use internal_baml_core::ir::Expression;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::path::PathBuf;
use wasm_bindgen::prelude::*;

//Run: wasm-pack test --firefox --headless  --features internal,wasm
// but for browser we likely need to do         wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
// Node is run using: wasm-pack test --node --features internal,wasm

#[wasm_bindgen(start)]
pub fn on_wasm_init() {
    match console_log::init_with_level(log::Level::Warn) {
        Ok(_) => web_sys::console::log_1(&"Initialized BAML runtime logging".into()),
        Err(e) => web_sys::console::log_1(
            &format!("Failed to initialize BAML runtime logging: {:?}", e).into(),
        ),
    }

    console_error_panic_hook::set_once();
}

#[wasm_bindgen(inspectable)]
#[derive(Serialize, Deserialize)]

pub struct WasmProject {
    root_dir_name: String,
    // This is the version of the file on disk
    files: HashMap<String, String>,
    // This is the version of the file that is currently being edited
    // (unsaved changes)
    unsaved_files: HashMap<String, String>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Debug)]
pub struct WasmDiagnosticError {
    errors: DiagnosticsError,
    pub all_files: Vec<String>,
}

// use serde::Serialize;

#[wasm_bindgen(getter_with_clone)]
pub struct SymbolLocation {
    pub uri: String,
    pub start_line: usize,
    pub start_character: usize,
    pub end_line: usize,
    pub end_character: usize,
}

#[wasm_bindgen]
impl WasmDiagnosticError {
    #[wasm_bindgen]
    pub fn errors(&self) -> Vec<WasmError> {
        self.errors
            .errors()
            .iter()
            .map(|e| {
                let (start, end) = e.span().line_and_column();

                WasmError {
                    file_path: e.span().file.path(),
                    start_ch: e.span().start,
                    end_ch: e.span().end,
                    start_line: start.0,
                    start_column: start.1,
                    end_line: end.0,
                    end_column: end.1,
                    r#type: "error".to_string(),
                    message: e.message().to_string(),
                }
            })
            .chain(self.errors.warnings().iter().map(|e| {
                let (start, end) = e.span().line_and_column();

                WasmError {
                    file_path: e.span().file.path(),
                    start_ch: e.span().start,
                    end_ch: e.span().end,
                    start_line: start.0,
                    start_column: start.1,
                    end_line: end.0,
                    end_column: end.1,
                    r#type: "warning".to_string(),
                    message: e.message().to_string(),
                }
            }))
            .collect()
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmError {
    #[wasm_bindgen(readonly)]
    pub r#type: String,
    #[wasm_bindgen(readonly)]
    pub file_path: String,
    #[wasm_bindgen(readonly)]
    pub start_ch: usize,
    #[wasm_bindgen(readonly)]
    pub end_ch: usize,
    #[wasm_bindgen(readonly)]
    pub start_line: usize,
    #[wasm_bindgen(readonly)]
    pub start_column: usize,
    #[wasm_bindgen(readonly)]
    pub end_line: usize,
    #[wasm_bindgen(readonly)]
    pub end_column: usize,
    #[wasm_bindgen(readonly)]
    pub message: String,
}

#[wasm_bindgen]
impl WasmProject {
    #[wasm_bindgen]
    pub fn new(root_dir_name: &str, files: JsValue) -> Result<WasmProject, JsError> {
        let files: HashMap<String, String> =
            serde_wasm_bindgen::from_value(files).map_err(|e| e)?;

        Ok(WasmProject {
            root_dir_name: root_dir_name.to_string(),
            files,
            unsaved_files: HashMap::new(),
        })
    }

    #[wasm_bindgen]
    pub fn root_dir_name(&self) -> String {
        self.root_dir_name.clone()
    }

    #[wasm_bindgen]
    pub fn files(&self) -> Vec<String> {
        let mut saved_files = self.files.clone();
        self.unsaved_files.iter().for_each(|(k, v)| {
            saved_files.insert(k.clone(), v.clone());
        });
        let formatted_files = saved_files
            .iter()
            .map(|(k, v)| format!("{}BAML_PATH_SPLTTER{}", k, v))
            .collect::<Vec<String>>();
        formatted_files
    }

    #[wasm_bindgen]
    pub fn update_file(&mut self, name: &str, content: Option<String>) {
        if let Some(content) = content {
            self.files.insert(name.to_string(), content);
        } else {
            self.files.remove(name);
        }
    }

    #[wasm_bindgen]
    pub fn save_file(&mut self, name: &str, content: &str) {
        self.files.insert(name.to_string(), content.to_string());
        self.unsaved_files.remove(name);
    }

    #[wasm_bindgen]
    pub fn set_unsaved_file(&mut self, name: &str, content: Option<String>) {
        if let Some(content) = content {
            self.unsaved_files.insert(name.to_string(), content);
        } else {
            self.unsaved_files.remove(name);
        }
    }

    #[wasm_bindgen]
    pub fn diagnostics(&self, rt: &WasmRuntime) -> WasmDiagnosticError {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        WasmDiagnosticError {
            errors: rt.runtime.internal().diagnostics().clone(),
            all_files: hm.keys().map(|s| s.to_string()).collect(),
        }
    }

    #[wasm_bindgen]
    pub fn runtime(&self, env_vars: JsValue) -> Result<WasmRuntime, JsValue> {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        let env_vars: HashMap<String, String> =
            serde_wasm_bindgen::from_value(env_vars).map_err(|e| {
                JsValue::from_str(&format!(
                    "Expected env_vars to be HashMap<string, string>. {}",
                    e
                ))
            })?;

        BamlRuntime::from_file_content(&self.root_dir_name, &hm, env_vars)
            .map(|r| WasmRuntime { runtime: r })
            .map_err(|e| match e.downcast::<DiagnosticsError>() {
                Ok(e) => {
                    let wasm_error = WasmDiagnosticError {
                        errors: e,
                        all_files: hm.keys().map(|s| s.to_string()).collect(),
                    }
                    .into();
                    wasm_error
                }
                Err(e) => {
                    log::debug!("Error: {:#?}", e);
                    JsValue::from_str(&e.to_string())
                }
            })
    }

    #[wasm_bindgen]
    pub fn run_generators(
        &self,
        no_version_check: Option<bool>,
    ) -> Result<Vec<generator::WasmGeneratorOutput>, wasm_bindgen::JsError> {
        let fake_map: HashMap<String, String> = HashMap::new();
        let no_version_check = no_version_check.unwrap_or(false);

        let js_value = serde_wasm_bindgen::to_value(&fake_map).unwrap();
        let runtime = self.runtime(js_value);
        log::info!("Files are: {:#?}", self.files);
        let res = match runtime {
            Ok(runtime) => runtime.run_generators(&self.files, no_version_check),
            Err(e) => Err(wasm_bindgen::JsError::new(
                format!("Failed to create runtime: {:#?}", e).as_str(),
            )),
        };

        res
    }
}

#[wasm_bindgen(inspectable, getter_with_clone)]
pub struct WasmRuntime {
    runtime: BamlRuntime,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmFunction {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub test_cases: Vec<WasmTestCase>,
    #[wasm_bindgen(readonly)]
    pub test_snippet: String,
    #[wasm_bindgen(readonly)]
    pub signature: String,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmSpan {
    #[wasm_bindgen(readonly)]
    pub file_path: String,
    #[wasm_bindgen(readonly)]
    pub start: usize,
    #[wasm_bindgen(readonly)]
    pub end: usize,
    #[wasm_bindgen(readonly)]
    pub start_line: usize,
    #[wasm_bindgen(readonly)]
    pub end_line: usize,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmGeneratorConfig {
    #[wasm_bindgen(readonly)]
    pub output_type: String,
    #[wasm_bindgen(readonly)]
    pub version: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
}

impl From<&baml_runtime::internal_core::internal_baml_diagnostics::Span> for WasmSpan {
    fn from(span: &baml_runtime::internal_core::internal_baml_diagnostics::Span) -> Self {
        let (start, end) = span.line_and_column();
        WasmSpan {
            file_path: span.file.path().to_string(),
            start: span.start,
            end: span.end,
            start_line: start.0,
            end_line: end.0,
        }
    }
}

impl Default for WasmSpan {
    fn default() -> Self {
        WasmSpan {
            file_path: "".to_string(),
            start: 0,
            end: 0,
            start_line: 0,
            end_line: 0,
        }
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmParentFunction {
    #[wasm_bindgen(readonly)]
    pub start: usize,
    #[wasm_bindgen(readonly)]
    pub end: usize,
    #[wasm_bindgen(readonly)]
    pub name: String,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmTestCase {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub inputs: Vec<WasmParam>,
    #[wasm_bindgen(readonly)]
    pub error: Option<String>,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub parent_functions: Vec<WasmParentFunction>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone)]
pub struct WasmParam {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub value: Option<String>,
    #[wasm_bindgen(readonly)]
    pub error: Option<String>,
}

#[wasm_bindgen]
pub struct WasmFunctionResponse {
    function_response: baml_runtime::FunctionResult,
}

#[wasm_bindgen]
pub struct WasmTestResponse {
    test_response: anyhow::Result<baml_runtime::TestResponse>,
    span: Option<uuid::Uuid>,
    tracing_project_id: Option<String>,
}

#[wasm_bindgen]
pub enum TestStatus {
    Passed,
    LLMFailure,
    ParseFailure,
    UnableToRun,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmLLMResponse {
    scope: OrchestrationScope,
    pub model: String,
    prompt: RenderedPrompt,
    pub content: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub input_tokens: Option<u64>,
    pub output_tokens: Option<u64>,
    pub total_tokens: Option<u64>,
    pub stop_reason: Option<String>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmLLMFailure {
    scope: OrchestrationScope,
    pub model: Option<String>,
    prompt: RenderedPrompt,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub message: String,
    pub code: String,
}

#[wasm_bindgen]
impl WasmLLMFailure {
    #[wasm_bindgen]
    pub fn client_name(&self) -> String {
        self.scope.name()
    }
    pub fn prompt(&self) -> WasmPrompt {
        (&self.prompt, &self.scope).into()
    }
}

#[wasm_bindgen]
impl WasmLLMResponse {
    #[wasm_bindgen]
    pub fn client_name(&self) -> String {
        self.scope.name()
    }

    pub fn prompt(&self) -> WasmPrompt {
        (&self.prompt, &self.scope).into()
    }
}

#[wasm_bindgen]
impl WasmFunctionResponse {
    pub fn parsed_response(&self) -> Option<String> {
        self.function_response
            .parsed_content()
            .map(|p| serde_json::to_string(&BamlValue::from(p)))
            .map_or_else(|_| None, |s| s.ok())
    }

    #[wasm_bindgen]
    pub fn llm_failure(&self) -> Option<WasmLLMFailure> {
        llm_response_to_wasm_error(
            self.function_response.llm_response(),
            self.function_response.scope(),
        )
    }

    #[wasm_bindgen]
    pub fn llm_response(&self) -> Option<WasmLLMResponse> {
        (
            self.function_response.llm_response(),
            self.function_response.scope(),
        )
            .into_wasm()
    }
}

#[wasm_bindgen]
impl WasmTestResponse {
    #[wasm_bindgen]
    pub fn status(&self) -> TestStatus {
        match &self.test_response {
            Ok(t) => match t.status() {
                baml_runtime::TestStatus::Pass => TestStatus::Passed,
                baml_runtime::TestStatus::Fail(r) => match r {
                    baml_runtime::TestFailReason::TestUnspecified(_) => TestStatus::UnableToRun,
                    baml_runtime::TestFailReason::TestLLMFailure(_) => TestStatus::LLMFailure,
                    baml_runtime::TestFailReason::TestParseFailure(_) => TestStatus::ParseFailure,
                },
            },
            Err(_) => TestStatus::UnableToRun,
        }
    }

    #[wasm_bindgen]
    pub fn parsed_response(&self) -> Option<String> {
        self.test_response.as_ref().ok().and_then(|r| {
            r.function_response
                .parsed()
                .as_ref()
                .map(|p| {
                    p.as_ref()
                        .map(|p| serde_json::to_string(&BamlValue::from(p)))
                        .map_or_else(|_| None, |s| s.ok())
                })
                .flatten()
        })
    }

    #[wasm_bindgen]
    pub fn llm_failure(&self) -> Option<WasmLLMFailure> {
        self.test_response.as_ref().ok().and_then(|r| {
            llm_response_to_wasm_error(
                r.function_response.llm_response(),
                r.function_response.scope(),
            )
        })
    }

    #[wasm_bindgen]
    pub fn llm_response(&self) -> Option<WasmLLMResponse> {
        self.test_response.as_ref().ok().and_then(|r| {
            (
                r.function_response.llm_response(),
                r.function_response.scope(),
            )
                .into_wasm()
        })
    }

    #[wasm_bindgen]
    pub fn failure_message(&self) -> Option<String> {
        match self.test_response.as_ref() {
            Ok(r) => match r.status() {
                baml_runtime::TestStatus::Pass => None,
                baml_runtime::TestStatus::Fail(r) => r.render_error(),
            },
            Err(e) => Some(e.to_string()),
        }
    }

    fn _trace_url(&self) -> anyhow::Result<String> {
        let test_response = match self.test_response.as_ref() {
            Ok(t) => t,
            Err(e) => anyhow::bail!("Failed to get test response: {:?}", e),
        };
        let start_time = match test_response.function_response.llm_response() {
            LLMResponse::Success(s) => s.start_time,
            LLMResponse::LLMFailure(f) => f.start_time,
            _ => anyhow::bail!("Test has no start time"),
        };
        let start_time = time::OffsetDateTime::from_unix_timestamp(
            start_time
                .duration_since(web_time::UNIX_EPOCH)?
                .as_secs()
                .try_into()?,
        )?
        .format(&time::format_description::well_known::Rfc3339)?;

        let event_span_id = self
            .span
            .as_ref()
            .ok_or(anyhow::anyhow!("Test has no span ID"))?
            .to_string();
        let subevent_span_id = test_response
            .function_span
            .as_ref()
            .ok_or(anyhow::anyhow!("Function call has no span ID"))?
            .to_string();

        Ok(format!(
            "https://app.boundaryml.com/dashboard/projects/{}/drilldown?start_time={start_time}&eid={event_span_id}&s_eid={subevent_span_id}&test=false&onlyRootEvents=true",
            self.tracing_project_id.as_ref().ok_or(anyhow::anyhow!("No project ID specified"))?
        ))
    }

    #[wasm_bindgen]
    pub fn trace_url(&self) -> Option<String> {
        self._trace_url().ok()
    }
}

fn llm_response_to_wasm_error(
    r: &baml_runtime::internal::llm_client::LLMResponse,
    scope: &OrchestrationScope,
) -> Option<WasmLLMFailure> {
    match &r {
        LLMResponse::LLMFailure(f) => Some(WasmLLMFailure {
            scope: scope.clone(),
            model: f.model.clone(),
            prompt: f.prompt.clone(),
            start_time_unix_ms: f
                .start_time
                .duration_since(web_time::UNIX_EPOCH)
                .unwrap_or(web_time::Duration::ZERO)
                .as_millis() as u64,
            latency_ms: f.latency.as_millis() as u64,
            message: f.message.clone(),
            code: f.code.to_string(),
        }),
        _ => None,
    }
}

trait IntoWasm {
    type Output;
    fn into_wasm(&self) -> Self::Output;
}

impl IntoWasm
    for (
        &baml_runtime::internal::llm_client::LLMResponse,
        &OrchestrationScope,
    )
{
    type Output = Option<WasmLLMResponse>;

    fn into_wasm(&self) -> Self::Output {
        match &self.0 {
            baml_runtime::internal::llm_client::LLMResponse::Success(s) => Some(WasmLLMResponse {
                scope: self.1.clone(),
                model: s.model.clone(),
                prompt: s.prompt.clone(),
                content: s.content.clone(),
                start_time_unix_ms: s
                    .start_time
                    .duration_since(web_time::UNIX_EPOCH)
                    .unwrap_or(web_time::Duration::ZERO)
                    .as_millis() as u64,
                latency_ms: s.latency.as_millis() as u64,
                input_tokens: s.metadata.prompt_tokens,
                output_tokens: s.metadata.output_tokens,
                total_tokens: s.metadata.total_tokens,
                stop_reason: s.metadata.finish_reason.clone(),
            }),
            _ => None,
        }
    }
}

trait WithRenderError {
    fn render_error(&self) -> Option<String>;
}

impl WithRenderError for baml_runtime::TestFailReason<'_> {
    fn render_error(&self) -> Option<String> {
        match self {
            baml_runtime::TestFailReason::TestUnspecified(e) => Some(e.to_string()),
            baml_runtime::TestFailReason::TestLLMFailure(f) => f.render_error(),
            baml_runtime::TestFailReason::TestParseFailure(e) => Some(e.to_string()),
        }
    }
}

impl WithRenderError for baml_runtime::internal::llm_client::LLMResponse {
    fn render_error(&self) -> Option<String> {
        match self {
            baml_runtime::internal::llm_client::LLMResponse::Success(_) => None,
            baml_runtime::internal::llm_client::LLMResponse::LLMFailure(f) => {
                format!("{} {}", f.message, f.code.to_string()).into()
            }
            baml_runtime::internal::llm_client::LLMResponse::OtherFailure(e) => {
                format!("{}", e).into()
            }
        }
    }
}

fn get_dummy_value(
    indent: usize,
    allow_multiline: bool,
    t: &baml_runtime::FieldType,
) -> Option<String> {
    let indent_str = "  ".repeat(indent);
    match t {
        baml_runtime::FieldType::Primitive(t) => {
            let dummy = match t {
                baml_runtime::TypeValue::String => {
                    if allow_multiline {
                        format!(
                            "#\"\n{indent1}hello world\n{indent_str}\"#",
                            indent1 = "  ".repeat(indent + 1)
                        )
                    } else {
                        "\"a_string\"".to_string()
                    }
                }
                baml_runtime::TypeValue::Int => "123".to_string(),
                baml_runtime::TypeValue::Float => "0.5".to_string(),
                baml_runtime::TypeValue::Bool => "true".to_string(),
                baml_runtime::TypeValue::Null => "null".to_string(),
                baml_runtime::TypeValue::Image => {
                    "{ url \"https://imgs.xkcd.com/comics/standards.png\"}".to_string()
                }
                baml_runtime::TypeValue::Audio => {
                    "{ url \"https://actions.google.com/sounds/v1/emergency/beeper_emergency_call.ogg\"}".to_string()
                }
            };

            Some(dummy)
        }
        baml_runtime::FieldType::Enum(_) => None,
        baml_runtime::FieldType::Class(_) => None,
        baml_runtime::FieldType::List(item) => {
            let dummy = get_dummy_value(indent + 1, allow_multiline, item);
            // Repeat it 2 times
            match dummy {
                Some(dummy) => {
                    if allow_multiline {
                        Some(format!(
                            "[\n{indent1}{dummy},\n{indent1}{dummy}\n{indent_str}]",
                            dummy = dummy,
                            indent1 = "  ".repeat(indent + 1)
                        ))
                    } else {
                        Some(format!("[{}, {}]", dummy, dummy))
                    }
                }
                _ => None,
            }
        }
        baml_runtime::FieldType::Map(k, v) => {
            let dummy_k = get_dummy_value(indent, false, k);
            let dummy_v = get_dummy_value(indent + 1, allow_multiline, v);
            match (dummy_k, dummy_v) {
                (Some(k), Some(v)) => {
                    if allow_multiline {
                        Some(format!(
                            r#"{{
{indent1}{k} {v}
{indent_str}}}"#,
                            indent1 = "  ".repeat(indent + 1),
                        ))
                    } else {
                        Some(format!("{{ {k} {v} }}"))
                    }
                }
                _ => None,
            }
        }
        baml_runtime::FieldType::Union(fields) => fields
            .iter()
            .filter_map(|f| get_dummy_value(indent, allow_multiline, f))
            .next(),
        baml_runtime::FieldType::Tuple(vals) => {
            let dummy = vals
                .iter()
                .filter_map(|f| get_dummy_value(0, false, f))
                .collect::<Vec<_>>()
                .join(", ");
            Some(format!("({},)", dummy))
        }
        baml_runtime::FieldType::Optional(_) => None,
    }
}

fn get_dummy_field(indent: usize, name: &str, t: &baml_runtime::FieldType) -> Option<String> {
    let indent_str = "  ".repeat(indent);
    let dummy = get_dummy_value(indent, true, t);
    match dummy {
        Some(dummy) => Some(format!("{indent_str}{name} {dummy}")),
        _ => None,
    }
}

// Rust-only methods
impl WasmRuntime {
    pub fn run_generators(
        &self,
        input_files: &HashMap<String, String>,
        no_version_check: bool,
    ) -> Result<Vec<generator::WasmGeneratorOutput>, wasm_bindgen::JsError> {
        Ok(self
            .runtime
            // convert the input_files into HashMap(PathBuf, string)
            .run_generators(
                &input_files
                    .iter()
                    .map(|(k, v)| (PathBuf::from(k), v.clone()))
                    .collect(),
                no_version_check,
            )
            .map_err(|e| JsError::new(format!("{e:#}").as_str()))?
            .into_iter()
            .map(|g| g.into())
            .collect())
    }
}

#[wasm_bindgen]
impl WasmRuntime {
    #[wasm_bindgen]

    pub fn check_if_in_prompt(&self, cursor_idx: usize) -> bool {
        self.runtime.internal().ir().walk_functions().any(|f| {
            f.elem().configs().expect("configs").iter().any(|config| {
                let span = &config.prompt_span;
                cursor_idx >= span.start && cursor_idx <= span.end
            })
        })
    }

    #[wasm_bindgen]
    pub fn list_functions(&self) -> Vec<WasmFunction> {
        self.runtime
            .internal()
            .ir()
            .walk_functions()
            .map(|f| {
                let snippet = format!(
                    r#"test TestName {{
  functions [{name}]
  args {{
{args}
  }}
}}
"#,
                    name = f.name(),
                    args = f
                        .inputs()
                        .iter()
                        .map(|(k, t)| get_dummy_field(2, k, t))
                        .filter_map(|x| x) // Add this line to filter out None values
                        .collect::<Vec<_>>()
                        .join("\n")
                );

                let wasm_span = match f.span() {
                    Some(span) => span.into(),
                    None => WasmSpan::default(),
                };

                WasmFunction {
                    name: f.name().to_string(),
                    span: wasm_span,
                    signature: {
                        let inputs = f
                            .inputs()
                            .iter()
                            .map(|(k, t)| get_dummy_field(2, k, t))
                            .filter_map(|x| x) // Add this line to filter out None values
                            .collect::<Vec<_>>()
                            .join(",");

                        format!("({}) -> {}", inputs, f.output().to_string())
                    },
                    test_snippet: snippet,
                    test_cases: f
                        .walk_tests()
                        .map(|tc| {
                            let params = match tc.test_case_params(&self.runtime.env_vars()) {
                                Ok(params) => Ok(params
                                    .iter()
                                    .map(|(k, v)| {
                                        let as_str = match v {
                                            Ok(v) => match serde_json::to_string(v) {
                                                Ok(s) => Ok(s),
                                                Err(e) => Err(e.to_string()),
                                            },
                                            Err(e) => Err(e.to_string()),
                                        };

                                        let (value, error) = match as_str {
                                            Ok(s) => (Some(s), None),
                                            Err(e) => (None, Some(e)),
                                        };

                                        WasmParam {
                                            name: k.to_string(),
                                            value,
                                            error,
                                        }
                                    })
                                    .collect()),
                                Err(e) => Err(e.to_string()),
                            };

                            let (mut params, error) = match params {
                                Ok(p) => (p, None),
                                Err(e) => (Vec::new(), Some(e)),
                            };

                            // Any missing params should be set to an error
                            f.inputs().iter().for_each(|(param_name, t)| {
                                if !params.iter().any(|p| p.name == *param_name) && !t.is_optional()
                                {
                                    params.insert(
                                        0,
                                        WasmParam {
                                            name: param_name.to_string(),
                                            value: None,
                                            error: Some("Missing parameter".to_string()),
                                        },
                                    );
                                }
                            });

                            let wasm_span = match tc.span() {
                                Some(span) => span.into(),
                                None => WasmSpan::default(),
                            };

                            WasmTestCase {
                                name: tc.test_case().name.clone(),
                                inputs: params,
                                error,
                                span: wasm_span,
                                parent_functions: tc
                                    .test_case()
                                    .functions
                                    .iter()
                                    .map(|f| {
                                        let (start, end) = f
                                            .attributes
                                            .span
                                            .as_ref()
                                            .map_or((0, 0), |f| (f.start, f.end));
                                        WasmParentFunction {
                                            start,
                                            end,
                                            name: f.elem.name().to_string(),
                                        }
                                    })
                                    .collect(),
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    }

    #[wasm_bindgen]
    pub fn list_generators(&self) -> Vec<WasmGeneratorConfig> {
        self.runtime
            .internal()
            .ir()
            .configuration()
            .generators
            .iter()
            .map(|(generator, _)| WasmGeneratorConfig {
                output_type: generator.output_type.clone().to_string(),
                version: generator.version.clone(),
                span: WasmSpan {
                    file_path: generator.span.file.path().to_string(),
                    start: generator.span.start,
                    end: generator.span.end,
                    start_line: generator.span.line_and_column().0 .0,
                    end_line: generator.span.line_and_column().1 .0,
                },
            })
            .collect()
    }

    #[wasm_bindgen]
    pub fn check_version(
        generator_version: &str,
        current_version: &str,
        generator_type: &str,
        version_check_mode: &str,
        generator_language: &str,
        is_diagnostic: bool,
    ) -> Option<String> {
        // Convert string parameters to enums
        let generator_type = match generator_type {
            "VSCodeCLI" => GeneratorType::VSCodeCLI,
            "VSCode" => GeneratorType::VSCode,
            "CLI" => GeneratorType::CLI,
            _ => return Some("Invalid generator type".to_string()),
        };

        let version_check_mode = match version_check_mode {
            "Strict" => VersionCheckMode::Strict,
            "None" => VersionCheckMode::None,
            _ => return Some("Invalid version check mode".to_string()),
        };

        let generator_language = match generator_language {
            "python/pydantic" => GeneratorOutputType::PythonPydantic,
            "typescript" => GeneratorOutputType::Typescript,
            "ruby/sorbet" => GeneratorOutputType::RubySorbet,
            _ => return Some("Invalid generator language".to_string()),
        };

        check_version(
            generator_version,
            current_version,
            generator_type,
            version_check_mode,
            generator_language,
            is_diagnostic,
        )
        .map(|error| error.msg())
    }

    #[wasm_bindgen]
    pub fn required_env_vars(&self) -> Vec<String> {
        self.runtime
            .internal()
            .ir()
            .required_env_vars()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[wasm_bindgen]
    pub fn search_for_symbol(&self, symbol: &str) -> Option<SymbolLocation> {
        let runtime = self.runtime.internal().ir();

        if let Ok(walker) = runtime.find_enum(symbol) {
            let elem = walker.span().unwrap();

            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }
        if let Ok(walker) = runtime.find_class(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_function(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_client(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();

            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_retry_policy(symbol) {
            let elem = walker.span().unwrap();

            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        if let Ok(walker) = runtime.find_template_string(symbol) {
            let elem = walker.span().unwrap();
            let _uri_str = elem.file.path().to_string(); // Store the String in a variable
            let ((s_line, s_character), (e_line, e_character)) = elem.line_and_column();
            return Some(SymbolLocation {
                uri: elem.file.path().to_string(), // Use the variable here
                start_line: s_line,
                start_character: s_character,
                end_line: e_line,
                end_character: e_character,
            });
        }

        None
    }

    #[wasm_bindgen]
    pub fn get_function_at_position(
        &self,
        file_name: &str,
        selected_func: &str,
        cursor_idx: usize,
    ) -> Option<WasmFunction> {
        let functions = self.list_functions();

        for function in functions.clone() {
            let span = function.span.clone(); // Clone the span

            if span.file_path.as_str().ends_with(file_name)
                && ((span.start + 1)..=(span.end + 1)).contains(&cursor_idx)
            {
                return Some(function);
            }
        }

        let testcases = self.list_testcases();

        for tc in testcases {
            let span = tc.span;
            if span.file_path.as_str().ends_with(file_name)
                && ((span.start + 1)..=(span.end + 1)).contains(&cursor_idx)
            {
                if let Some(parent_function) =
                    tc.parent_functions.iter().find(|f| f.name == selected_func)
                {
                    return functions.into_iter().find(|f| f.name == selected_func);
                } else if let Some(first_function) = tc.parent_functions.get(0) {
                    return functions
                        .into_iter()
                        .find(|f| f.name == first_function.name);
                }
            }
        }

        None
    }

    #[wasm_bindgen]
    pub fn get_function_of_testcase(
        &self,
        file_name: &str,
        cursor_idx: usize,
    ) -> Option<WasmParentFunction> {
        let testcases = self.list_testcases();

        for tc in testcases {
            let span = tc.span;
            if span.file_path.as_str().ends_with(file_name)
                && ((span.start + 1)..=(span.end + 1)).contains(&cursor_idx)
            {
                let first_function = tc
                    .parent_functions
                    .iter()
                    .find(|f| f.start <= cursor_idx && cursor_idx <= f.end)
                    .cloned();

                return first_function;
            }
        }
        None
    }

    #[wasm_bindgen]
    pub fn list_testcases(&self) -> Vec<WasmTestCase> {
        self.runtime
            .internal()
            .ir()
            .walk_tests()
            .map(|tc| {
                let params = match tc.test_case_params(&self.runtime.env_vars()) {
                    Ok(params) => Ok(params
                        .iter()
                        .map(|(k, v)| {
                            let as_str = match v {
                                Ok(v) => match serde_json::to_string(v) {
                                    Ok(s) => Ok(s),
                                    Err(e) => Err(e.to_string()),
                                },
                                Err(e) => Err(e.to_string()),
                            };

                            let (value, error) = match as_str {
                                Ok(s) => (Some(s), None),
                                Err(e) => (None, Some(e)),
                            };

                            WasmParam {
                                name: k.to_string(),
                                value,
                                error,
                            }
                        })
                        .collect()),
                    Err(e) => Err(e.to_string()),
                };

                let (mut params, error) = match params {
                    Ok(p) => (p, None),
                    Err(e) => (Vec::new(), Some(e)),
                };
                // Any missing params should be set to an error
                // Any missing params should be set to an error
                tc.function().inputs().iter().for_each(|func_params| {
                    let (param_name, t) = func_params;
                    if !params.iter().any(|p| p.name == *param_name) && !t.is_optional() {
                        params.push(WasmParam {
                            name: param_name.to_string(),
                            value: None,
                            error: Some("Missing parameter".to_string()),
                        });
                    }
                });
                let wasm_span = match tc.span() {
                    Some(span) => span.into(),
                    None => WasmSpan::default(),
                };

                WasmTestCase {
                    name: tc.test_case().name.clone(),
                    inputs: params,
                    error,
                    span: wasm_span,
                    parent_functions: tc
                        .test_case()
                        .functions
                        .iter()
                        .map(|f| {
                            let (start, end) = f
                                .attributes
                                .span
                                .as_ref()
                                .map_or((0, 0), |f| (f.start, f.end));
                            WasmParentFunction {
                                start,
                                end,
                                name: f.elem.name().to_string(),
                            }
                        })
                        .collect(),
                }
            })
            .collect()
    }

    #[wasm_bindgen]
    pub fn get_testcase_from_position(
        &self,
        parent_function: WasmFunction,
        cursor_idx: usize,
    ) -> Option<WasmTestCase> {
        let testcases = parent_function.test_cases;
        for testcase in testcases {
            let span = testcase.clone().span;

            if span.file_path.as_str() == (parent_function.span.file_path)
                && ((span.start + 1)..=(span.end + 1)).contains(&cursor_idx)
            {
                return Some(testcase);
            }
        }
        None
    }
}

#[wasm_bindgen]
impl WasmFunction {
    #[wasm_bindgen]
    pub fn render_prompt(
        &self,
        rt: &WasmRuntime,
        params: JsValue,
    ) -> Result<WasmPrompt, wasm_bindgen::JsError> {
        let params = serde_wasm_bindgen::from_value::<BamlMap<String, BamlValue>>(params)?;
        let missing_env_vars = rt.runtime.internal().ir().required_env_vars();
        let ctx = rt
            .runtime
            .create_ctx_manager(BamlValue::String("wasm".to_string()))
            .create_ctx_with_default(missing_env_vars.iter());

        rt.runtime
            .internal()
            .render_prompt(&self.name, &ctx, &params, None)
            .as_ref()
            .map(|(p, scope)| (p, scope).into())
            .map_err(|e| wasm_bindgen::JsError::new(format!("{e:?}").as_str()))
    }

    #[wasm_bindgen]
    pub async fn render_raw_curl(
        &self,
        rt: &WasmRuntime,
        params: JsValue,
        stream: bool,
    ) -> Result<String, wasm_bindgen::JsError> {
        let params = serde_wasm_bindgen::from_value::<BamlMap<String, BamlValue>>(params)?;
        let missing_env_vars = rt.runtime.internal().ir().required_env_vars();

        let ctx = rt
            .runtime
            .create_ctx_manager(BamlValue::String("wasm".to_string()))
            .create_ctx_with_default(missing_env_vars.iter());

        let result = rt
            .runtime
            .internal()
            .render_prompt(&self.name, &ctx, &params, None);

        let final_prompt = match result {
            Ok((prompt, _)) => match prompt {
                RenderedPrompt::Chat(chat_messages) => chat_messages,
                RenderedPrompt::Completion(_) => vec![], // or handle this case differently
            },
            Err(e) => return Err(wasm_bindgen::JsError::new(format!("{:#?}", e).as_str())),
        };

        rt.runtime
            .internal()
            .render_raw_curl(&self.name, &ctx, &final_prompt, stream, None)
            .await
            .map_err(|e| wasm_bindgen::JsError::new(format!("{e:#?}").as_str()))
    }

    #[wasm_bindgen]
    pub async fn run_test(
        &self,
        rt: &mut WasmRuntime,
        test_name: String,
        cb: js_sys::Function,
    ) -> Result<WasmTestResponse, JsValue> {
        let rt = &rt.runtime;

        let function_name = self.name.clone();

        let cb = Box::new(move |r| {
            let this = JsValue::NULL;
            let res = WasmFunctionResponse {
                function_response: r,
            }
            .into();
            cb.call1(&this, &res).unwrap();
        });

        let ctx = rt.create_ctx_manager(BamlValue::String("wasm".to_string()));
        let (test_response, span) = rt
            .run_test(&function_name, &test_name, &ctx, Some(cb))
            .await;

        Ok(WasmTestResponse {
            test_response,
            span,
            tracing_project_id: rt.env_vars().get("BOUNDARY_PROJECT_ID").cloned(),
        })
    }
}
