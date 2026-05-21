//! `bex_events::RuntimeEvent` -> protobuf `RuntimeEvent` conversion.

use bex_events::{CustomEvent, EventKind, FunctionEvent, LogEvent, RuntimeEvent};

use crate::{
    CffiHandleTableOptions,
    baml_core::cffi::{
        self, EventKind as ProtoEventKind, FunctionEndEvent, FunctionStartEvent,
        RuntimeEvent as ProtoRuntimeEvent, SetTagsEvent, TagEntry,
        event_kind::Kind as ProtoEventKindVariant,
    },
    error::CtypesError,
    value_encode::external_to_outbound,
};

/// Convert a `bex_events::RuntimeEvent` to protobuf `RuntimeEvent`.
pub fn runtime_event_to_proto(
    event: &RuntimeEvent,
    options: &CffiHandleTableOptions,
) -> Result<ProtoRuntimeEvent, CtypesError> {
    let timestamp_ms = event
        .timestamp
        .duration_since(web_time::UNIX_EPOCH)
        .map(|d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX))
        .unwrap_or(0);

    let call_stack: Vec<String> = event.call_stack.iter().map(ToString::to_string).collect();

    let event_kind = event_kind_to_proto(&event.event, options)?;

    Ok(ProtoRuntimeEvent {
        span_id: event.ctx.span_id.to_string(),
        parent_span_id: event.ctx.parent_span_id.as_ref().map(ToString::to_string),
        root_span_id: event.ctx.root_span_id.to_string(),
        timestamp_ms,
        call_stack,
        event: Some(event_kind),
        call_id: event.call_id.0,
    })
}

fn event_kind_to_proto(
    event: &EventKind,
    options: &CffiHandleTableOptions,
) -> Result<ProtoEventKind, CtypesError> {
    let kind = match event {
        EventKind::Function(FunctionEvent::Start(start)) => {
            let args: Result<Vec<_>, _> = start
                .args
                .iter()
                .map(|arg| external_to_outbound(arg, options))
                .collect();
            let tags: Vec<TagEntry> = start
                .tags
                .iter()
                .map(|(k, v)| TagEntry {
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect();
            ProtoEventKindVariant::FunctionStart(FunctionStartEvent {
                name: start.name.clone(),
                args: args?,
                tags,
            })
        }
        EventKind::Function(FunctionEvent::End(end)) => {
            let result = external_to_outbound(&end.result, options)?;
            let duration_ms = u64::try_from(end.duration.as_millis()).unwrap_or(u64::MAX);
            ProtoEventKindVariant::FunctionEnd(FunctionEndEvent {
                name: end.name.clone(),
                result: Some(result),
                duration_ms,
                error: end.error.clone(),
            })
        }
        EventKind::SetTags(tags) => {
            let tag_entries: Vec<TagEntry> = tags
                .iter()
                .map(|(k, v)| TagEntry {
                    key: k.clone(),
                    value: v.clone(),
                })
                .collect();
            ProtoEventKindVariant::SetTags(SetTagsEvent { tags: tag_entries })
        }
        EventKind::Log(LogEvent {
            level,
            data,
            source,
        }) => {
            let data_proto = external_to_outbound(data, options)?;
            let source_proto = source.as_ref().map(|s| cffi::SourceLocation {
                file_id: s.file_id,
                line: s.line,
                column: s.column,
                start_offset: s.start_offset,
                end_offset: s.end_offset,
            });
            ProtoEventKindVariant::Log(cffi::LogEvent {
                level: level.clone(),
                data: Some(data_proto),
                source: source_proto,
            })
        }
        EventKind::Custom(CustomEvent { name, data }) => {
            let data_proto = external_to_outbound(data, options)?;
            ProtoEventKindVariant::Custom(cffi::CustomEvent {
                name: name.clone(),
                data: Some(data_proto),
            })
        }
    };

    Ok(ProtoEventKind { kind: Some(kind) })
}

/// Serialize a `RuntimeEvent` to protobuf bytes.
pub fn runtime_event_to_bytes(
    event: &RuntimeEvent,
    options: &CffiHandleTableOptions,
) -> Result<Vec<u8>, CtypesError> {
    use prost::Message;
    let proto = runtime_event_to_proto(event, options)?;
    Ok(proto.encode_to_vec())
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use bex_events::{FunctionEnd, FunctionStart, SpanContext, SpanId};
    use bex_project::BexExternalValue;
    use web_time::SystemTime;

    use super::*;

    #[test]
    fn test_function_start_to_proto() {
        use bex_events::CallId;

        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id.clone(),
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::Start(FunctionStart {
                name: "my_func".into(),
                args: vec![
                    BexExternalValue::Int(42),
                    BexExternalValue::String("hello".into()),
                ],
                tags: vec![("env".into(), "prod".into())],
            })),
        };

        let options = CffiHandleTableOptions::for_in_process();
        let proto = runtime_event_to_proto(&event, &options).unwrap();

        assert!(!proto.span_id.is_empty());
        assert!(proto.parent_span_id.is_none());
        assert!(!proto.root_span_id.is_empty());
        assert!(proto.timestamp_ms > 0);

        if let Some(ProtoEventKind {
            kind: Some(ProtoEventKindVariant::FunctionStart(start)),
        }) = proto.event
        {
            assert_eq!(start.name, "my_func");
            assert_eq!(start.args.len(), 2);
            assert_eq!(start.tags.len(), 1);
            assert_eq!(start.tags[0].key, "env");
            assert_eq!(start.tags[0].value, "prod");
        } else {
            panic!("Expected FunctionStart event");
        }
    }

    #[test]
    fn test_log_event_to_proto() {
        use bex_events::CallId;

        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id.clone(),
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Log(LogEvent {
                level: "info".into(),
                data: BexExternalValue::String("hello world".into()),
                source: Some(bex_events::SourceLocation {
                    file_id: 1,
                    line: 42,
                    column: 8,
                    start_offset: 100,
                    end_offset: 120,
                }),
            }),
        };

        let options = CffiHandleTableOptions::for_in_process();
        let proto = runtime_event_to_proto(&event, &options).unwrap();

        if let Some(ProtoEventKind {
            kind: Some(ProtoEventKindVariant::Log(log)),
        }) = proto.event
        {
            assert_eq!(log.level, "info");
            assert!(log.data.is_some());
            let source = log.source.unwrap();
            assert_eq!(source.file_id, 1);
            assert_eq!(source.line, 42);
            assert_eq!(source.column, 8);
        } else {
            panic!("Expected Log event");
        }
    }

    #[test]
    fn test_function_end_to_proto() {
        use bex_events::CallId;

        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id.clone(),
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(Box::new(FunctionEnd {
                name: "my_func".into(),
                result: BexExternalValue::Bool(true),
                duration: Duration::from_millis(150),
                error: None,
            }))),
        };

        let options = CffiHandleTableOptions::for_in_process();
        let proto = runtime_event_to_proto(&event, &options).unwrap();

        if let Some(ProtoEventKind {
            kind: Some(ProtoEventKindVariant::FunctionEnd(end)),
        }) = proto.event
        {
            assert_eq!(end.name, "my_func");
            assert_eq!(end.duration_ms, 150);
            assert_eq!(end.error, None);
            assert!(end.result.is_some());
        } else {
            panic!("Expected FunctionEnd event");
        }
    }

    #[test]
    fn test_function_end_media_serializes_for_wire() {
        use std::sync::Arc;

        use bex_events::CallId;
        use bex_project::{BexExternalAdt, MediaContent, MediaKind, MediaValue};

        let span_id = SpanId::new();
        let media = MediaValue::new(
            MediaKind::Image,
            MediaContent::Base64 {
                base64_data: "aW1hZ2U=".into(),
            },
            Some("image/png".into()),
        );
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id.clone(),
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Function(FunctionEvent::End(Box::new(FunctionEnd {
                name: "image_func".into(),
                result: BexExternalValue::Adt(BexExternalAdt::Media(Arc::new(media))),
                duration: Duration::from_millis(1),
                error: None,
            }))),
        };

        let options = CffiHandleTableOptions::for_wire();
        let proto = runtime_event_to_proto(&event, &options).unwrap();

        let Some(ProtoEventKind {
            kind: Some(ProtoEventKindVariant::FunctionEnd(end)),
        }) = proto.event
        else {
            panic!("Expected FunctionEnd event");
        };
        let result = end.result.expect("expected function result");
        match result.value {
            Some(crate::baml_core::cffi::baml_outbound_value::Value::MediaValue(media)) => {
                assert_eq!(
                    media.value,
                    Some(crate::baml_core::cffi::baml_value_media::Value::Base64(
                        "aW1hZ2U=".into()
                    ))
                );
            }
            other => panic!("Expected media value, got {other:?}"),
        }
    }

    #[test]
    fn test_event_to_bytes_roundtrip() {
        use bex_events::CallId;
        use prost::Message;

        let span_id = SpanId::new();
        let event = RuntimeEvent {
            call_id: CallId(0),
            ctx: SpanContext {
                span_id: span_id.clone(),
                parent_span_id: None,
                root_span_id: span_id.clone(),
            },
            call_stack: vec![span_id],
            timestamp: SystemTime::now(),
            event: EventKind::Custom(CustomEvent {
                name: "test_event".into(),
                data: BexExternalValue::Int(123),
            }),
        };

        let options = CffiHandleTableOptions::for_in_process();
        let bytes = runtime_event_to_bytes(&event, &options).unwrap();

        let decoded = ProtoRuntimeEvent::decode(bytes.as_slice()).unwrap();
        assert!(!decoded.span_id.is_empty());

        if let Some(ProtoEventKind {
            kind: Some(ProtoEventKindVariant::Custom(custom)),
        }) = decoded.event
        {
            assert_eq!(custom.name, "test_event");
        } else {
            panic!("Expected Custom event");
        }
    }
}
