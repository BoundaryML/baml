use anyhow::{Context, Result};
use std::{
    fmt::Debug,
    {
        collections::HashMap,
        sync::{atomic::AtomicUsize, Arc},
    },
};

use internal_baml_core::ir::{repr::ClientSpec, ClientWalker};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
        OrchestratorNodeIterator,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};
use serde::Serialize;
use serde::Serializer;

#[derive(Serialize, Debug)]
pub struct RoundRobinStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    client_specs: Vec<ClientSpec>,
    #[serde(serialize_with = "serialize_atomic")]
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
    mut properties: HashMap<String, serde_json::Value>,
    _ctx: &RuntimeContext,
) -> Result<(Vec<ClientSpec>, usize)> {
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

    Ok((
        strategy.into_iter().map(ClientSpec::new_from_id).collect(),
        start,
    ))
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from(
        (client, ctx): (&ClientProperty, &RuntimeContext),
    ) -> std::result::Result<Self, Self::Error> {
        let (strategy, start) = resolve_strategy(
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
            client_specs: strategy,
            current_index: AtomicUsize::new(start),
        })
    }
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let (strategy, start) = resolve_strategy(properties, ctx)?;
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
