use anyhow::{Context, Result};
use std::{
    fmt::Debug,
    {
        collections::HashMap,
        sync::{atomic::AtomicUsize, Arc},
    },
};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
        OrchestratorNodeIterator,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};
use internal_baml_core::ir::ClientWalker;
use serde::Serialize;
use serde::Serializer;

struct MyAtomicUsize(AtomicUsize);

impl Clone for MyAtomicUsize {
    fn clone(&self) -> Self {
        Self(AtomicUsize::new(
            self.0.load(std::sync::atomic::Ordering::Relaxed),
        ))
    }
}

impl Debug for MyAtomicUsize {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        self.0.load(std::sync::atomic::Ordering::Relaxed).fmt(f)
    }
}

impl Serialize for MyAtomicUsize {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        self.0
            .load(std::sync::atomic::Ordering::Relaxed)
            .serialize(serializer)
    }
}

#[derive(Clone, Serialize, Debug)]
pub struct RoundRobinStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    clients: Vec<String>,
    current_index: MyAtomicUsize,
}

impl RoundRobinStrategy {
    pub fn current_index(&self) -> usize {
        self.current_index
            .0
            .load(std::sync::atomic::Ordering::Relaxed)
    }

    pub fn increment_index(&self) {
        self.current_index
            .0
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
            current_index: MyAtomicUsize(AtomicUsize::new(start)),
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
            current_index: MyAtomicUsize(AtomicUsize::new(start)),
        })
    }
}

impl IterOrchestrator for RoundRobinStrategy {
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
            ExecutionScope::RoundRobin(Arc::new(self.clone()), next).into(),
            ctx,
            client_lookup,
        )
    }
}
