// use anyhow::Result;
// use opentelemetry::Context;
// use std::collections::HashMap;
// use std::sync::Mutex;

// use opentelemetry::{
//     trace::{Span, SpanId, TraceContextExt, Tracer},
//     ContextGuard, Key,
// };
// use tracing::warn;

// use once_cell::sync::OnceCell;

// fn get_ctx() -> &'static Mutex<HashMap<SpanId, BamlSpanContext>> {
//     static CURRENT_CTX: OnceCell<Mutex<HashMap<SpanId, BamlSpanContext>>> = OnceCell::new();
//     // Initialize the global context if it doesn't exist
//     CURRENT_CTX.get_or_init(|| Mutex::new(HashMap::new()))
// }

// #[derive(Debug)]
// struct BamlSpanContext {
//     parent_history: Vec<(SpanId, String)>,
//     tags: Option<HashMap<String, opentelemetry::Value>>,
// }

// pub struct BamlSpanContextManager {
//     guard: ContextGuard,
// }

// pub type ArgAndType = (String, String);
// pub type NamedArgs = HashMap<String, ArgAndType>;
// pub type PositionalArgs = Vec<(Option<String>, ArgAndType)>;
// pub type FunctionArgs = (PositionalArgs, NamedArgs);

// impl BamlSpanContextManager {
//     pub fn new_for_async(name: &str) -> Context {
//         let parent_span_id = {
//             let ctx = opentelemetry::Context::current();
//             let span = ctx.span();
//             span.span_context().span_id()
//         };
//         let parent_span_id = if parent_span_id == opentelemetry::trace::SpanId::INVALID {
//             println!("No parent span found");
//             None
//         } else {
//             Some(parent_span_id)
//         };

//         let span = opentelemetry::global::tracer("baml-client-lib").start(name.to_string());

//         let span_id = span.span_context().span_id();
//         let otel_ctx = opentelemetry::Context::current_with_span(span);

//         let ctx = get_ctx();
//         {
//             let mut ctx = ctx.lock().unwrap();
//             let mut parent_history = parent_span_id
//                 .and_then(|id| ctx.get(&id).map(|span| span.parent_history.clone()))
//                 .unwrap_or_default();
//             parent_history.push((span_id, name.to_string()));

//             ctx.insert(
//                 span_id,
//                 BamlSpanContext {
//                     parent_history,
//                     tags: None,
//                 },
//             );
//         }
//         otel_ctx
//     }

//     pub fn from_context(ctx: Context) -> Self {
//         BamlSpanContextManager {
//             guard: ctx.attach(),
//         }
//     }

//     pub fn new(name: &str) -> Self {
//         let otel_ctx = Self::new_for_async(name);
//         BamlSpanContextManager {
//             guard: otel_ctx.attach(),
//         }
//     }

//     pub fn record_input(args: PositionalArgs, kwargs: NamedArgs) {
//         // TODO: Remove the need for cloning
//         let kwargs = kwargs.iter().flat_map(|(k, v)| {
//             [
//                 opentelemetry::KeyValue::new(k.clone(), v.0.clone()),
//                 opentelemetry::KeyValue::new(format!("{}.type", k), v.1.clone()),
//             ]
//         });
//         let args = args.iter().enumerate().flat_map(|(i, (name, v))| {
//             let name = name
//                 .as_ref()
//                 .map(|s| s.clone())
//                 .unwrap_or(format!("arg_{}", i));
//             [
//                 opentelemetry::KeyValue::new(format!("{name}.type"), v.1.clone()),
//                 opentelemetry::KeyValue::new(name, v.0.clone()),
//             ]
//         });
//         let ctx = opentelemetry::Context::current();

//         println!("record_input: parent: {}", ctx.has_active_span(),);
//         let span = ctx.span();
//         println!("record_input: Span ID: {}", span.span_context().span_id());
//         span.add_event("input", args.chain(kwargs).collect());
//     }

//     pub fn record_output(value: String, r#type: &String) {
//         let ctx = opentelemetry::Context::current();
//         let span = ctx.span();
//         println!("record_output: Span ID: {}", span.span_context().span_id());
//         // TODO: Remove the need for cloning
//         span.add_event(
//             "output",
//             vec![
//                 opentelemetry::KeyValue::new("result", value),
//                 opentelemetry::KeyValue::new("result.type", r#type.clone()),
//             ],
//         );
//     }
// }

// impl Drop for BamlSpanContextManager {
//     fn drop(&mut self) {
//         let otel_ctx = opentelemetry::Context::current();
//         let span = otel_ctx.span();
//         let span_id = span.span_context().span_id();
//         let ctx = get_ctx();
//         let res = {
//             let mut ctx = ctx.lock().unwrap();

//             match ctx.remove(&span_id) {
//                 Some(span_ctx) => {
//                     let parent_tags = if span_ctx.tags.is_none() {
//                         span_ctx
//                             .parent_history
//                             .iter()
//                             .rev()
//                             .filter_map(|(id, _)| ctx.get(id))
//                             .find_map(|span| span.tags.clone())
//                     } else {
//                         None
//                     };

//                     Ok((span_ctx, parent_tags))
//                 }
//                 None => Err(anyhow::anyhow!(
//                     "No span context found for span id {}",
//                     span_id
//                 )),
//             }
//         };

//         match res {
//             Ok((mut span_ctx, parent_tags)) => {
//                 span.set_attribute(opentelemetry::KeyValue {
//                     key: "root_span_ids".into(),
//                     value: opentelemetry::Value::Array(opentelemetry::Array::I64(
//                         span_ctx
//                             .parent_history
//                             .iter()
//                             .map(|(id, _)| i64::from_be_bytes(id.to_bytes()))
//                             .collect(),
//                     )),
//                 });

//                 span.set_attribute(opentelemetry::KeyValue {
//                     key: "root_span_names".into(),
//                     value: opentelemetry::Value::Array(opentelemetry::Array::String(
//                         span_ctx
//                             .parent_history
//                             .iter_mut()
//                             .map(|(_, name)| std::mem::take(name).into())
//                             .collect(),
//                     )),
//                 });

//                 span_ctx.tags.as_ref().or(parent_tags.as_ref()).map(|tags| {
//                     if !tags.is_empty() {
//                         span.add_event(
//                             "set_tags",
//                             tags.iter()
//                                 .map(|(k, v)| opentelemetry::KeyValue::new(k.clone(), v.clone()))
//                                 .collect(),
//                         );
//                     }
//                 });
//             }
//             Err(e) => {
//                 warn!("{}", e);
//             }
//         }
//     }
// }

// // // Decorator function that takes a closure and does
// // // the tracing and logging of the input and output.
// // pub fn trace<T, U>(
// //     f: impl Fn(T) -> U,
// //     name: String,
// //     args: T,
// //     serialize_args: impl Fn(&T) -> FunctionArgs,
// //     serialize_value: impl Fn(&U) -> ArgAndType,
// // ) -> U {
// //     let mut _guard = BamlSpanContextManager::new(&name, serialize_args(&args));

// //     let ret = f(args);

// //     _guard.record_output(serialize_value(&ret));

// //     ret
// // }

// #[derive(Debug, serde::Deserialize, serde::Serialize)]
// struct LLMRequestStartData {
//     prompt: crate::api_wrapper::core_types::Template,
//     provider: Option<String>,
// }

// #[derive(Debug, serde::Deserialize, serde::Serialize)]
// struct LLMRequestEndData {
//     model_name: Option<String>,
//     generated: Option<String>,
//     metadata: Option<crate::api_wrapper::core_types::LLMOutputModelMetadata>,
// }

// #[derive(Debug, serde::Deserialize, serde::Serialize)]
// struct LLMRequestErrorData {
//     error_code: i64,
//     message: Option<String>,
//     traceback: Option<String>,
// }

// pub fn set_llm_event(event_name: String, metadata: String) -> Result<()> {
//     let attrs = match serde_json::from_str::<SpanEvent>(format!("\"{event_name}\"").as_str())? {
//         SpanEvent::LlmPromptTemplate => {
//             todo!();
//         }
//         SpanEvent::LlmRequestArgs => {
//             let data =
//                 serde_json::from_str::<HashMap<String, serde_json::Value>>(metadata.as_str())?;
//             data.iter()
//                 .map(|(k, v)| {
//                     opentelemetry::KeyValue::new(k.clone(), serde_json::to_string(v).unwrap())
//                 })
//                 .collect()
//         }
//         SpanEvent::LlmRequestCacheHit => {
//             let latency_ms = serde_json::from_str::<i32>(metadata.as_str())?;
//             vec![opentelemetry::KeyValue::new(
//                 Key::from_static_str("latency_ms"),
//                 opentelemetry::Value::I64(latency_ms.into()),
//             )]
//         }
//         SpanEvent::LlmRequestEnd => {
//             let data = serde_json::from_str::<LLMRequestEndData>(metadata.as_str())?;
//             let mut v = vec![];
//             if let Some(model_name) = data.model_name {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("model_name"),
//                     opentelemetry::Value::String(model_name.into()),
//                 ));
//             }
//             if let Some(generated) = data.generated {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("generated"),
//                     opentelemetry::Value::String(generated.into()),
//                 ));
//             }
//             if let Some(metadata) = data.metadata {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("metadata"),
//                     opentelemetry::Value::String(serde_json::to_string(&metadata).unwrap().into()),
//                 ));
//             }
//             v
//         }
//         SpanEvent::LlmRequestError => {
//             let data = serde_json::from_str::<LLMRequestErrorData>(metadata.as_str())?;
//             let mut v = vec![opentelemetry::KeyValue::new(
//                 Key::from_static_str("error_code"),
//                 opentelemetry::Value::I64(data.error_code.into()),
//             )];
//             if let Some(message) = data.message {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("message"),
//                     opentelemetry::Value::String(message.into()),
//                 ));
//             }
//             if let Some(traceback) = data.traceback {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("traceback"),
//                     opentelemetry::Value::String(traceback.into()),
//                 ));
//             }
//             v
//         }
//         SpanEvent::LlmRequestStart => {
//             let data = serde_json::from_str::<LLMRequestStartData>(metadata.as_str())?;
//             let mut v = match data.prompt {
//                 crate::api_wrapper::core_types::Template::Single(prompt) => {
//                     vec![opentelemetry::KeyValue::new(
//                         Key::from_static_str("prompt"),
//                         opentelemetry::Value::String(prompt.into()),
//                     )]
//                 }
//                 crate::api_wrapper::core_types::Template::Multiple(prompts) => {
//                     vec![opentelemetry::KeyValue::new(
//                         Key::from_static_str("chat_prompt"),
//                         opentelemetry::Value::Array(
//                             prompts
//                                 .into_iter()
//                                 .map(|s| {
//                                     opentelemetry::StringValue::from(
//                                         serde_json::to_string(&s).unwrap(),
//                                     )
//                                 })
//                                 .collect::<Vec<_>>()
//                                 .into(),
//                         ),
//                     )]
//                 }
//             };
//             if let Some(provider) = data.provider {
//                 v.push(opentelemetry::KeyValue::new(
//                     Key::from_static_str("provider"),
//                     opentelemetry::Value::String(provider.into()),
//                 ));
//             }
//             v
//         }
//         SpanEvent::SetTags | SpanEvent::Variant | SpanEvent::Exception | SpanEvent::Unknown => {
//             return Err(anyhow::anyhow!(
//                 "Event {} is not a valid LLM event",
//                 event_name
//             ));
//         }
//     };

//     let otel_ctx = opentelemetry::Context::current();
//     let span = otel_ctx.span();
//     span.add_event(event_name, attrs);

//     Ok(())
// }

// /// Add tags to the current span, or remove tags from the current span.
// pub fn set_tags(tags: HashMap<String, Option<String>>) -> Result<()> {
//     // print the thread id
//     println!("Thread id: {:?}", std::thread::current().id());

//     // let otel_ctx = opentelemetry::Context::current();
//     // let span_id = otel_ctx.span().span_context().span_id();

//     // Get the id of the current span
//     let span_id = opentelemetry::trace::get_active_span(|span| span.span_context().span_id());

//     let ctx = get_ctx();
//     let mut ctx = ctx.lock().unwrap();

//     let span = match ctx.get(&span_id) {
//         None => {
//             return Err(anyhow::anyhow!(
//                 "No span context found for span id {}\n{:?}",
//                 span_id,
//                 ctx
//             ));
//         }
//         Some(span_ctx) => span_ctx,
//     };

//     let spn_tags = if let Some(tags) = match &span.tags {
//         Some(_) => None,
//         None => {
//             // Get the parents tags and add the new tags
//             match span
//                 .parent_history
//                 .iter()
//                 .filter_map(|(id, _)| ctx.get(id))
//                 .find_map(|span| span.tags.as_ref())
//             {
//                 Some(tags) => Some(tags.clone()),
//                 None => Some(HashMap::new()),
//             }
//         }
//     } {
//         let spn = ctx.get_mut(&span_id).unwrap();
//         spn.tags = Some(tags);
//         spn.tags.as_mut().unwrap()
//     } else {
//         ctx.get_mut(&span_id).unwrap().tags.as_mut().unwrap()
//     };

//     for (key, value) in tags {
//         if let Some(value) = value {
//             // TODO: Support more types, but for now we only support strings
//             spn_tags.insert(key, value.into());
//         } else {
//             spn_tags.remove(&key);
//         }
//     }

//     Ok(())
// }
