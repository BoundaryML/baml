use std::sync::Arc;

use anyhow::Result;
mod fallback;
pub mod roundrobin;

use internal_baml_core::ir::ClientWalker;

use crate::{runtime_interface::InternalClientLookup, RuntimeContext};

use self::{fallback::FallbackStrategy, roundrobin::RoundRobinStrategy};

use super::{
    orchestrator::{
        IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
        OrchestratorNodeIterator,
    },
    traits::WithRetryPolicy,
};

pub enum LLMStrategyProvider {
    RoundRobin(Arc<RoundRobinStrategy>),
    Fallback(FallbackStrategy),
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for LLMStrategyProvider {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        match client.elem().provider.as_str() {
            "baml-round-robin" | "round-robin" => RoundRobinStrategy::try_from((client, ctx))
                .map(Arc::new)
                .map(LLMStrategyProvider::RoundRobin),
            "baml-fallback" | "fallback" => {
                FallbackStrategy::try_from((client, ctx)).map(LLMStrategyProvider::Fallback)
            }
            other => {
                let options = ["round-robin", "fallback"];
                anyhow::bail!(
                    "Unsupported strategy provider: {}. Available ones are: {}",
                    other,
                    options.join(", ")
                )
            }
        }
    }
}

impl WithRetryPolicy for LLMStrategyProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match self {
            LLMStrategyProvider::RoundRobin(strategy) => strategy.retry_policy.as_deref(),
            LLMStrategyProvider::Fallback(strategy) => strategy.retry_policy.as_deref(),
        }
    }
}

impl IterOrchestrator for LLMStrategyProvider {
    fn iter_orchestrator<'a>(
        &self,
        state: &mut OrchestrationState,
        previous: OrchestrationScope,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> OrchestratorNodeIterator {
        match self {
            LLMStrategyProvider::Fallback(f) => {
                f.iter_orchestrator(state, previous, ctx, client_lookup)
            }
            LLMStrategyProvider::RoundRobin(r) => {
                r.iter_orchestrator(state, previous, ctx, client_lookup)
            }
        }
    }
}
