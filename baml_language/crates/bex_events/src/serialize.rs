//! JSONL serialization for `RuntimeEvent`.

use bex_external_types::{BexExternalAdt, BexExternalValue};
use serde::Serialize;

use crate::{CustomEvent, EventKind, FunctionEvent, LogEvent, RuntimeEvent};

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

/// Convert a Vec<BexExternalValue> to a JSON array.
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
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::String(s) => serde_json::Value::String(s.clone()),
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
    }
}

/// Render a `BexExternalValue` as a Rust Debug-style string.
///
/// Examples:
/// - Class: `Person { name: "Alice", age: 30 }`
/// - Array: `[1, 2, 3]`
/// - Map: `{"key": "value"}`
/// - Enum: `Status::Active`
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
        BexExternalValue::Float(f) => {
            if f.fract() == 0.0 {
                format!("{f:.1}")
            } else {
                f.to_string()
            }
        }
        BexExternalValue::String(s) => format!("\"{s}\""),
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
                    .map(|(k, v)| format!("\"{k}\": {}", bex_value_to_debug_impl(v, depth + 1)))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
        }
        BexExternalValue::Instance {
            class_name, fields, ..
        } => {
            if fields.is_empty() {
                format!("{class_name} {{}}")
            } else {
                let inner: Vec<_> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", bex_value_to_debug_impl(v, depth + 1)))
                    .collect();
                format!("{class_name} {{ {} }}", inner.join(", "))
            }
        }
        BexExternalValue::Variant {
            enum_name,
            variant_name,
        } => format!("{enum_name}::{variant_name}"),
        BexExternalValue::Union { value, .. } => bex_value_to_debug_impl(value, depth + 1),
        BexExternalValue::Handle(_) => "<handle>".to_string(),
        BexExternalValue::Uint8Array(bytes) => format!("<bytes[{}]>", bytes.len()),
        BexExternalValue::RustData(_) => "<rust_data>".to_string(),
        BexExternalValue::FunctionRef { global_index } => format!("<fn@{global_index}>"),
        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => "<Collector>".to_string(),
        BexExternalValue::Adt(BexExternalAdt::Type(ty)) => format!("<Type({ty})>"),
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => "<PromptAst>".to_string(),
        BexExternalValue::Adt(BexExternalAdt::Media(_)) => "<Media>".to_string(),
    }
}

#[cfg(test)]
mod tests {
    use bex_external_types::{Ty, TyAttr, UnionMetadata};
    use indexmap::IndexMap;
    use web_time::SystemTime;

    use super::*;
    use crate::{CustomEvent, FunctionStart, LogEvent, SpanContext, SpanId};

    #[test]
    fn test_serialize_log_event() {
        let span_id = SpanId::new();
        let root_id = span_id.clone();
        let parent_id = SpanId::new();
        let event = RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: Some(parent_id.clone()),
                root_span_id: root_id.clone(),
            },
            call_stack: vec![span_id.clone()],
            timestamp: SystemTime::now(),
            event: EventKind::Log(LogEvent {
                level: "info".into(),
                data: BexExternalValue::String("hello world".into()),
                source: Some(crate::SourceLocation {
                    file_id: 1,
                    line: 42,
                    column: 8,
                    start_offset: 100,
                    end_offset: 120,
                }),
            }),
        };

        let jsonl = event_to_jsonl(&event);
        let parsed: serde_json::Value = serde_json::from_str(&jsonl).unwrap();

        assert_eq!(parsed["content"]["type"], "log");
        assert_eq!(parsed["content"]["data"]["level"], "info");
        assert_eq!(parsed["content"]["data"]["data"], "hello world");
        assert_eq!(parsed["content"]["data"]["source"]["file_id"], 1);
        assert_eq!(parsed["content"]["data"]["source"]["line"], 42);
        assert_eq!(parsed["content"]["data"]["source"]["column"], 8);
        assert_eq!(parsed["call_id"], span_id.to_string());
        assert_eq!(parsed["parent_span_id"], parent_id.to_string());
        assert_eq!(parsed["root_span_id"], root_id.to_string());
    }

    #[test]
    fn test_serialize_custom_event() {
        let span_id = SpanId::new();
        let root_id = span_id.clone();
        let event = RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: root_id,
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Custom(CustomEvent {
                name: "user_clicked".into(),
                data: BexExternalValue::String("payload".into()),
            }),
        };

        let jsonl = event_to_jsonl(&event);
        let parsed: serde_json::Value = serde_json::from_str(&jsonl).unwrap();
        assert_eq!(parsed["content"]["type"], "custom");
        assert_eq!(parsed["content"]["data"]["name"], "user_clicked");
        assert_eq!(parsed["content"]["data"]["data"], "payload");
    }

    #[test]
    fn test_serialize_function_start() {
        let span_id = SpanId::new();
        let root_id = span_id.clone();
        let event = RuntimeEvent {
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: root_id,
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: "my_func".into(),
                args: vec![BexExternalValue::Int(42)],
                tags: vec![],
            })),
        };

        let jsonl = event_to_jsonl(&event);
        let parsed: serde_json::Value = serde_json::from_str(&jsonl).unwrap();

        assert!(parsed["call_id"].is_string());
        assert!(parsed["function_event_id"].is_string());
        assert!(parsed["call_stack"].is_array());
        assert_eq!(parsed["content"]["type"], "function_start");
        assert_eq!(
            parsed["content"]["data"]["function_display_name"],
            "my_func"
        );
        assert_eq!(parsed["content"]["data"]["args"][0], 42);
    }

    // === Complex Type Serialization Tests ===

    #[test]
    fn test_serialize_class_instance() {
        let mut fields = IndexMap::new();
        fields.insert("name".into(), BexExternalValue::String("Alice".into()));
        fields.insert("age".into(), BexExternalValue::Int(30));

        let value = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "Person");
        assert_eq!(json["name"], "Alice");
        assert_eq!(json["age"], 30);
    }

    #[test]
    fn test_serialize_nested_class() {
        let mut address_fields = IndexMap::new();
        address_fields.insert("city".into(), BexExternalValue::String("NYC".into()));
        address_fields.insert("zip".into(), BexExternalValue::String("10001".into()));

        let address = BexExternalValue::Instance {
            class_name: "Address".into(),
            fields: address_fields,
        };

        let mut person_fields = IndexMap::new();
        person_fields.insert("name".into(), BexExternalValue::String("Bob".into()));
        person_fields.insert("address".into(), address);

        let value = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields: person_fields,
        };

        let json = bex_value_to_json(&value);
        println!("{}", serde_json::to_string_pretty(&json).unwrap());
        assert_eq!(json["$baml"]["type"], "Person");
        assert_eq!(json["name"], "Bob");
        assert_eq!(json["address"]["$baml"]["type"], "Address");
        assert_eq!(json["address"]["city"], "NYC");
    }

    #[test]
    fn test_serialize_recursive_type_list() {
        let leaf = BexExternalValue::Instance {
            class_name: "TreeNode".into(),
            fields: {
                let mut f = IndexMap::new();
                f.insert("value".into(), BexExternalValue::Int(1));
                f.insert(
                    "children".into(),
                    BexExternalValue::Array {
                        element_type: Ty::class("TreeNode"),
                        items: vec![],
                    },
                );
                f
            },
        };

        let parent = BexExternalValue::Instance {
            class_name: "TreeNode".into(),
            fields: {
                let mut f = IndexMap::new();
                f.insert("value".into(), BexExternalValue::Int(0));
                f.insert(
                    "children".into(),
                    BexExternalValue::Array {
                        element_type: Ty::class("TreeNode"),
                        items: vec![leaf],
                    },
                );
                f
            },
        };

        let json = bex_value_to_json(&parent);
        assert_eq!(json["$baml"]["type"], "TreeNode");
        assert_eq!(json["value"], 0);
        assert_eq!(json["children"][0]["$baml"]["type"], "TreeNode");
        assert_eq!(json["children"][0]["value"], 1);
    }

    #[test]
    fn test_serialize_deep_structure_truncates() {
        fn make_nested(depth: usize) -> BexExternalValue {
            if depth == 0 {
                BexExternalValue::Int(42)
            } else {
                BexExternalValue::Array {
                    element_type: Ty::int(),
                    items: vec![make_nested(depth - 1)],
                }
            }
        }

        let deep = make_nested(60);
        let json = bex_value_to_json(&deep);

        fn find_truncation(val: &serde_json::Value) -> bool {
            match val {
                serde_json::Value::String(s) if s == "..." => true,
                serde_json::Value::Array(arr) => arr.iter().any(find_truncation),
                _ => false,
            }
        }

        assert!(
            find_truncation(&json),
            "Expected deep structure to be truncated with '...'"
        );
    }

    #[test]
    fn test_serialize_within_depth_limit() {
        fn make_nested(depth: usize) -> BexExternalValue {
            if depth == 0 {
                BexExternalValue::Int(42)
            } else {
                BexExternalValue::Array {
                    element_type: Ty::int(),
                    items: vec![make_nested(depth - 1)],
                }
            }
        }

        let shallow = make_nested(10);
        let json = bex_value_to_json(&shallow);

        fn find_deepest_int(val: &serde_json::Value) -> Option<i64> {
            match val {
                serde_json::Value::Number(n) => n.as_i64(),
                serde_json::Value::Array(arr) => arr.iter().find_map(find_deepest_int),
                _ => None,
            }
        }

        assert_eq!(
            find_deepest_int(&json),
            Some(42),
            "Should find the leaf value 42 within depth limit"
        );
    }

    #[test]
    fn test_serialize_map_of_primitives() {
        let mut entries = IndexMap::new();
        entries.insert("one".into(), BexExternalValue::Int(1));
        entries.insert("two".into(), BexExternalValue::Int(2));
        entries.insert("pi".into(), BexExternalValue::Float(3.14));

        let value = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::Union(vec![Ty::int(), Ty::float()], TyAttr::default()),
            entries,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["one"], 1);
        assert_eq!(json["two"], 2);
        assert_eq!(json["pi"], 3.14);
    }

    #[test]
    fn test_serialize_map_of_classes() {
        let mut entries = IndexMap::new();

        let mut alice_fields = IndexMap::new();
        alice_fields.insert("age".into(), BexExternalValue::Int(30));
        entries.insert(
            "alice".into(),
            BexExternalValue::Instance {
                class_name: "Person".into(),
                fields: alice_fields,
            },
        );

        let mut bob_fields = IndexMap::new();
        bob_fields.insert("age".into(), BexExternalValue::Int(25));
        entries.insert(
            "bob".into(),
            BexExternalValue::Instance {
                class_name: "Person".into(),
                fields: bob_fields,
            },
        );

        let value = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::class("Person"),
            entries,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["alice"]["$baml"]["type"], "Person");
        assert_eq!(json["alice"]["age"], 30);
        assert_eq!(json["bob"]["$baml"]["type"], "Person");
        assert_eq!(json["bob"]["age"], 25);
    }

    #[test]
    fn test_serialize_list_of_classes() {
        let items: Vec<BexExternalValue> = (0..3)
            .map(|i| {
                let mut fields = IndexMap::new();
                fields.insert("id".into(), BexExternalValue::Int(i));
                BexExternalValue::Instance {
                    class_name: "Item".into(),
                    fields,
                }
            })
            .collect();

        let value = BexExternalValue::Array {
            element_type: Ty::class("Item"),
            items,
        };

        let json = bex_value_to_json(&value);
        assert!(json.is_array());
        assert_eq!(json[0]["$baml"]["type"], "Item");
        assert_eq!(json[0]["id"], 0);
        assert_eq!(json[2]["id"], 2);
    }

    #[test]
    fn test_serialize_class_with_map_field() {
        let mut scores = IndexMap::new();
        scores.insert("math".into(), BexExternalValue::Int(95));
        scores.insert("english".into(), BexExternalValue::Int(88));

        let mut fields = IndexMap::new();
        fields.insert("name".into(), BexExternalValue::String("Student".into()));
        fields.insert(
            "scores".into(),
            BexExternalValue::Map {
                key_type: Ty::string(),
                value_type: Ty::int(),
                entries: scores,
            },
        );

        let value = BexExternalValue::Instance {
            class_name: "Student".into(),
            fields,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "Student");
        assert_eq!(json["scores"]["math"], 95);
        assert_eq!(json["scores"]["english"], 88);
    }

    #[test]
    fn test_serialize_class_with_list_field() {
        let tags = BexExternalValue::Array {
            element_type: Ty::string(),
            items: vec![
                BexExternalValue::String("rust".into()),
                BexExternalValue::String("baml".into()),
            ],
        };

        let mut fields = IndexMap::new();
        fields.insert("title".into(), BexExternalValue::String("Article".into()));
        fields.insert("tags".into(), tags);

        let value = BexExternalValue::Instance {
            class_name: "BlogPost".into(),
            fields,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "BlogPost");
        assert_eq!(json["tags"][0], "rust");
        assert_eq!(json["tags"][1], "baml");
    }

    // === Union Serialization Tests ===

    #[test]
    fn test_serialize_union_with_int() {
        let union_type = Ty::Union(vec![Ty::int(), Ty::class("MyClass")], TyAttr::default());
        let selected = Ty::int();

        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Int(42)),
            metadata: UnionMetadata::new(union_type, selected),
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "$union");
        assert!(json["$baml"]["union"].as_str().is_some());
        assert!(json["$baml"]["selected"].as_str().is_some());
        assert_eq!(json["value"], 42);
    }

    #[test]
    fn test_serialize_union_with_class() {
        let union_type = Ty::Union(vec![Ty::int(), Ty::class("MyClass")], TyAttr::default());
        let selected = Ty::class("MyClass");

        let mut fields = IndexMap::new();
        fields.insert("data".into(), BexExternalValue::String("hello".into()));

        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Instance {
                class_name: "MyClass".into(),
                fields,
            }),
            metadata: UnionMetadata::new(union_type, selected),
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "$union");
        assert!(json["$baml"]["union"].as_str().is_some());
        assert!(json["$baml"]["selected"].as_str().is_some());
        assert_eq!(json["value"]["$baml"]["type"], "MyClass");
        assert_eq!(json["value"]["data"], "hello");
    }

    #[test]
    fn test_serialize_optional_with_value() {
        let optional_type = Ty::Optional(Box::new(Ty::string()), TyAttr::default());
        let selected = Ty::string();

        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::String("present".into())),
            metadata: UnionMetadata::new(optional_type, selected),
        };

        let json = bex_value_to_json(&value);
        assert!(json["$baml"]["union"].as_str().unwrap().contains("string"));
        assert_eq!(json["value"], "present");
    }

    #[test]
    fn test_serialize_optional_with_null() {
        let optional_type = Ty::Optional(Box::new(Ty::string()), TyAttr::default());
        let selected = Ty::null();

        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Null),
            metadata: UnionMetadata::new(optional_type, selected),
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "$union");
        assert!(json["value"].is_null());
    }

    #[test]
    fn test_serialize_nested_union_in_class() {
        let union_type = Ty::Union(vec![Ty::int(), Ty::string()], TyAttr::default());
        let selected = Ty::string();

        let union_value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::String("nested".into())),
            metadata: UnionMetadata::new(union_type, selected),
        };

        let mut fields = IndexMap::new();
        fields.insert("data".into(), union_value);

        let value = BexExternalValue::Instance {
            class_name: "Container".into(),
            fields,
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "Container");
        assert_eq!(json["data"]["$baml"]["type"], "$union");
        assert_eq!(json["data"]["value"], "nested");
    }

    #[test]
    fn test_serialize_enum_variant() {
        let value = BexExternalValue::Variant {
            enum_name: "Status".into(),
            variant_name: "Active".into(),
        };

        let json = bex_value_to_json(&value);
        assert_eq!(json["$baml"]["type"], "$enum");
        assert_eq!(json["$baml"]["enum"], "Status");
        assert_eq!(json["value"], "Active");
    }

    #[test]
    fn test_serialize_list_of_unions() {
        let union_type = Ty::Union(vec![Ty::int(), Ty::string()], TyAttr::default());

        let items = vec![
            BexExternalValue::Union {
                value: Box::new(BexExternalValue::Int(1)),
                metadata: UnionMetadata::new(union_type.clone(), Ty::int()),
            },
            BexExternalValue::Union {
                value: Box::new(BexExternalValue::String("two".into())),
                metadata: UnionMetadata::new(union_type.clone(), Ty::string()),
            },
            BexExternalValue::Union {
                value: Box::new(BexExternalValue::Int(3)),
                metadata: UnionMetadata::new(union_type.clone(), Ty::int()),
            },
        ];

        let value = BexExternalValue::Array {
            element_type: union_type,
            items,
        };

        let json = bex_value_to_json(&value);
        assert!(json.is_array());
        assert_eq!(json[0]["value"], 1);
        assert_eq!(json[1]["value"], "two");
        assert_eq!(json[2]["value"], 3);
    }

    /// Run with: cargo test -p bex_events test_print_serialization_examples -- --nocapture
    #[test]
    fn test_print_serialization_examples() {
        println!("\n\n========== SERIALIZATION EXAMPLES ==========\n");

        println!("=== Class Instance ===");
        let mut fields = IndexMap::new();
        fields.insert("name".into(), BexExternalValue::String("Alice".into()));
        fields.insert("age".into(), BexExternalValue::Int(30));
        let value = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields,
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Nested Class ===");
        let mut address_fields = IndexMap::new();
        address_fields.insert("city".into(), BexExternalValue::String("NYC".into()));
        let address = BexExternalValue::Instance {
            class_name: "Address".into(),
            fields: address_fields,
        };
        let mut person_fields = IndexMap::new();
        person_fields.insert("name".into(), BexExternalValue::String("Bob".into()));
        person_fields.insert("address".into(), address);
        let value = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields: person_fields,
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Union with Int (int | MyClass -> int) ===");
        let union_type = Ty::Union(vec![Ty::int(), Ty::class("MyClass")], TyAttr::default());
        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Int(42)),
            metadata: UnionMetadata::new(union_type, Ty::int()),
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Union with Class (int | MyClass -> MyClass) ===");
        let union_type = Ty::Union(vec![Ty::int(), Ty::class("MyClass")], TyAttr::default());
        let mut fields = IndexMap::new();
        fields.insert("data".into(), BexExternalValue::String("hello".into()));
        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Instance {
                class_name: "MyClass".into(),
                fields,
            }),
            metadata: UnionMetadata::new(union_type, Ty::class("MyClass")),
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Optional with Value (string? -> string) ===");
        let optional_type = Ty::Optional(Box::new(Ty::string()), TyAttr::default());
        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::String("present".into())),
            metadata: UnionMetadata::new(optional_type, Ty::string()),
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Optional with Null (string? -> null) ===");
        let optional_type = Ty::Optional(Box::new(Ty::string()), TyAttr::default());
        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Null),
            metadata: UnionMetadata::new(optional_type, Ty::null()),
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Recursive TreeNode ===");
        let leaf = BexExternalValue::Instance {
            class_name: "TreeNode".into(),
            fields: {
                let mut f = IndexMap::new();
                f.insert("value".into(), BexExternalValue::Int(1));
                f.insert(
                    "children".into(),
                    BexExternalValue::Array {
                        element_type: Ty::class("TreeNode"),
                        items: vec![],
                    },
                );
                f
            },
        };
        let parent = BexExternalValue::Instance {
            class_name: "TreeNode".into(),
            fields: {
                let mut f = IndexMap::new();
                f.insert("value".into(), BexExternalValue::Int(0));
                f.insert(
                    "children".into(),
                    BexExternalValue::Array {
                        element_type: Ty::class("TreeNode"),
                        items: vec![leaf],
                    },
                );
                f
            },
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&parent)).unwrap()
        );

        println!("=== Deep Structure (60 levels, truncated at depth 15) ===");
        fn make_nested(depth: usize) -> BexExternalValue {
            if depth == 0 {
                BexExternalValue::Int(42)
            } else {
                BexExternalValue::Array {
                    element_type: Ty::int(),
                    items: vec![make_nested(depth - 1)],
                }
            }
        }
        let deep = make_nested(60);
        let json = bex_value_to_json(&deep);
        println!("{}\n", serde_json::to_string_pretty(&json).unwrap());

        println!("=== Map of Classes ===");
        let mut entries = IndexMap::new();
        let mut alice_fields = IndexMap::new();
        alice_fields.insert("age".into(), BexExternalValue::Int(30));
        entries.insert(
            "alice".into(),
            BexExternalValue::Instance {
                class_name: "Person".into(),
                fields: alice_fields,
            },
        );
        let value = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::class("Person"),
            entries,
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== Enum Variant ===");
        let value = BexExternalValue::Variant {
            enum_name: "Status".into(),
            variant_name: "Active".into(),
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("=== List of Unions ===");
        let union_type = Ty::Union(vec![Ty::int(), Ty::string()], TyAttr::default());
        let items = vec![
            BexExternalValue::Union {
                value: Box::new(BexExternalValue::Int(1)),
                metadata: UnionMetadata::new(union_type.clone(), Ty::int()),
            },
            BexExternalValue::Union {
                value: Box::new(BexExternalValue::String("two".into())),
                metadata: UnionMetadata::new(union_type.clone(), Ty::string()),
            },
        ];
        let value = BexExternalValue::Array {
            element_type: union_type,
            items,
        };
        println!(
            "{}\n",
            serde_json::to_string_pretty(&bex_value_to_json(&value)).unwrap()
        );

        println!("==========================================\n");
    }

    // === Debug String Tests ===

    #[test]
    fn test_debug_string_primitives() {
        assert_eq!(bex_value_to_debug_string(&BexExternalValue::Null), "null");
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::Bool(true)),
            "true"
        );
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::Bool(false)),
            "false"
        );
        assert_eq!(bex_value_to_debug_string(&BexExternalValue::Int(42)), "42");
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::Int(-123)),
            "-123"
        );
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::Float(3.14)),
            "3.14"
        );
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::Float(5.0)),
            "5.0"
        );
        assert_eq!(
            bex_value_to_debug_string(&BexExternalValue::String("hello".into())),
            "\"hello\""
        );
    }

    #[test]
    fn test_debug_string_array() {
        let empty = BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![],
        };
        assert_eq!(bex_value_to_debug_string(&empty), "[]");

        let nums = BexExternalValue::Array {
            element_type: Ty::int(),
            items: vec![
                BexExternalValue::Int(1),
                BexExternalValue::Int(2),
                BexExternalValue::Int(3),
            ],
        };
        assert_eq!(bex_value_to_debug_string(&nums), "[1, 2, 3]");
    }

    #[test]
    fn test_debug_string_map() {
        let empty = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::int(),
            entries: IndexMap::new(),
        };
        assert_eq!(bex_value_to_debug_string(&empty), "{}");

        let mut entries = IndexMap::new();
        entries.insert("a".into(), BexExternalValue::Int(1));
        entries.insert("b".into(), BexExternalValue::Int(2));
        let map = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::int(),
            entries,
        };
        assert_eq!(bex_value_to_debug_string(&map), "{\"a\": 1, \"b\": 2}");
    }

    #[test]
    fn test_debug_string_class() {
        let mut fields = IndexMap::new();
        fields.insert("name".into(), BexExternalValue::String("Alice".into()));
        fields.insert("age".into(), BexExternalValue::Int(30));
        let person = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields,
        };
        assert_eq!(
            bex_value_to_debug_string(&person),
            "Person { name: \"Alice\", age: 30 }"
        );
    }

    #[test]
    fn test_debug_string_empty_class() {
        let empty = BexExternalValue::Instance {
            class_name: "Empty".into(),
            fields: IndexMap::new(),
        };
        assert_eq!(bex_value_to_debug_string(&empty), "Empty {}");
    }

    #[test]
    fn test_debug_string_nested_class() {
        let mut address_fields = IndexMap::new();
        address_fields.insert("city".into(), BexExternalValue::String("NYC".into()));
        let address = BexExternalValue::Instance {
            class_name: "Address".into(),
            fields: address_fields,
        };

        let mut person_fields = IndexMap::new();
        person_fields.insert("name".into(), BexExternalValue::String("Bob".into()));
        person_fields.insert("address".into(), address);
        let person = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields: person_fields,
        };
        assert_eq!(
            bex_value_to_debug_string(&person),
            "Person { name: \"Bob\", address: Address { city: \"NYC\" } }"
        );
    }

    #[test]
    fn test_debug_string_enum() {
        let variant = BexExternalValue::Variant {
            enum_name: "Status".into(),
            variant_name: "Active".into(),
        };
        assert_eq!(bex_value_to_debug_string(&variant), "Status::Active");
    }

    #[test]
    fn test_debug_string_union() {
        let union_type = Ty::Union(vec![Ty::int(), Ty::string()], TyAttr::default());
        let value = BexExternalValue::Union {
            value: Box::new(BexExternalValue::Int(42)),
            metadata: UnionMetadata::new(union_type, Ty::int()),
        };
        assert_eq!(bex_value_to_debug_string(&value), "42");
    }

    #[test]
    fn test_debug_string_array_of_classes() {
        let items: Vec<BexExternalValue> = (0..2)
            .map(|i| {
                let mut fields = IndexMap::new();
                fields.insert("id".into(), BexExternalValue::Int(i));
                BexExternalValue::Instance {
                    class_name: "Item".into(),
                    fields,
                }
            })
            .collect();

        let value = BexExternalValue::Array {
            element_type: Ty::class("Item"),
            items,
        };
        assert_eq!(
            bex_value_to_debug_string(&value),
            "[Item { id: 0 }, Item { id: 1 }]"
        );
    }

    #[test]
    fn test_debug_string_deep_truncation() {
        fn make_nested(depth: usize) -> BexExternalValue {
            if depth == 0 {
                BexExternalValue::Int(42)
            } else {
                BexExternalValue::Array {
                    element_type: Ty::int(),
                    items: vec![make_nested(depth - 1)],
                }
            }
        }

        let deep = make_nested(20);
        let result = bex_value_to_debug_string(&deep);
        assert!(result.contains("..."), "Should truncate deep structures");
    }

    /// Run with: cargo test -p bex_events test_print_debug_string_examples -- --nocapture
    #[test]
    fn test_print_debug_string_examples() {
        println!("\n\n========== DEBUG STRING EXAMPLES ==========\n");

        println!("=== Primitives ===");
        println!(
            "null: {}",
            bex_value_to_debug_string(&BexExternalValue::Null)
        );
        println!(
            "bool: {}",
            bex_value_to_debug_string(&BexExternalValue::Bool(true))
        );
        println!(
            "int: {}",
            bex_value_to_debug_string(&BexExternalValue::Int(42))
        );
        println!(
            "float: {}",
            bex_value_to_debug_string(&BexExternalValue::Float(3.14))
        );
        println!(
            "string: {}",
            bex_value_to_debug_string(&BexExternalValue::String("hello".into()))
        );
        println!();

        println!("=== Class Instance ===");
        let mut fields = IndexMap::new();
        fields.insert("name".into(), BexExternalValue::String("Alice".into()));
        fields.insert("age".into(), BexExternalValue::Int(30));
        let person = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields,
        };
        println!("{}\n", bex_value_to_debug_string(&person));

        println!("=== Nested Class ===");
        let mut address_fields = IndexMap::new();
        address_fields.insert("city".into(), BexExternalValue::String("NYC".into()));
        address_fields.insert("zip".into(), BexExternalValue::String("10001".into()));
        let address = BexExternalValue::Instance {
            class_name: "Address".into(),
            fields: address_fields,
        };
        let mut person_fields = IndexMap::new();
        person_fields.insert("name".into(), BexExternalValue::String("Bob".into()));
        person_fields.insert("address".into(), address);
        let person = BexExternalValue::Instance {
            class_name: "Person".into(),
            fields: person_fields,
        };
        println!("{}\n", bex_value_to_debug_string(&person));

        println!("=== Enum Variant ===");
        let variant = BexExternalValue::Variant {
            enum_name: "Color".into(),
            variant_name: "Red".into(),
        };
        println!("{}\n", bex_value_to_debug_string(&variant));

        println!("=== Array of Classes ===");
        let items: Vec<BexExternalValue> = (0..3)
            .map(|i| {
                let mut fields = IndexMap::new();
                fields.insert("id".into(), BexExternalValue::Int(i));
                fields.insert("name".into(), BexExternalValue::String(format!("item{i}")));
                BexExternalValue::Instance {
                    class_name: "Item".into(),
                    fields,
                }
            })
            .collect();
        let array = BexExternalValue::Array {
            element_type: Ty::class("Item"),
            items,
        };
        println!("{}\n", bex_value_to_debug_string(&array));

        println!("=== Map ===");
        let mut entries = IndexMap::new();
        entries.insert("one".into(), BexExternalValue::Int(1));
        entries.insert("two".into(), BexExternalValue::Int(2));
        let map = BexExternalValue::Map {
            key_type: Ty::string(),
            value_type: Ty::int(),
            entries,
        };
        println!("{}\n", bex_value_to_debug_string(&map));

        println!("==========================================\n");
    }
}
