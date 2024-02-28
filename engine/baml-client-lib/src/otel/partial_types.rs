// use std::{collections::HashMap, env};

// use chrono::prelude::{DateTime, Utc};

// use opentelemetry::StringValue;
// use opentelemetry_sdk::export::trace::SpanData;
// use tracing::info;

// use crate::api_wrapper::{
//     core_types::{
//         Error, EventChain, EventType, IOValue, LLMEventInput, LLMEventInputPrompt, LLMEventSchema,
//         LLMOutputModel, LogSchema, LogSchemaContext, Template, TypeSchema, ValueType, IO,
//     },
//     APIWrapper,
// };

// use super::span_events::SpanEvent;

// pub struct PartialLogSchema<'a> {
//     project_id: Option<&'a str>,
//     event_type: EventType,
//     root_event_id: String,
//     event_id: String,
//     parent_event_id: Option<String>,
//     context: LogSchemaContext,
//     io: IO,
//     error: Option<Error>,
//     metadata: Vec<PartialMetadataType>,
// }

// #[derive(Default)]
// pub struct PartialMetadataType {
//     model_name: Option<String>,
//     provider: Option<String>,
//     input: Option<LLMEventInput>,
//     output: Option<LLMOutputModel>,
//     error: Option<Error>,
// }

// #[derive(Default)]
// pub(super) struct UUIDLutImpl {
//     uuid_lut: HashMap<u64, HashMap<u64, String>>,
// }

// impl UUIDLutImpl {
//     fn get_or_create_uuid<'a>(&'a mut self, root_id: u64, item_id: u64) -> &'a str {
//         let root = self
//             .uuid_lut
//             .entry(root_id)
//             .or_insert_with(std::collections::HashMap::new);
//         root.entry(item_id)
//             .or_insert_with(|| uuid::Uuid::new_v4().to_string())
//     }
// }

// impl<'b> PartialLogSchema<'b> {
//     pub fn maybe_create<'a>(
//         uuid_manager: &'a mut UUIDLutImpl,
//         config: &'a APIWrapper,
//         span: &SpanData,
//     ) -> Option<Vec<LogSchema>> {
//         if span.resource.get("baml".into()).is_none() {
//             return None;
//         }
//         let parent_history = span
//             .attributes
//             .iter()
//             .filter(|kv| kv.key.as_str().cmp("root_span_ids".into()).is_eq())
//             .filter_map(|kv| match kv.value {
//                 opentelemetry::Value::Array(ref v) => match v {
//                     opentelemetry::Array::I64(ref v) => Some(v),
//                     _ => None,
//                 },
//                 _ => None,
//             })
//             .flatten()
//             .collect::<Vec<_>>();
//         let root_span_names = span
//             .attributes
//             .iter()
//             .filter(|kv| kv.key.as_str().cmp("root_span_names".into()).is_eq())
//             .filter_map(|kv| match kv.value {
//                 opentelemetry::Value::Array(ref v) => match v {
//                     opentelemetry::Array::String(ref v) => Some(v),
//                     _ => None,
//                 },
//                 _ => None,
//             })
//             .flatten()
//             .collect::<Vec<_>>();

//         if parent_history.is_empty()
//             || root_span_names.is_empty()
//             || parent_history.len() != root_span_names.len()
//         {
//             return None;
//         }

//         assert_eq!(
//             span.name.to_string(),
//             root_span_names.last().unwrap().as_str(),
//             "Root span name mismatch"
//         );

//         assert_eq!(
//             span.span_context.span_id(),
//             opentelemetry::trace::SpanId::from(
//                 parent_history
//                     .last()
//                     .and_then(|id| Some(**id as u64))
//                     .unwrap_or(0)
//             ),
//             "Root span id mismatch"
//         );

//         let root_id = parent_history
//             .first()
//             .and_then(|id| Some(**id as u64))
//             .unwrap();

//         let self_id = parent_history
//             .last()
//             .and_then(|id| Some(**id as u64))
//             .unwrap();

//         let root_event_id = uuid_manager
//             .get_or_create_uuid(root_id, root_id)
//             .to_string();
//         let event_id = uuid_manager
//             .get_or_create_uuid(root_id, self_id)
//             .to_string();

//         let parent_event_id = if let Some(pid) = parent_history
//             .get(parent_history.len() - 2)
//             .and_then(|id| Some(**id as u64))
//         {
//             Some(uuid_manager.get_or_create_uuid(root_id, pid).to_string())
//         } else {
//             None
//         };

//         let mut partial = PartialLogSchema {
//             project_id: config.project_id(),
//             event_type: EventType::FuncCode,
//             root_event_id,
//             event_id,
//             parent_event_id,
//             context: LogSchemaContext::from((config, span, root_span_names)),
//             io: Default::default(),
//             error: None,
//             metadata: vec![],
//         };

//         span.events.iter().for_each(|event| {
//             partial.update_with_event(event);
//         });

//         partial.to_final()
//     }

//     fn update_with_event(&mut self, event: &'b opentelemetry::trace::Event) {
//         let name = match &event.name {
//             std::borrow::Cow::Borrowed(b) => *b,
//             std::borrow::Cow::Owned(o) => &o,
//         };

//         let span_event = match serde_json::from_str::<SpanEvent>(format!("\"{name}\"").as_str()) {
//             Ok(v) => v,
//             Err(e) => {
//                 println!("Failed to parse event name: {}", e);
//                 return;
//             }
//         };

//         match span_event {
//             SpanEvent::SetTags => {
//                 event
//                     .attributes
//                     .iter()
//                     .filter(|kv| kv.key.as_str().cmp("__BAML_ID__".into()).is_ne())
//                     .for_each(|kv| {
//                         self.context
//                             .tags
//                             .insert(kv.key.to_string(), kv.value.to_string());
//                     });
//             }
//             SpanEvent::Input => self.io.input = get_io_value(event),
//             SpanEvent::Output => self.io.output = get_io_value(event),
//             SpanEvent::LlmPromptTemplate => {
//                 todo!("LlmPromptTemplate")
//             }
//             SpanEvent::LlmRequestCacheHit => {
//                 self.context.tags.insert("__cached".into(), "1".into());

//                 let latency = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| {
//                         if kv.key.as_str().cmp("latency_ms".into()).is_eq() {
//                             Some(&kv.value)
//                         } else {
//                             None
//                         }
//                     })
//                     .unwrap();
//                 self.context
//                     .tags
//                     .insert("__cached_latency_ms".into(), latency.to_string());
//             }
//             SpanEvent::LlmRequestStart => {
//                 self.event_type = EventType::FuncLlm;

//                 if let Some(partial) = self.metadata.last() {
//                     if partial.input.is_some()
//                         && partial.output.is_none()
//                         && partial.error.is_none()
//                     {
//                         // Early return if we have already seen an input and no output or error
//                         return;
//                     }
//                 }

//                 let prompt = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "prompt" => Some(Template::Single(kv.value.to_string())),
//                         "chat_prompt" => match &kv.value {
//                             opentelemetry::Value::Array(ref v) => match v {
//                                 opentelemetry::Array::String(ref v) => Some(Template::Multiple(
//                                     v.iter()
//                                         .filter_map(|s| serde_json::from_str(s.as_str()).ok())
//                                         .collect(),
//                                 )),
//                                 _ => None,
//                             },
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();

//                 let provider = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "provider" => Some(kv.value.to_string()),
//                         _ => None,
//                     });

//                 self.metadata.push(PartialMetadataType {
//                     model_name: None,
//                     provider,
//                     input: Some(LLMEventInput {
//                         prompt: LLMEventInputPrompt {
//                             template: prompt,
//                             template_args: Default::default(),
//                             r#override: None,
//                         },
//                         invocation_params: Default::default(),
//                     }),
//                     output: None,
//                     error: None,
//                 });
//             }
//             SpanEvent::LlmRequestError => {
//                 let last_partial = if let Some(partial) = self.metadata.last_mut() {
//                     partial
//                 } else {
//                     return;
//                 };

//                 let error_code = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "error_code" => match kv.value {
//                             opentelemetry::Value::I64(ref v) => Some(*v as i32),
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();

//                 let message = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "message" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v),
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();

//                 let traceback = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "traceback" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v.to_string()),
//                             _ => None,
//                         },
//                         _ => None,
//                     });

//                 last_partial.error = Some(Error {
//                     code: error_code,
//                     message: message.to_string(),
//                     traceback,
//                     r#override: None,
//                 });
//             }
//             SpanEvent::LlmRequestArgs => {
//                 let last_partial = if let Some(partial) = self.metadata.last_mut() {
//                     if let Some(partial) = partial.input.as_mut() {
//                         partial
//                     } else {
//                         return;
//                     }
//                 } else {
//                     return;
//                 };

//                 last_partial.invocation_params = event
//                     .attributes
//                     .iter()
//                     .map(|kv| {
//                         (
//                             kv.key.to_string(),
//                             serde_json::from_str(kv.value.as_str().as_ref()).unwrap(),
//                         )
//                     })
//                     .collect::<HashMap<_, _>>();
//             }
//             SpanEvent::LlmRequestEnd => {
//                 let last_partial = if let Some(partial) = self.metadata.last_mut() {
//                     partial
//                 } else {
//                     return;
//                 };

//                 last_partial.model_name =
//                     event
//                         .attributes
//                         .iter()
//                         .find_map(|kv| match kv.key.as_str() {
//                             "model_name" => match kv.value {
//                                 opentelemetry::Value::String(ref v) => Some(v.as_str()),
//                                 _ => None,
//                             },
//                             _ => None,
//                         });

//                 let generated_text = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "generated" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v),
//                             _ => None,
//                         },
//                         _ => None,
//                     });
//                 let metadata = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "metadata" => match kv.value {
//                             opentelemetry::Value::String(ref v) => {
//                                 serde_json::from_str(v.as_str()).ok()
//                             }
//                             _ => None,
//                         },
//                         _ => None,
//                     });

//                 match (generated_text, metadata) {
//                     (Some(generated_text), Some(metadata)) => {
//                         last_partial.output = Some(LLMOutputModel {
//                             raw_text: generated_text.to_string(),
//                             metadata,
//                             r#override: None,
//                         });
//                     }
//                     _ => {}
//                 }
//             }
//             SpanEvent::Variant => {
//                 let variant_name = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "name" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v.as_str()),
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();
//                 self.context
//                     .event_chain
//                     .last_mut()
//                     .map(|v| v.variant_name = Some(variant_name.into()));
//             }
//             SpanEvent::Exception => {
//                 let e_type = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "exception.type" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v.as_str()),
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();
//                 let e_value = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "exception.message" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v.as_str()),
//                             _ => None,
//                         },
//                         _ => None,
//                     })
//                     .unwrap();

//                 let traceback = event
//                     .attributes
//                     .iter()
//                     .find_map(|kv| match kv.key.as_str() {
//                         "exception.stacktrace" => match kv.value {
//                             opentelemetry::Value::String(ref v) => Some(v.to_string()),
//                             _ => None,
//                         },
//                         _ => None,
//                     });

//                 self.error = Some(Error {
//                     code: 2, // This is for unknown exceptions
//                     message: format!("{}: {}", e_type, e_value),
//                     traceback: traceback,
//                     r#override: None,
//                 });
//             }
//             SpanEvent::Unknown => panic!("Unknown span event: {name}"),
//         }
//     }

// }

// impl From<(&APIWrapper, &SpanData, Vec<&StringValue>)> for LogSchemaContext {
//     fn from((config, span, names): (&APIWrapper, &SpanData, Vec<&StringValue>)) -> Self {
//         let start_time: DateTime<Utc> = span.start_time.into();

//         LogSchemaContext {
//             hostname: span
//                 .resource
//                 .get("hostname".into())
//                 .map_or("UNKNOWN".into(), |v| v.to_string()),
//             process_id: config.session_id().to_string(),
//             stage: Some(config.stage().to_string()),
//             latency_ms: span
//                 .end_time
//                 .duration_since(span.start_time)
//                 .map_or(0, |d| d.as_millis() as u64),
//             // span.start_Time as iso8601
//             start_time: format!("{}Z", start_time.format("%+")),
//             tags: HashMap::from_iter(std::iter::once((
//                 "baml.version".to_string(),
//                 span.resource
//                     .get("baml.version".into())
//                     .map_or(env!("CARGO_PKG_VERSION").to_string(), |v| v.to_string()),
//             ))),
//             event_chain: names
//                 .iter()
//                 .map(|v| EventChain {
//                     function_name: v.to_string(),
//                     variant_name: None,
//                 })
//                 .collect(),
//         }
//     }
// }

// fn get_io_value(event: &opentelemetry::trace::Event) -> Option<IOValue> {
//     let mut field_types = HashMap::new();
//     let mut vals = vec![];
//     event.attributes.iter().for_each(|kv| {
//         if !kv.key.as_str().contains('.') {
//             vals.push(kv.value.to_string());
//         } else {
//             let mut parts = kv.key.as_str().split('.');
//             let key = parts.next().unwrap();
//             field_types.insert(key.into(), kv.value.to_string());
//         }
//     });

//     match vals.len() {
//         0 => None,
//         1 => Some(IOValue {
//             r#type: TypeSchema {
//                 name: crate::api_wrapper::core_types::TypeSchemaName::Single,
//                 fields: field_types,
//             },
//             value: ValueType::String(vals.pop().unwrap()),
//             r#override: None,
//         }),
//         _ => Some(IOValue {
//             r#type: TypeSchema {
//                 name: crate::api_wrapper::core_types::TypeSchemaName::Multi,
//                 fields: field_types,
//             },
//             value: ValueType::List(vals),
//             r#override: None,
//         }),
//     }
// }
