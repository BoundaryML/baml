mod runtime_ctx;
pub mod runtime_prompt;

use std::collections::HashMap;

pub use self::runtime_ctx::WasmRuntimeContext;
use crate::runtime_wasm::runtime_prompt::WasmPrompt;
use baml_runtime::internal::llm_client::orchestrator::OrchestrationScope;
use baml_runtime::runtime_interface::PublicInterface;
use baml_runtime::InternalRuntimeInterface;
use baml_runtime::{
    internal::llm_client::LLMResponse, BamlRuntime, DiagnosticsError, IRHelper, RenderedPrompt,
};
use baml_types::BamlMap;
use baml_types::BamlValue;
use js_sys::JsString;
use serde::Deserialize;
use serde::Serialize;
use wasm_bindgen::prelude::*;

//Run: wasm-pack test --firefox --headless  --features internal,wasm
// but for browser we likely need to do         wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
// Node is run using: wasm-pack test --node --features internal,wasm

#[wasm_bindgen(start)]
pub fn on_wasm_init() {
    match console_log::init_with_level(log::Level::Debug) {
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
use wasm_bindgen::prelude::*;

#[wasm_bindgen(getter_with_clone)]
pub struct SymbolLocation {
    pub uri: String,
    pub start_line: usize,
    pub start_character: usize,
    pub end_line: usize,
    pub end_character: usize,
}

// impl std::error::Error for WasmDiagnosticError {}

// impl std::fmt::Display for WasmDiagnosticError {
//     fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
//         write!(f, "{:?}", self.errors)
//     }
// }

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
        log::info!("Saving file: {}", name);
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
    pub fn runtime(&self, ctx: &WasmRuntimeContext) -> Result<WasmRuntime, JsValue> {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        BamlRuntime::from_file_content(&self.root_dir_name, &hm, &ctx.ctx)
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
}

#[wasm_bindgen(inspectable, getter_with_clone)]
pub struct WasmRuntime {
    runtime: BamlRuntime,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmFunction {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub test_cases: Vec<WasmTestCase>,
    #[wasm_bindgen(readonly)]
    pub test_snippet: String,
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
        self.test_response
            .as_ref()
            .ok()
            .and_then(|r| match r.status() {
                baml_runtime::TestStatus::Pass => None,
                baml_runtime::TestStatus::Fail(r) => r.render_error(),
            })
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
            };

            Some(dummy)
        }
        baml_runtime::FieldType::Enum(EnumId) => None,
        baml_runtime::FieldType::Class(ClassId) => None,
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

#[wasm_bindgen]
impl WasmRuntime {
    #[wasm_bindgen]
    pub fn list_functions(&self, ctx: &WasmRuntimeContext) -> Vec<WasmFunction> {
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
                        .right()
                        .map(|func_params| {
                            func_params
                                .iter()
                                .filter_map(|(k, t)| get_dummy_field(2, k, t))
                                .collect::<Vec<_>>()
                                .join("\n")
                        })
                        .unwrap_or_default()
                );

                let wasm_span = match f.span() {
                    Some(span) => WasmSpan {
                        file_path: span.file.path().to_string(),
                        start: span.start,
                        end: span.end,
                    },
                    None => WasmSpan {
                        file_path: "".to_string(),
                        start: 0,
                        end: 0,
                    },
                };

                WasmFunction {
                    name: f.name().to_string(),
                    span: wasm_span,
                    test_snippet: snippet,
                    test_cases: f
                        .walk_tests()
                        .map(|tc| {
                            let params = match tc.test_case_params(&ctx.ctx.env) {
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
                            let _ = f.inputs().right().map(|func_params| {
                                for (param_name, _) in func_params {
                                    if !params.iter().any(|p| p.name.cmp(param_name).is_eq()) {
                                        params.insert(
                                            0,
                                            WasmParam {
                                                name: param_name.to_string(),
                                                value: None,
                                                error: Some("Missing parameter".to_string()),
                                            },
                                        );
                                    }
                                }
                            });

                            WasmTestCase {
                                name: tc.test_case().name.clone(),
                                inputs: params,
                                error,
                            }
                        })
                        .collect(),
                }
            })
            .collect()
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
    pub fn searchForSymbol(&self, symbol: &str) -> Option<SymbolLocation> {
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

            let uri_str = elem.file.path().to_string(); // Store the String in a variable
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

            let uri_str = elem.file.path().to_string(); // Store the String in a variable
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

            let uri_str = elem.file.path().to_string(); // Store the String in a variable
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

            let uri_str = elem.file.path().to_string(); // Store the String in a variable
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
            let uri_str = elem.file.path().to_string(); // Store the String in a variable
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
}

#[wasm_bindgen]
impl WasmFunction {
    #[wasm_bindgen]
    pub fn render_prompt(
        &self,
        rt: &mut WasmRuntime,
        ctx: &runtime_ctx::WasmRuntimeContext,
        params: JsValue,
    ) -> Result<WasmPrompt, wasm_bindgen::JsError> {
        let mut params = serde_wasm_bindgen::from_value::<BamlMap<String, BamlValue>>(params)?;
        let env_vars = rt.runtime.internal().ir().required_env_vars();

        // For anything env vars that are not provided, fill with empty strings
        let mut ctx = ctx.ctx.clone();

        for var in env_vars {
            if !ctx.env.contains_key(var) {
                ctx.env.insert(var.into(), "".to_string());
            }
        }

        rt.runtime
            .internal()
            .render_prompt(&self.name, &ctx, &params, None)
            .as_ref()
            .map(|(p, scope)| (p, scope).into())
            .map_err(|e| wasm_bindgen::JsError::new(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn run_test(
        &self,
        rt: &mut WasmRuntime,
        ctx: &runtime_ctx::WasmRuntimeContext,
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
        let (test_response, span) = rt
            .run_test(&function_name, &test_name, ctx.ctx.clone(), Some(cb))
            .await;

        Ok(WasmTestResponse {
            test_response,
            span,
        })
    }
}
