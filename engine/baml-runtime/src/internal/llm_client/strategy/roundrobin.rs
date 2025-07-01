use std::{
    collections::HashMap,
    fmt::Debug,
    sync::{atomic::AtomicUsize, Arc},
};

use anyhow::{Context, Result};
use internal_baml_core::ir::ClientWalker;
use internal_llm_client::{
    ClientProvider, ClientSpec, ResolvedClientProperty, UnresolvedClientProperty,
};
use serde::{Serialize, Serializer};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
        OrchestratorNodeIterator,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};

#[derive(Debug)]
pub struct RoundRobinStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    client_specs: Vec<ClientSpec>,
    current_index: AtomicUsize,
}

fn serialize_atomic<S>(value: &AtomicUsize, serializer: S) -> Result<S::Ok, S::Error>
where
    S: Serializer,
{
    serializer.serialize_u64(value.load(std::sync::atomic::Ordering::Relaxed) as u64)
}

impl RoundRobinStrategy {
    pub fn current_index(&self) -> usize {
        self.current_index
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn increment_index(&self) {
        self.current_index
            .fetch_add(1, std::sync::atomic::Ordering::Relaxed);
    }
}

fn resolve_strategy(
    provider: &ClientProvider,
    properties: &UnresolvedClientProperty<()>,
    ctx: &RuntimeContext,
) -> Result<(Vec<ClientSpec>, usize)> {
    let properties = properties.resolve(provider, &ctx.eval_ctx(false))?;
    let ResolvedClientProperty::RoundRobin(props) = properties else {
        anyhow::bail!(
            "Invalid client property. Should have been a round-robin property but got: {}",
            properties.name()
        );
    };
    let start = match props.start_index {
        Some(start) => (start as usize) % props.strategy.len(),
        None => {
            if cfg!(target_arch = "wasm32") {
                // For VSCode, we don't want a random start point,
                // as it can make rendering inconsistent
                0
            } else {
                fastrand::usize(..props.strategy.len())
            }
        }
    };
    Ok((props.strategy, start))
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from(
        (client, ctx): (&ClientProperty, &RuntimeContext),
    ) -> std::result::Result<Self, Self::Error> {
        let (strategy, start) =
            resolve_strategy(&client.provider, &client.unresolved_options()?, ctx)?;

        Ok(RoundRobinStrategy {
            name: client.name.clone(),
            retry_policy: client.retry_policy.clone(),
            client_specs: strategy,
            current_index: AtomicUsize::new(start),
        })
    }
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let (strategy, start) = resolve_strategy(&client.elem().provider, client.options(), ctx)?;
        Ok(Self {
            name: client.item.elem.name.clone(),
            retry_policy: client.retry_policy().as_ref().map(String::from),
            client_specs: strategy,
            current_index: AtomicUsize::new(start),
        })
    }
}

impl IterOrchestrator for Arc<RoundRobinStrategy> {
    fn iter_orchestrator<'a>(
        &self,
        state: &mut OrchestrationState,
        _previous: OrchestrationScope,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<OrchestratorNodeIterator> {
        let offset = state.client_to_usage.entry(self.name.clone()).or_insert(0);
        let next = (self.current_index() + *offset) % self.client_specs.len();

        // Update the usage count
        *offset += 1;

        let client_spec = &self.client_specs[next];
        let client = client_lookup.get_llm_provider(client_spec, ctx).unwrap();
        let client = client.clone();
        client.iter_orchestrator(
            state,
            ExecutionScope::RoundRobin(self.clone(), next).into(),
            ctx,
            client_lookup,
        )
    }
}
