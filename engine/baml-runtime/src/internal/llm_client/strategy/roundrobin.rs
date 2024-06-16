use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    sync::{atomic::AtomicUsize, Arc},
};

use internal_baml_core::ir::ClientWalker;

use crate::{
    client_builder::ClientProperty,
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
        OrchestratorNodeIterator,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};

pub struct RoundRobinStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    clients: Vec<String>,
    current_index: AtomicUsize,
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

impl TryFrom<(&ClientProperty, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from(
        (client, ctx): (&ClientProperty, &RuntimeContext),
    ) -> std::result::Result<Self, Self::Error> {
        let (strategy, start) = resolve_properties(
            client
                .options
                .iter()
                .map(|(k, v)| Ok((k.clone(), serde_json::json!(v))))
                .collect::<Result<HashMap<_, _>>>()?,
            ctx,
        )?;

        Ok(RoundRobinStrategy {
            name: client.name.clone(),
            retry_policy: client.retry_policy.clone(),
            clients: strategy,
            current_index: AtomicUsize::new(start),
        })
    }
}

fn resolve_properties(
    mut properties: HashMap<String, serde_json::Value>,
    _ctx: &RuntimeContext,
) -> Result<(Vec<String>, usize)> {
    let strategy = properties
        .remove("strategy")
        .map(|v| serde_json::from_value::<Vec<String>>(v))
        .transpose()
        .context("Failed to resolve strategy into string[]")?;

    let strategy = if let Some(strategy) = strategy {
        if strategy.is_empty() {
            anyhow::bail!("Empty strategy array, at least one client is required");
        }
        strategy
    } else {
        anyhow::bail!("Missing a strategy field");
    };

    let start = properties
        .remove("start")
        .map(|v| serde_json::from_value::<usize>(v))
        .transpose()
        .context("Invalid start index (not a number)")?;

    if !properties.is_empty() {
        let supported_keys = ["strategy", "start"];
        let unknown_keys = properties.keys().map(String::from).collect::<Vec<_>>();
        anyhow::bail!(
            "Unknown keys: {}. Supported keys are: {}",
            unknown_keys.join(", "),
            supported_keys.join(", ")
        );
    }

    let start = match start {
        Some(start) => start % strategy.len(),
        None => {
            #[cfg(not(target_arch = "wasm32"))]
            {
                fastrand::usize(..strategy.len())
            }

            // For VSCode, we don't want a random start point,
            // as it can make rendering inconsistent
            #[cfg(target_arch = "wasm32")]
            {
                0
            }
        }
    };

    Ok((strategy, start))
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let (strategy, start) = resolve_properties(properties, ctx)?;
        Ok(Self {
            name: client.item.elem.name.clone(),
            retry_policy: client.retry_policy().as_ref().map(String::from),
            clients: strategy,
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
    ) -> OrchestratorNodeIterator {
        let offset = state.client_to_usage.entry(self.name.clone()).or_insert(0);
        let next = (self.current_index() + *offset) % self.clients.len();

        // Update the usage count
        *offset += 1;

        let client = &self.clients[next];
        let client = client_lookup.get_llm_provider(client, ctx).unwrap();
        let client = client.clone();
        client.iter_orchestrator(
            state,
            ExecutionScope::RoundRobin(self.clone(), next).into(),
            ctx,
            client_lookup,
        )
    }
}
