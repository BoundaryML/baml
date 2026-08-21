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
    pub signature: String,
    pub source_position: FunctionSourcePosition,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub capabilities: Option<LlmCapabilities>,
    /// `None` = no schema available (UI degrades to raw JSON); `Some(vec![])`
    /// = nullary function. The wire shape must match
    /// `bex_project::FunctionInfo` exactly.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub params: Option<Vec<ParamSchema>>,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FunctionSourcePosition {
    pub file: String,
    pub line: u32,
    pub column: u32,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ParamSchema {
    pub name: String,
    pub has_default: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub default_expression: Option<String>,
    pub schema: FieldSchema,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct FieldSchemaField {
    pub name: String,
    pub schema: FieldSchema,
}

/// Twin of `baml_project::FieldSchema` (via `bex_project`); both serialize
/// with `tag = "type"` + camelCase so the JSON matches the WebSocket
/// transport, which serializes the source struct directly. Named types are
/// `Ref`s into the `ProjectUpdate.types` table.
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum FieldSchema {
    String,
    Int,
    Float,
    Bool,
    Null,
    Bigint,
    Media {
        kind: String,
    },
    Literal {
        value: serde_json::Value,
    },
    Ref {
        name: String,
    },
    EnumVariant {
        name: String,
        value: String,
    },
    List {
        item: Box<FieldSchema>,
    },
    Map {
        key: Box<FieldSchema>,
        value: Box<FieldSchema>,
    },
    Optional {
        inner: Box<FieldSchema>,
    },
    Union {
        variants: Vec<FieldSchema>,
    },
    Unsupported {
        display: String,
    },
}

/// Twin of `baml_project::TypeSchema` — one entry per named type in the
/// per-project table, keyed by canonical dotted FQN.
#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(tag = "kind", rename_all = "camelCase")]
pub enum TypeSchema {
    Class { fields: Vec<FieldSchemaField> },
    Enum { values: Vec<String> },
    Alias { schema: FieldSchema },
}

impl From<bex_project::ParamSchema> for ParamSchema {
    fn from(p: bex_project::ParamSchema) -> Self {
        ParamSchema {
            name: p.name,
            has_default: p.has_default,
            default_expression: p.default_expression,
            schema: p.schema.into(),
        }
    }
}

impl From<bex_project::FieldSchemaField> for FieldSchemaField {
    fn from(f: bex_project::FieldSchemaField) -> Self {
        FieldSchemaField {
            name: f.name,
            schema: f.schema.into(),
        }
    }
}

impl From<bex_project::FieldSchema> for FieldSchema {
    fn from(s: bex_project::FieldSchema) -> Self {
        use bex_project::FieldSchema as Src;
        match s {
            Src::String => FieldSchema::String,
            Src::Int => FieldSchema::Int,
            Src::Float => FieldSchema::Float,
            Src::Bool => FieldSchema::Bool,
            Src::Null => FieldSchema::Null,
            Src::Bigint => FieldSchema::Bigint,
            Src::Media { kind } => FieldSchema::Media { kind },
            Src::Literal { value } => FieldSchema::Literal { value },
            Src::Ref { name } => FieldSchema::Ref { name },
            Src::EnumVariant { name, value } => FieldSchema::EnumVariant { name, value },
            Src::List { item } => FieldSchema::List {
                item: Box::new((*item).into()),
            },
            Src::Map { key, value } => FieldSchema::Map {
                key: Box::new((*key).into()),
                value: Box::new((*value).into()),
            },
            Src::Optional { inner } => FieldSchema::Optional {
                inner: Box::new((*inner).into()),
            },
            Src::Union { variants } => FieldSchema::Union {
                variants: variants.into_iter().map(Into::into).collect(),
            },
            Src::Unsupported { display } => FieldSchema::Unsupported { display },
        }
    }
}

impl From<bex_project::TypeSchema> for TypeSchema {
    fn from(t: bex_project::TypeSchema) -> Self {
        use bex_project::TypeSchema as Src;
        match t {
            Src::Class { fields } => TypeSchema::Class {
                fields: fields.into_iter().map(Into::into).collect(),
            },
            Src::Enum { values } => TypeSchema::Enum { values },
            Src::Alias { schema } => TypeSchema::Alias {
                schema: schema.into(),
            },
        }
    }
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
pub struct TestInfo {
    pub name: String,
    pub function_name: String,
    pub args_json: String,
}

#[derive(Tsify, Serialize)]
#[tsify(into_wasm_abi)]
#[serde(rename_all = "camelCase")]
pub struct ProjectUpdate {
    pub is_bex_current: bool,
    pub generation: u64,
    pub functions: Vec<FunctionInfo>,
    pub tests: Vec<TestInfo>,
    /// Shared type table for `FunctionInfo.params` refs; `None` = binary
    /// predates the args form. Must match `bex_project::ProjectUpdate`.
    #[serde(skip_serializing_if = "Option::is_none")]
    pub types: Option<std::collections::BTreeMap<String, TypeSchema>>,
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
        #[serde(skip_serializing_if = "Option::is_none")]
        function_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        test_name: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        testset_name: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    ControlFlowGraphResult {
        function_name: String,
        graph: Option<serde_json::Value>,
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u32>,
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
    #[serde(rename_all = "camelCase")]
    RunStarted {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        run: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    RunPatch { patch: serde_json::Value },
    #[serde(rename_all = "camelCase")]
    RunSnapshot {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        boundary_id: String,
        snapshot: serde_json::Value,
    },
    #[serde(rename_all = "camelCase")]
    ValueBody {
        request_id: u64,
        boundary_id: String,
        value_ref_id: String,
        codec: String,
        availability: String,
        #[serde(skip_serializing_if = "Option::is_none")]
        body_base64: Option<String>,
        #[serde(skip_serializing_if = "Option::is_none")]
        diagnostic: Option<String>,
    },
    #[serde(rename_all = "camelCase")]
    RunList {
        request_id: u64,
        runs: Vec<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    HistoryList {
        request_id: u64,
        runs: Vec<serde_json::Value>,
    },
    #[serde(rename_all = "camelCase")]
    RunCursorExpired {
        #[serde(skip_serializing_if = "Option::is_none")]
        request_id: Option<u64>,
        subscription_id: String,
        boundary_id: String,
        reason: String,
    },
    #[serde(rename_all = "camelCase")]
    CommandAck { request_id: u64, outcome: String },
    #[serde(rename_all = "camelCase")]
    CommandError {
        request_id: u64,
        code: String,
        message: String,
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
                        generation: update.generation,
                        functions: update
                            .functions
                            .into_iter()
                            .map(|f| FunctionInfo {
                                name: f.name,
                                signature: f.signature,
                                source_position: FunctionSourcePosition {
                                    file: f.source_position.file,
                                    line: f.source_position.line,
                                    column: f.source_position.column,
                                },
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
                                params: f.params.map(|ps| ps.into_iter().map(Into::into).collect()),
                            })
                            .collect(),
                        tests: update
                            .tests
                            .into_iter()
                            .map(|test| TestInfo {
                                name: test.name,
                                function_name: test.function_name,
                                args_json: test.args_json,
                            })
                            .collect(),
                        types: update.types.map(|types| {
                            types
                                .into_iter()
                                .map(|(name, t)| (name, t.into()))
                                .collect()
                        }),
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
                test_name,
                testset_name,
            } => PlaygroundNotification::OpenPlayground {
                project,
                function_name,
                test_name,
                testset_name,
            },
            bex_project::PlaygroundNotification::ControlFlowGraphResult {
                function_name,
                graph,
                request_id,
            } => PlaygroundNotification::ControlFlowGraphResult {
                function_name,
                graph,
                request_id,
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
        send_wasm_playground_notification(callback, &wasm_notif);
    }
}

pub(crate) fn send_wasm_playground_notification(
    callback: &Function,
    notification: &PlaygroundNotification,
) {
    // Serialize through JSON text rather than serde-wasm-bindgen/Tsify. With
    // serde_json's `arbitrary_precision` feature, direct conversion renders
    // Value numbers as `{ "$serde_json::private::Number": "…" }`, which broke
    // graph node ids, parent ids, edges, and RunStore cursors on the JS side.
    let js_notif: JsValue =
        match crate::wasm_lsp::to_json_jsvalue(notification, "playground notification") {
            Ok(value) => value,
            Err(e) => {
                log::error!("failed to serialize playground notification for JS: {e}");
                return;
            }
        };
    let _ = callback.call1(&JsValue::NULL, &js_notif);
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The tsify twins must serialize byte-identically to the
    /// `bex_project` source types: the WebSocket transport sends the source
    /// structs directly, so any serde divergence here would present as a
    /// worker-transport-only wire difference. Exercises every `FieldSchema`
    /// and `TypeSchema` variant through the `From` mappings.
    #[test]
    fn schema_twins_serialize_identically_to_source_types() {
        use std::collections::BTreeMap;

        use bex_project::{
            FieldSchema as Src, FieldSchemaField as SrcField, ParamSchema as SrcParam,
            TypeSchema as SrcType,
        };
        let src = SrcParam {
            name: "p".to_string(),
            has_default: true,
            default_expression: Some("constants.DEFAULT".to_string()),
            schema: Src::Union {
                variants: vec![
                    Src::String,
                    Src::Int,
                    Src::Float,
                    Src::Bool,
                    Src::Null,
                    Src::Bigint,
                    Src::Media {
                        kind: "image".to_string(),
                    },
                    Src::Literal {
                        value: serde_json::json!({ "k": [1, "two", true, null] }),
                    },
                    Src::Ref {
                        name: "user.Person".to_string(),
                    },
                    Src::EnumVariant {
                        name: "user.Status".to_string(),
                        value: "Active".to_string(),
                    },
                    Src::List {
                        item: Box::new(Src::String),
                    },
                    Src::Map {
                        key: Box::new(Src::String),
                        value: Box::new(Src::Float),
                    },
                    Src::Unsupported {
                        display: "callback".to_string(),
                    },
                ],
            },
        };
        let twin: ParamSchema = src.clone().into();
        assert_eq!(
            serde_json::to_value(&src).unwrap(),
            serde_json::to_value(&twin).unwrap(),
        );

        let src_types = BTreeMap::from([
            (
                "user.Person".to_string(),
                SrcType::Class {
                    fields: vec![SrcField {
                        name: "age".to_string(),
                        schema: Src::Optional {
                            inner: Box::new(Src::Int),
                        },
                    }],
                },
            ),
            (
                "user.Color".to_string(),
                SrcType::Enum {
                    values: vec!["Red".to_string(), "Green".to_string()],
                },
            ),
            (
                "user.JSON".to_string(),
                SrcType::Alias {
                    schema: Src::Union {
                        variants: vec![
                            Src::String,
                            Src::List {
                                item: Box::new(Src::Ref {
                                    name: "user.JSON".to_string(),
                                }),
                            },
                        ],
                    },
                },
            ),
        ]);
        let twin_types: BTreeMap<String, TypeSchema> = src_types
            .clone()
            .into_iter()
            .map(|(name, t)| (name, t.into()))
            .collect();
        assert_eq!(
            serde_json::to_value(&src_types).unwrap(),
            serde_json::to_value(&twin_types).unwrap(),
        );
    }
}
