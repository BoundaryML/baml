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
    pub origin: FunctionOrigin,
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
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    AutoDerive,
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
        generation: u64,
        call_id: u64,
        data: Vec<u8>,
        #[serde(skip_serializing_if = "Option::is_none")]
        expand_error: Option<bex_project::TestExpandError>,
    },
    /// A runtime event was emitted during execution (protobuf-encoded).
    #[serde(rename_all = "camelCase")]
    RuntimeEvent {
        /// Protobuf-encoded `RuntimeEvent` bytes (decode with `RuntimeEvent.decode()`)
        data: Vec<u8>,
        call_id: u64,
    },
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
                                origin: match f.origin {
                                    bex_project::FunctionOrigin::UserDefined => {
                                        FunctionOrigin::UserDefined
                                    }
                                    bex_project::FunctionOrigin::Companion => {
                                        FunctionOrigin::Companion
                                    }
                                    bex_project::FunctionOrigin::Internal => {
                                        FunctionOrigin::Internal
                                    }
                                    bex_project::FunctionOrigin::AutoDerive => {
                                        FunctionOrigin::AutoDerive
                                    }
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
                generation,
                call_id,
                data,
                expand_error,
            } => PlaygroundNotification::TestCollectionResult {
                project,
                generation,
                call_id,
                data,
                expand_error,
            },
            bex_project::PlaygroundNotification::RuntimeEvent { data, call_id } => {
                PlaygroundNotification::RuntimeEvent { data, call_id }
            }
        }
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

/// Event sink for WASM that forwards events to the playground notification callback.
pub(crate) struct WasmEventSink {
    callback: SendWrapper<Function>,
}

impl WasmEventSink {
    pub(crate) fn new(callback: Function) -> Self {
        Self {
            callback: SendWrapper::new(callback),
        }
    }
}

impl bex_events::EventSink for WasmEventSink {
    fn send(&self, event: bex_events::RuntimeEvent) {
        let call_id = event.call_id.0;
        let options = bridge_ctypes::CffiHandleTableOptions::for_wire();
        match bridge_ctypes::runtime_event_to_bytes(&event, &options) {
            Ok(data) => {
                let notification = PlaygroundNotification::RuntimeEvent { data, call_id };
                let callback = self.callback.inner();
                let _ = callback.call1(&JsValue::NULL, &notification.into());
            }
            Err(e) => {
                log::error!("Failed to encode runtime event: {e}");
            }
        }
    }

    fn flush(&self) {
        // WASM is single-threaded and sends synchronously, nothing to flush.
    }
}
