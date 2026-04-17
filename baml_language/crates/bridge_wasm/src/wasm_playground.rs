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
    },
    /// A runtime event was emitted during execution.
    #[serde(rename_all = "camelCase")]
    RuntimeEvent {
        span_id: String,
        parent_span_id: Option<String>,
        root_span_id: String,
        timestamp_ms: u64,
        /// Event type: "function_start", "function_end", "log", "custom"
        event_type: String,
        event_data: serde_json::Value,
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
            } => PlaygroundNotification::TestCollectionResult {
                project,
                generation,
                call_id,
                data,
            },
            bex_project::PlaygroundNotification::RuntimeEvent {
                span_id,
                parent_span_id,
                root_span_id,
                timestamp_ms,
                event_type,
                event_data,
            } => PlaygroundNotification::RuntimeEvent {
                span_id,
                parent_span_id,
                root_span_id,
                timestamp_ms,
                event_type,
                event_data,
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

    /// Convert a RuntimeEvent to the playground notification format.
    fn runtime_event_to_notification(
        event: &bex_events::RuntimeEvent,
    ) -> PlaygroundNotification {
        let timestamp_ms = event
            .timestamp
            .duration_since(web_time::UNIX_EPOCH)
            .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
            .unwrap_or(0);

        let (event_type, event_data) = match &event.event {
            bex_events::EventKind::Function(bex_events::FunctionEvent::Start(start)) => {
                let args_json = bex_values_to_json(&start.args);
                let tags_map: serde_json::Map<String, serde_json::Value> = start
                    .tags
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                (
                    "function_start".to_string(),
                    serde_json::json!({
                        "function_display_name": start.name,
                        "args": args_json,
                        "tags": tags_map,
                    }),
                )
            }
            bex_events::EventKind::Function(bex_events::FunctionEvent::End(end)) => {
                let result_json = bex_value_to_json(&end.result);
                (
                    "function_end".to_string(),
                    serde_json::json!({
                        "function_display_name": end.name,
                        "result": result_json,
                        "duration_ms": u64::try_from(end.duration.as_millis()).unwrap_or(u64::MAX),
                    }),
                )
            }
            bex_events::EventKind::SetTags(tags) => {
                let tags_map: serde_json::Map<String, serde_json::Value> = tags
                    .iter()
                    .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                    .collect();
                ("set_tags".to_string(), serde_json::json!({ "tags": tags_map }))
            }
            bex_events::EventKind::Log(log_event) => (
                "log".to_string(),
                serde_json::json!({
                    "level": log_event.level,
                    "data": bex_value_to_json(&log_event.data),
                }),
            ),
            bex_events::EventKind::Custom(custom_event) => (
                "custom".to_string(),
                serde_json::json!({
                    "name": custom_event.name,
                    "data": bex_value_to_json(&custom_event.data),
                }),
            ),
        };

        PlaygroundNotification::RuntimeEvent {
            span_id: event.ctx.span_id.to_string(),
            parent_span_id: event.ctx.parent_span_id.as_ref().map(ToString::to_string),
            root_span_id: event.ctx.root_span_id.to_string(),
            timestamp_ms,
            event_type,
            event_data,
        }
    }
}

impl bex_events::EventSink for WasmEventSink {
    fn send(&self, event: bex_events::RuntimeEvent) {
        let notification = Self::runtime_event_to_notification(&event);
        let callback = self.callback.inner();
        let _ = callback.call1(&JsValue::NULL, &notification.into());
    }

    fn flush(&self) {
        // WASM is single-threaded and sends synchronously, nothing to flush.
    }
}

/// Convert a Vec<BexExternalValue> to a JSON value.
fn bex_values_to_json(values: &[bex_external_types::BexExternalValue]) -> serde_json::Value {
    serde_json::Value::Array(values.iter().map(bex_value_to_json).collect())
}

/// Convert a single `BexExternalValue` to a JSON value.
fn bex_value_to_json(value: &bex_external_types::BexExternalValue) -> serde_json::Value {
    use bex_external_types::{BexExternalAdt, BexExternalValue};

    match value {
        BexExternalValue::Null => serde_json::Value::Null,
        BexExternalValue::Bool(b) => serde_json::Value::Bool(*b),
        BexExternalValue::Int(i) => serde_json::json!(i),
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::String(s) => serde_json::Value::String(s.clone()),
        BexExternalValue::Array { items, .. } => bex_values_to_json(items),
        BexExternalValue::Map { entries, .. } => {
            let obj: serde_json::Map<String, serde_json::Value> = entries
                .iter()
                .map(|(k, v)| (k.clone(), bex_value_to_json(v)))
                .collect();
            serde_json::Value::Object(obj)
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let mut obj = serde_json::Map::new();
            obj.insert(
                "__class".into(),
                serde_json::Value::String(class_name.clone()),
            );
            for (k, v) in fields {
                obj.insert(k.clone(), bex_value_to_json(v));
            }
            serde_json::Value::Object(obj)
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => {
            serde_json::json!({"__enum": enum_name, "value": variant_name})
        }
        BexExternalValue::Union { value, .. } => bex_value_to_json(value),
        BexExternalValue::Handle(_) => serde_json::Value::String("<handle>".into()),
        BexExternalValue::Uint8Array(bytes) => {
            serde_json::json!({"__uint8array_len": bytes.len()})
        }
        BexExternalValue::RustData(_) => serde_json::Value::String("<rust_data>".into()),
        BexExternalValue::FunctionRef { global_index } => {
            serde_json::json!({"__function_ref": global_index})
        }
        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
            serde_json::json!({"__adt": "Collector"})
        }
        BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
            serde_json::json!({"__adt": "Type", "value": format!("{ty}")})
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
            serde_json::json!({"__adt": "PromptAst"})
        }
        BexExternalValue::Adt(BexExternalAdt::Media(_)) => {
            serde_json::json!({"__adt": "Media"})
        }
    }
}
