use js_sys::Function;
use serde::Serialize;
use tsify::Tsify;
use wasm_bindgen::JsValue;

use crate::send_wrapper::SendWrapper;

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FunctionInfo {
    pub name: String,
    pub kind: FunctionKind,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub enum FunctionKind {
    Llm,
    Expr,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct LlmCapabilities {
    pub render_prompt: bool,
    pub build_request: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub client_name: Option<String>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ProjectDiagnostic {
    pub severity: String,
    pub message: String,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub is_bex_current: bool,
    pub functions: Vec<FunctionInfo>,
    pub diagnostics: Vec<ProjectDiagnostic>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTestInfo {
    pub name: String,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct RuntimeTestSetInfo {
    pub name: String,
    pub items: Vec<TestDef>,
    pub loading_time_ms: u64,
    pub total_loading_time_ms: u64,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum TestDef {
    Test(RuntimeTestInfo),
    TestSet(RuntimeTestSetInfo),
    #[serde(rename_all = "camelCase")]
    LazyTestSet {
        name: String,
    },
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TestCollectionStatus {
    Collecting,
    Done { items: Vec<TestDef> },
    Error { message: String },
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum PlaygroundNotification {
    #[serde(rename_all = "camelCase")]
    ListProjects { projects: Vec<String> },
    #[serde(rename_all = "camelCase")]
    UpdateProject {
        project: String,
        update: ProjectUpdate,
    },
    #[serde(rename_all = "camelCase")]
    OpenPlayground {
        project: String,
        function_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ControlFlowGraphResult {
        function_name: String,
        graph: Option<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    CursorContext { context: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    TestCollectionResult {
        project: String,
        package: String,
        result: TestCollectionStatus,
    },
    #[serde(rename_all = "camelCase")]
    TestSetExpandResult {
        project: String,
        name: String,
        result: TestSetExpandResultPayload,
    },
    #[serde(rename_all = "camelCase")]
    BackgroundTaskCount { project: String, count: u32 },
    #[serde(rename_all = "camelCase")]
    TestRunResult {
        project: String,
        name: String,
        report_json: serde_json::Value,
    },
}

/// Payload for `TestSetExpandResult` — either a successfully expanded set or an error string.
///
/// We use a plain struct/enum here (not `Result<T, E>`) because `tsify` cannot
/// derive TypeScript for `Result` directly.
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "status", rename_all = "camelCase")]
pub enum TestSetExpandResultPayload {
    #[serde(rename_all = "camelCase")]
    Ok { set: RuntimeTestSetInfo },
    #[serde(rename_all = "camelCase")]
    Err { message: String },
}

impl From<bex_project::PlaygroundNotification> for PlaygroundNotification {
    fn from(n: bex_project::PlaygroundNotification) -> Self {
        match n {
            bex_project::PlaygroundNotification::ListProjects { projects } => {
                PlaygroundNotification::ListProjects { projects }
            }
            bex_project::PlaygroundNotification::UpdateProject { project, update } => {
                PlaygroundNotification::UpdateProject {
                    project,
                    update: ProjectUpdate {
                        is_bex_current: update.is_bex_current,
                        functions: update
                            .functions
                            .into_iter()
                            .map(|f| FunctionInfo {
                                name: f.name,
                                kind: match f.kind {
                                    bex_project::FunctionKind::Llm => FunctionKind::Llm,
                                    bex_project::FunctionKind::Expr => FunctionKind::Expr,
                                },
                                capabilities: f.capabilities.map(|c| LlmCapabilities {
                                    render_prompt: c.render_prompt,
                                    build_request: c.build_request,
                                    client_name: c.client_name,
                                }),
                            })
                            .collect(),
                        diagnostics: update
                            .diagnostics
                            .into_iter()
                            .map(|d| ProjectDiagnostic {
                                severity: d.severity.to_string(),
                                message: d.message,
                            })
                            .collect(),
                    },
                }
            }
            bex_project::PlaygroundNotification::OpenPlayground {
                project,
                function_name,
            } => PlaygroundNotification::OpenPlayground {
                project,
                function_name,
            },
            bex_project::PlaygroundNotification::ControlFlowGraphResult {
                function_name,
                graph,
            } => PlaygroundNotification::ControlFlowGraphResult {
                function_name,
                graph,
            },
            bex_project::PlaygroundNotification::CursorContext { context } => {
                PlaygroundNotification::CursorContext { context }
            }
            bex_project::PlaygroundNotification::TestCollectionResult {
                project,
                package,
                result,
            } => PlaygroundNotification::TestCollectionResult {
                project,
                package,
                result: match result {
                    bex_project::TestCollectionStatus::Collecting => {
                        TestCollectionStatus::Collecting
                    }
                    bex_project::TestCollectionStatus::Done { items } => {
                        TestCollectionStatus::Done {
                            items: items.into_iter().map(convert_test_def).collect(),
                        }
                    }
                    bex_project::TestCollectionStatus::Error { message } => {
                        TestCollectionStatus::Error { message }
                    }
                },
            },
            bex_project::PlaygroundNotification::TestSetExpandResult {
                project,
                name,
                result,
            } => PlaygroundNotification::TestSetExpandResult {
                project,
                name,
                result: match result {
                    Ok(set) => TestSetExpandResultPayload::Ok {
                        set: convert_runtime_test_set_info(set),
                    },
                    Err(message) => TestSetExpandResultPayload::Err { message },
                },
            },
            bex_project::PlaygroundNotification::BackgroundTaskCount { project, count } => {
                PlaygroundNotification::BackgroundTaskCount { project, count }
            }
            bex_project::PlaygroundNotification::TestRunResult {
                project,
                name,
                report_json,
            } => PlaygroundNotification::TestRunResult {
                project,
                name,
                report_json,
            },
        }
    }
}

fn convert_test_def(def: bex_project::TestDef) -> TestDef {
    match def {
        bex_project::TestDef::Test(t) => TestDef::Test(RuntimeTestInfo { name: t.name }),
        bex_project::TestDef::TestSet(ts) => TestDef::TestSet(RuntimeTestSetInfo {
            name: ts.name,
            items: ts.items.into_iter().map(convert_test_def).collect(),
            loading_time_ms: ts.loading_time_ms,
            total_loading_time_ms: ts.total_loading_time_ms,
        }),
        bex_project::TestDef::LazyTestSet { name } => TestDef::LazyTestSet { name },
    }
}

fn convert_runtime_test_set_info(ts: bex_project::RuntimeTestSetInfo) -> RuntimeTestSetInfo {
    RuntimeTestSetInfo {
        name: ts.name,
        items: ts.items.into_iter().map(convert_test_def).collect(),
        loading_time_ms: ts.loading_time_ms,
        total_loading_time_ms: ts.total_loading_time_ms,
    }
}

pub(crate) struct WasmPlaygroundSender {
    callback: SendWrapper<Function>,
}

impl WasmPlaygroundSender {
    pub(crate) fn new(callback: Function) -> Self {
        Self {
            callback: SendWrapper::new(callback),
        }
    }
}

impl bex_project::PlaygroundSender for WasmPlaygroundSender {
    fn send_playground_notification(&self, notification: bex_project::PlaygroundNotification) {
        let wasm_notif: PlaygroundNotification = notification.into();
        let callback = self.callback.inner();
        let _ = callback.call1(&JsValue::NULL, &wasm_notif.into());
    }
}
