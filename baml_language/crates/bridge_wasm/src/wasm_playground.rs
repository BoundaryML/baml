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
    pub params: Vec<ParamInfo>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ParamInfo {
    pub name: String,
    pub field_type: FieldType,
}

#[derive(Tsify, Serialize)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldType {
    String,
    Int,
    Float,
    Bool,
    Null,
    Enum { name: String, values: Vec<String> },
    Class { name: String, fields: Vec<ParamInfo> },
    List { item: Box<FieldType> },
    Map { key: Box<FieldType>, value: Box<FieldType> },
    Optional { inner: Box<FieldType> },
    Union { variants: Vec<FieldType> },
    Literal { value: LiteralValue },
    RecursiveRef { name: String },
    Any,
    Media { media_type: String },
}

#[derive(Tsify, Serialize)]
#[serde(tag = "type", content = "value", rename_all = "camelCase")]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
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
pub struct TestInfo {
    pub name: String,
    pub function_name: String,
    pub args_json: String,
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
    pub tests: Vec<TestInfo>,
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
                                params: f.params.into_iter().map(convert_param_info).collect(),
                            })
                            .collect(),
                        tests: update
                            .tests
                            .into_iter()
                            .map(|t| TestInfo {
                                name: t.name,
                                function_name: t.function_name,
                                args_json: t.args_json,
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

fn convert_param_info(p: bex_project::ParamInfo) -> ParamInfo {
    ParamInfo {
        name: p.name,
        field_type: convert_field_type(p.field_type),
    }
}

fn convert_field_type(ft: bex_project::FieldType) -> FieldType {
    match ft {
        bex_project::FieldType::String => FieldType::String,
        bex_project::FieldType::Int => FieldType::Int,
        bex_project::FieldType::Float => FieldType::Float,
        bex_project::FieldType::Bool => FieldType::Bool,
        bex_project::FieldType::Null => FieldType::Null,
        bex_project::FieldType::Enum { name, values } => FieldType::Enum { name, values },
        bex_project::FieldType::Class { name, fields } => FieldType::Class {
            name,
            fields: fields.into_iter().map(convert_param_info).collect(),
        },
        bex_project::FieldType::List { item } => FieldType::List {
            item: Box::new(convert_field_type(*item)),
        },
        bex_project::FieldType::Map { key, value } => FieldType::Map {
            key: Box::new(convert_field_type(*key)),
            value: Box::new(convert_field_type(*value)),
        },
        bex_project::FieldType::Optional { inner } => FieldType::Optional {
            inner: Box::new(convert_field_type(*inner)),
        },
        bex_project::FieldType::Union { variants } => FieldType::Union {
            variants: variants.into_iter().map(convert_field_type).collect(),
        },
        bex_project::FieldType::Literal { value } => FieldType::Literal {
            value: match value {
                bex_project::LiteralValue::String(s) => LiteralValue::String(s),
                bex_project::LiteralValue::Int(i) => LiteralValue::Int(i),
                bex_project::LiteralValue::Bool(b) => LiteralValue::Bool(b),
            },
        },
        bex_project::FieldType::RecursiveRef { name } => FieldType::RecursiveRef { name },
        bex_project::FieldType::Any => FieldType::Any,
        bex_project::FieldType::Media { media_type } => FieldType::Media { media_type },
    }
}
