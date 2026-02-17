//! JSONL serialization for `RuntimeEvent`.

use bex_external_types::{BexExternalAdt, BexExternalValue};

use crate::{EventKind, FunctionEvent, RuntimeEvent};

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
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);

    let content = match &event.event {
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
    };

    let event_json = serde_json::json!({
        "call_id": call_id,
        "function_event_id": function_event_id,
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

/// Convert a Vec<BexExternalValue> to a JSON value.
fn bex_value_vec_to_json(values: &[BexExternalValue]) -> serde_json::Value {
    serde_json::Value::Array(values.iter().map(bex_value_to_json).collect())
}

/// Convert a single `BexExternalValue` to a JSON value.
fn bex_value_to_json(value: &BexExternalValue) -> serde_json::Value {
    match value {
        BexExternalValue::Null => serde_json::Value::Null,
        BexExternalValue::Bool(b) => serde_json::Value::Bool(*b),
        BexExternalValue::Int(i) => serde_json::json!(i),
        BexExternalValue::Float(f) => serde_json::json!(f),
        BexExternalValue::String(s) => serde_json::Value::String(s.clone()),
        BexExternalValue::Array { items, .. } => bex_value_vec_to_json(items),
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
        BexExternalValue::Resource(_) => serde_json::Value::String("<resource>".into()),
        BexExternalValue::FunctionRef { global_index } => {
            serde_json::json!({"__function_ref": global_index})
        }
        BexExternalValue::Adt(BexExternalAdt::Media(_)) => {
            serde_json::json!({"__adt": "Media"})
        }
        BexExternalValue::Adt(BexExternalAdt::PromptAst(_)) => {
            serde_json::json!({"__adt": "PromptAst"})
        }
        BexExternalValue::Adt(BexExternalAdt::Collector(_)) => {
            serde_json::json!({"__adt": "Collector"})
        }
    }
}

#[cfg(test)]
mod tests {
    use std::time::SystemTime;

    use super::*;
    use crate::{FunctionStart, SpanContext, SpanId};

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
}
