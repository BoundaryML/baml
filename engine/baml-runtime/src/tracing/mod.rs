pub mod api_wrapper;

use crate::on_log_event::LogEventCallbackSync;
use crate::InnerTraceStats;
use anyhow::{Context, Result};
use baml_types::{BamlMap, BamlMediaType, BamlValue};
use cfg_if::cfg_if;
use colored::Colorize;
use internal_baml_jinja::RenderedPrompt;
use std::collections::HashMap;

use uuid::Uuid;

use crate::{
    internal::llm_client::LLMResponse,
    tracing::api_wrapper::core_types::Role, FunctionResult, RuntimeContextManager, SpanCtx, TestResponse, TraceStats,
};

use self::api_wrapper::{
    core_types::{
        ContentPart, EventChain, IOValue, LLMChat, LLMEventInput, LLMEventInputPrompt,
        LLMEventSchema, LLMOutputModel, LogSchema, LogSchemaContext, MetadataType, Template,
        TypeSchema, IO,
    },
    APIWrapper,
};

cfg_if! {
    if #[cfg(target_arch = "wasm32")] {
        mod wasm_tracer;
        use self::wasm_tracer::NonThreadedTracer as TracerImpl;
    } else {
        mod threaded_tracer;
        use self::threaded_tracer::ThreadedTracer as TracerImpl;
    }
}

#[derive(Debug)]
pub struct TracingSpan {
    span_id: Uuid,
    params: BamlMap<String, BamlValue>,
    start_time: web_time::SystemTime,
}

pub struct BamlTracer {
    options: APIWrapper,
    enabled: bool,
    tracer: Option<TracerImpl>,
    trace_stats: TraceStats,
}

#[cfg(not(target_arch = "wasm32"))]
static_assertions::assert_impl_all!(BamlTracer: Send, Sync);

impl BamlTracer {
    pub fn new<T: AsRef<str>>(
        options: Option<APIWrapper>,
        env_vars: impl Iterator<Item = (T, T)>,
    ) -> Result<Self> {
        let options = match options {
            Some(wrapper) => wrapper,
            None => APIWrapper::from_env_vars(env_vars)?,
        };

        let trace_stats = TraceStats::default();

        let tracer = BamlTracer {
            tracer: if options.enabled() {
                Some(TracerImpl::new(&options, 20, trace_stats.clone()))
            } else {
                None
            },
            enabled: options.enabled(),
            options,
            trace_stats,
        };
        Ok(tracer)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn set_log_event_callback(&self, log_event_callback: Option<LogEventCallbackSync>) {
        if let Some(tracer) = &self.tracer {
            tracer.set_log_event_callback(log_event_callback);
        }
    }

    pub(crate) fn flush(&self) -> Result<()> {
        if let Some(ref tracer) = self.tracer {
            tracer.flush().context("Failed to flush BAML traces")?;
        }

        Ok(())
    }

    pub(crate) fn drain_stats(&self) -> InnerTraceStats {
        self.trace_stats.drain()
    }

    pub(crate) fn start_span(
        &self,
        function_name: &str,
        ctx: &RuntimeContextManager,
        params: &BamlMap<String, BamlValue>,
    ) -> Option<TracingSpan> {
        self.trace_stats.guard().start();
        let span_id = ctx.enter(function_name);
        log::trace!("Entering span {:#?} in {:?}", span_id, function_name);
        if !self.enabled {
            return None;
        }
        let span = TracingSpan {
            span_id,
            params: params.clone(),
            start_time: web_time::SystemTime::now(),
        };

        Some(span)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn finish_span(
        &self,
        span: TracingSpan,
        ctx: &RuntimeContextManager,
        response: Option<BamlValue>,
    ) -> Result<Option<uuid::Uuid>> {
        let guard = self.trace_stats.guard();

        let Some((span_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!(
                "Attempting to finish a span {:#?} without first starting one. Current context {:#?}",
                span,
                ctx
            );
        };

        if span.span_id != span_id {
            anyhow::bail!("Span ID mismatch: {} != {}", span.span_id, span_id);
        }

        if let Some(tracer) = &self.tracer {
            tracer
                .submit(response.to_log_schema(&self.options, event_chain, tags, span))
                .await?;
            guard.done();
            Ok(Some(span_id))
        } else {
            guard.done();
            Ok(None)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finish_span(
        &self,
        span: TracingSpan,
        ctx: &RuntimeContextManager,
        response: Option<BamlValue>,
    ) -> Result<Option<uuid::Uuid>> {
        let guard = self.trace_stats.guard();
        let Some((span_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!(
                "Attempting to finish a span {:#?} without first starting one. Current context {:#?}",
                span,
                ctx
            );
        };
        log::trace!(
            "Finishing span: {:#?} {}\nevent chain {:?}",
            span,
            span_id,
            event_chain
        );

        if span.span_id != span_id {
            anyhow::bail!("Span ID mismatch: {} != {}", span.span_id, span_id);
        }

        if let Some(tracer) = &self.tracer {
            tracer.submit(response.to_log_schema(&self.options, event_chain, tags, span))?;
            guard.finalize();
            Ok(Some(span_id))
        } else {
            guard.done();
            Ok(None)
        }
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn finish_baml_span(
        &self,
        span: TracingSpan,
        ctx: &RuntimeContextManager,
        response: &Result<FunctionResult>,
    ) -> Result<Option<uuid::Uuid>> {
        let guard = self.trace_stats.guard();
        let Some((span_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!("Attempting to finish a span without first starting one");
        };

        if span.span_id != span_id {
            anyhow::bail!("Span ID mismatch: {} != {}", span.span_id, span_id);
        }

        if let Ok(response) = &response {
            let name = event_chain.last().map(|s| s.name.as_str());
            let is_ok = response.parsed().as_ref().is_some_and(|r| r.is_ok());
            log::log!(
                target: "baml_events",
                if is_ok { log::Level::Info } else { log::Level::Warn },
                "{}{}",
                name.map(|s| format!("Function {}:\n", s)).unwrap_or_default().purple(),
                response
            );
        }

        if let Some(tracer) = &self.tracer {
            tracer
                .submit(response.to_log_schema(&self.options, event_chain, tags, span))
                .await?;
            guard.done();
            Ok(Some(span_id))
        } else {
            guard.done();
            Ok(None)
        }
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finish_baml_span(
        &self,
        span: TracingSpan,
        ctx: &RuntimeContextManager,
        response: &Result<FunctionResult>,
    ) -> Result<Option<uuid::Uuid>> {
        let guard = self.trace_stats.guard();
        let Some((span_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!("Attempting to finish a span without first starting one");
        };

        log::trace!(
            "Finishing baml span: {:#?} {}\nevent chain {:?}",
            span,
            span_id,
            event_chain
        );

        if span.span_id != span_id {
            anyhow::bail!("Span ID mismatch: {} != {}", span.span_id, span_id);
        }

        if let Ok(response) = &response {
            let name = event_chain.last().map(|s| s.name.as_str());
            let is_ok = response.parsed().as_ref().is_some_and(|r| r.is_ok());
            log::log!(
                target: "baml_events",
                if is_ok { log::Level::Info } else { log::Level::Warn },
                "{}{}",
                name.map(|s| format!("Function {}:\n", s)).unwrap_or_default().purple(),
                response
            );
        }

        if let Some(tracer) = &self.tracer {
            tracer.submit(response.to_log_schema(&self.options, event_chain, tags, span))?;
            guard.finalize();
            Ok(Some(span_id))
        } else {
            guard.done();
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

impl
    From<(
        &APIWrapper,
        Vec<SpanCtx>,
        HashMap<String, BamlValue>,
        &TracingSpan,
    )> for LogSchemaContext
{
    fn from(
        (api, event_chain, tags, span): (
            &APIWrapper,
            Vec<SpanCtx>,
            HashMap<String, BamlValue>,
            &TracingSpan,
        ),
    ) -> Self {
        let parent_chain = event_chain
            .iter()
            .map(|ctx| EventChain {
                function_name: ctx.name.clone(),
                variant_name: None,
            })
            .collect::<Vec<_>>();
        LogSchemaContext {
            hostname: api.host_name().to_string(),
            stage: Some(api.stage().to_string()),
            latency_ms: span
                .start_time
                .elapsed()
                .map(|d| d.as_millis() as i128)
                .unwrap_or(0),
            process_id: api.session_id().to_string(),
            tags: tags
                .into_iter()
                .filter_map(|(k, v)| match v.as_str() {
                    Some(v) => Some((k, v.to_string())),
                    None => Some((
                        k,
                        serde_json::to_string(&v).unwrap_or_else(|_| "<unknown>".to_string()),
                    )),
                })
                .chain(std::iter::once((
                    "baml.runtime".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                )))
                .collect(),
            event_chain: parent_chain,
            start_time: to_iso_string(&span.start_time),
        }
    }
}

impl From<&BamlMap<String, BamlValue>> for IOValue {
    fn from(items: &BamlMap<String, BamlValue>) -> Self {
        log::trace!("Converting IOValue from BamlMap: {:#?}", items);
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
    // Event_chain is guaranteed to have at least one element
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<SpanCtx>,
        tags: HashMap<String, BamlValue>,
        span: TracingSpan,
    ) -> LogSchema;
}

impl<T: ToLogSchema> ToLogSchema for Result<T> {
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<SpanCtx>,
        tags: HashMap<String, BamlValue>,
        span: TracingSpan,
    ) -> LogSchema {
        match self {
            Ok(r) => r.to_log_schema(api, event_chain, tags, span),
            Err(e) => LogSchema {
                project_id: api.project_id().map(|s| s.to_string()),
                event_type: api_wrapper::core_types::EventType::FuncCode,
                root_event_id: event_chain.first().map(|s| s.span_id).unwrap().to_string(),
                event_id: event_chain.last().map(|s| s.span_id).unwrap().to_string(),
                parent_event_id: None,
                context: (api, event_chain, tags, &span).into(),
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
            },
        }
    }
}

impl ToLogSchema for Option<BamlValue> {
    // Event_chain is guaranteed to have at least one element
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<SpanCtx>,
        tags: HashMap<String, BamlValue>,
        span: TracingSpan,
    ) -> LogSchema {
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncCode,
            root_event_id: event_chain.first().map(|s| s.span_id).unwrap().to_string(),
            event_id: event_chain.last().map(|s| s.span_id).unwrap().to_string(),
            parent_event_id: if event_chain.len() >= 2 {
                event_chain
                    .get(event_chain.len() - 2)
                    .map(|s| s.span_id.to_string())
            } else {
                None
            },
            context: (api, event_chain, tags, &span).into(),
            io: IO {
                input: Some((&span.params).into()),
                output: self.as_ref().map(|r| r.into()),
            },
            error: None,
            metadata: None,
        }
    }
}

impl ToLogSchema for TestResponse {
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<SpanCtx>,
        tags: HashMap<String, BamlValue>,
        span: TracingSpan,
    ) -> LogSchema {
        self.function_response
            .to_log_schema(api, event_chain, tags, span)
    }
}

impl ToLogSchema for FunctionResult {
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<SpanCtx>,
        tags: HashMap<String, BamlValue>,
        span: TracingSpan,
    ) -> LogSchema {
        LogSchema {
            project_id: api.project_id().map(|s| s.to_string()),
            event_type: api_wrapper::core_types::EventType::FuncLlm,
            root_event_id: event_chain.first().map(|s| s.span_id).unwrap().to_string(),
            event_id: event_chain.last().map(|s| s.span_id).unwrap().to_string(),
            // Second to last element in the event chain
            parent_event_id: if event_chain.len() >= 2 {
                event_chain
                    .get(event_chain.len() - 2)
                    .map(|s| s.span_id.to_string())
            } else {
                None
            },
            context: (api, event_chain, tags, &span).into(),
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
                    request_options: Default::default(),
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
                    request_options: s.request_options.clone(),
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
                    request_options: s.request_options.clone(),
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
                                internal_baml_jinja::ChatMessagePart::Image(media)
                                | internal_baml_jinja::ChatMessagePart::Audio(media) => match media
                                {
                                    baml_types::BamlMedia::Base64(media_type, data) => {
                                        match media_type {
                                            BamlMediaType::Image => {
                                                ContentPart::B64Image(data.base64.clone())
                                            }
                                            BamlMediaType::Audio => {
                                                ContentPart::B64Audio(data.base64.clone())
                                            }
                                        }
                                    }
                                    baml_types::BamlMedia::Url(media_type, data) => {
                                        match media_type {
                                            BamlMediaType::Image => {
                                                ContentPart::UrlImage(data.url.clone())
                                            }
                                            BamlMediaType::Audio => {
                                                ContentPart::UrlAudio(data.url.clone())
                                            }
                                        }
                                    }
                                },
                            })
                            .collect::<Vec<_>>(),
                    })
                    .collect::<Vec<_>>(),
            ),
        }
    }
}
