mod call;
mod stream;

use std::{collections::HashMap, sync::Arc};

use anyhow::Result;
use baml_ids::HttpRequestId;
use baml_types::{
    tracing::events::{
        HTTPRequest, HTTPResponse, LLMChatMessage, LLMChatMessagePart, LLMUsage, LoggedLLMRequest,
        LoggedLLMResponse, TraceData, TraceEvent,
    },
    BamlValue,
};
pub use call::orchestrate as orchestrate_call;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::{ChatMessagePart, RenderedChatMessage, RenderedPrompt};
use serde::Serialize;
use serde_json::json;
pub use stream::orchestrate_stream;
use web_time::Duration; // Add this line
use web_time::SystemTime;

pub use super::primitive::LLMPrimitiveProvider;
use super::{
    strategy::roundrobin::RoundRobinStrategy,
    traits::{
        HttpContext, StreamResponse, WithClientProperties, WithPrompt, WithRenderRawCurl,
        WithSingleCallable, WithStreamable,
    },
    LLMCompleteResponse, LLMResponse,
};
use crate::{
    internal::prompt_renderer::PromptRenderer,
    runtime_interface::InternalClientLookup,
    tracing::Visualize,
    tracingv2::storage::{make_trace_event_for_response, storage::BAML_TRACER},
    RenderCurlSettings, RuntimeContext,
};
pub struct OrchestratorNode {
    pub scope: OrchestrationScope,
    pub provider: Arc<LLMPrimitiveProvider>,
}

impl std::fmt::Display for ExecutionScope {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExecutionScope::Direct(s) => write!(f, "{s}"),
            ExecutionScope::Retry(policy, count, delay) => {
                write!(f, "Retry({}, {}, {}ms)", policy, count, delay.as_millis())
            }
            ExecutionScope::RoundRobin(strategy, index) => {
                write!(f, "RoundRobin({}, {})", strategy.name, index)
            }
            ExecutionScope::Fallback(strategy, index) => {
                write!(f, "Fallback({strategy}, {index})")
            }
        }
    }
}

impl std::fmt::Display for OrchestratorNode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "OrchestratorNode: [")?;
        for scope in &self.scope.scope {
            write!(f, "{scope} + ")?;
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
            .map(|scope| format!("{scope}"))
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
    async fn single_call(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> LLMResponse {
        // Create IDs for the function call and content
        {
            let request = LoggedLLMRequest {
                request_id: ctx.http_request_id().clone(),
                client_name: self.provider.name().to_string(),
                client_provider: self.provider.provider_name().to_string(),
                params: self.provider.request_options().clone(),
                prompt: match prompt {
                    RenderedPrompt::Chat(chat) => chat
                        .iter()
                        .map(|m| LLMChatMessage {
                            role: m.role.clone(),
                            content: m.parts.iter().map(|p| p.into()).collect(),
                        })
                        .collect(),
                    RenderedPrompt::Completion(completion) => {
                        todo!("not implemented")
                    }
                },
            };

            let event = TraceEvent::new_llm_request(
                ctx.runtime_context().call_id_stack.clone(),
                Arc::new(request),
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(event));
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
        let response = self.provider.single_call(ctx, prompt).await;

        // After we get the response, log LLMResponse
        {
            let trace_event = make_trace_event_for_response(
                &response,
                ctx.runtime_context().call_id_stack.clone(),
                ctx.http_request_id(),
                self.scope
                    .scope
                    .iter()
                    .map(ExecutionScope::to_string)
                    .collect(),
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
        }

        response
    }
}

impl WithStreamable for OrchestratorNode {
    async fn stream(&self, ctx: &impl HttpContext, prompt: &RenderedPrompt) -> StreamResponse {
        {
            let request = LoggedLLMRequest {
                request_id: ctx.http_request_id().clone(),
                client_name: self.provider.name().to_string(),
                client_provider: self.provider.provider_name().to_string(),
                params: self.provider.request_options().clone(),
                prompt: match prompt {
                    RenderedPrompt::Chat(chat) => chat
                        .iter()
                        .map(|m| LLMChatMessage {
                            role: m.role.clone(),
                            content: m.parts.iter().map(|p| p.into()).collect(),
                        })
                        .collect(),
                    RenderedPrompt::Completion(completion) => {
                        todo!("not implemented")
                    }
                },
            };

            let event = TraceEvent::new_llm_request(
                ctx.runtime_context().call_id_stack.clone(),
                Arc::new(request),
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(event));
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
        let result = self.provider.stream(ctx, prompt).await;

        // We do not log the full LLMResponse here the same way as single_call,
        // because streaming typically emits chunked events. If you want to log
        // events at the end of the stream, you could handle it after collecting
        // all chunks. For now, just log any immediate error:
        if let Err(err_resp) = &result {
            let trace_event = make_trace_event_for_response(
                err_resp,
                ctx.runtime_context().call_id_stack.clone(),
                ctx.http_request_id(),
                self.scope
                    .scope
                    .iter()
                    .map(ExecutionScope::to_string)
                    .collect(),
            );
            BAML_TRACER.lock().unwrap().put(Arc::new(trace_event));
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
