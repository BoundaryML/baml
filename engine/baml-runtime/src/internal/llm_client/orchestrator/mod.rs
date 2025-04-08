mod call;
mod stream;

use baml_types::tracing::events::{HttpRequestId, LLMUsage};
use serde_json::json;
use web_time::Duration; // Add this line

use crate::tracingv2::storage::make_trace_event_for_response;
use crate::tracingv2::storage::storage::BAML_TRACER;
use crate::RenderCurlSettings;
use crate::{
    internal::prompt_renderer::PromptRenderer, runtime_interface::InternalClientLookup,
    RuntimeContext,
};

use super::traits::{WithClientProperties, WithRenderRawCurl};
use super::LLMCompleteResponse;
use super::{
    strategy::roundrobin::RoundRobinStrategy,
    traits::{StreamResponse, WithPrompt, WithSingleCallable, WithStreamable},
    LLMResponse,
};

pub use super::primitive::LLMPrimitiveProvider;
pub use call::orchestrate as orchestrate_call;
pub use stream::orchestrate_stream;

use crate::tracing::Visualize;
use anyhow::Result;
use baml_types::tracing::events::{
    ContentId, FunctionId, HTTPRequest, HTTPResponse, LoggedLLMRequest, LoggedLLMResponse,
    TraceData, TraceEvent, TraceLevel,
};
use baml_types::BamlValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::RenderedChatMessage;
use internal_baml_jinja::RenderedPrompt;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
use web_time::SystemTime;
pub struct OrchestratorNode {
    pub scope: OrchestrationScope,
    pub provider: Arc<LLMPrimitiveProvider>,
}

impl std::fmt::Display for ExecutionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionScope::Direct(s) => write!(f, "{}", s),
            ExecutionScope::Retry(policy, count, delay) => {
                write!(f, "Retry({}, {}, {}ms)", policy, count, delay.as_millis())
            }
            ExecutionScope::RoundRobin(strategy, index) => {
                write!(f, "RoundRobin({}, {})", strategy.name, index)
            }
            ExecutionScope::Fallback(strategy, index) => {
                write!(f, "Fallback({}, {})", strategy, index)
            }
        }
    }
}

impl std::fmt::Display for OrchestratorNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OrchestratorNode: [")?;
        for scope in &self.scope.scope {
            write!(f, "{} + ", scope)?;
        }
        write!(f, "{}]", self.provider)
    }
}

impl OrchestratorNode {
    pub fn new(scope: impl Into<OrchestrationScope>, provider: Arc<LLMPrimitiveProvider>) -> Self {
        OrchestratorNode {
            scope: scope.into(),
            provider,
        }
    }

    pub fn prefix(&self, scope: impl Into<OrchestrationScope>) -> OrchestratorNode {
        OrchestratorNode {
            scope: self.scope.prefix_scopes(scope.into().scope),
            provider: self.provider.clone(),
        }
    }

    pub fn error_sleep_duration(&self) -> Option<&Duration> {
        // in reverse find the first retry scope, and return the delay
        self.scope.scope.iter().rev().find_map(|scope| match scope {
            ExecutionScope::Retry(_, _, delay) if !delay.is_zero() => Some(delay),
            _ => None,
        })
    }
}

#[derive(Debug, Default, Clone)]
pub struct OrchestrationScope {
    pub scope: Vec<ExecutionScope>,
}

impl From<ExecutionScope> for OrchestrationScope {
    fn from(scope: ExecutionScope) -> Self {
        OrchestrationScope { scope: vec![scope] }
    }
}

impl From<Vec<ExecutionScope>> for OrchestrationScope {
    fn from(scope: Vec<ExecutionScope>) -> Self {
        OrchestrationScope { scope }
    }
}

impl OrchestrationScope {
    pub fn name(&self) -> String {
        self.scope
            .iter()
            .filter(|scope| !matches!(scope, ExecutionScope::Retry(..)))
            .map(|scope| format!("{}", scope))
            .collect::<Vec<_>>()
            .join(" + ")
    }

    pub fn extend(&self, scope: ExecutionScope) -> OrchestrationScope {
        OrchestrationScope {
            scope: self
                .scope
                .clone()
                .into_iter()
                .chain(std::iter::once(scope))
                .collect(),
        }
    }

    pub fn prefix_scopes(&self, scopes: Vec<ExecutionScope>) -> OrchestrationScope {
        OrchestrationScope {
            scope: scopes.into_iter().chain(self.scope.clone()).collect(),
        }
    }

    pub fn direct_client_name(&self) -> Option<&String> {
        match self.scope.last() {
            Some(ExecutionScope::Direct(d)) => Some(d),
            _ => None,
        }
    }
}

#[derive(Clone, Debug)]
pub enum ExecutionScope {
    Direct(String),
    // PolicyName, RetryCount, RetryDelayMs
    Retry(String, usize, Duration),
    // StrategyName, ClientIndex
    RoundRobin(Arc<RoundRobinStrategy>, usize),
    // StrategyName, ClientIndex
    Fallback(String, usize),
}

pub type OrchestratorNodeIterator = Vec<OrchestratorNode>;

#[derive(Default)]
pub struct OrchestrationState {
    // Number of times a client was used so far
    pub client_to_usage: HashMap<String, usize>,
}

pub trait IterOrchestrator {
    fn iter_orchestrator<'a>(
        &self,
        state: &mut OrchestrationState,
        previous: OrchestrationScope,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<OrchestratorNodeIterator>;
}

impl<'ir> WithPrompt<'ir> for OrchestratorNode {
    async fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt> {
        self.provider.render_prompt(ir, renderer, ctx, params).await
    }
}

impl WithRenderRawCurl for OrchestratorNode {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &[RenderedChatMessage],
        render_settings: RenderCurlSettings,
    ) -> Result<String> {
        self.provider
            .render_raw_curl(ctx, prompt, render_settings)
            .await
    }
}

impl WithSingleCallable for OrchestratorNode {
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
        http_request_id: HttpRequestId,
    ) -> LLMResponse {
        // Create IDs for the function call and content
        if let Some(span_id) = ctx.span_id {
            let function_id = FunctionId(span_id.to_string());
            let event_id = ContentId(uuid::Uuid::new_v4().to_string());
            // Log LLMRequest
            BAML_TRACER.lock().unwrap().put(Arc::new(TraceEvent {
                span_id: function_id.clone(),
                event_id: event_id.clone(),
                span_chain: vec![],
                timestamp: SystemTime::now(),
                callsite: "OrchestratorNode::single_call".to_string(),
                verbosity: TraceLevel::Info,
                content: TraceData::LLMRequest(Arc::new(LoggedLLMRequest {
                    request_id: http_request_id.clone(),
                    client_name: self.provider.name().to_string(),
                    client_provider: self.provider.provider_name().to_string(),
                    params: serde_json::json!({
                        "request_options": "",
                    }),
                    // Some placeholder JSON representation of the prompt
                    prompt: serde_json::to_value(prompt).unwrap(),
                })),
                tags: Default::default(),
            }));
        } else {
            log::warn!(
                "No span id found for function while emitting logs. Log event may be dropped."
            );
        }

        // Possibly increment RoundRobin scope
        self.scope
            .scope
            .iter()
            .filter_map(|scope| match scope {
                ExecutionScope::RoundRobin(a, _) => Some(a),
                _ => None,
            })
            .map(|a| a.increment_index())
            .for_each(drop);

        // Call the underlying LLM
        let response = self
            .provider
            .single_call(ctx, prompt, http_request_id.clone())
            .await;

        // After we get the response, log LLMResponse
        if let Some(span_id) = ctx.span_id {
            let function_id = FunctionId(span_id.to_string());
            let trace_event = make_trace_event_for_response(
                &response,
                &function_id,
                &http_request_id,
                "OrchestratorNode::single_call::response",
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
        } else {
            log::warn!(
                "No span id found for function while emitting logs. Log event may be dropped."
            );
        }

        response
    }
}

impl WithStreamable for OrchestratorNode {
    async fn stream(
        &self,
        ctx: &RuntimeContext,
        prompt: &RenderedPrompt,
        http_request_id: HttpRequestId,
    ) -> StreamResponse {
        let request_id = ContentId(uuid::Uuid::new_v4().to_string());
        let event_id = ContentId(uuid::Uuid::new_v4().to_string());

        // Log streaming request
        if let Some(span_id) = ctx.span_id {
            BAML_TRACER.lock().unwrap().put(Arc::new(TraceEvent {
                span_id: FunctionId(span_id.to_string()),
                event_id: event_id.clone(),
                span_chain: vec![],
                timestamp: SystemTime::now(),
                callsite: "OrchestratorNode::stream".to_string(),
                verbosity: TraceLevel::Info,
                content: TraceData::LLMRequest(Arc::new(LoggedLLMRequest {
                    request_id: http_request_id.clone(),
                    client_name: self.provider.name().to_string(),
                    client_provider: self.provider.provider_name().to_string(),
                    params: serde_json::json!({
                        "request_options": "",
                    }),
                    prompt: serde_json::to_value(prompt).unwrap(),
                })),
                tags: Default::default(),
            }));
        } else {
            log::warn!(
                "No span id found for function while emitting logs. Log event may be dropped."
            );
        }

        // Possibly increment RoundRobin scope
        self.scope
            .scope
            .iter()
            .filter_map(|scope| match scope {
                ExecutionScope::RoundRobin(a, _) => Some(a),
                _ => None,
            })
            .map(|a| a.increment_index())
            .for_each(drop);

        // Perform the streaming call
        let result = self
            .provider
            .stream(ctx, prompt, http_request_id.clone())
            .await;

        // We do not log the full LLMResponse here the same way as single_call,
        // because streaming typically emits chunked events. If you want to log
        // events at the end of the stream, you could handle it after collecting
        // all chunks. For now, just log any immediate error:
        if let Err(err_resp) = &result {
            if let Some(span_id) = ctx.span_id {
                let function_id = FunctionId(span_id.to_string());
                let trace_event = make_trace_event_for_response(
                    err_resp,
                    &function_id,
                    &http_request_id,
                    "OrchestratorNode::stream",
                );
                BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
            } else {
                log::warn!(
                    "No span id found for function while emitting logs. Log event may be dropped."
                );
            }
        }

        result
    }
}

impl WithClientProperties for OrchestratorNode {
    fn default_role(&self) -> String {
        self.provider.default_role()
    }

    fn allowed_metadata(&self) -> &internal_llm_client::AllowedRoleMetadata {
        self.provider.allowed_metadata()
    }

    fn supports_streaming(&self) -> bool {
        self.provider.supports_streaming()
    }

    fn finish_reason_filter(&self) -> &internal_llm_client::FinishReasonFilter {
        self.provider.finish_reason_filter()
    }

    fn allowed_roles(&self) -> Vec<String> {
        self.provider.allowed_roles()
    }
}
