mod call;
mod stream;

use web_time::Duration; // Add this line

use crate::{
    internal::prompt_renderer::PromptRenderer, runtime_interface::InternalClientLookup,
    RuntimeContext,
};

use super::traits::WithRenderRawCurl;
use super::{
    strategy::roundrobin::RoundRobinStrategy,
    traits::{StreamResponse, WithPrompt, WithSingleCallable, WithStreamable},
    LLMResponse,
};

pub use super::primitive::LLMPrimitiveProvider;
pub use call::orchestrate as orchestrate_call;
pub use stream::orchestrate_stream;

use anyhow::Result;
use baml_types::BamlValue;
use internal_baml_core::ir::repr::IntermediateRepr;
use internal_baml_jinja::RenderedChatMessage;
use internal_baml_jinja::RenderedPrompt;
use serde::Serialize;
use std::{collections::HashMap, sync::Arc};
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

#[derive(Default, Clone, Serialize)]
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
            scope: scopes
                .into_iter()
                .chain(self.scope.clone().into_iter())
                .collect(),
        }
    }
}

#[derive(Clone, Serialize)]
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
    ) -> OrchestratorNodeIterator;
}

impl<'ir> WithPrompt<'ir> for OrchestratorNode {
    fn render_prompt(
        &'ir self,
        ir: &'ir IntermediateRepr,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlValue,
    ) -> Result<RenderedPrompt> {
        self.provider.render_prompt(ir, renderer, ctx, params)
    }
}

impl WithRenderRawCurl for OrchestratorNode {
    async fn render_raw_curl(
        &self,
        ctx: &RuntimeContext,
        prompt: &Vec<RenderedChatMessage>,
        stream: bool,
    ) -> Result<String> {
        self.provider.render_raw_curl(ctx, prompt, stream).await
    }
}

impl WithSingleCallable for OrchestratorNode {
    async fn single_call(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> LLMResponse {
        self.scope
            .scope
            .iter()
            .filter_map(|scope| match scope {
                ExecutionScope::RoundRobin(a, _) => Some(a),
                _ => None,
            })
            .map(|a| a.increment_index())
            .for_each(drop);
        self.provider.single_call(ctx, prompt).await
    }
}

impl WithStreamable for OrchestratorNode {
    async fn stream(&self, ctx: &RuntimeContext, prompt: &RenderedPrompt) -> StreamResponse {
        self.scope
            .scope
            .iter()
            .filter_map(|scope| match scope {
                ExecutionScope::RoundRobin(a, _) => Some(a),
                _ => None,
            })
            .map(|a| a.increment_index())
            .for_each(drop);
        self.provider.stream(ctx, prompt).await
    }
}
