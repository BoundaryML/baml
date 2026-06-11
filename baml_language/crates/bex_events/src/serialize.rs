//! JSONL serialization for `RuntimeEvent`.

use baml_builtins2::MediaContent;
use bex_external_types::{BexExternalAdt, BexExternalValue, MediaKind};
use serde::Serialize;

use crate::{
    CustomEvent, DiskEventV1, EventFileHeaderV1, EventKind, FunctionEndStatus, FunctionEvent,
    LogEvent, RuntimeEvent, ThreadEndStatus,
    metadata::{RuntimeFunctionKind, RuntimeFunctionOrigin},
};

const MAX_SERIALIZATION_DEPTH: usize = 15;

/// Metadata blob for BAML values, matching the TypeScript `$baml` discriminator format.
///
/// For classes: `{ "type": "ClassName" }`
/// For special types: `{ "type": "$enum", "enum": "EnumName" }`, etc.
#[derive(Debug, Clone, Serialize)]
pub struct BamlMeta {
    /// The type discriminator. For classes, this is the class name.
    /// For special types, this is a `$`-prefixed tag like `$enum`, `$union`, etc.
    pub r#type: String,
    /// For `$enum`: the enum name
    #[serde(rename = "enum", skip_serializing_if = "Option::is_none")]
    pub enum_name: Option<String>,
    /// For `$union`: the union type description
    #[serde(skip_serializing_if = "Option::is_none")]
    pub union: Option<String>,
    /// For `$union`: the selected variant
    #[serde(skip_serializing_if = "Option::is_none")]
    pub selected: Option<String>,
    /// For `$uint8array`: byte length
    #[serde(skip_serializing_if = "Option::is_none")]
    pub len: Option<usize>,
    /// For `$function_ref`: global index
    #[serde(skip_serializing_if = "Option::is_none")]
    pub index: Option<usize>,
    /// For `$type`: the type value
    #[serde(skip_serializing_if = "Option::is_none")]
    pub value: Option<String>,
}

impl BamlMeta {
    /// Create metadata for a class instance.
    pub fn class(name: impl Into<String>) -> Self {
        Self {
            r#type: name.into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for an enum variant.
    pub fn enum_variant(enum_name: impl Into<String>) -> Self {
        Self {
            r#type: "$enum".into(),
            enum_name: Some(enum_name.into()),
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a union value.
    pub fn union(union_type: impl Into<String>, selected: impl Into<String>) -> Self {
        Self {
            r#type: "$union".into(),
            enum_name: None,
            union: Some(union_type.into()),
            selected: Some(selected.into()),
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a handle.
    pub fn handle() -> Self {
        Self {
            r#type: "$handle".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a uint8array.
    pub fn uint8array(len: usize) -> Self {
        Self {
            r#type: "$uint8array".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: Some(len),
            index: None,
            value: None,
        }
    }

    /// Create metadata for rust data.
    pub fn rust_data() -> Self {
        Self {
            r#type: "$rust_data".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a function reference.
    pub fn function_ref(index: usize) -> Self {
        Self {
            r#type: "$function_ref".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: Some(index),
            value: None,
        }
    }

    /// Create metadata for a collector ADT.
    pub fn collector() -> Self {
        Self {
            r#type: "$collector".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a type ADT.
    pub fn type_adt(value: impl Into<String>) -> Self {
        Self {
            r#type: "$type".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: Some(value.into()),
        }
    }

    /// Create metadata for a prompt AST ADT.
    pub fn prompt_ast() -> Self {
        Self {
            r#type: "$prompt_ast".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }

    /// Create metadata for a media ADT.
    pub fn media() -> Self {
        Self {
            r#type: "$media".into(),
            enum_name: None,
            union: None,
            selected: None,
            len: None,
            index: None,
            value: None,
        }
    }
}

/// Serialize a `RuntimeEvent` to a single-line JSON string (JSONL format).
pub fn event_to_jsonl(event: &RuntimeEvent) -> String {
    let call_id = event.ctx.span_id.to_string();
    let function_event_id = uuid::Uuid::new_v4().to_string();

    let call_stack: Vec<String> = event
        .call_stack
        .iter()
        .map(std::string::ToString::to_string)
        .collect();

    let timestamp_epoch_ms = event
        .timestamp
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);

    let content = event_content_to_json(&event.event);

    let parent_span_id = event
        .ctx
        .parent_span_id
        .as_ref()
        .map(std::string::ToString::to_string);
    let root_span_id = event.ctx.root_span_id.to_string();

    let event_json = serde_json::json!({
        "call_id": call_id,
        "bex_identity": event.identity.as_ref().map(|identity| serde_json::json!({
            "thread_id": identity.thread_id.0,
            "call_id": identity.call_id.0,
            "parent_call_id": identity.parent_call_id.map(|id| id.0),
            "function_id": identity.function_id.map(|id| id.0),
            "call_ref": identity.call_ref.encode(),
        })),
        "function_event_id": function_event_id,
        "parent_span_id": parent_span_id,
        "root_span_id": root_span_id,
        "call_stack": call_stack,
        "timestamp_epoch_ms": timestamp_epoch_ms,
        "content": content,
    });

    serde_json::to_string(&event_json).unwrap_or_else(|e| {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Failed to serialize trace event: {e}");
        }
        String::new()
    })
}

/// Serialize a compact disk/batch BEX event to a single-line JSON string.
///
/// `engine_id` is written onto every line. This is interim-transport-only:
/// the contract scopes events by their file/batch header (which carries the
/// engine id once), but this JSONL writer appends events from *every* engine
/// in the process to one file, where header-only scoping is ambiguous —
/// after the second header, `{thread 1, call 1}` could belong to either
/// engine. Delete the per-line field when per-engine transport lands.
pub fn disk_event_to_jsonl(engine_id: crate::ids::EngineId, event: &DiskEventV1) -> String {
    let mut event_json = match event {
        DiskEventV1::StartThread {
            thread_id,
            parent_thread_id,
            parent_call_id,
            name,
            timestamp_ns,
        } => serde_json::json!({
            "type": "bex_start_thread",
            "thread_id": thread_id.0,
            "parent_thread_id": parent_thread_id.map(|id| id.0),
            "parent_call_id": parent_call_id.map(|id| id.0),
            "name": name,
            "timestamp_ns": timestamp_ns,
        }),
        DiskEventV1::CallFunction {
            thread_id,
            call_id,
            parent_call_id,
            function_id,
            timestamp_ns,
        } => serde_json::json!({
            "type": "bex_call_function",
            "thread_id": thread_id.0,
            "call_id": call_id.0,
            "parent_call_id": parent_call_id.map(|id| id.0),
            "function_id": function_id.0,
            "timestamp_ns": timestamp_ns,
        }),
        DiskEventV1::SetId {
            thread_id,
            call_id,
            id,
            timestamp_ns,
        } => serde_json::json!({
            "type": "bex_set_id",
            "thread_id": thread_id.0,
            "call_id": call_id.0,
            "id": base64_url(id),
            "timestamp_ns": timestamp_ns,
        }),
        DiskEventV1::EndFunction {
            thread_id,
            call_id,
            status,
            timestamp_ns,
        } => serde_json::json!({
            "type": "bex_end_function",
            "thread_id": thread_id.0,
            "call_id": call_id.0,
            "status": function_end_status(status),
            "timestamp_ns": timestamp_ns,
        }),
        DiskEventV1::EndThread {
            thread_id,
            status,
            timestamp_ns,
        } => serde_json::json!({
            "type": "bex_end_thread",
            "thread_id": thread_id.0,
            "status": thread_end_status(status),
            "timestamp_ns": timestamp_ns,
        }),
        DiskEventV1::Heartbeat { timestamp_ns } => serde_json::json!({
            "type": "bex_heartbeat",
            "timestamp_ns": timestamp_ns,
        }),
    };
    if let Some(map) = event_json.as_object_mut() {
        map.insert("engine_id".to_string(), serde_json::json!(engine_id.0));
    }

    serde_json::to_string(&event_json).unwrap_or_else(|e| {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Failed to serialize BEX disk event: {e}");
        }
        String::new()
    })
}

/// Serialize a BEX event file/batch header to a single-line JSON string.
pub fn event_file_header_to_jsonl(header: &EventFileHeaderV1) -> String {
    let functions: Vec<_> = header
        .function_table
        .functions
        .iter()
        .map(|function| {
            serde_json::json!({
                "function_id": function.function_id.0,
                "fqn": function.fqn,
                "display_name": function.display_name,
                "source_file": function.source_file,
                "source_span": function.source_span.as_ref().map(|span| serde_json::json!({
                    "file_id": span.file_id,
                    "start": span.start,
                    "end": span.end,
                })),
                "kind": runtime_function_kind(&function.kind),
                "origin": runtime_function_origin(&function.origin),
                "owner_type": function.owner_type.as_ref().map(|key| key.0.as_str()),
                "parent_function": function.parent_function.as_ref().map(|key| key.0.as_str()),
                "lambda_path": function.lambda_path,
                "definition_key": function.definition_key.as_ref().map(|key| key.0.as_str()),
                "package_name": function.package_name,
                "namespace": function.namespace,
                "source_snapshot_id": function.source_snapshot_id.as_ref().map(|id| base64_url(&id.0)),
                "revision_id": function.revision_id.as_ref().map(|id| id.0.as_str()),
                "semantic_lanes": function.semantic_lanes.as_ref().map(|lanes| serde_json::json!({
                    "direct_interface": base64_url(&lanes.direct_interface.0),
                    "effective_interface": base64_url(&lanes.effective_interface.0),
                    "direct_implementation": lanes.direct_implementation.as_ref().map(|hash| base64_url(&hash.0)),
                    "effective_implementation": lanes.effective_implementation.as_ref().map(|hash| base64_url(&hash.0)),
                })),
            })
        })
        .collect();

    let event_json = serde_json::json!({
        "type": "bex_header_v1",
        "process_euid": base64_url(&header.process_euid.0),
        "engine_id": header.engine_id.0,
        "program_id": base64_url(&header.program_id.0),
        "source_snapshot_id": header.source_snapshot_id.as_ref().map(|id| base64_url(&id.0)),
        "revision_id": header.revision_id.as_ref().map(|id| id.0.as_str()),
        "started_at_epoch_ns": header.started_at_epoch_ns.to_string(),
        "function_table": functions,
    });

    serde_json::to_string(&event_json).unwrap_or_else(|e| {
        #[allow(clippy::print_stderr)]
        {
            eprintln!("Failed to serialize BEX event header: {e}");
        }
        String::new()
    })
}

fn runtime_function_kind(kind: &RuntimeFunctionKind) -> serde_json::Value {
    match kind {
        RuntimeFunctionKind::Bytecode => serde_json::json!({"type": "bytecode"}),
        RuntimeFunctionKind::SysOp(op) => serde_json::json!({"type": "sys_op", "op": op}),
        RuntimeFunctionKind::Native => serde_json::json!({"type": "native"}),
        RuntimeFunctionKind::NativeUnresolved => {
            serde_json::json!({"type": "native_unresolved"})
        }
    }
}

fn runtime_function_origin(origin: &RuntimeFunctionOrigin) -> &'static str {
    match origin {
        RuntimeFunctionOrigin::UserDefined => "user_defined",
        RuntimeFunctionOrigin::Companion => "companion",
        RuntimeFunctionOrigin::Internal => "internal",
        RuntimeFunctionOrigin::Builtin => "builtin",
        RuntimeFunctionOrigin::AutoDerive => "auto_derive",
    }
}

fn function_end_status(status: &FunctionEndStatus) -> &'static str {
    match status {
        FunctionEndStatus::Ok => "ok",
        FunctionEndStatus::Error => "error",
        FunctionEndStatus::Cancelled => "cancelled",
    }
}

fn thread_end_status(status: &ThreadEndStatus) -> &'static str {
    match status {
        ThreadEndStatus::Completed => "completed",
        ThreadEndStatus::Error => "error",
        ThreadEndStatus::Cancelled => "cancelled",
    }
}

fn base64_url(bytes: &[u8]) -> String {
    use base64::{Engine as _, engine::general_purpose::URL_SAFE_NO_PAD};
    URL_SAFE_NO_PAD.encode(bytes)
}

/// Serialize the event content (the `EventKind` portion) to JSON.
fn event_content_to_json(event: &EventKind) -> serde_json::Value {
    match event {
        EventKind::Function(FunctionEvent::Start(start)) => {
            let args_json = bex_value_vec_to_json(&start.args);
            let tags_map: serde_json::Map<String, serde_json::Value> = start
                .tags
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            serde_json::json!({
                "type": "function_start",
                "data": {
                    "function_display_name": start.name,
                    "args": args_json,
                    "tags": tags_map,
                }
            })
        }
        EventKind::Function(FunctionEvent::End(end)) => {
            let result_json = bex_value_to_json(&end.result);
            serde_json::json!({
                "type": "function_end",
                "data": {
                    "function_display_name": end.name,
                    "result": result_json,
                    "duration_ms": u64::try_from(end.duration.as_millis()).unwrap_or(u64::MAX),
                    "error": end.error.as_deref(),
                    "status": if end.error.is_some() { "error" } else { "success" },
                }
            })
        }
        EventKind::SetTags(tags) => {
            let tags_map: serde_json::Map<String, serde_json::Value> = tags
                .iter()
                .map(|(k, v)| (k.clone(), serde_json::Value::String(v.clone())))
                .collect();
            serde_json::json!({
                "type": "intermediate",
                "data": {
                    "SetTags": tags_map,
                }
            })
        }
        EventKind::Log(LogEvent {
            level,
            data,
            source,
        }) => {
            let data_json = bex_value_to_json(data);
            let source_json = source.as_ref().map(|s| {
                serde_json::json!({
                    "file_id": s.file_id,
                    "line": s.line,
                    "column": s.column,
                    "start_offset": s.start_offset,
                    "end_offset": s.end_offset,
                })
            });
            serde_json::json!({
                "type": "log",
                "data": {
                    "level": level,
                    "data": data_json,
                    "source": source_json,
                }
            })
        }
        EventKind::Custom(CustomEvent { name, data }) => {
            let data_json = bex_value_to_json(data);
            serde_json::json!({
                "type": "custom",
                "data": {
                    "name": name,
                    "data": data_json,
                }
            })
        }
    }
}

/// Convert a `Vec<BexExternalValue>` to a JSON array.
pub fn bex_value_vec_to_json(values: &[BexExternalValue]) -> serde_json::Value {
    bex_value_vec_to_json_impl(values, 0)
}

fn bex_value_vec_to_json_impl(values: &[BexExternalValue], depth: usize) -> serde_json::Value {
    let arr: Vec<_> = values
        .iter()
        .map(|v| bex_value_to_json_impl(v, depth))
        .collect();
    serde_json::Value::Array(arr)
}

/// Convert a single `BexExternalValue` to a JSON value.
///
/// Deep structures are truncated at depth 15 with "..." to prevent stack overflow.
/// This never errors - logging should be robust.
pub fn bex_value_to_json(value: &BexExternalValue) -> serde_json::Value {
    bex_value_to_json_impl(value, 0)
}

fn bex_value_to_json_impl(value: &BexExternalValue, depth: usize) -> serde_json::Value {
    if depth > MAX_SERIALIZATION_DEPTH {
        return serde_json::Value::String("...".into());
    }
    match value {
        BexExternalValue::Null => serde_json::Value::Null,
        BexExternalValue::Bool(b) => serde_json::Value::Bool(*b),
        BexExternalValue::Int(i) => serde_json::json!(i),
        // Bigints can exceed JSON number precision; emit as a decimal string.
        BexExternalValue::Bigint(b) => serde_json::json!(b.to_string()),
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::String(s) => serde_json::Value::String(s.to_string()),
        BexExternalValue::Array { items, .. } => bex_value_vec_to_json_impl(items, depth + 1),
        BexExternalValue::Map { entries, .. } => {
            let mut obj = serde_json::Map::new();
            for (k, v) in entries {
                obj.insert(k.clone(), bex_value_to_json_impl(v, depth + 1));
            }
            serde_json::Value::Object(obj)
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let meta = BamlMeta::class(class_name);
            let mut obj = serde_json::Map::new();
            obj.insert(
                "$baml".into(),
                serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null),
            );
            for (k, v) in fields {
                obj.insert(k.clone(), bex_value_to_json_impl(v, depth + 1));
            }
            serde_json::Value::Object(obj)
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => {
            let meta = BamlMeta::enum_variant(enum_name);
            serde_json::json!({
                "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null),
                "value": variant_name
            })
        }
        BexExternalValue::Union { value, metadata } => {
            let meta = BamlMeta::union(
                format!("{}", metadata.union_type),
                format!("{}", metadata.selected_option),
            );
            let inner = bex_value_to_json_impl(value, depth + 1);
            serde_json::json!({
                "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null),
                "value": inner
            })
        }
        BexExternalValue::Handle(_) => {
            let meta = BamlMeta::handle();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Uint8Array(bytes) => {
            let meta = BamlMeta::uint8array(bytes.len());
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::RustData(_) => {
            let meta = BamlMeta::rust_data();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::FunctionRef { global_index } => {
            let meta = BamlMeta::function_ref(*global_index);
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
            let meta = BamlMeta::collector();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Adt(BexExternalAdt::Type(ty)) => {
            let meta = BamlMeta::type_adt(format!("{ty}"));
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
            let meta = BamlMeta::prompt_ast();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Adt(BexExternalAdt::Media(_)) => {
            let meta = BamlMeta::media();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { .. }) => {
            let meta = BamlMeta::rust_data();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
        BexExternalValue::HostValue(_) => {
            let meta = BamlMeta::handle();
            serde_json::json!({ "$baml": serde_json::to_value(&meta).unwrap_or(serde_json::Value::Null) })
        }
    }
}

/// Render a `BexExternalValue` using the same debug formatting shown in the
/// `typescript2` playground execution panel.
///
/// Examples:
/// - Class: `Person { name: "Alice", age: 30 }`
/// - Array: `[1, 2, 3]`
/// - Map: `{key: "value"}`
/// - Enum: `Status.Ready` (`EnumName.Variant`)
/// - Union: shows the inner value
pub fn bex_value_to_debug_string(value: &BexExternalValue) -> String {
    bex_value_to_debug_impl(value, 0)
}

fn bex_value_to_debug_impl(value: &BexExternalValue, depth: usize) -> String {
    if depth > MAX_SERIALIZATION_DEPTH {
        return "...".to_string();
    }

    match value {
        BexExternalValue::Null => "null".to_string(),
        BexExternalValue::Bool(b) => b.to_string(),
        BexExternalValue::Int(i) => i.to_string(),
        // Display bigint in decimal so logs are LLM- and human-readable;
        // the FFI wire format uses hex (see `bridge_ctypes::value_encode`).
        BexExternalValue::Bigint(bi) => bi.to_string(),
        BexExternalValue::Float(f) => bex_vm_types::format_float(*f),
        BexExternalValue::String(s) => format!("{s:?}"),
        BexExternalValue::Array { items, .. } => {
            if items.is_empty() {
                "[]".to_string()
            } else {
                let inner: Vec<_> = items
                    .iter()
                    .map(|v| bex_value_to_debug_impl(v, depth + 1))
                    .collect();
                format!("[{}]", inner.join(", "))
            }
        }
        BexExternalValue::Map { entries, .. } => {
            if entries.is_empty() {
                "{}".to_string()
            } else {
                let inner: Vec<_> = entries
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", bex_value_to_debug_impl(v, depth + 1)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            let inner: Vec<_> = fields
                .iter()
                .map(|(k, v)| format!("{k}: {}", bex_value_to_debug_impl(v, depth + 1)))
                .collect();
            format!("{class_name} {{ {} }}", inner.join(", "))
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => format!("{enum_name}.{variant_name}"),
        BexExternalValue::Union { value, .. } => bex_value_to_debug_impl(value, depth + 1),
        BexExternalValue::Handle(handle) => format!("<handle #{}>", handle.slab_key()),
        BexExternalValue::Uint8Array(bytes) => format!("<bytes: {}>", bytes.len()),
        BexExternalValue::RustData(_) => "<rust_data>".to_string(),
        BexExternalValue::FunctionRef { global_index } => format!("<fn #{global_index}>"),
        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => "<collector>".to_string(),
        BexExternalValue::Adt(BexExternalAdt::Type(ty)) => format!("<type: {ty}>"),
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => "<prompt_ast>".to_string(),
        BexExternalValue::Adt(BexExternalAdt::Media(media)) => media_to_debug_string(media),
        BexExternalValue::Adt(BexExternalAdt::TaggedHeapHandle { ty, heap_handle }) => {
            format!("<tagged_heap_handle {ty} #{}>", heap_handle.slab_key())
        }
        BexExternalValue::HostValue(hv) => format!("<host_value #{}>", hv.key),
    }
}

fn media_to_debug_string(media: &baml_builtins2::MediaValue) -> String {
    let media_type = match media.kind {
        MediaKind::Image => "image",
        MediaKind::Audio => "audio",
        MediaKind::Pdf => "pdf",
        MediaKind::Video => "video",
        MediaKind::Generic => "other",
    };

    media.read_content(|content| match content {
        MediaContent::Url { url, .. } => format!("<{media_type}: {url}>"),
        MediaContent::File { file, .. } => format!("<{media_type}: file://{file}>"),
        MediaContent::Base64 { .. } => format!("<{media_type}: base64...>"),
    })
}

#[cfg(test)]
mod tests {
    use std::sync::Arc;

    use baml_builtins2::{MediaContent, MediaValue};
    use bex_external_types::{Handle, MediaKind, Ty, WeakHeapRef};
    use indexmap::IndexMap;

    use super::{bex_value_to_debug_string, disk_event_to_jsonl, event_file_header_to_jsonl};
    use crate::{
        DefinitionKey, DiskEventV1, EventFileHeaderV1, FunctionMetadata, FunctionMetadataTable,
        RuntimeFunctionKind, RuntimeFunctionOrigin,
        ids::{BexCallId, BexThreadId, EngineId, FunctionId, ProcessEuid, ProgramId},
    };

    struct NoopHeap;

    impl WeakHeapRef for NoopHeap {
        fn release_handle(&self, _slab_key: usize) {}

        fn resolve_handle_ptr(&self, _slab_key: usize) -> Option<bex_vm_types::HeapPtr> {
            None
        }
    }

    #[test]
    fn test_playground_debug_format_matches_core_shapes() {
        let value = bex_external_types::BexExternalValue::Instance {
            class_name: "Person".into(),
            fields: IndexMap::from([
                (
                    "name".into(),
                    bex_external_types::BexExternalValue::String("Alice\nBob".into()),
                ),
                (
                    "metadata".into(),
                    bex_external_types::BexExternalValue::Map {
                        key_type: Ty::string(),
                        value_type: Ty::int(),
                        entries: IndexMap::from([(
                            "age".into(),
                            bex_external_types::BexExternalValue::Int(30),
                        )]),
                    },
                ),
            ]),
        };

        assert_eq!(
            bex_value_to_debug_string(&value),
            "Person { name: \"Alice\\nBob\", metadata: {age: 30} }"
        );
    }

    #[test]
    fn test_string_debug_escapes_special_characters() {
        // Strings must escape newlines, tabs, carriage returns, and backslashes
        // so that single-line consumers (stderr logs, playground output) do not
        // break across lines or silently drop characters.
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::String(
                "line1\nline2".into()
            )),
            "\"line1\\nline2\""
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::String(
                "tab\there".into()
            )),
            "\"tab\\there\""
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::String(
                "back\\slash".into()
            )),
            "\"back\\\\slash\""
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::String(
                "quote\"inside".into()
            )),
            "\"quote\\\"inside\""
        );
    }

    #[test]
    fn test_playground_debug_format_handles_special_values() {
        let media = MediaValue::new(
            MediaKind::Image,
            MediaContent::Url {
                url: "https://example.com/cat.png".into(),
                base64_data: None,
            },
            None,
        );
        let handle = Handle::new(42, Arc::new(NoopHeap));

        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::Variant {
                enum_name: "Status".into(),
                variant_name: "Ready".into(),
            }),
            "Status.Ready"
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::Handle(handle,)),
            "<handle #42>"
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::Uint8Array(vec![
                1, 2, 3
            ])),
            "<bytes: 3>"
        );
        assert_eq!(
            bex_value_to_debug_string(&bex_external_types::BexExternalValue::Adt(
                bex_external_types::BexExternalAdt::Media(Arc::new(media)),
            )),
            "<image: https://example.com/cat.png>"
        );
    }

    #[test]
    fn disk_set_id_serializes_override_uuid_payload() {
        let json: serde_json::Value = serde_json::from_str(&disk_event_to_jsonl(
            EngineId(3),
            &DiskEventV1::SetId {
                thread_id: BexThreadId(7),
                call_id: BexCallId(8),
                id: [1; 16],
                timestamp_ns: 9,
            },
        ))
        .unwrap();

        assert_eq!(json["type"], "bex_set_id");
        assert_eq!(json["engine_id"], 3);
        assert_eq!(json["thread_id"], 7);
        assert_eq!(json["call_id"], 8);
        assert_eq!(json["id"], "AQEBAQEBAQEBAQEBAQEBAQ");
        assert_eq!(json["timestamp_ns"], 9);
    }

    #[test]
    fn event_file_header_serializes_scoping_and_function_table() {
        let json: serde_json::Value =
            serde_json::from_str(&event_file_header_to_jsonl(&EventFileHeaderV1 {
                process_euid: ProcessEuid([1; 16]),
                engine_id: EngineId(2),
                program_id: ProgramId([3; 16]),
                source_snapshot_id: None,
                revision_id: None,
                started_at_epoch_ns: 4,
                function_table: FunctionMetadataTable {
                    functions: vec![FunctionMetadata {
                        function_id: FunctionId(5),
                        fqn: "user.main".to_string(),
                        display_name: "main".to_string(),
                        source_file: Some("main.baml".to_string()),
                        source_span: None,
                        kind: RuntimeFunctionKind::Bytecode,
                        origin: RuntimeFunctionOrigin::UserDefined,
                        owner_type: None,
                        parent_function: None,
                        lambda_path: None,
                        definition_key: Some(DefinitionKey("function:user.main".to_string())),
                        package_name: Some("user".to_string()),
                        namespace: Vec::new(),
                        source_snapshot_id: None,
                        revision_id: None,
                        semantic_lanes: None,
                    }],
                },
            }))
            .unwrap();

        assert_eq!(json["type"], "bex_header_v1");
        assert_eq!(json["process_euid"], "AQEBAQEBAQEBAQEBAQEBAQ");
        assert_eq!(json["engine_id"], 2);
        assert_eq!(json["program_id"], "AwMDAwMDAwMDAwMDAwMDAw");
        assert_eq!(json["started_at_epoch_ns"], "4");
        assert_eq!(json["function_table"][0]["function_id"], 5);
        assert_eq!(json["function_table"][0]["fqn"], "user.main");
        assert_eq!(
            json["function_table"][0]["definition_key"],
            "function:user.main"
        );
    }

    /// T33: per-variant JSONL shape pins — exact key sets and `type` strings
    /// are wire contract for interim consumers; a key typo here ships
    /// silently otherwise.
    #[test]
    fn disk_event_jsonl_shapes_are_pinned_per_variant() {
        use crate::{FunctionEndStatus, ThreadEndStatus};
        fn keys(value: &serde_json::Value) -> Vec<String> {
            let mut k: Vec<String> = value.as_object().unwrap().keys().cloned().collect();
            k.sort();
            k
        }

        let engine = EngineId(5);

        let start_thread = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::StartThread {
                thread_id: BexThreadId(1),
                parent_thread_id: Some(BexThreadId(2)),
                parent_call_id: Some(BexCallId(3)),
                name: Some("worker".to_string()),
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert_eq!(start_thread["type"], "bex_start_thread");
        assert_eq!(
            keys(&start_thread),
            [
                "engine_id",
                "name",
                "parent_call_id",
                "parent_thread_id",
                "thread_id",
                "timestamp_ns",
                "type"
            ]
        );
        assert_eq!(start_thread["parent_thread_id"], 2);
        assert_eq!(start_thread["parent_call_id"], 3);

        // Option fields serialize as null, not as absent keys.
        let root_start = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::StartThread {
                thread_id: BexThreadId(1),
                parent_thread_id: None,
                parent_call_id: None,
                name: None,
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert!(root_start["parent_thread_id"].is_null());
        assert!(root_start["parent_call_id"].is_null());
        assert!(root_start["name"].is_null());

        let call_function = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::CallFunction {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                parent_call_id: Some(BexCallId(1)),
                function_id: FunctionId(7),
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert_eq!(call_function["type"], "bex_call_function");
        assert_eq!(
            keys(&call_function),
            [
                "call_id",
                "engine_id",
                "function_id",
                "parent_call_id",
                "thread_id",
                "timestamp_ns",
                "type"
            ]
        );
        assert_eq!(call_function["function_id"], 7);

        let end_function = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::EndFunction {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                status: FunctionEndStatus::Ok,
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert_eq!(end_function["type"], "bex_end_function");
        assert_eq!(
            keys(&end_function),
            [
                "call_id",
                "engine_id",
                "status",
                "thread_id",
                "timestamp_ns",
                "type"
            ]
        );

        let end_thread = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::EndThread {
                thread_id: BexThreadId(1),
                status: ThreadEndStatus::Completed,
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert_eq!(end_thread["type"], "bex_end_thread");
        assert_eq!(
            keys(&end_thread),
            ["engine_id", "status", "thread_id", "timestamp_ns", "type"]
        );

        let heartbeat = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::Heartbeat { timestamp_ns: 4 },
        ))
        .unwrap();
        assert_eq!(heartbeat["type"], "bex_heartbeat");
        assert_eq!(keys(&heartbeat), ["engine_id", "timestamp_ns", "type"]);

        let set_id = serde_json::from_str::<serde_json::Value>(&disk_event_to_jsonl(
            engine,
            &DiskEventV1::SetId {
                thread_id: BexThreadId(1),
                call_id: BexCallId(2),
                id: [1; 16],
                timestamp_ns: 4,
            },
        ))
        .unwrap();
        assert_eq!(set_id["type"], "bex_set_id");
        assert_eq!(
            keys(&set_id),
            [
                "call_id",
                "engine_id",
                "id",
                "thread_id",
                "timestamp_ns",
                "type"
            ]
        );
    }

    /// T33 (statuses): the status strings are wire contract.
    #[test]
    fn status_strings_are_wire_contract() {
        use crate::{FunctionEndStatus, ThreadEndStatus};
        assert_eq!(super::function_end_status(&FunctionEndStatus::Ok), "ok");
        assert_eq!(
            super::function_end_status(&FunctionEndStatus::Error),
            "error"
        );
        assert_eq!(
            super::function_end_status(&FunctionEndStatus::Cancelled),
            "cancelled"
        );
        assert_eq!(
            super::thread_end_status(&ThreadEndStatus::Completed),
            "completed"
        );
        assert_eq!(super::thread_end_status(&ThreadEndStatus::Error), "error");
        assert_eq!(
            super::thread_end_status(&ThreadEndStatus::Cancelled),
            "cancelled"
        );
    }
}
