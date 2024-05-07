mod runtime_ctx;
mod runtime_prompt;

use std::collections::HashMap;

#[allow(unused_imports)]
use baml_runtime::{
    internal::llm_client::LLMResponse, BamlRuntime, DiagnosticsError, RenderedPrompt,
    RuntimeInterface,
};
use wasm_bindgen::prelude::*;

use baml_runtime::InternalRuntimeInterface;

use crate::runtime_wasm::runtime_prompt::WasmPrompt;

use self::runtime_ctx::WasmRuntimeContext;

#[wasm_bindgen(start)]
pub fn on_wasm_init() {
    match console_log::init_with_level(log::Level::Debug) {
        Ok(_) => web_sys::console::log_1(&"Initialized BAML runtime logging".into()),
        Err(e) => web_sys::console::log_1(
            &format!("Failed to initialize BAML runtime logging: {:?}", e).into(),
        ),
    }
}

#[wasm_bindgen]
pub struct WasmProject {
    root_dir_name: String,
    // This is the version of the file on disk
    files: HashMap<String, String>,
    // This is the version of the file that is currently being edited
    // (unsaved changes)
    unsaved_files: HashMap<String, String>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Debug)]
pub struct WasmDiagnosticError {
    errors: DiagnosticsError,
    pub all_files: Vec<String>,
}

impl std::error::Error for WasmDiagnosticError {}

impl std::fmt::Display for WasmDiagnosticError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}", self.errors)
    }
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

#[wasm_bindgen(getter_with_clone)]
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
        saved_files
            .iter()
            .map(|(k, v)| format!("{}BAML_PATH_SPLTTER{}", k, v))
            .collect()
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
    pub fn runtime(&self) -> Result<WasmRuntime, JsValue> {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        BamlRuntime::from_file_content(&self.root_dir_name, &hm)
            .map(|r| WasmRuntime { runtime: r })
            .map_err(|e| match e.downcast::<DiagnosticsError>() {
                Ok(e) => WasmDiagnosticError {
                    errors: e,
                    all_files: hm.keys().map(|s| s.to_string()).collect(),
                }
                .into(),
                Err(e) => JsValue::from_str(&e.to_string()),
            })
    }
}

#[wasm_bindgen]
pub struct WasmRuntime {
    runtime: BamlRuntime,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmFunction {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub test_cases: Vec<WasmTestCase>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmTestCase {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub inputs: Vec<WasmParam>,
}

#[wasm_bindgen(getter_with_clone)]
#[derive(Clone)]
pub struct WasmParam {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub value: JsValue,
}

#[wasm_bindgen]
pub struct WasmTestResponse {
    test_response: baml_runtime::TestResponse,
}

#[wasm_bindgen]
pub enum TestStatus {
    Passed,
    LLMFailure,
    ParseFailure,
    UnableToRun,
}

#[wasm_bindgen(getter_with_clone)]
pub struct WasmLLMResponse {
    pub client: String,
    pub model: String,
    prompt: RenderedPrompt,
    pub content: String,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
}

impl WasmLLMResponse {
    pub fn prompt(&self) -> WasmPrompt {
        (self.prompt.clone(), self.client.clone()).into()
    }
}

#[wasm_bindgen]
impl WasmTestResponse {
    #[wasm_bindgen]
    pub fn status(&self) -> TestStatus {
        match self.test_response.status() {
            baml_runtime::TestStatus::Pass => TestStatus::Passed,
            baml_runtime::TestStatus::Fail(r) => match r {
                baml_runtime::TestFailReason::TestUnspecified(_) => TestStatus::UnableToRun,
                baml_runtime::TestFailReason::TestLLMFailure(_) => TestStatus::LLMFailure,
                baml_runtime::TestFailReason::TestParseFailure(_) => TestStatus::ParseFailure,
            },
        }
    }

    #[wasm_bindgen]
    pub fn llm_response(&self) -> Option<WasmLLMResponse> {
        match &self.test_response.function_response {
            Ok(f) => f.llm_response.into_wasm(),
            Err(_e) => None,
        }
    }

    #[wasm_bindgen]
    pub fn failure_message(&self) -> Option<String> {
        match self.test_response.status() {
            baml_runtime::TestStatus::Pass => None,
            baml_runtime::TestStatus::Fail(r) => r.render_error(),
        }
    }
}
trait IntoWasm {
    type Output;
    fn into_wasm(&self) -> Self::Output;
}

impl IntoWasm for baml_runtime::internal::llm_client::LLMResponse {
    type Output = Option<WasmLLMResponse>;

    fn into_wasm(&self) -> Self::Output {
        match &self {
            baml_runtime::internal::llm_client::LLMResponse::Success(s) => Some(WasmLLMResponse {
                client: s.client.clone(),
                model: s.model.clone(),
                prompt: s.prompt.clone(),
                content: s.content.clone(),
                start_time_unix_ms: s.start_time_unix_ms,
                latency_ms: s.latency_ms,
            }),
            baml_runtime::internal::llm_client::LLMResponse::Retry(r) if r.passed.is_some() => {
                r.passed.as_ref().and_then(|p| p.into_wasm())
            }
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
            baml_runtime::internal::llm_client::LLMResponse::Retry(r) => {
                if let Some(passed) = &r.passed {
                    None
                } else {
                    r.failed.last().and_then(|f| f.render_error())
                }
            }
            baml_runtime::internal::llm_client::LLMResponse::OtherFailures(o) => {
                Some(o.to_string())
            }
        }
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
            .map(|f| WasmFunction {
                name: f.name().to_string(),
                test_cases: f
                    .walk_tests()
                    .map(|tc| WasmTestCase {
                        name: tc.test_case().name.clone(),
                        inputs: match tc.test_case_params(&ctx.ctx.env) {
                            Ok(params) => params
                                .iter()
                                .map(|(k, v)| WasmParam {
                                    name: k.to_string(),
                                    value: match v {
                                        Ok(v) => serde_json::to_string_pretty(v).unwrap().into(),
                                        Err(e) => {
                                            serde_wasm_bindgen::to_value(&e.to_string()).unwrap()
                                        }
                                    },
                                })
                                .collect(),
                            Err(_) => vec![],
                        },
                    })
                    .collect(),
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
        let params = serde_wasm_bindgen::from_value(params)?;
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
            .render_prompt(&self.name, &ctx, &params)
            .map(|p| p.into())
            .map_err(|e| wasm_bindgen::JsError::new(&e.to_string()))
    }

    #[wasm_bindgen]
    pub async fn run_test(
        &self,
        rt: &mut WasmRuntime,
        ctx: &runtime_ctx::WasmRuntimeContext,
        test_name: String,
    ) -> Result<WasmTestResponse, JsValue> {
        // For anything env vars that are not provided, fill with empty strings
        let ctx = ctx.ctx.clone();

        let rt = &rt.runtime;

        let function_name = self.name.clone();

        let res = rt
            .run_test(&function_name, &test_name, &ctx)
            .await
            .map_err(|e| JsValue::from_str(&e.to_string()))?;

        Ok(WasmTestResponse { test_response: res })
    }
}
