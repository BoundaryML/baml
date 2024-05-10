mod runtime_ctx;
mod runtime_prompt;

use std::collections::HashMap;

#[allow(unused_imports)]
use baml_runtime::{
    internal::llm_client::LLMResponse, BamlRuntime, DiagnosticsError, RenderedPrompt,
    RuntimeInterface,
};
use serde_json::error;
use wasm_bindgen::prelude::*;

use baml_runtime::{InternalRuntimeInterface, RuntimeContext};

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
    pub fn runtime(&self, ctx: &WasmRuntimeContext) -> Result<WasmRuntime, JsValue> {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        BamlRuntime::from_file_content(&self.root_dir_name, &hm, &ctx.ctx)
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
    #[wasm_bindgen(readonly)]
    pub error: Option<String>,
}

#[wasm_bindgen(getter_with_clone)]
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

#[wasm_bindgen(getter_with_clone)]
pub struct WasmLLMFailure {
    pub client: String,
    pub model: Option<String>,
    prompt: RenderedPrompt,
    pub start_time_unix_ms: u64,
    pub latency_ms: u64,
    pub message: String,
    pub code: String,
}

impl WasmLLMFailure {
    pub fn prompt(&self) -> WasmPrompt {
        (self.prompt.clone(), self.client.clone()).into()
    }
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
    pub fn parsed_response(&self) -> Option<String> {
        match &self.test_response.function_response {
            Ok(f) => match &f.parsed {
                Some(Ok((p, _))) => match serde_json::to_string(p) {
                    Ok(s) => Some(s),
                    Err(_) => None,
                },
                _ => None,
            },
            Err(_) => None,
        }
    }

    #[wasm_bindgen]
    pub fn llm_failure(&self) -> Option<WasmLLMFailure> {
        match &self.test_response.function_response {
            Ok(f) => llm_response_to_wasm_error(&f.llm_response),
            Err(_) => None,
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

fn llm_response_to_wasm_error(
    r: &baml_runtime::internal::llm_client::LLMResponse,
) -> Option<WasmLLMFailure> {
    match &r {
        LLMResponse::LLMFailure(f) => Some(WasmLLMFailure {
            client: f.client.clone(),
            model: f.model.clone(),
            prompt: f.prompt.clone(),
            start_time_unix_ms: f.start_time_unix_ms,
            latency_ms: f.latency_ms,
            message: f.message.clone(),
            code: f.code.to_string(),
        }),
        LLMResponse::Retry(f) if f.passed.is_none() => {
            f.failed.last().and_then(|e| llm_response_to_wasm_error(e))
        }
        _ => None,
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
        let mut params = serde_wasm_bindgen::from_value::<
            indexmap::IndexMap<String, serde_json::Value>,
        >(params)?;
        let env_vars = rt.runtime.internal().ir().required_env_vars();

        // For anything env vars that are not provided, fill with empty strings
        let mut ctx = ctx.ctx.clone();

        for var in env_vars {
            if !ctx.env.contains_key(var) {
                ctx.env.insert(var.into(), "".to_string());
            }
        }

        // Fill any missing params with empty strings
        for var in self
            .test_cases
            .iter()
            .flat_map(|tc| tc.inputs.iter().map(|p| &p.name))
        {
            if !params.contains_key(var) {
                params.insert(var.clone(), serde_json::Value::Null);
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
