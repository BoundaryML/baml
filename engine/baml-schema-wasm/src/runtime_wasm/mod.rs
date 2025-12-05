pub mod generator;
pub mod runtime_prompt;
use std::{cell::RefCell, collections::HashMap, path::PathBuf, rc::Rc, str::FromStr};

use anyhow::Context;
use baml_compiler::{hir::HeaderContext, watch::shared_handler};
// Conditional runtime selection based on the "thir-interpreter" feature flag
#[cfg(feature = "thir-interpreter")]
pub use baml_runtime::async_interpreter_runtime::BamlAsyncInterpreterRuntime as CoreBamlRuntime;
#[cfg(not(feature = "thir-interpreter"))]
pub use baml_runtime::async_vm_runtime::BamlAsyncVmRuntime as CoreBamlRuntime;
use baml_runtime::{
    control_flow::{ControlFlowVisualization, NodeType as RuntimeNodeType},
    internal::{
        llm_client::{
            orchestrator::{ExecutionScope, OrchestrationScope, OrchestratorNode},
            LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    internal_baml_diagnostics::SerializedSpan,
    BamlSrcReader, DiagnosticsError, IRHelper, InternalRuntimeInterface, RenderCurlSettings,
    RenderedPrompt,
};
use baml_types::{BamlValue, GeneratorOutputType, ResponseCheck};
use generators_lib::version_check::{check_version, GeneratorType, VersionCheckMode};
use indexmap::IndexMap;
use internal_baml_core::feature_flags::FeatureFlags;
use internal_llm_client::AllowedRoleMetadata;
use itertools::join;
use js_sys::{Promise, Uint8Array};
use jsonish::ResponseBamlValue;
use serde::{Deserialize, Serialize};
use wasm_bindgen::{prelude::*, JsError, JsValue};
use wasm_bindgen_futures::JsFuture;

use self::runtime_prompt::WasmScope;
use crate::runtime_wasm::runtime_prompt::WasmPrompt;

type JsResult<T> = core::result::Result<T, JsError>;

// trait IntoJs<T> {
//     fn into_js(self) -> JsResult<T>;
// }

// impl<T, E: Into<anyhow::Error> + Send> IntoJs<T> for core::result::Result<T, E> {
//     fn into_js(self) -> JsResult<T> {
//         self.map_err(|e| JsError::new(format!("{:#}", anyhow::Error::from(e)).as_str()))
//     }
// }

//Run: wasm-pack test --firefox --headless  --features internal,wasm
// but for browser we likely need to do
//         wasm_bindgen_test::wasm_bindgen_test_configure!(run_in_browser);
// Node is run using: wasm-pack test --node --features internal,wasm
use std::panic;
#[wasm_bindgen(start)]
pub fn on_wasm_init() {
    // TODO: set LOG_LEVEL to ::Debug if you wish to see logs.
    // this is disabled by default because its slows down release mode builds.
    cfg_if::cfg_if! {
        if #[cfg(debug_assertions)] {
            const LOG_LEVEL: log::Level = log::Level::Info;
        } else {
            const LOG_LEVEL: log::Level = log::Level::Info;
        }
    };
    // I dont think we need this line anymore -- seems to break logging if you add it.
    //wasm_logger::init(wasm_logger::Config::new(LOG_LEVEL));
    match console_log::init_with_level(LOG_LEVEL) {
        Ok(_) => web_sys::console::log_1(
            &format!("Initialized BAML runtime logging as log::{LOG_LEVEL}").into(),
        ),
        Err(e) => web_sys::console::log_1(
            &format!("Failed to initialize BAML runtime logging: {e:?}").into(),
        ),
    }

    // Set up panic hook that calls both our custom handler AND console_error_panic_hook
    panic::set_hook(Box::new(|info| {
        // First, call our custom handler to notify JS
        let msg = info.to_string();
        on_wasm_panic(&msg);

        // Then call console_error_panic_hook for nice console formatting
        console_error_panic_hook::hook(info);
    }));
}

#[wasm_bindgen]
extern "C" {
    #[wasm_bindgen(js_name = __onWasmPanic)]
    fn on_wasm_panic(msg: &str);
}

/// Helper to serialize a header to JSON.
/// Used for both "header" (enter) and "header_stopped" (exit) events.
fn serialize_header_to_json(header: &HeaderContext, event_type: &str) -> String {
    let serialized_span = SerializedSpan::serialize(&header.span);
    serde_json::json!({
        "type": event_type,
        "label": header.title,
        "level": header.level,
        "span": {
            "file_path": serialized_span.file_path,
            "start_line": serialized_span.start_line,
            "start_column": serialized_span.start,
            "end_line": serialized_span.end_line,
            "end_column": serialized_span.end,
        }
    })
    .to_string()
}

/// HACK: Tracks active headers and emits synthetic "stopped" events when new headers arrive.
/// This is a workaround until proper exit events are emitted from the interpreter.
///
/// When a new header comes in at level N, we emit "stopped" events for all headers
/// at level >= N (in reverse order, deepest first).
struct HeaderTracker {
    /// Stack of active headers, sorted by level (lower levels first)
    active_headers: Vec<HeaderContext>,
}

impl HeaderTracker {
    fn new() -> Self {
        Self {
            active_headers: Vec::new(),
        }
    }

    /// Process a new header entering scope.
    /// Returns headers that should be marked as stopped (in the order they should be emitted).
    fn on_header_enter(&mut self, header: HeaderContext) -> Vec<HeaderContext> {
        let new_level = header.level;

        // Find all headers at level >= new_level and remove them
        let mut stopped_headers = Vec::new();
        while let Some(last) = self.active_headers.last() {
            if last.level >= new_level {
                stopped_headers.push(self.active_headers.pop().unwrap());
            } else {
                break;
            }
        }

        // Push the new header onto the stack
        self.active_headers.push(header);

        // Return stopped headers (already in reverse order - deepest first)
        stopped_headers
    }

    /// Flush all remaining active headers (e.g., at end of execution).
    /// Returns headers that should be marked as stopped (deepest first).
    fn flush(&mut self) -> Vec<HeaderContext> {
        let mut stopped = Vec::new();
        while let Some(header) = self.active_headers.pop() {
            stopped.push(header);
        }
        stopped
    }

    /// Get count of active headers (for debugging)
    fn active_count(&self) -> usize {
        self.active_headers.len()
    }

    /// Get the current (deepest) active header, if any.
    /// This is the header that variable notifications should be associated with.
    fn current_header(&self) -> Option<&HeaderContext> {
        self.active_headers.last()
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct WasmProject {
    #[wasm_bindgen(readonly)]
    pub root_dir_name: String,
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
#[derive(Debug)]
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
#[derive(Debug)]
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
        let files: HashMap<String, String> = serde_wasm_bindgen::from_value(files)?;

        Ok(WasmProject {
            root_dir_name: root_dir_name.to_string(),
            files,
            unsaved_files: HashMap::new(),
        })
    }

    #[wasm_bindgen]
    pub fn files(&self) -> Vec<String> {
        let mut saved_files = self.files.clone();
        self.unsaved_files.iter().for_each(|(k, v)| {
            saved_files.insert(k.clone(), v.clone());
        });
        let formatted_files = saved_files
            .iter()
            .map(|(k, v)| format!("{k}BAML_PATH_SPLTTER{v}"))
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
            errors: rt.runtime.diagnostics().clone(),
            all_files: hm.keys().map(|s| s.to_string()).collect(),
        }
    }

    #[wasm_bindgen]
    pub fn runtime(
        &self,
        env_vars: JsValue,
        feature_flags: JsValue,
    ) -> Result<WasmRuntime, JsValue> {
        let mut hm = self.files.iter().collect::<HashMap<_, _>>();
        hm.extend(self.unsaved_files.iter());

        let env_vars: HashMap<String, String> =
            serde_wasm_bindgen::from_value(env_vars).map_err(|e| {
                JsValue::from_str(&format!(
                    "Expected env_vars to be HashMap<string, string>. {e}"
                ))
            })?;

        let feature_flags = if feature_flags.is_undefined() || feature_flags.is_null() {
            FeatureFlags::new()
        } else {
            let flags: Vec<String> =
                serde_wasm_bindgen::from_value(feature_flags).map_err(|e| {
                    JsValue::from_str(&format!("Expected feature_flags to be Array<string>. {e}"))
                })?;
            FeatureFlags::from_vec(flags)
                .map_err(|e| JsValue::from_str(&format!("Invalid feature flags: {e:?}")))?
        };

        CoreBamlRuntime::from_file_content_with_features(
            &self.root_dir_name,
            &hm,
            env_vars,
            feature_flags,
        )
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
                log::debug!("Error: {e:#?}");
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
        let empty_flags = JsValue::undefined();
        let runtime = self.runtime(js_value, empty_flags);
        let res = match runtime {
            Ok(runtime) => runtime.run_generators(&self.files, no_version_check),
            Err(e) => Err(wasm_bindgen::JsError::new(
                format!("Failed to create runtime: {e:#?}").as_str(),
            )),
        };

        res
    }
}

#[wasm_bindgen(inspectable, getter_with_clone)]
#[derive(Clone)]
pub struct WasmRuntime {
    runtime: CoreBamlRuntime,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmFunction {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub function_type: WasmFunctionKind,
    #[wasm_bindgen(readonly)]
    pub test_cases: Vec<WasmTestCase>,
    #[wasm_bindgen(readonly)]
    pub test_snippet: String,
    #[wasm_bindgen(readonly)]
    pub signature: String,
}

#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmFunctionKind {
    Llm,
    Expr,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
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
    pub start_column: usize,
    #[wasm_bindgen(readonly)]
    pub end_line: usize,
    #[wasm_bindgen(readonly)]
    pub end_column: usize,
}

impl WasmSpan {
    fn contains(&self, file_path: &str, cursor_idx: usize) -> bool {
        // NB(sam): we should probably do an == comparison, but ends_with is the
        // existing behavior and handles file:// ambiguity
        self.file_path.as_str().ends_with(file_path)
            && ((self.start)..=(self.end)).contains(&cursor_idx)
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmEntityAtPosition {
    /// The type of entity: "function", "node", or "test"
    #[wasm_bindgen(readonly)]
    pub entity_type: String,
    /// The name of the entity (function name, node label, or test name)
    #[wasm_bindgen(readonly)]
    pub entity_name: String,
    /// The name of the function this entity belongs to.
    /// For function entities, this equals entity_name.
    /// For node entities, this is the parent function name.
    /// For test entities, this is the parent function name.
    #[wasm_bindgen(readonly)]
    pub function_name: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub function_type: Option<WasmFunctionKind>,
    #[wasm_bindgen(readonly)]
    pub node_id: Option<String>,
    #[wasm_bindgen(readonly)]
    pub node_label: Option<String>,
    /// For test entities, the name of the test case
    #[wasm_bindgen(readonly)]
    pub test_name: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum WasmControlFlowNodeType {
    FunctionRoot,
    HeaderContextEnter,
    BranchGroup,
    BranchArm,
    Loop,
    OtherScope,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmControlFlowNode {
    #[wasm_bindgen(readonly)]
    pub id: u32,
    #[wasm_bindgen(readonly)]
    pub parent_id: Option<u32>,
    #[wasm_bindgen(readonly)]
    pub lexical_id: String,
    #[wasm_bindgen(readonly)]
    pub label: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
    #[wasm_bindgen(readonly)]
    pub node_type: WasmControlFlowNodeType,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmControlFlowEdge {
    #[wasm_bindgen(readonly)]
    pub src: u32,
    #[wasm_bindgen(readonly)]
    pub dst: u32,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug, Default)]
pub struct WasmControlFlowGraph {
    #[wasm_bindgen(readonly)]
    pub nodes: Vec<WasmControlFlowNode>,
    #[wasm_bindgen(readonly)]
    pub edges: Vec<WasmControlFlowEdge>,
}

impl From<&RuntimeNodeType> for WasmControlFlowNodeType {
    fn from(value: &RuntimeNodeType) -> Self {
        match value {
            RuntimeNodeType::FunctionRoot => WasmControlFlowNodeType::FunctionRoot,
            RuntimeNodeType::HeaderContextEnter => WasmControlFlowNodeType::HeaderContextEnter,
            RuntimeNodeType::BranchGroup => WasmControlFlowNodeType::BranchGroup,
            RuntimeNodeType::BranchArm => WasmControlFlowNodeType::BranchArm,
            RuntimeNodeType::Loop => WasmControlFlowNodeType::Loop,
            RuntimeNodeType::OtherScope => WasmControlFlowNodeType::OtherScope,
        }
    }
}

impl From<ControlFlowVisualization> for WasmControlFlowGraph {
    fn from(viz: ControlFlowVisualization) -> Self {
        let nodes = viz
            .nodes
            .values()
            .map(|node| WasmControlFlowNode {
                id: node.id.raw(),
                parent_id: node.parent_node_id.map(|id| id.raw()),
                lexical_id: node.lexical_id.clone(),
                label: node.label.clone(),
                span: (&node.span).into(),
                node_type: WasmControlFlowNodeType::from(&node.node_type),
            })
            .collect();

        let edges = viz
            .edges_by_src
            .values()
            .flat_map(|edges| edges.iter())
            .map(|edge| WasmControlFlowEdge {
                src: edge.src.raw(),
                dst: edge.dst.raw(),
            })
            .collect();

        WasmControlFlowGraph { nodes, edges }
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmGeneratorConfig {
    #[wasm_bindgen(readonly)]
    pub output_type: String,
    #[wasm_bindgen(readonly)]
    pub version: String,
    #[wasm_bindgen(readonly)]
    pub span: WasmSpan,
}

impl From<&baml_runtime::internal_baml_diagnostics::Span> for WasmSpan {
    fn from(span: &baml_runtime::internal_baml_diagnostics::Span) -> Self {
        let (start, end) = span.line_and_column();
        WasmSpan {
            file_path: span.file.path().to_string(),
            start: span.start,
            end: span.end,
            start_line: start.0,
            start_column: start.1,
            end_line: end.0,
            end_column: end.1,
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
            start_column: 0,
            end_line: 0,
            end_column: 0,
        }
    }
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmParentFunction {
    #[wasm_bindgen(readonly)]
    pub start: usize,
    #[wasm_bindgen(readonly)]
    pub end: usize,
    #[wasm_bindgen(readonly)]
    pub name: String,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
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
    pub function_type: WasmFunctionKind,
    #[wasm_bindgen(readonly)]
    pub parent_functions: Vec<WasmParentFunction>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Clone, Debug)]
pub struct WasmParam {
    #[wasm_bindgen(readonly)]
    pub name: String,
    #[wasm_bindgen(readonly)]
    pub value: Option<String>,
    #[wasm_bindgen(readonly)]
    pub error: Option<String>,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Debug, Clone)]
pub struct WasmFunctionTestPair {
    #[wasm_bindgen(readonly)]
    pub function_name: String,
    #[wasm_bindgen(readonly)]
    pub test_name: String,
}

#[wasm_bindgen]
pub struct WasmFunctionResponse {
    function_response: baml_runtime::FunctionResult,
    func_test_pair: WasmFunctionTestPair,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Debug)]
pub struct WasmTestResponses {
    responses: Vec<WasmTestResponse>,
}

#[wasm_bindgen]
impl WasmTestResponses {
    // #[wasm_bindgen(typescript_type = "WasmTestResponse | null")]
    #[wasm_bindgen]
    pub fn yield_next(&mut self) -> Option<WasmTestResponse> {
        self.responses.pop()
    }
}

#[wasm_bindgen]
#[derive(Debug)]
#[allow(dead_code)]
pub struct WasmTestResponse {
    test_response: anyhow::Result<baml_runtime::TestResponse>,
    span: Option<String>,
    tracing_project_id: Option<String>,
    func_test_pair: WasmFunctionTestPair,
}

#[wasm_bindgen(getter_with_clone, inspectable)]
pub struct WasmParsedTestResponse {
    #[wasm_bindgen(readonly)]
    pub value: String,
    #[wasm_bindgen(readonly)]
    pub check_count: usize,
    #[wasm_bindgen(readonly)]
    /// JSON-string of the explanation, if there were any ParsingErrors
    pub explanation: Option<String>,
}

#[wasm_bindgen]
#[derive(Clone, Debug)]
pub enum TestStatus {
    Passed,
    LLMFailure,
    ParseFailure,
    FinishReasonFailed,
    ConstraintsFailed,
    AssertFailed,
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
        // TODO: This is a hack. We shouldn't hardcode AllowedRoleMetadata::All
        // here, but instead plumb it through the LLMErrors
        (&self.prompt, &self.scope, &AllowedRoleMetadata::All).into()
    }
}

#[wasm_bindgen]
impl WasmLLMResponse {
    #[wasm_bindgen]
    pub fn client_name(&self) -> String {
        self.scope.name()
    }

    pub fn prompt(&self) -> WasmPrompt {
        // TODO: This is a hack. We shouldn't hardcode AllowedRoleMetadata::All
        // here, but instead plumb it through the LLMErrors
        (&self.prompt, &self.scope, &AllowedRoleMetadata::All).into()
    }
}

#[wasm_bindgen]
impl WasmFunctionResponse {
    pub fn parsed_response(&self) -> Option<String> {
        self.function_response
            .result_with_constraints_content()
            .map(|p| serde_json::to_string(&p.serialize_partial()))
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
            .to_wasm()
    }

    #[wasm_bindgen]
    pub fn func_test_pair(&self) -> WasmFunctionTestPair {
        self.func_test_pair.clone()
    }
}

fn serialize_value_counting_checks(value: &ResponseBamlValue) -> (serde_json::Value, usize) {
    let checks = value
        .0
        .meta()
        .1
        .iter()
        .map(|ResponseCheck { name, status, .. }| (name.clone(), status.clone()))
        .collect::<IndexMap<String, String>>();

    let sub_check_count: usize = value.0.iter().map(|node| node.meta().1.len()).sum();
    let json_value: serde_json::Value = serde_json::to_value(value.serialize_final())
        .unwrap_or("Error converting value to JSON".into());

    let check_count = checks.len() + sub_check_count;

    (json_value, check_count)
}

#[wasm_bindgen]
impl WasmTestResponse {
    #[wasm_bindgen]
    pub fn status(&self) -> TestStatus {
        match &self.test_response {
            Ok(t) => match t.status() {
                baml_runtime::TestStatus::Pass => TestStatus::Passed,
                baml_runtime::TestStatus::NeedsHumanEval(_) => TestStatus::ConstraintsFailed,
                baml_runtime::TestStatus::Fail(r) => match r {
                    baml_runtime::TestFailReason::TestUnspecified(_) => TestStatus::UnableToRun,
                    baml_runtime::TestFailReason::TestLLMFailure(_) => TestStatus::LLMFailure,
                    baml_runtime::TestFailReason::TestParseFailure(_) => TestStatus::ParseFailure,
                    baml_runtime::TestFailReason::TestFinishReasonFailed(_) => {
                        TestStatus::FinishReasonFailed
                    }
                    baml_runtime::TestFailReason::TestConstraintsFailure {
                        failed_assert, ..
                    } => {
                        if failed_assert.is_some() {
                            TestStatus::AssertFailed
                        } else {
                            TestStatus::ConstraintsFailed
                        }
                    }
                },
            },
            Err(_) => TestStatus::UnableToRun,
        }
    }

    fn parsed_response_impl(&self) -> anyhow::Result<WasmParsedTestResponse> {
        let test_response = self
            .test_response
            .as_ref()
            .ok()
            .context("No test response")?;

        log::debug!(
            "[BAML parsed_response_impl] has function_response: {}, has expr_function_response: {}",
            test_response.function_response.is_some(),
            test_response.expr_function_response.is_some()
        );

        // Check for LLM function response first
        let maybe_parsed_response = test_response
            .function_response
            .as_ref()
            .and_then(|fr| fr.parsed().as_ref());

        // If no LLM function response, check for expr function response
        let parsed_response = match maybe_parsed_response {
            Some(Ok(value)) => {
                log::debug!("[BAML parsed_response_impl] Using LLM function response");
                Ok(value)
            }
            _ => {
                // Try expr function response
                if let Some(expr_response) = &test_response.expr_function_response {
                    log::debug!(
                        "[BAML parsed_response_impl] Found expr_function_response: {:?}",
                        expr_response.as_ref().map(|v| format!("{v:?}"))
                    );
                    match expr_response {
                        Ok(value) => {
                            log::debug!(
                                "[BAML parsed_response_impl] Using expr function response value"
                            );
                            Ok(value)
                        }
                        Err(e) => {
                            log::debug!("[BAML parsed_response_impl] Expr function error: {e}");
                            Err(anyhow::anyhow!("Expr function error: {}", e))
                        }
                    }
                } else {
                    log::debug!(
                        "[BAML parsed_response_impl] No parsed value found in either response type"
                    );
                    Err(anyhow::anyhow!("No parsed value"))
                }
            }
        }
        .context("No parsed value")?;
        let (flattened_checks, check_count) = serialize_value_counting_checks(parsed_response);
        Ok(WasmParsedTestResponse {
            value: serde_json::to_string(&flattened_checks)?,
            check_count,
            explanation: {
                let j = parsed_response.explanation_json();
                if j.is_empty() {
                    None
                } else {
                    Some(serde_json::to_string(&j)?)
                }
            },
        })
    }

    #[wasm_bindgen]
    pub fn parsed_response(&self) -> Option<WasmParsedTestResponse> {
        self.parsed_response_impl().ok()
    }

    #[wasm_bindgen]
    pub fn llm_failure(&self) -> Option<WasmLLMFailure> {
        self.test_response.as_ref().ok().and_then(|r| {
            r.function_response
                .as_ref()
                .and_then(|fr| llm_response_to_wasm_error(fr.llm_response(), fr.scope()))
        })
    }

    #[wasm_bindgen]
    pub fn llm_response(&self) -> Option<WasmLLMResponse> {
        self.test_response.as_ref().ok().and_then(|r| {
            r.function_response
                .as_ref()
                .and_then(|fr| (fr.llm_response(), fr.scope()).to_wasm())
        })
    }

    #[wasm_bindgen]
    pub fn failure_message(&self) -> Option<String> {
        match self.test_response.as_ref() {
            Ok(r) => match r.status() {
                baml_runtime::TestStatus::Pass => None,
                baml_runtime::TestStatus::Fail(r) => r.render_error(),
                baml_runtime::TestStatus::NeedsHumanEval(checks) => Some(format!(
                    "Checks require human validation: {}",
                    join(checks, ", ")
                )),
            },
            Err(e) => Some(format!("{e:#}")),
        }
    }

    fn _trace_url(&self) -> anyhow::Result<String> {
        let test_response = match self.test_response.as_ref() {
            Ok(t) => t,
            Err(e) => anyhow::bail!("Failed to get test response: {:?}", e),
        };
        let start_time = match test_response.function_response.as_ref() {
            Some(fr) => match fr.llm_response() {
                LLMResponse::Success(s) => s.start_time,
                LLMResponse::LLMFailure(f) => f.start_time,
                _ => anyhow::bail!("Test has no start time"),
            },
            None => anyhow::bail!("Test has no LLM function response"),
        };
        let _start_time = time::OffsetDateTime::from_unix_timestamp(
            start_time
                .duration_since(web_time::UNIX_EPOCH)?
                .as_secs()
                .try_into()?,
        )?
        .format(&time::format_description::well_known::Rfc3339)?;

        // TODO: update this.
        // let event_span_id = self
        //     .span
        //     .as_ref()
        //     .ok_or(anyhow::anyhow!("Test has no span ID"))?
        //     .to_string();
        // let subevent_span_id = test_response
        //     .function_call
        //     .as_ref()
        //     .ok_or(anyhow::anyhow!("Function call has no span ID"))?
        //     .to_string();

        // Ok(format!(
        //     "https://app.boundaryml.com/dashboard/projects/{}/drilldown?start_time={start_time}&eid={event_span_id}&s_eid={subevent_span_id}&test=false&onlyRootEvents=true",
        //     self.tracing_project_id.as_ref().ok_or(anyhow::anyhow!("No project ID specified"))?
        // ))
        Ok("https://app.boundaryml.com/dashboard/projects/".to_string())
    }

    #[wasm_bindgen]
    pub fn trace_url(&self) -> Option<String> {
        self._trace_url().ok()
    }

    #[wasm_bindgen]
    pub fn func_test_pair(&self) -> WasmFunctionTestPair {
        self.func_test_pair.clone()
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

trait ToWasm {
    type Output;
    fn to_wasm(&self) -> Self::Output;
}

impl ToWasm
    for (
        &baml_runtime::internal::llm_client::LLMResponse,
        &OrchestrationScope,
    )
{
    type Output = Option<WasmLLMResponse>;

    fn to_wasm(&self) -> Self::Output {
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
        match &self {
            baml_runtime::TestFailReason::TestUnspecified(e) => Some(format!("{e:#}")),
            baml_runtime::TestFailReason::TestLLMFailure(f) => f.render_error(),
            baml_runtime::TestFailReason::TestParseFailure(e)
            | baml_runtime::TestFailReason::TestFinishReasonFailed(e) => {
                match e.downcast_ref::<baml_runtime::errors::ExposedError>() {
                    Some(exposed_error) => match exposed_error {
                        baml_runtime::errors::ExposedError::ValidationError { message, .. } => {
                            Some(message.clone())
                        }
                        baml_runtime::errors::ExposedError::FinishReasonError {
                            message, ..
                        } => Some(message.clone()),
                        baml_runtime::errors::ExposedError::ClientHttpError { message, .. } => {
                            Some(message.clone())
                        }
                        baml_runtime::errors::ExposedError::AbortError { .. } => {
                            Some("AbortError".to_string())
                        }
                        baml_runtime::errors::ExposedError::TimeoutError { .. } => {
                            Some("TimeoutError".to_string())
                        }
                    },
                    None => Some(format!("{e:#}")),
                }
            }
            baml_runtime::TestFailReason::TestConstraintsFailure {
                checks,
                failed_assert,
            } => {
                let checks_msg = if !checks.is_empty() {
                    let check_msgs = checks.iter().map(|(name, pass)| {
                        format!("{name}: {}", if *pass { "Passed" } else { "Failed" })
                    });
                    format!("Check results:\n{}", join(check_msgs, "\n"))
                } else {
                    String::new()
                };
                let assert_msg = failed_assert
                    .as_ref()
                    .map_or("".to_string(), |name| format!("\nFailed assert: {name}"));
                Some(format!("{checks_msg}{assert_msg}"))
            }
        }
    }
}

impl WithRenderError for baml_runtime::internal::llm_client::LLMResponse {
    fn render_error(&self) -> Option<String> {
        match self {
            baml_runtime::internal::llm_client::LLMResponse::Success(_) => None,
            baml_runtime::internal::llm_client::LLMResponse::LLMFailure(f) => {
                format!("{} {}", f.message, f.code).into()
            }
            baml_runtime::internal::llm_client::LLMResponse::UserFailure(e) => {
                format!("user error: {e}").into()
            }
            baml_runtime::internal::llm_client::LLMResponse::InternalFailure(e) => {
                e.to_string().into()
            }
            baml_runtime::internal::llm_client::LLMResponse::Cancelled(msg) => {
                format!("cancelled: {msg}").into()
            }
        }
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
            .run_codegen(
                &input_files
                    .iter()
                    .map(|(k, v)| (PathBuf::from(k), v.clone()))
                    .collect(),
                no_version_check,
                GeneratorType::VSCodeCLI,
                false, // strip_tests - not applicable for WASM runtime
            )
            .map_err(|e| JsError::new(format!("{e:#}").as_str()))?
            .into_iter()
            .map(|g| g.into())
            .collect())
    }

    fn list_functions_internal(&self, filter: Option<WasmFunctionKind>) -> Vec<WasmFunction> {
        let ctx = &self
            .runtime
            .create_ctx_manager(BamlValue::String("wasm".to_string()), None);
        let ctx = ctx.create_ctx_with_default();
        let ctx = ctx.eval_ctx(false);

        let include_llm = matches!(filter, None | Some(WasmFunctionKind::Llm));
        let include_expr = matches!(filter, None | Some(WasmFunctionKind::Expr));

        let mut functions = Vec::new();

        if include_llm {
            functions.extend(
                self.runtime.ir().walk_functions().map(|f| {
                    Self::build_wasm_function(&ctx, &self.runtime, f, WasmFunctionKind::Llm)
                }),
            );
        }

        if include_expr {
            functions.extend(self.runtime.ir().walk_expr_fns().map(|f| {
                Self::build_expr_wasm_function(&ctx, &self.runtime, f, WasmFunctionKind::Expr)
            }));
        }

        functions
    }
}

#[wasm_bindgen]
impl WasmRuntime {
    #[wasm_bindgen]
    pub fn check_if_in_prompt(&self, cursor_idx: usize) -> bool {
        self.runtime.ir().walk_functions().any(|f| {
            f.elem().configs().expect("configs").iter().any(|config| {
                let span = &config.prompt_span;
                cursor_idx >= span.start && cursor_idx <= span.end
            })
        })
    }

    #[wasm_bindgen]
    pub fn list_functions(&self) -> Vec<WasmFunction> {
        self.list_functions_internal(None)
    }

    fn build_wasm_function(
        ctx: &baml_types::EvaluationContext<'_>,
        runtime: &CoreBamlRuntime,
        f: internal_baml_core::ir::FunctionWalker<'_>,
        function_type: WasmFunctionKind,
    ) -> WasmFunction {
        let snippet = format!(
            r#"test TestName {{
  functions [{name}]
  args {{
{args}
  }}
}}
"#,
            name = f.name(),
            args = {
                let params = f
                    .inputs()
                    .iter()
                    .map(|(k, runtime_type)| (k.clone(), runtime_type.clone()))
                    .collect::<indexmap::IndexMap<String, _>>();

                runtime.ir().get_dummy_args(2, true, &params)
            }
        );

        let wasm_span = match f.span() {
            Some(span) => span.into(),
            None => {
                log::warn!("[WasmRuntime] Missing span for function {}", f.name());
                WasmSpan::default()
            }
        };

        WasmFunction {
            name: f.name().to_string().clone(),
            span: wasm_span,
            function_type,
            signature: {
                let params = f
                    .inputs()
                    .iter()
                    .map(|(k, runtime_type)| (k.clone(), runtime_type.clone()))
                    .collect::<indexmap::IndexMap<String, _>>();

                let inputs = runtime
                    .ir()
                    .get_dummy_args(2, false, &params)
                    .split("\n")
                    .map(|line| line.trim().to_string())
                    .collect::<Vec<_>>()
                    .join(", ");

                format!("({}) -> {}", inputs, f.output())
            },
            test_snippet: snippet,
            test_cases: f
                .walk_tests()
                .map(|tc| Self::build_wasm_test_case(ctx, tc, function_type))
                .collect(),
        }
    }

    fn build_expr_wasm_function(
        ctx: &baml_types::EvaluationContext<'_>,
        runtime: &CoreBamlRuntime,
        f: internal_baml_core::ir::ExprFunctionWalker<'_>,
        function_type: WasmFunctionKind,
    ) -> WasmFunction {
        let snippet = format!(
            r#"test TestName {{
  functions [{name}]
  args {{
{args}
  }}
}}"#,
            name = f.name(),
            args = {
                let params = f
                    .inputs()
                    .iter()
                    .map(|(k, runtime_type)| (k.clone(), runtime_type.clone()))
                    .collect::<indexmap::IndexMap<String, _>>();

                runtime.ir().get_dummy_args(2, true, &params)
            }
        );

        let wasm_span = match f.span() {
            Some(span) => span.into(),
            None => WasmSpan::default(),
        };

        let params = f
            .inputs()
            .iter()
            .map(|(k, runtime_type)| (k.clone(), runtime_type.clone()))
            .collect::<indexmap::IndexMap<String, _>>();
        let signature_inputs = runtime
            .ir()
            .get_dummy_args(2, false, &params)
            .split("\n")
            .map(|line| line.trim().to_string())
            .collect::<Vec<_>>()
            .join(", ");

        let test_cases = f
            .walk_tests()
            .map(|tc| {
                let params = match tc.test_case_params(ctx) {
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

                f.inputs().iter().for_each(|(param_name, t)| {
                    if !params.iter().any(|p| p.name == *param_name) && !t.is_optional() {
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
                    function_type,
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
            .collect();

        WasmFunction {
            name: f.name().to_string(),
            span: wasm_span,
            function_type,
            signature: format!("({}) -> {}", signature_inputs, f.output()),
            test_snippet: snippet,
            test_cases,
        }
    }

    fn build_wasm_test_case(
        ctx: &baml_types::EvaluationContext<'_>,
        tc: internal_baml_core::ir::TestCaseWalker<'_>,
        function_type: WasmFunctionKind,
    ) -> WasmTestCase {
        let params = match tc.test_case_params(ctx) {
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

        tc.function().inputs().iter().for_each(|(param_name, t)| {
            if !params.iter().any(|p| p.name == *param_name) && !t.is_optional() {
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
            function_type,
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
    }

    #[wasm_bindgen]
    pub fn list_generators(&self) -> Vec<WasmGeneratorConfig> {
        self.runtime
            .codegen_generators()
            .map(|generator| WasmGeneratorConfig {
                output_type: generator.output_type.clone().to_string(),
                version: generator.version.clone(),
                span: WasmSpan {
                    file_path: generator.span.file.path().to_string(),
                    start: generator.span.start,
                    end: generator.span.end,
                    start_line: generator.span.line_and_column().0 .0,
                    start_column: generator.span.line_and_column().0 .1,
                    end_line: generator.span.line_and_column().1 .0,
                    end_column: generator.span.line_and_column().1 .1,
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
            other => return Some(format!("Invalid generator type: {other:?}")),
        };

        let version_check_mode = match version_check_mode {
            "Strict" => VersionCheckMode::Strict,
            "None" => VersionCheckMode::None,
            other => return Some(format!("Invalid version check mode: {other:?}")),
        };

        let Ok(generator_language) = GeneratorOutputType::from_str(generator_language) else {
            return Some(format!(
                "Invalid generator language: {generator_language:?}"
            ));
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
            .ir()
            .required_env_vars()
            .into_iter()
            .map(|s| s.to_string())
            .collect()
    }

    #[wasm_bindgen]
    pub fn search_for_symbol(&self, symbol: &str) -> Option<SymbolLocation> {
        let runtime = self.runtime.ir();

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
        if let Ok(walker) = runtime.find_type_alias(symbol) {
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
    pub fn is_valid_class(&self, symbol: &str) -> bool {
        self.runtime.ir().find_class(symbol).is_ok()
    }

    #[wasm_bindgen]
    pub fn is_valid_enum(&self, symbol: &str) -> bool {
        self.runtime.ir().find_enum(symbol).is_ok()
    }

    #[wasm_bindgen]
    pub fn is_valid_type_alias(&self, symbol: &str) -> bool {
        self.runtime.ir().find_type_alias(symbol).is_ok()
    }

    #[wasm_bindgen]
    pub fn is_valid_function(&self, symbol: &str) -> bool {
        let ir = self.runtime.ir();
        ir.find_function(symbol).is_ok() || ir.find_expr_fn(symbol).is_ok()
    }

    #[wasm_bindgen]
    pub fn search_for_class_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.runtime
            .ir()
            .find_class_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    #[wasm_bindgen]
    pub fn search_for_enum_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.runtime
            .ir()
            .find_enum_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    #[wasm_bindgen]
    pub fn search_for_type_alias_locations(&self, symbol: &str) -> Vec<SymbolLocation> {
        self.runtime
            .ir()
            .find_type_alias_locations(symbol)
            .into_iter()
            .map(|span| {
                let ((start_line, start_character), (end_line, end_character)) =
                    span.line_and_column();
                SymbolLocation {
                    uri: span.file.path().to_string(),
                    start_line,
                    start_character,
                    end_line,
                    end_character,
                }
            })
            .collect()
    }

    // Use get_entity_at_position instead. This is internal.
    pub fn get_function_at_position(
        &self,
        file_name: &str,
        selected_func: &str,
        cursor_idx: usize,
    ) -> Option<WasmFunction> {
        log::info!(
            "get_function_at_position: file_name={}, selected_func={}, cursor_idx={}",
            file_name,
            selected_func,
            cursor_idx
        );
        let functions = self.list_functions_internal(None);

        for function in functions.clone() {
            let span = function.span.clone(); // Clone the span

            if span.contains(file_name, cursor_idx) {
                return Some(function);
            }
        }

        let testcases = self.list_testcases();

        for tc in testcases {
            let span = tc.span;
            if span.contains(file_name, cursor_idx) {
                if let Some(_parent_function) =
                    tc.parent_functions.iter().find(|f| f.name == selected_func)
                {
                    return functions
                        .clone()
                        .into_iter()
                        .find(|f| f.name == selected_func);
                } else if let Some(first_function) = tc.parent_functions.first() {
                    return functions
                        .clone()
                        .into_iter()
                        .find(|f| f.name == first_function.name);
                }
            }
        }

        let testcases = self.list_testcases();

        for tc in testcases {
            let span = tc.span;
            if span.contains(file_name, cursor_idx) {
                if let Some(_parent_function) =
                    tc.parent_functions.iter().find(|f| f.name == selected_func)
                {
                    return functions.into_iter().find(|f| f.name == selected_func);
                } else if let Some(first_function) = tc.parent_functions.first() {
                    return functions
                        .into_iter()
                        .find(|f| f.name == first_function.name);
                }
            }
        }

        None
    }

    #[wasm_bindgen]
    pub fn get_entity_at_position(
        &self,
        file_name: &str,
        cursor_idx: usize,
    ) -> Option<WasmEntityAtPosition> {
        // First check if cursor is in a test case
        let testcases = self.list_testcases();
        for tc in &testcases {
            if tc.span.contains(file_name, cursor_idx) {
                // Found a test case - get the parent function name
                let parent_function = tc.parent_functions.first()?;
                return Some(WasmEntityAtPosition {
                    entity_type: "test".to_string(),
                    entity_name: tc.name.clone(),
                    function_name: parent_function.name.clone(),
                    span: tc.span.clone(),
                    function_type: None,
                    node_id: None,
                    node_label: None,
                    test_name: Some(tc.name.clone()),
                });
            }
        }

        // Find the function at this position
        let function = self.get_function_at_position(file_name, "", cursor_idx)?;

        // If it's an Expr function, extend node spans to cover content until next node
        if function.function_type == WasmFunctionKind::Expr {
            if let Ok(graph) = function.function_graph_v2(self) {
                // Filter nodes that belong to this file and sort by start position
                let mut file_nodes: Vec<&WasmControlFlowNode> = graph
                    .nodes
                    .iter()
                    .filter(|node| node.span.file_path == file_name)
                    .collect();
                file_nodes.sort_by_key(|node| node.span.start);

                // Find the node whose extended span contains the cursor
                // Each node's span extends from its start to the start of the next node
                for (i, node) in file_nodes.iter().enumerate() {
                    let span_start = node.span.start;
                    let span_end = if i + 1 < file_nodes.len() {
                        // Extend to the start of the next node
                        file_nodes[i + 1].span.start
                    } else {
                        // Last node extends to the end of the function
                        function.span.end
                    };

                    if cursor_idx >= span_start && cursor_idx < span_end {
                        return Some(WasmEntityAtPosition {
                            entity_type: "node".to_string(),
                            entity_name: node.label.clone(),
                            function_name: function.name.clone(),
                            span: node.span.clone(),
                            function_type: Some(function.function_type),
                            node_id: Some(node.lexical_id.clone()),
                            node_label: Some(node.label.clone()),
                            test_name: None,
                        });
                    }
                }
            }
        }
        // If it's an LLM function, return the function span
        else if function.function_type == WasmFunctionKind::Llm {
            return Some(WasmEntityAtPosition {
                entity_type: "function".to_string(),
                entity_name: function.name.clone(),
                function_name: function.name.clone(),
                span: function.span.clone(),
                function_type: Some(function.function_type),
                node_id: None,
                node_label: None,
                test_name: None,
            });
        }
        // Return the function as the entity
        Some(WasmEntityAtPosition {
            entity_type: "function".to_string(),
            entity_name: function.name.clone(),
            function_name: function.name.clone(),
            span: function.span.clone(),
            function_type: Some(function.function_type),
            node_id: None,
            node_label: None,
            test_name: None,
        })
    }

    #[wasm_bindgen]
    pub fn list_testcases(&self) -> Vec<WasmTestCase> {
        let ctx = self
            .runtime
            .create_ctx_manager(BamlValue::String("wasm".to_string()), None);

        let ctx = ctx.create_ctx_with_default();
        let ctx = ctx.eval_ctx(true);

        let ir = self.runtime.ir();

        // Combine both LLM function test pairs and expr function test pairs
        let llm_tests = ir.walk_function_test_pairs().map(|tc| {
            let params = match tc.test_case_params(&ctx) {
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
                function_type: WasmFunctionKind::Llm,
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
        });

        let expr_tests = ir.walk_expr_fn_test_pairs().map(|tc| {
            let params = match tc.test_case_params(&ctx) {
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
                function_type: WasmFunctionKind::Expr,
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
        });

        llm_tests.chain(expr_tests).collect()
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

            if span.contains(&parent_function.span.file_path, cursor_idx) {
                return Some(testcase);
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
            if span.contains(file_name, cursor_idx) {
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
    pub async fn run_tests(
        // NOTE: This needs to be `&self` so that the runtime can be read
        // by the UI, e.g to re-enumerate functions. In case you *really* need `&mut` access,
        // consider `RwLock`, ideally only for the data you're going to be mutating and not the
        // entire runtime.
        &self,
        function_test_pairs: js_sys::Array,
        on_partial_response: js_sys::Function,
        get_baml_src_cb: js_sys::Function,
        env: js_sys::Object,
        abort_signal: Option<js_sys::Object>,
        watch_handler: js_sys::Function,
        parallel: Option<bool>,
    ) -> Result<WasmTestResponses, JsValue> {
        let parallel = parallel.unwrap_or(false);
        // Convert abort signal to tripwire
        let tripwire = match crate::abort_controller::js_abort_signal_to_tripwire(abort_signal) {
            Ok(tripwire) => tripwire,
            Err(_e) => {
                log::error!("WASM Parallel: Failed to setup abort handler");
                baml_runtime::TripWire::new(None)
            }
        };

        // Create a vector to store all test futures
        let mut test_futures = Vec::new();

        for i in 0..function_test_pairs.length() {
            if let Ok(pair) = js_sys::Reflect::get(&function_test_pairs, &i.into()) {
                if let (Ok(function_name), Ok(test_name)) = (
                    js_sys::Reflect::get(&pair, &JsValue::from_str("functionName")),
                    js_sys::Reflect::get(&pair, &JsValue::from_str("testName")),
                ) {
                    let function_name = function_name.as_string().unwrap_or_default();
                    let test_name = test_name.as_string().unwrap_or_default();

                    let fn_name_copy = function_name.clone();
                    let test_name_copy = test_name.clone();

                    // Create a closure to handle partial responses for this test
                    let on_partial_response_clone = on_partial_response.clone();
                    let cb = Box::new(move |r| {
                        let this = JsValue::NULL;
                        let res = WasmFunctionResponse {
                            function_response: r,
                            func_test_pair: WasmFunctionTestPair {
                                function_name: fn_name_copy.clone(),
                                test_name: test_name_copy.clone(),
                            },
                        }
                        .into();
                        on_partial_response_clone.call1(&this, &res).unwrap();
                    });

                    // Create evaluation context for the test
                    let ctx = self
                        .runtime
                        .create_ctx_manager_for_wasm(js_fn_to_baml_src_reader(
                            get_baml_src_cb.clone(),
                        ));

                    // Reference to the runtime
                    let rt = &self.runtime;
                    let entries = js_sys::Object::entries(&env);
                    let mut env_vars = HashMap::new();
                    for entry in entries.iter() {
                        let arr = entry.dyn_into::<js_sys::Array>().unwrap();
                        let key = arr.get(0).as_string().unwrap();
                        let value = arr.get(1).as_string().unwrap_or_default();
                        env_vars.insert(key, value);
                    }

                    // Clone tripwire for this test
                    let test_tripwire = tripwire.clone();
                    let on_tick = if false { Some(|| {}) } else { None };

                    // Create watch handler callback for this test
                    // HACK: Track active headers to emit synthetic "stopped" events
                    let header_tracker = Rc::new(RefCell::new(HeaderTracker::new()));
                    let header_tracker_clone = header_tracker.clone();
                    let header_tracker_for_flush = header_tracker.clone();
                    let watch_handler_clone = watch_handler.clone();
                    // Clone for use after execution to flush remaining headers
                    let watch_handler_for_flush = watch_handler.clone();
                    let function_name_for_flush = function_name.clone();

                    let watch_handler_cb = shared_handler(move |notification| {
                        // Helper to create and send a JS notification
                        let send_notification =
                            |function_name: &str, is_stream: bool, value_json: &str| {
                                let js_notification = js_sys::Object::new();

                                js_sys::Reflect::set(
                                    &js_notification,
                                    &JsValue::from_str("function_name"),
                                    &JsValue::from_str(function_name),
                                )
                                .unwrap();

                                js_sys::Reflect::set(
                                    &js_notification,
                                    &JsValue::from_str("is_stream"),
                                    &JsValue::from_bool(is_stream),
                                )
                                .unwrap();

                                js_sys::Reflect::set(
                                    &js_notification,
                                    &JsValue::from_str("value"),
                                    &JsValue::from_str(value_json),
                                )
                                .unwrap();

                                watch_handler_clone
                                    .call1(&JsValue::NULL, &js_notification)
                                    .unwrap();
                            };

                        // Check if this is a Header notification - if so, emit stopped events first
                        if let baml_compiler::watch::WatchBamlValue::Header(header) =
                            &notification.value
                        {
                            log::info!(
                                "[WASM run_tests] Header enter: level={} title={:?}",
                                header.level,
                                header.title
                            );
                            // Get headers that need to be stopped
                            let stopped_headers = header_tracker_clone
                                .borrow_mut()
                                .on_header_enter(header.clone());

                            log::info!(
                                "[WASM run_tests] After on_header_enter: {} headers to stop, {} active",
                                stopped_headers.len(),
                                header_tracker_clone.borrow().active_count()
                            );

                            // Emit stopped notifications for each (deepest first)
                            for stopped_header in stopped_headers {
                                let stopped_json =
                                    serialize_header_to_json(&stopped_header, "header_stopped");
                                send_notification(
                                    &notification.function_name,
                                    false,
                                    &stopped_json,
                                );
                            }
                        }

                        // Now handle the actual notification
                        // Convert notification to a JS object
                        let js_notification = js_sys::Object::new();

                        if let Some(ref var_name) = notification.variable_name {
                            js_sys::Reflect::set(
                                &js_notification,
                                &JsValue::from_str("variable_name"),
                                &JsValue::from_str(var_name),
                            )
                            .unwrap();
                        }

                        if let Some(ref channel_name) = notification.channel_name {
                            js_sys::Reflect::set(
                                &js_notification,
                                &JsValue::from_str("channel_name"),
                                &JsValue::from_str(channel_name),
                            )
                            .unwrap();
                        }

                        js_sys::Reflect::set(
                            &js_notification,
                            &JsValue::from_str("function_name"),
                            &JsValue::from_str(&notification.function_name),
                        )
                        .unwrap();

                        js_sys::Reflect::set(
                            &js_notification,
                            &JsValue::from_str("is_stream"),
                            &JsValue::from_bool(notification.is_stream),
                        )
                        .unwrap();

                        // HACK: For variable notifications, include the current header's title
                        // as lexical_node_id so the UI knows which block the variable belongs to
                        if matches!(
                            &notification.value,
                            baml_compiler::watch::WatchBamlValue::Value(_)
                                | baml_compiler::watch::WatchBamlValue::StreamStart(_)
                                | baml_compiler::watch::WatchBamlValue::StreamUpdate(_, _)
                                | baml_compiler::watch::WatchBamlValue::StreamEnd(_)
                        ) {
                            if let Some(current_header) =
                                header_tracker_clone.borrow().current_header()
                            {
                                js_sys::Reflect::set(
                                    &js_notification,
                                    &JsValue::from_str("lexical_node_id"),
                                    &JsValue::from_str(&current_header.title),
                                )
                                .unwrap();
                            }
                        }

                        // Serialize the value as JSON
                        let value_json = match &notification.value {
                            baml_compiler::watch::WatchBamlValue::Value(v) => {
                                let value: BamlValue = v.clone().into();
                                serde_json::to_string(&value)
                                    .unwrap_or_else(|_| format!("{value:?}"))
                            }
                            baml_compiler::watch::WatchBamlValue::Header(header) => {
                                serialize_header_to_json(header, "header")
                            }
                            baml_compiler::watch::WatchBamlValue::HeaderStopped(header) => {
                                serialize_header_to_json(header, "header_stopped")
                            }
                            baml_compiler::watch::WatchBamlValue::StreamStart(id) => {
                                serde_json::json!({ "type": "stream_start", "id": id }).to_string()
                            }
                            baml_compiler::watch::WatchBamlValue::StreamUpdate(id, v) => {
                                let value: BamlValue = v.clone().into();
                                let value_json = serde_json::to_string(&value)
                                    .unwrap_or_else(|_| format!("{value:?}"));
                                serde_json::json!({ "type": "stream_update", "id": id, "value": value_json }).to_string()
                            }
                            baml_compiler::watch::WatchBamlValue::StreamEnd(id) => {
                                serde_json::json!({ "type": "stream_end", "id": id }).to_string()
                            }
                        };

                        js_sys::Reflect::set(
                            &js_notification,
                            &JsValue::from_str("value"),
                            &JsValue::from_str(&value_json),
                        )
                        .unwrap();

                        watch_handler_clone
                            .call1(&JsValue::NULL, &js_notification)
                            .unwrap();
                    });

                    // Create a future for this test
                    let future = async move {
                        let (test_response, span) = rt
                            .run_test(
                                &function_name,
                                &test_name,
                                &ctx,
                                Some(cb),
                                None,
                                env_vars.clone(),
                                None,          // tags
                                test_tripwire, // Pass tripwire to each test
                                on_tick,
                                Some(watch_handler_cb),
                            )
                            .await;

                        // HACK: Flush remaining active headers after execution completes
                        let remaining_headers = header_tracker_for_flush.borrow_mut().flush();
                        log::info!(
                            "[WASM run_tests] Flushing {} remaining headers for function={}",
                            remaining_headers.len(),
                            function_name_for_flush
                        );
                        for stopped_header in remaining_headers {
                            let js_notification = js_sys::Object::new();

                            js_sys::Reflect::set(
                                &js_notification,
                                &JsValue::from_str("function_name"),
                                &JsValue::from_str(&function_name_for_flush),
                            )
                            .unwrap();

                            js_sys::Reflect::set(
                                &js_notification,
                                &JsValue::from_str("is_stream"),
                                &JsValue::from_bool(false),
                            )
                            .unwrap();

                            let stopped_json =
                                serialize_header_to_json(&stopped_header, "header_stopped");
                            js_sys::Reflect::set(
                                &js_notification,
                                &JsValue::from_str("value"),
                                &JsValue::from_str(&stopped_json),
                            )
                            .unwrap();

                            watch_handler_for_flush
                                .call1(&JsValue::NULL, &js_notification)
                                .unwrap();
                        }

                        // Return WasmTestResponse for this test
                        WasmTestResponse {
                            test_response,
                            span: Some(span.to_string()),
                            tracing_project_id: rt
                                .tracer_wrapper()
                                .get_or_create_tracer(&env_vars)
                                .tracing_project_id(),
                            // tracing_project_id: rt.env_vars().get("BOUNDARY_PROJECT_ID").cloned(),
                            func_test_pair: WasmFunctionTestPair {
                                function_name: function_name.clone(),
                                test_name: test_name.clone(),
                            },
                        }
                    };

                    test_futures.push(future);
                }
            }
        }

        // Run tests based on parallel flag
        let results = if parallel {
            // Run all tests in parallel
            futures::future::join_all(test_futures).await
        } else {
            // Run tests sequentially
            let mut results = Vec::with_capacity(test_futures.len());
            for future in test_futures {
                results.push(future.await);
            }
            results
        };

        Ok(WasmTestResponses { responses: results })
    }
}

// Define a new struct to store the important information
#[wasm_bindgen(getter_with_clone, inspectable)]
#[derive(Serialize, Deserialize, Debug)]
pub struct SerializableOrchestratorNode {
    pub provider: String,
}

impl From<&OrchestratorNode> for SerializableOrchestratorNode {
    fn from(node: &OrchestratorNode) -> Self {
        SerializableOrchestratorNode {
            provider: node.provider.to_string(),
        }
    }
}

#[cfg(target_arch = "wasm32")]
fn js_fn_to_baml_src_reader(get_baml_src_cb: js_sys::Function) -> BamlSrcReader {
    Some(Box::new(move |path| {
        Box::pin({
            let path = path.to_string();
            let get_baml_src_cb = get_baml_src_cb.clone();
            async move {
                // Windows-specific hotfix: VSCode resolves relative paths relative to workspace root
                // instead of BAML file location. For BAML files directly in baml_src/, prepend "baml_src/".
                // Since WASM can't use cfg!(windows), we detect Windows by checking for backslashes in paths
                // or by checking the user agent, but for simplicity, we'll check if the path contains backslashes.
                let is_windows = web_sys::window()
                    .and_then(|w| w.navigator().user_agent().ok())
                    .map(|ua| ua.contains("Windows"))
                    .unwrap_or(false);

                let adjusted_path =
                    if is_windows && (path.starts_with("../") || path.starts_with("./")) {
                        let result = format!("baml_src/{path}");
                        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                            "WASM Windows path fix applied: '{path}' → '{result}'"
                        )));
                        result
                    } else {
                        web_sys::console::log_1(&wasm_bindgen::JsValue::from_str(&format!(
                            "WASM path unchanged: '{}' (windows={}, relative={})",
                            path,
                            is_windows,
                            path.starts_with("../") || path.starts_with("./")
                        )));
                        path.clone()
                    };

                let null = JsValue::NULL;
                let Ok(read) = get_baml_src_cb.call1(&null, &JsValue::from(adjusted_path)) else {
                    anyhow::bail!("readFileRef did not return a promise");
                };

                let read = JsFuture::from(Promise::unchecked_from_js(read)).await;

                let read = match read {
                    Ok(read) => read,
                    Err(err) => {
                        if let Some(e) = err.dyn_ref::<js_sys::Error>() {
                            if let Some(e_str) = e.message().as_string() {
                                anyhow::bail!("readFileRef failure: {}", e_str);
                            }
                        }

                        anyhow::bail!("readFileRef rejected: {:?}", err);
                    }
                };

                // TODO: how does JsValue -> Uint8Array work without try_from?
                Ok(Uint8Array::from(read).to_vec())
            }
        })
    }))
}

#[cfg(not(target_arch = "wasm32"))]
fn js_fn_to_baml_src_reader(get_baml_src_cb: js_sys::Function) -> BamlSrcReader {
    None
}

#[wasm_bindgen]
pub struct WasmCallContext {
    /// Index of the orchestration graph node to use for the call
    /// Defaults to 0 when unset
    node_index: Option<usize>,
}

#[wasm_bindgen]
impl WasmCallContext {
    #[wasm_bindgen(constructor)]
    pub fn new() -> Self {
        Self { node_index: None }
    }

    #[wasm_bindgen(setter)]
    pub fn set_node_index(&mut self, node_index: Option<usize>) {
        self.node_index = node_index;
    }
}

#[wasm_bindgen]
impl WasmFunction {
    fn ensure_llm(&self) -> Result<(), JsError> {
        if self.function_type == WasmFunctionKind::Llm {
            Ok(())
        } else {
            Err(JsError::new(&format!(
                "function `{}` does not support LLM-only operation",
                self.name
            )))
        }
    }

    #[wasm_bindgen]
    pub async fn render_prompt_for_test(
        &self,
        rt: &WasmRuntime,
        test_name: String,
        wasm_call_context: &WasmCallContext,
        get_baml_src_cb: js_sys::Function,
        env: js_sys::Object,
    ) -> JsResult<WasmPrompt> {
        self.ensure_llm()?;
        log::info!(
            "[WasmFunction] render_prompt_for_test start function={} test={}",
            self.name,
            test_name
        );
        let context_manager = rt.runtime.create_ctx_manager(
            BamlValue::String("wasm".to_string()),
            js_fn_to_baml_src_reader(get_baml_src_cb),
        );

        let test_type_builder = rt
            .runtime
            .internal()
            .get_test_type_builder(&self.name, &test_name)
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        let entries = js_sys::Object::entries(&env);
        let mut env_vars = HashMap::new();
        for entry in entries.iter() {
            let arr = entry.dyn_into::<js_sys::Array>().unwrap();
            let key = arr.get(0).as_string().unwrap();
            let value = arr.get(1).as_string().unwrap_or_default();
            env_vars.insert(key, value);
        }

        let ctx = context_manager
            .create_ctx(
                test_type_builder.as_ref(),
                None,
                env_vars,
                vec![baml_ids::FunctionCallId::new()],
            )
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        let params = rt
            .runtime
            .get_test_params(&self.name, &test_name, &ctx, false)
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        match rt
            .runtime
            .internal()
            .render_prompt(&self.name, &ctx, &params, wasm_call_context.node_index)
            .await
        {
            Ok(rendered) => {
                log::info!(
                    "[WasmFunction] render_prompt_for_test success function={} test={}",
                    self.name,
                    test_name
                );
                let prompt = (&rendered.0, &rendered.1, &rendered.2).into();
                Ok(prompt)
            }
            Err(e) => {
                log::error!(
                    "[WasmFunction] render_prompt_for_test error function={} test={} err={:?}",
                    self.name,
                    test_name,
                    e
                );
                Err(JsError::new(format!("{e:?}").as_str()))
            }
        }
    }

    #[wasm_bindgen]
    pub fn client_name(&self, rt: &WasmRuntime) -> Result<String, JsValue> {
        if self.function_type != WasmFunctionKind::Llm {
            return Ok(String::new());
        }
        let rt = &rt.runtime;
        let ctx_manager = rt.create_ctx_manager(BamlValue::String("wasm".to_string()), None);
        let ctx = ctx_manager.create_ctx_with_default();
        let ir = rt.ir();

        // Try to find as LLM function first, if not found check if it's an expr function
        let walker = match ir.find_function(&self.name) {
            Ok(w) => w,
            Err(_) => {
                // Check if it's an expr function - they don't have clients
                if ir.find_expr_fn(&self.name).is_ok() {
                    // Expr functions don't have clients, return empty string
                    return Ok(String::new());
                }
                // Neither LLM nor expr function found, return the original error
                return Err(JsValue::from_str(&format!(
                    "function `{}` not found",
                    self.name
                )));
            }
        };

        let renderer = PromptRenderer::from_function(&walker, ir, &ctx)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        Ok(renderer.client_spec().to_string())
    }

    #[wasm_bindgen]
    pub async fn render_raw_curl_for_test(
        &self,
        rt: &WasmRuntime,
        test_name: String,
        wasm_call_context: &WasmCallContext,
        stream: bool,
        expand_images: bool,
        get_baml_src_cb: js_sys::Function,
        env: js_sys::Object,
        expose_secrets: bool,
    ) -> Result<String, wasm_bindgen::JsError> {
        self.ensure_llm()?;
        let context_manager = rt.runtime.create_ctx_manager(
            BamlValue::String("wasm".to_string()),
            js_fn_to_baml_src_reader(get_baml_src_cb),
        );

        let test_type_builder = rt
            .runtime
            .internal()
            .get_test_type_builder(&self.name, &test_name)
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        let entries = js_sys::Object::entries(&env);
        let mut env_vars = HashMap::new();
        for entry in entries.iter() {
            let arr = entry.dyn_into::<js_sys::Array>().unwrap();
            let key = arr.get(0).as_string().unwrap();
            let value = arr.get(1).as_string().unwrap_or_default();
            env_vars.insert(key, value);
        }

        let ctx = context_manager
            .create_ctx(
                test_type_builder.as_ref(),
                None,
                env_vars,
                vec![baml_ids::FunctionCallId::new()],
            )
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        let params = rt
            .runtime
            .get_test_params(&self.name, &test_name, &ctx, false)
            .map_err(|e| JsError::new(format!("{e:?}").as_str()))?;

        let result = rt
            .runtime
            .internal()
            .render_prompt(&self.name, &ctx, &params, wasm_call_context.node_index)
            .await;

        let final_prompt = match result {
            Ok((prompt, _, _)) => match prompt {
                RenderedPrompt::Chat(chat_messages) => chat_messages,
                RenderedPrompt::Completion(_) => vec![], // or handle this case differently
            },
            Err(e) => return Err(wasm_bindgen::JsError::new(format!("{e:?}").as_str())),
        };

        rt.runtime
            .internal()
            .render_raw_curl(
                &self.name,
                &ctx,
                &final_prompt,
                RenderCurlSettings {
                    stream,
                    as_shell_commands: !expand_images,
                    expose_secrets,
                },
                wasm_call_context.node_index,
            )
            .await
            .map_err(|e| wasm_bindgen::JsError::new(format!("{e:?}").as_str()))
    }

    pub fn function_graph(&self, rt: &WasmRuntime) -> Result<String, JsValue> {
        let rt = &rt.runtime;
        let ctx = rt
            .create_ctx_manager(BamlValue::String("wasm".to_string()), None)
            .create_ctx_with_default();

        let graph = rt
            .internal()
            .function_graph(&self.name, &ctx)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        Ok(graph)
    }

    #[wasm_bindgen]
    pub fn function_graph_v2(&self, rt: &WasmRuntime) -> Result<WasmControlFlowGraph, JsValue> {
        let rt = &rt.runtime;
        let ctx = rt
            .create_ctx_manager(BamlValue::String("wasm".to_string()), None)
            .create_ctx_with_default();

        let graph = rt
            .internal()
            .function_graph_v2(&self.name, &ctx)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        Ok(graph.into())
    }

    pub fn orchestration_graph(&self, rt: &WasmRuntime) -> Result<Vec<WasmScope>, JsValue> {
        if self.function_type != WasmFunctionKind::Llm {
            return Ok(Vec::new());
        }
        let rt = &rt.runtime;

        let ctx = rt
            .create_ctx_manager(BamlValue::String("wasm".to_string()), None)
            .create_ctx_with_default();

        let ir = rt.ir();

        // Try to find as LLM function first, if not found try expr function
        let walker = match ir.find_function(&self.name) {
            Ok(w) => w,
            Err(_) => {
                // Check if it's an expr function - they don't have orchestration graphs
                if ir.find_expr_fn(&self.name).is_ok() {
                    // Expr functions don't have orchestration graphs, return empty
                    return Ok(Vec::new());
                }
                // Neither LLM nor expr function found, return the original error
                return Err(JsValue::from_str(&format!(
                    "function `{}` not found",
                    self.name
                )));
            }
        };
        let renderer = PromptRenderer::from_function(&walker, ir, &ctx)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;
        let client_spec = renderer.client_spec();

        let graph = rt
            .internal()
            .orchestration_graph(client_spec, &ctx)
            .map_err(|e| JsValue::from_str(&format!("{e:?}")))?;

        // Serialize the scopes to JsValue
        let mut scopes = Vec::new();
        for scope in graph {
            scopes.push(WasmScope::from(scope.scope));
        }
        Ok(scopes)
    }
}
trait ToJsValue {
    fn to_js_value(&self) -> JsValue;
}

impl ToJsValue for ExecutionScope {
    fn to_js_value(&self) -> JsValue {
        let obj = js_sys::Object::new();
        let set_property = |obj: &js_sys::Object, key: &str, value: JsValue| {
            js_sys::Reflect::set(obj, &JsValue::from_str(key), &value).is_ok()
        };

        match self {
            ExecutionScope::Direct(name) => {
                set_property(&obj, "type", JsValue::from_str("Direct"));
                set_property(&obj, "name", JsValue::from_str(name));
            }
            ExecutionScope::Retry(name, count, delay) => {
                set_property(&obj, "type", JsValue::from_str("Retry"));
                set_property(&obj, "name", JsValue::from_str(name));
                set_property(&obj, "count", JsValue::from_f64(*count as f64));
                set_property(&obj, "delay", JsValue::from_f64(delay.as_millis() as f64));
            }
            ExecutionScope::RoundRobin(strategy, index) => {
                set_property(&obj, "type", JsValue::from_str("RoundRobin"));
                set_property(
                    &obj,
                    "strategy_name",
                    JsValue::from_str(&format!("{:?}", strategy.name)),
                );
                set_property(&obj, "index", JsValue::from_f64(*index as f64));
            }
            ExecutionScope::Fallback(name, index) => {
                set_property(&obj, "type", JsValue::from_str("Fallback"));
                set_property(&obj, "name", JsValue::from_str(name));
                set_property(&obj, "index", JsValue::from_f64(*index as f64));
            }
        }
        obj.into()
    }
}

impl ToJsValue for OrchestrationScope {
    fn to_js_value(&self) -> JsValue {
        let array = js_sys::Array::new();
        for scope in &self.scope {
            array.push(&scope.to_js_value());
        }
        array.into()
    }
}
