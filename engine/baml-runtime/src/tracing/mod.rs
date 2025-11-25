pub mod api_wrapper;

use std::{
    collections::HashMap,
    sync::{Arc, Mutex},
    time::SystemTime,
};

use ::tracing as rust_tracing;
use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{EvaluationContext, FunctionStart, FunctionType, TraceData, TraceEvent},
    BamlMap, BamlMediaType, BamlValue, BamlValueWithMeta,
};
use cfg_if::cfg_if;
use colored::{ColoredString, Colorize};
use internal_baml_core::ir::ir_helpers::{infer_type, infer_value_with_type};
use internal_baml_jinja::RenderedPrompt;
use jsonish::ResponseBamlValue;
use serde::Serialize;
use tracing::Instrument;
use uuid::Uuid;
use valuable::Valuable;

use self::api_wrapper::{
    core_types::{
        ContentPart, EventChain, IOValue, LLMChat, LLMEventInput, LLMEventInputPrompt,
        LLMEventSchema, LLMOutputModel, LogSchema, LogSchemaContext, MetadataType, Template,
        TypeSchema, IO,
    },
    APIWrapper,
};
use crate::{
    client_registry::ClientRegistry,
    internal::llm_client::LLMResponse,
    on_log_event::LogEventCallbackSync,
    tracing::api_wrapper::core_types::Role,
    tracingv2::storage::storage::{Collector, BAML_TRACER},
    type_builder::TypeBuilder,
    CallCtx, FunctionResult, InnerTraceStats, RuntimeContext, RuntimeContextManager, TestResponse,
    TraceStats,
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

#[derive(Debug, Clone)]
pub struct TracingCall {
    pub call_id: Uuid,
    pub function_name: String,
    pub new_call_id_stack: Vec<baml_ids::FunctionCallId>,
    params: BamlMap<String, BamlValue>,
    start_time: web_time::SystemTime,
    tags: HashMap<String, BamlValue>,
    pub function_type: FunctionType,
}

impl TracingCall {
    pub fn curr_call_id(&self) -> baml_ids::FunctionCallId {
        self.new_call_id_stack
            .last()
            .expect("Call ID chain is empty")
            .clone()
    }
}

pub struct BamlTracer {
    options: APIWrapper,
    tracer: Option<TracerImpl>,
    trace_stats: TraceStats,
}

#[cfg(not(target_arch = "wasm32"))]
static_assertions::assert_impl_all!(BamlTracer: Send, Sync);

/// Trait for types that can be visualized in terminal logs
pub trait Visualize {
    fn visualize(&self, max_chunk_size: impl Into<baml_log::MaxMessageLength> + Clone) -> String;
}

fn log_str() -> ColoredString {
    "...[log trimmed]...".yellow().dimmed()
}

pub fn truncate_string(
    s: &str,
    max_message_length: impl Into<baml_log::MaxMessageLength>,
) -> String {
    let max_message_length = max_message_length.into();
    if let Some(max_size) = max_message_length.maybe_truncate_to(s.len()) {
        let half_size = max_size / 2;
        // We use UTF-8 aware char_indices to get the correct byte index (can't just do s[..half_size])
        let start = s
            .char_indices()
            .take(half_size)
            .map(|(i, _)| i)
            .last()
            .unwrap_or(0);
        let end = s
            .char_indices()
            .rev()
            .take(half_size)
            .map(|(i, _)| i)
            .last()
            .unwrap_or(s.len());
        format!("{}{}{}", &s[..start], log_str(), &s[end..])
    } else {
        s.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_truncate_string() {
        assert_eq!(truncate_string("1234567890", 10), "1234567890".to_string());
        assert_eq!(
            truncate_string("12345678901", 10),
            format!("1234{}78901", log_str())
        );
        assert_eq!(truncate_string("12345678901", 0), "12345678901".to_string());
    }

    #[test]
    fn test_unicode_truncate_string() {
        assert_eq!(
            truncate_string(r#"👍👍👍👍👍👍👍"#, 4),
            format!(r#"👍{}👍👍"#, log_str())
        );
    }
}

impl Visualize for FunctionResult {
    fn visualize(&self, max_chunk_size: impl Into<baml_log::MaxMessageLength> + Clone) -> String {
        let max_chunk_size: baml_log::MaxMessageLength = max_chunk_size.into();
        let mut s = vec![];
        if self.event_chain().len() > 1 {
            s.push(format!(
                "{}",
                format!("({} other previous tries)", self.event_chain().len() - 1).yellow()
            ));
        }
        s.push(self.llm_response().visualize(max_chunk_size));

        match self.result_with_constraints() {
            Some(Ok(val)) => {
                if matches!(self.llm_response(), LLMResponse::Success(_)) {
                    s.push(format!(
                        "{}",
                        format!("---Parsed Response ({})---", val.0.r#type()).blue()
                    ));
                    let json_str = serde_json::to_string_pretty(&val.serialize_final()).unwrap();

                    if let Some(max_size) = max_chunk_size.maybe_truncate_to(json_str.len()) {
                        s.push(truncate_string(&json_str, max_size).to_string());
                    } else {
                        s.push(json_str.to_string());
                    }
                }
            }
            Some(Err(e)) => {
                // Do nothing as self.llm_response().visualize() already prints the error

                // s.push(format!(
                //     "{}",
                //     format!("---Parsed Response ({})---", "Error".red()).blue()
                // ));
                // s.push(format!(
                //     "{}",
                //     truncate_string(&e.to_string(), max_chunk_size).red()
                // ));
            }
            None => {}
        };
        s.join("\n")
    }
}

// A best effort way of serializing the baml_event log into a structured format.
// Users will see this as JSON in their logs (primarily in baml server)
// We may break this at any time.
// It differs from the LogEvent that is sent to the on_log_event callback in that it doesn't include
// actual tracing details like call_id, event_chain, (for now).
#[derive(Serialize)]
struct BamlEventJson {
    // Metadata
    function_name: String,
    start_time: String,
    num_tries: usize,
    total_tries: usize,

    // LLM Info
    client: String,
    model: String,
    latency_ms: u128,
    stop_reason: Option<String>,

    // Content
    prompt: Option<RenderedPrompt>,
    llm_reply: Option<String>,
    // JSON string
    request_options_json: Option<serde_json::Value>,

    // Token Usage
    tokens: Option<TokenUsage>,

    // Response/Error Info
    parsed_response_type: Option<String>,
    parsed_response: Option<serde_json::Value>,
    error: Option<String>,
}

struct BamlEventLoggable<'a> {
    function_name: &'a str,
    call: &'a TracingCall,
    data: &'a Result<FunctionResult>,
}

impl baml_log::Loggable for BamlEventLoggable<'_> {
    fn as_baml_log_string(&self, max_message_length: &baml_log::MaxMessageLength) -> String {
        let function_name = format!("Function {}", self.function_name).purple();
        match self.data.as_ref() {
            Ok(response) => {
                let response = response.visualize(*max_message_length);
                format!("{function_name}:\n{response}")
            }
            Err(error) => {
                format!("{function_name}:\n{error}")
            }
        }
    }

    fn as_baml_log_json(
        &self,
        _: &baml_log::MaxMessageLength,
    ) -> Result<serde_json::Value, baml_log::LogError> {
        serde_json::to_value(self.build_baml_event_json()).map_err(|e| e.into())
    }
}

impl BamlEventLoggable<'_> {
    fn build_baml_event_json(&self) -> BamlEventJson {
        let call = self.call;

        let start_time = to_iso_string(&call.start_time);
        match self.data.as_ref() {
            Ok(response) => {
                let last_ctx = response.llm_response();
                let num_tries = response.event_chain().len();
                let total_tries = response.event_chain().len();
                match last_ctx {
                    LLMResponse::Success(resp) => BamlEventJson {
                        function_name: self.function_name.to_string(),
                        start_time,
                        num_tries,
                        total_tries,
                        client: resp.client.clone(),
                        model: resp.model.clone(),
                        latency_ms: resp.latency.as_millis(),
                        stop_reason: resp.metadata.finish_reason.clone(),
                        prompt: Some(resp.prompt.clone()),
                        llm_reply: Some(resp.content.clone()),
                        request_options_json: Some(
                            serde_json::to_value(&resp.request_options).unwrap_or_default(),
                        ),
                        tokens: Some(TokenUsage {
                            prompt_tokens: resp.metadata.prompt_tokens,
                            completion_tokens: resp.metadata.output_tokens,
                            total_tokens: resp.metadata.total_tokens,
                        }),
                        parsed_response_type: response
                            .result_with_constraints()
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .map(|v| v.0.r#type().to_string()),
                        parsed_response: response
                            .result_with_constraints()
                            .as_ref()
                            .and_then(|r| r.as_ref().ok())
                            .map(|v| serde_json::to_value(v.serialize_final()).unwrap_or_default()),
                        error: None,
                    },
                    LLMResponse::LLMFailure(err) => BamlEventJson {
                        function_name: self.function_name.to_string(),
                        start_time,
                        num_tries,
                        total_tries,
                        client: err.client.clone(),
                        model: err.model.clone().unwrap_or_default(),
                        latency_ms: err.latency.as_millis(),
                        stop_reason: None,
                        prompt: Some(err.prompt.clone()),
                        llm_reply: None,
                        request_options_json: Some(
                            serde_json::to_value(&err.request_options).unwrap_or_default(),
                        ),
                        tokens: None,
                        parsed_response_type: None,
                        parsed_response: None,
                        error: None,
                    },
                    LLMResponse::UserFailure(msg)
                    | LLMResponse::InternalFailure(msg)
                    | LLMResponse::Cancelled(msg) => BamlEventJson {
                        function_name: self.function_name.to_string(),
                        start_time,
                        num_tries,
                        total_tries,
                        client: "unknown".to_string(),
                        model: "unknown".to_string(),
                        latency_ms: 0,
                        stop_reason: None,
                        prompt: None,
                        llm_reply: None,
                        request_options_json: None,
                        tokens: None,
                        parsed_response_type: None,
                        parsed_response: None,
                        error: Some(msg.clone()),
                    },
                }
            }
            Err(error) => BamlEventJson {
                function_name: self.function_name.to_string(),
                start_time,
                num_tries: 0,
                total_tries: 0,
                client: "unknown".to_string(),
                model: "unknown".to_string(),
                latency_ms: 0,
                stop_reason: None,
                prompt: None,
                llm_reply: None,
                request_options_json: None,
                tokens: None,
                parsed_response_type: None,
                parsed_response: None,
                error: Some(error.to_string()),
            },
        }
    }
}

#[derive(Serialize)]
struct TokenUsage {
    prompt_tokens: Option<u64>,
    completion_tokens: Option<u64>,
    total_tokens: Option<u64>,
}

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

    pub(crate) fn start_call(
        &self,
        function_name: &str,
        ctx: &RuntimeContextManager,
        params: &BamlMap<String, BamlValue>,
        is_baml_function: bool,
        is_stream: bool,
        // baml_src_hash: Option<String>,
        collectors: Option<Vec<Arc<Collector>>>,
        tags: Option<&HashMap<String, String>>,
    ) -> TracingCall {
        self.trace_stats.guard().start();
        let (call_id, call_stack, mut ctx_tags, global_tags) = ctx.enter(function_name);

        if let Some(tag_map) = tags {
            if !tag_map.is_empty() {
                log::debug!("start_call: incoming tags: {tag_map:#?}");
                let tag_values: HashMap<String, BamlValue> = tag_map
                    .iter()
                    .map(|(k, v)| (k.clone(), BamlValue::String(v.clone())))
                    .collect();
                ctx.upsert_tags(tag_values.clone());
                ctx_tags.extend(tag_values);
                log::debug!("start_call: ctx_tags after extend: {ctx_tags:#?}");
            }
        }

        log::trace!(
            "\n{}------------------- Entering {:?}, ctx chain {:#?}",
            "        ".repeat(ctx.context_depth()),
            function_name,
            ctx
        );

        let function_type = if is_baml_function {
            FunctionType::BamlLlm
        } else {
            FunctionType::Native
        };

        let call = TracingCall {
            call_id,
            function_name: function_name.to_string(),
            new_call_id_stack: call_stack.clone(),
            params: params.clone(),
            start_time: web_time::SystemTime::now(),
            // Note these tags are the ones currently on the stack. While the function runs we may register
            // more tags with set_tags(). Those are picked up via a diff event (SetTags)
            tags: ctx_tags.clone(),
            function_type: function_type.clone(),
        };
        // println!("---- {} ctx {:#?}", function_name, ctx);
        // baml_log::info!("---- {} ctx {:#?}", function_name, ctx);

        // This must happen before the first event is sent.
        if let Some(collectors) = collectors {
            // log::debug!("collectors: {:#?}", collectors);
            for collector in collectors.iter() {
                collector.track_function(call.curr_call_id());
            }
        }

        // Add function start trace event
        // log::info!("Creating trace event for {}", function_name);
        let trace_event = TraceEvent::new_function_start(
            call_stack,
            function_name.to_string(),
            params
                .iter()
                .map(|(k, v)| (k.clone(), infer_value_with_type(v)))
                .collect(),
            EvaluationContext {
                tags: global_tags
                    .into_iter()
                    .chain(ctx_tags)
                    .map(|(k, v)| (k, serde_json::to_value(v).unwrap_or_default()))
                    .collect(),
            },
            function_type,
            is_stream,
        );
        BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));

        call
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn finish_call(
        &self,
        call: TracingCall,
        ctx: &RuntimeContextManager,
        response: Option<BamlValue>,
    ) -> Result<uuid::Uuid> {
        let guard = self.trace_stats.guard();

        let Some((call_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!(
                "Attempting to finish a call {:#?} without first starting one. Current context {:#?}",
                call,
                ctx
            );
        };

        if call.call_id != call_id {
            anyhow::bail!("Call ID mismatch: {} != {}", call.call_id, call_id);
        }

        if let Some(tracer) = &self.tracer {
            tracer
                .submit(response.to_log_schema(&self.options, event_chain, tags, call))
                .await?;
            guard.done();
            Ok(call_id)
        } else {
            guard.done();
            Ok(call_id)
        }
    }

    // For non-LLM function calls -- used by FFI boundary like with @trace in python
    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finish_call(
        &self,
        call: TracingCall,
        ctx: &RuntimeContextManager,
        response: Option<BamlValue>,
    ) -> Result<uuid::Uuid> {
        use baml_types::type_meta::base::TypeMeta;

        let guard = self.trace_stats.guard();
        let Some((call_id, event_chain, global_and_user_tags)) = ctx.exit() else {
            anyhow::bail!(
                "Attempting to finish a call {:#?} without first starting one. Current context {:#?}",
                call,
                ctx
            );
        };
        log::trace!(
            "\n{}------------------- Finishing call: {:#?} {}\nevent chain {:#?}",
            "        ".repeat(ctx.context_depth()),
            call.function_name,
            call_id,
            event_chain
        );

        if call.call_id != call_id {
            anyhow::bail!("Call ID mismatch: {} != {}", call.call_id, call_id);
        }
        // Tracerv1 code below (deprecate soon)
        if let Some(tracer) = &self.tracer {
            tracer.submit(response.to_log_schema(
                &self.options,
                event_chain,
                global_and_user_tags.clone(),
                call.clone(),
            ))?;
            guard.finalize();
        } else {
            guard.done();
        }

        // Tracerv2 event publishing here
        // Check if this is a Python exception (marked with special __PythonException__ class)
        let is_python_exception =
            matches!(&response, Some(BamlValue::Class(name, _)) if name == "__PythonException__");

        let event = if is_python_exception {
            // Extract error message from the exception
            let error_message = match &response {
                Some(BamlValue::Class(_, fields)) => {
                    let msg = fields
                        .get("message")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Unknown Python exception");
                    let exc_type = fields
                        .get("type")
                        .and_then(|v| v.as_str())
                        .unwrap_or("Exception");
                    format!("{exc_type}: {msg}")
                }
                _ => "Unknown Python exception".to_string(),
            };

            TraceEvent::new_function_end(
                call.new_call_id_stack.clone(),
                Err(baml_types::tracing::events::BamlError::External {
                    message: std::borrow::Cow::Owned(error_message),
                }),
                call.function_type.clone(),
            )
        } else {
            // Normal success case
            let field_type_for_meta = match &response {
                Some(val) => infer_type(val).unwrap_or_else(|| {
                    log::warn!(
                        "Failed to infer FieldType for BamlValue in tracing. Defaulting to Null."
                    );
                    baml_types::ir_type::TypeNonStreaming::Primitive(
                        baml_types::TypeValue::Null,
                        Default::default(),
                    )
                }),
                None => baml_types::ir_type::TypeNonStreaming::Primitive(
                    baml_types::TypeValue::Null,
                    Default::default(),
                ),
            };
            let baml_value_with_meta: BamlValueWithMeta<baml_types::ir_type::TypeNonStreaming> =
                BamlValueWithMeta::with_same_meta_at_all_nodes(
                    response.as_ref().unwrap_or(&baml_types::BamlValue::Null),
                    field_type_for_meta,
                );

            TraceEvent::new_function_end(
                call.new_call_id_stack.clone(),
                Ok(baml_value_with_meta),
                call.function_type,
            )
        };

        BAML_TRACER.lock().unwrap().put(Arc::new(event));

        Ok(call_id)
    }

    #[cfg(target_arch = "wasm32")]
    pub(crate) async fn finish_baml_call(
        &self,
        call: TracingCall,
        ctx: &RuntimeContextManager,
        response: &Result<FunctionResult>,
    ) -> Result<(uuid::Uuid, Vec<baml_ids::FunctionCallId>)> {
        let guard = self.trace_stats.guard();
        let Some((call_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!("Attempting to finish a call without first starting one");
        };

        if call.call_id != call_id {
            anyhow::bail!("Call ID mismatch: {} != {}", call.call_id, call_id);
        }

        if let Ok(response) = &response {
            let name = event_chain.last().map(|s| s.name.as_str());
            let is_ok = response
                .result_with_constraints()
                .as_ref()
                .is_some_and(|r| r.is_ok());
            if is_ok {
                baml_log::info!(
                    "{}{}",
                    name.map(|s| format!("Function {s}:\n"))
                        .unwrap_or_default()
                        .purple(),
                    response.visualize(self.options.config.max_log_chunk_chars())
                );
            } else {
                baml_log::warn!(
                    "{}{}",
                    name.map(|s| format!("Function {s}:\n"))
                        .unwrap_or_default()
                        .purple(),
                    response.visualize(self.options.config.max_log_chunk_chars())
                );
            }
        }
        let new_call_ids = event_chain.iter().map(|s| s.new_call_id.clone()).collect();

        if let Some(tracer) = &self.tracer {
            tracer
                .submit(response.to_log_schema(&self.options, event_chain.clone(), tags, call))
                .await?;
            guard.done();
        } else {
            guard.done();
        }
        Ok((call_id, new_call_ids))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub(crate) fn finish_baml_call(
        &self,
        call: TracingCall,
        ctx: &RuntimeContextManager,
        response: &Result<FunctionResult>,
    ) -> Result<(uuid::Uuid, Vec<baml_ids::FunctionCallId>)> {
        let guard = self.trace_stats.guard();
        let Some((call_id, event_chain, tags)) = ctx.exit() else {
            anyhow::bail!("Attempting to finish a call without first starting one");
        };

        log::trace!(
            "Finishing baml call: {:#?} {}\nevent chain {:#?}",
            call.function_name,
            call_id,
            event_chain
        );

        if call.call_id != call_id {
            anyhow::bail!("Call ID mismatch: {} != {}", call.call_id, call_id);
        }

        let log_level = match response {
            Ok(response) => {
                if response
                    .result_with_constraints()
                    .as_ref()
                    .is_some_and(|r| r.is_ok())
                {
                    baml_log::Level::Info
                } else {
                    baml_log::Level::Warn
                }
            }
            Err(_) => baml_log::Level::Error,
        };

        let event = BamlEventLoggable {
            function_name: event_chain
                .last()
                .map(|s| s.name.as_str())
                .unwrap_or_default(),
            data: response,
            call: &call,
        };

        baml_log::elog!(log_level, &event);

        let new_call_ids = event_chain.iter().map(|s| s.new_call_id.clone()).collect();
        if let Some(tracer) = &self.tracer {
            tracer.submit(response.to_log_schema(&self.options, event_chain, tags, call))?;
            guard.finalize();
        } else {
            guard.done();
        }
        Ok((call_id, new_call_ids))
    }

    /// Returns true if the tracer's config matches the config from the given env vars.
    pub fn config_matches_env_vars(
        &self,
        env_vars: &std::collections::HashMap<String, String>,
    ) -> bool {
        // Try to create a new APIWrapper from the env vars
        if let Ok(new_api_wrapper) =
            crate::tracing::api_wrapper::APIWrapper::from_env_vars(env_vars.iter())
        {
            // Compare the config in the current APIWrapper to the new one
            self.options.config == new_api_wrapper.config
        } else {
            false
        }
    }

    pub fn tracing_project_id(&self) -> Option<String> {
        self.options.project_id().map(str::to_string)
    }
}

fn log_json_event(is_ok: bool, log_event: BamlEventJson) -> Result<()> {
    if is_ok {
        baml_log::info!("{}", serde_json::to_string(&log_event)?);
    } else {
        baml_log::warn!("{}", serde_json::to_string(&log_event)?);
    }
    Ok(())
}

fn log_simple_event(
    is_ok: bool,
    name: Option<&str>,
    response: &FunctionResult,
    options: &APIWrapper,
) {
    if is_ok {
        baml_log::info!(
            "{}{}",
            name.map(|s| format!("Function {s}:\n"))
                .unwrap_or_default()
                .purple(),
            response.visualize(options.config.max_log_chunk_chars())
        );
    } else {
        baml_log::warn!(
            "{}{}",
            name.map(|s| format!("Function {s}:\n"))
                .unwrap_or_default()
                .purple(),
            response.visualize(options.config.max_log_chunk_chars())
        );
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
        Vec<CallCtx>,
        HashMap<String, BamlValue>,
        &TracingCall,
    )> for LogSchemaContext
{
    fn from(
        (api, event_chain, tags, call): (
            &APIWrapper,
            Vec<CallCtx>,
            HashMap<String, BamlValue>,
            &TracingCall,
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
            latency_ms: call
                .start_time
                .elapsed()
                .map(|d| d.as_millis() as i128)
                .unwrap_or(0),
            process_id: api.session_id().to_string(),
            tags: tags
                .into_iter()
                .map(|(k, v)| match v.as_str() {
                    Some(v) => (k, v.to_string()),
                    None => (
                        k,
                        serde_json::to_string(&v).unwrap_or_else(|_| "<unknown>".to_string()),
                    ),
                })
                .chain(std::iter::once((
                    "baml.runtime".to_string(),
                    env!("CARGO_PKG_VERSION").to_string(),
                )))
                .collect(),
            event_chain: parent_chain,
            start_time: to_iso_string(&call.start_time),
        }
    }
}

impl From<&BamlMap<String, BamlValue>> for IOValue {
    fn from(items: &BamlMap<String, BamlValue>) -> Self {
        log::trace!("Converting IOValue from BamlMap: {items:#?}");
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
    match result.result_with_constraints() {
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
            LLMResponse::UserFailure(s) => Some(api_wrapper::core_types::Error {
                code: 2,
                message: s.clone(),
                traceback: None,
                r#override: None,
            }),
            LLMResponse::InternalFailure(s) => Some(api_wrapper::core_types::Error {
                code: 2,
                message: s.clone(),
                traceback: None,
                r#override: None,
            }),
            LLMResponse::Cancelled(s) => Some(api_wrapper::core_types::Error {
                code: 2,
                message: format!("Cancelled: {s}"),
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
        event_chain: Vec<CallCtx>,
        tags: HashMap<String, BamlValue>,
        call: TracingCall,
    ) -> LogSchema;
}

impl<T: ToLogSchema> ToLogSchema for Result<T> {
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<CallCtx>,
        tags: HashMap<String, BamlValue>,
        call: TracingCall,
    ) -> LogSchema {
        match self {
            Ok(r) => r.to_log_schema(api, event_chain, tags, call),
            Err(e) => LogSchema {
                project_id: api.project_id().map(str::to_string),
                event_type: api_wrapper::core_types::EventType::FuncCode,
                root_event_id: event_chain.first().map(|s| s.call_id).unwrap().to_string(),
                event_id: event_chain.last().map(|s| s.call_id).unwrap().to_string(),
                parent_event_id: None,
                context: (api, event_chain, tags, &call).into(),
                io: IO {
                    input: Some((&call.params).into()),
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
        event_chain: Vec<CallCtx>,
        tags: HashMap<String, BamlValue>,
        call: TracingCall,
    ) -> LogSchema {
        LogSchema {
            project_id: api.project_id().map(str::to_string),
            event_type: api_wrapper::core_types::EventType::FuncCode,
            root_event_id: event_chain.first().map(|s| s.call_id).unwrap().to_string(),
            event_id: event_chain.last().map(|s| s.call_id).unwrap().to_string(),
            parent_event_id: if event_chain.len() >= 2 {
                event_chain
                    .get(event_chain.len() - 2)
                    .map(|s| s.call_id.to_string())
            } else {
                None
            },
            context: (api, event_chain, tags, &call).into(),
            io: IO {
                input: Some((&call.params).into()),
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
        event_chain: Vec<CallCtx>,
        tags: HashMap<String, BamlValue>,
        call: TracingCall,
    ) -> LogSchema {
        if let Some(func_response) = &self.function_response {
            func_response.to_log_schema(api, event_chain, tags, call)
        } else {
            // For expr functions, create a simpler log schema
            LogSchema {
                project_id: api.project_id().map(str::to_string),
                event_type: api_wrapper::core_types::EventType::FuncCode,
                root_event_id: event_chain.first().map(|s| s.call_id).unwrap().to_string(),
                event_id: event_chain.last().map(|s| s.call_id).unwrap().to_string(),
                parent_event_id: None,
                context: (api, event_chain, tags, &call).into(),
                io: IO {
                    input: Some((&call.params).into()),
                    output: self
                        .expr_function_response
                        .as_ref()
                        .and_then(|r| r.as_ref().ok())
                        .map(|r| {
                            let v: BamlValue = r.0.clone().into();
                            IOValue::from(&v)
                        }),
                },
                error: self
                    .expr_function_response
                    .as_ref()
                    .and_then(|r| r.as_ref().err())
                    .map(|e| api_wrapper::core_types::Error {
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

impl ToLogSchema for FunctionResult {
    fn to_log_schema(
        &self,
        api: &APIWrapper,
        event_chain: Vec<CallCtx>,
        tags: HashMap<String, BamlValue>,
        call: TracingCall,
    ) -> LogSchema {
        LogSchema {
            project_id: api.project_id().map(str::to_string),
            event_type: api_wrapper::core_types::EventType::FuncLlm,
            root_event_id: event_chain.first().map(|s| s.call_id).unwrap().to_string(),
            event_id: event_chain.last().map(|s| s.call_id).unwrap().to_string(),
            // Second to last element in the event chain
            parent_event_id: if event_chain.len() >= 2 {
                event_chain
                    .get(event_chain.len() - 2)
                    .map(|s| s.call_id.to_string())
            } else {
                None
            },
            context: (api, event_chain, tags, &call).into(),
            io: IO {
                input: Some((&call.params).into()),
                output: self
                    .result_with_constraints()
                    .as_ref()
                    .and_then(|r| r.as_ref().ok())
                    .map(|r| {
                        let v: BamlValue = r.0.clone().into();
                        IOValue::from(&v)
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
            LLMResponse::UserFailure(s) => LLMEventSchema {
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
            LLMResponse::InternalFailure(s) => LLMEventSchema {
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
            LLMResponse::Cancelled(s) => LLMEventSchema {
                model_name: "<unknown>".into(),
                provider: "<unknown>".into(),
                input: LLMEventInput {
                    prompt: LLMEventInputPrompt {
                        template: Template::Single("<cancelled>".into()),
                        template_args: Default::default(),
                        r#override: None,
                    },
                    request_options: Default::default(),
                },
                output: None,
                error: Some(format!("Cancelled: {s}")),
            },
        }
    }
}

impl From<&internal_baml_jinja::ChatMessagePart> for ContentPart {
    fn from(value: &internal_baml_jinja::ChatMessagePart) -> Self {
        match value {
            internal_baml_jinja::ChatMessagePart::Text(t) => ContentPart::Text(t.clone()),
            internal_baml_jinja::ChatMessagePart::Media(media) => {
                match (media.media_type, &media.content) {
                    // File
                    (BamlMediaType::Image, baml_types::BamlMediaContent::File(data)) => {
                        ContentPart::FileImage(
                            data.span_path.to_string_lossy().into_owned(),
                            data.relpath.to_string_lossy().into_owned(),
                        )
                    }
                    (BamlMediaType::Audio, baml_types::BamlMediaContent::File(data)) => {
                        ContentPart::FileAudio(
                            data.span_path.to_string_lossy().into_owned(),
                            data.relpath.to_string_lossy().into_owned(),
                        )
                    }
                    (BamlMediaType::Pdf, baml_types::BamlMediaContent::File(data)) => {
                        ContentPart::FilePdf(
                            data.span_path.to_string_lossy().into_owned(),
                            data.relpath.to_string_lossy().into_owned(),
                        )
                    }
                    (BamlMediaType::Video, baml_types::BamlMediaContent::File(data)) => {
                        ContentPart::FileVideo(
                            data.span_path.to_string_lossy().into_owned(),
                            data.relpath.to_string_lossy().into_owned(),
                        )
                    }

                    // Base64
                    (BamlMediaType::Image, baml_types::BamlMediaContent::Base64(data)) => {
                        ContentPart::B64Image(data.base64.clone())
                    }
                    (BamlMediaType::Audio, baml_types::BamlMediaContent::Base64(data)) => {
                        ContentPart::B64Audio(data.base64.clone())
                    }
                    (BamlMediaType::Pdf, baml_types::BamlMediaContent::Base64(data)) => {
                        ContentPart::B64Pdf(data.base64.clone())
                    }
                    (BamlMediaType::Video, baml_types::BamlMediaContent::Base64(data)) => {
                        ContentPart::B64Video(data.base64.clone())
                    }

                    // Url
                    (BamlMediaType::Image, baml_types::BamlMediaContent::Url(data)) => {
                        ContentPart::UrlImage(data.url.clone())
                    }
                    (BamlMediaType::Audio, baml_types::BamlMediaContent::Url(data)) => {
                        ContentPart::UrlAudio(data.url.clone())
                    }
                    (BamlMediaType::Pdf, baml_types::BamlMediaContent::Url(data)) => {
                        ContentPart::UrlPdf(data.url.clone())
                    }
                    (BamlMediaType::Video, baml_types::BamlMediaContent::Url(data)) => {
                        ContentPart::UrlVideo(data.url.clone())
                    }
                }
            }
            internal_baml_jinja::ChatMessagePart::WithMeta(inner, meta) => ContentPart::WithMeta(
                Box::new(inner.as_ref().into()),
                meta.iter().map(|(k, v)| (k.clone(), v.clone())).collect(),
            ),
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
                        content: c.parts.iter().map(|p| p.into()).collect::<Vec<_>>(),
                    })
                    .collect::<Vec<_>>(),
            ),
        }
    }
}
