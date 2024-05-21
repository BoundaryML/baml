mod api_wrapper;
#[cfg(not(feature = "wasm"))]
mod threaded_tracer;
#[cfg(feature = "wasm")]
mod wasm_tracer;

use anyhow::Result;
use baml_types::BamlValue;
use indexmap::IndexMap;
use internal_baml_core::ast::Span;
use internal_baml_jinja::RenderedPrompt;
use serde_json::json;
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    internal::llm_client::LLMResponse, tracing::api_wrapper::core_types::Role, FunctionResult,
    RuntimeContext, SpanCtx, TestResponse,
};

use self::api_wrapper::{
    core_types::{
        ContentPart, EventChain, IOValue, LLMChat, LLMEventInput, LLMEventInputPrompt,
        LLMEventSchema, LLMOutputModel, LogSchema, LogSchemaContext, MetadataType, Template,
        TypeSchema, IO,
    },
    APIWrapper,
};
#[cfg(not(feature = "wasm"))]
use self::threaded_tracer::ThreadedTracer;

#[cfg(feature = "wasm")]
use self::wasm_tracer::NonThreadedTracer;

#[cfg(not(feature = "wasm"))]
type TracerImpl = ThreadedTracer;
#[cfg(feature = "wasm")]
type TracerImpl = NonThreadedTracer;

pub struct TracingSpan {
    span_id: Uuid,
    function_name: String,
    params: IndexMap<String, BamlValue>,
    parent_ids: Option<Vec<(String, Uuid)>>,
    start_time: web_time::SystemTime,
    tags: HashMap<String, serde_json::Value>,
}

pub struct BamlTracer {
    options: APIWrapper,
    enabled: bool,
    tracer: Option<TracerImpl>,
}

impl BamlTracer {
    pub fn new(options: Option<APIWrapper>, ctx: &RuntimeContext) -> Self {
        let options = options.unwrap_or_else(|| ctx.into());

        let tracer = BamlTracer {
            tracer: if options.enabled() {
                Some(TracerImpl::new(
                    &options,
                    if options.stage() == "test" { 1 } else { 20 },
                ))
            } else {
                None
            },
            enabled: options.enabled(),
            options,
        };
        tracer
    }

    pub(crate) fn flush(&self) -> Result<()> {
        if let Some(tracer) = &self.tracer {
            tracer.flush()
        } else {
            Ok(())
        }
    }

    pub(crate) fn start_span(
        &self,
        function_name: &str,
        ctx: RuntimeContext,
        params: &IndexMap<String, BamlValue>,
    ) -> (Option<TracingSpan>, RuntimeContext) {
        if !self.enabled {
            return (None, ctx);
        }
        log::debug!(
            "Starting span: {} {} {:#?}",
            function_name,
            params.len(),
            params,
        );
        let span = TracingSpan {
            span_id: Uuid::new_v4(),
            function_name: function_name.to_string(),
            params: params.clone(),
            parent_ids: ctx.parent_thread.as_ref().map(|p| {
                p.iter()
                    .map(|p| (p.name.clone(), p.span_id))
                    .collect::<Vec<_>>()
            }),
            start_time: web_time::SystemTime::now(),
            tags: ctx.tags.clone(),
        };

        let mut new_ctx = ctx.clone();
        match &mut new_ctx.parent_thread {
            Some(parent_thread) => {
                parent_thread.push(SpanCtx {
                    span_id: span.span_id,
                    name: function_name.to_string(),
                });
            }
            None => {
                new_ctx.parent_thread = Some(vec![SpanCtx {
                    span_id: span.span_id,
                    name: function_name.to_string(),
                }]);
            }
        }

        (Some(span), new_ctx)
    }

    #[allow(dead_code)]
    pub(crate) async fn finish_span(
        &self,
        span: TracingSpan,
        response: Option<BamlValue>,
    ) -> Result<Option<uuid::Uuid>> {
        let target = span.span_id;
        if let Some(tracer) = &self.tracer {
            tracer
                .submit((&self.options, span, response).into())
                .await?;
            Ok(Some(target))
        } else {
            Ok(None)
        }
    }

    pub(crate) async fn finish_baml_span(
        &self,
        span: TracingSpan,
        response: &Result<FunctionResult>,
    ) -> Result<Option<uuid::Uuid>> {
        let target = span.span_id;
        if let Some(tracer) = &self.tracer {
            tracer
                .submit(response.to_log_schema(&self.options, span))
                .await?;
            Ok(Some(target))
        } else {
            Ok(None)
        }
    }
}

// Function to convert web_time::SystemTime to ISO 8601 string
fn to_iso_string(web_time: &web_time::SystemTime) -> String {
    let time = web_time.duration_since(web_time::UNIX_EPOCH).unwrap();
    // Convert to ISO 8601 string
    chrono::DateTime::from_timestamp_millis(time.as_millis() as i64)
        .unwrap()
        .to_rfc3339_opts(chrono::SecondsFormat::AutoSi, true)
}

impl From<(&APIWrapper, &TracingSpan)> for LogSchemaContext {
    fn from((api, span): (&APIWrapper, &TracingSpan)) -> Self {
        let parents = &span.parent_ids.as_ref();
        let mut parent_chain = parents
            .map(|p| {
                p.iter()
                    .map(|(name, id)| EventChain {
                        function_name: name.clone(),
                        variant_name: Some(id.to_string()),
                    })
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        parent_chain.push(EventChain {
            function_name: span.function_name.clone(),
            variant_name: None,
        });
        LogSchemaContext {
            hostname: api.host_name().to_string(),
            stage: Some(api.stage().to_string()),
            latency_ms: span
                .start_time
                .elapsed()
                .map(|d| d.as_millis() as i128)
                .unwrap_or(0),
            process_id: api.session_id().to_string(),
            tags: HashMap::new(),
            event_chain: parent_chain,
            start_time: to_iso_string(&span.start_time),
        }
    }
}

impl From<&IndexMap<String, BamlValue>> for IOValue {
    fn from(items: &IndexMap<String, BamlValue>) -> Self {
        log::info!("Converting IOValue from IndexMap: {:#?}", items);
        IOValue {
            r#type: TypeSchema {
                name: api_wrapper::core_types::TypeSchemaName::Multi,
                fields: items.iter().map(|(k, v)| (k.clone(), v.r#type())).collect(),
            },
            value: api_wrapper::core_types::ValueType::List(
                items
                    .iter()
                    .map(|(_, v)| {
                        serde_json::to_string(v).unwrap_or_else(|_| "<unknown>".to_string())
                    })
                    .collect(),
            ),
            r#override: None,
        }
    }
}

impl From<&BamlValue> for IOValue {
    fn from(value: &BamlValue) -> Self {
        match value {
            BamlValue::Map(obj) => obj.into(),
            _ => IOValue {
                r#type: TypeSchema {
                    name: api_wrapper::core_types::TypeSchemaName::Single,
                    fields: [("value".into(), value.r#type())].into(),
                },
                value: api_wrapper::core_types::ValueType::String(
                    serde_json::to_string(value).unwrap_or_else(|_| "<unknown>".to_string()),
                ),
                r#override: None,
            },
        }
    }
}

impl From<(&APIWrapper, TracingSpan, Option<BamlValue>)> for LogSchema {
    fn from((api, span, result): (&APIWrapper, TracingSpan, Option<BamlValue>)) -> Self {
        let parent_ids = &span.parent_ids.as_ref();
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncCode,
            root_event_id: parent_ids
                .and_then(|p| p.first().map(|(_, id)| *id))
                .unwrap_or(span.span_id)
                .to_string(),
            event_id: span.span_id.to_string(),
            parent_event_id: parent_ids.and_then(|p| p.last().map(|(_, id)| id.to_string())),
            context: (api, &span).into(),
            io: IO {
                input: Some((&span.params).into()),
                output: result.as_ref().map(|r| r.into()),
            },
            error: None,
            metadata: None,
        }
    }
}

fn error_from_result(result: &FunctionResult) -> Option<api_wrapper::core_types::Error> {
    match result.parsed() {
        Some(Ok(_)) => None,
        Some(Err(e)) => Some(api_wrapper::core_types::Error {
            code: 2,
            message: e.to_string(),
            traceback: None,
            r#override: None,
        }),
        None => match result.llm_response() {
            LLMResponse::Success(_) => None,
            LLMResponse::LLMFailure(s) => Some(api_wrapper::core_types::Error {
                code: 2,
                message: s.message.clone(),
                traceback: None,
                r#override: None,
            }),
            LLMResponse::OtherFailure(s) => Some(api_wrapper::core_types::Error {
                code: 2,
                message: s.clone(),
                traceback: None,
                r#override: None,
            }),
        },
    }
}

trait ToLogSchema {
    fn to_log_schema(&self, api: &APIWrapper, span: TracingSpan) -> LogSchema;
}

impl<T: ToLogSchema> ToLogSchema for Result<T> {
    fn to_log_schema(&self, api: &APIWrapper, span: TracingSpan) -> LogSchema {
        match self {
            Ok(r) => r.to_log_schema(api, span),
            Err(e) => {
                log::info!("Logging a failure: {:#?}", e);

                LogSchema {
                    project_id: api.project_id().map(|s| s.to_string()),
                    event_type: api_wrapper::core_types::EventType::FuncCode,
                    root_event_id: span.span_id.to_string(),
                    event_id: span.span_id.to_string(),
                    parent_event_id: None,
                    context: (api, &span).into(),
                    io: IO {
                        input: Some((&span.params).into()),
                        output: None,
                    },
                    error: Some(api_wrapper::core_types::Error {
                        code: 2,
                        message: e.to_string(),
                        traceback: None,
                        r#override: None,
                    }),
                    metadata: None,
                }
            }
        }
    }
}

impl ToLogSchema for TestResponse {
    fn to_log_schema(&self, api: &APIWrapper, span: TracingSpan) -> LogSchema {
        self.function_response.to_log_schema(api, span)
    }
}

impl ToLogSchema for FunctionResult {
    fn to_log_schema(&self, api: &APIWrapper, span: TracingSpan) -> LogSchema {
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncLlm,
            root_event_id: span
                .parent_ids
                .as_ref()
                .and_then(|p| p.first().map(|(_, id)| *id))
                .unwrap_or(span.span_id)
                .to_string(),
            event_id: span.span_id.to_string(),
            parent_event_id: span
                .parent_ids
                .as_ref()
                .and_then(|p| p.last().map(|(_, id)| id.to_string())),
            context: (api, &span).into(),
            io: IO {
                input: Some((&span.params).into()),
                output: self
                    .parsed()
                    .as_ref()
                    .map(|r| r.as_ref().ok())
                    .flatten()
                    .and_then(|r| {
                        let v: BamlValue = r.into();
                        Some(IOValue::from(&v))
                    }),
            },
            error: error_from_result(self),
            metadata: Some(self.into()),
        }
    }
}

impl From<&FunctionResult> for MetadataType {
    fn from(result: &FunctionResult) -> Self {
        MetadataType::Multi(
            result
                .event_chain()
                .iter()
                .map(|(_, r, _)| r.into())
                .collect::<Vec<_>>(),
        )
    }
}

impl From<&LLMResponse> for LLMEventSchema {
    fn from(response: &LLMResponse) -> Self {
        match response {
            LLMResponse::OtherFailure(s) => LLMEventSchema {
                model_name: "<unknown>".into(),
                provider: "<unknown>".into(),
                input: LLMEventInput {
                    prompt: LLMEventInputPrompt {
                        template: Template::Single("<unable to render prompt>".into()),
                        template_args: Default::default(),
                        r#override: None,
                    },
                    invocation_params: Default::default(),
                },
                output: None,
                error: Some(s.clone()),
            },
            LLMResponse::Success(s) => LLMEventSchema {
                model_name: s.model.clone(),
                provider: s.client.clone(),
                input: LLMEventInput {
                    prompt: LLMEventInputPrompt {
                        template: (&s.prompt).into(),
                        template_args: Default::default(),
                        r#override: None,
                    },
                    invocation_params: Default::default(),
                },
                output: Some(LLMOutputModel {
                    raw_text: s.content.clone(),
                    metadata: serde_json::to_value(&s.metadata)
                        .map_or_else(Err, serde_json::from_value)
                        .unwrap_or_default(),
                    r#override: None,
                }),
                error: None,
            },
            LLMResponse::LLMFailure(s) => LLMEventSchema {
                model_name: s
                    .model
                    .as_ref()
                    .map_or_else(|| "<unknown>", |f| f.as_str())
                    .into(),
                provider: s.client.clone(),
                input: LLMEventInput {
                    prompt: LLMEventInputPrompt {
                        template: (&s.prompt).into(),
                        template_args: Default::default(),
                        r#override: None,
                    },
                    invocation_params: Default::default(),
                },
                output: None,
                error: Some(s.message.clone()),
            },
        }
    }
}

impl From<&RenderedPrompt> for Template {
    fn from(value: &RenderedPrompt) -> Self {
        match value {
            RenderedPrompt::Completion(c) => Template::Single(c.clone()),
            RenderedPrompt::Chat(c) => Template::Multiple(
                c.iter()
                    .map(|c| LLMChat {
                        role: match serde_json::from_value::<Role>(serde_json::json!(c.role)) {
                            Ok(r) => r,
                            Err(e) => {
                                log::error!("Failed to parse role: {} {:#?}", e, c.role);
                                Role::Other(c.role.clone())
                            }
                        },
                        content: c
                            .parts
                            .iter()
                            .map(|p| match p {
                                internal_baml_jinja::ChatMessagePart::Text(t) => {
                                    ContentPart::Text(t.clone())
                                }
                                internal_baml_jinja::ChatMessagePart::Image(
                                    baml_types::BamlImage::Base64(u),
                                ) => ContentPart::B64Image(u.base64.clone()),
                                internal_baml_jinja::ChatMessagePart::Image(
                                    baml_types::BamlImage::Url(u),
                                ) => ContentPart::UrlImage(u.url.clone()),
                            })
                            .collect::<Vec<_>>(),
                    })
                    .collect::<Vec<_>>(),
            ),
        }
    }
}
