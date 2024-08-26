use std::collections::HashMap;

use anyhow::{Context, Result};

use internal_baml_core::ir::{repr::ClientSpec, ClientWalker};

use crate::{
    client_registry::ClientProperty,
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};

pub struct FallbackStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    client_specs: Vec<ClientSpec>,
}

fn resolve_strategy(
    mut properties: HashMap<String, serde_json::Value>,
    _ctx: &RuntimeContext,
) -> Result<Vec<ClientSpec>> {
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

    if !properties.is_empty() {
        let supported_keys = ["strategy"];
        let unknown_keys = properties.keys().map(String::from).collect::<Vec<_>>();
        anyhow::bail!(
            "Unknown keys: {}. Supported keys are: {}",
            unknown_keys.join(", "),
            supported_keys.join(", ")
        );
    }

    Ok(strategy.into_iter().map(ClientSpec::new_from_id).collect())
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for FallbackStrategy {
    type Error = anyhow::Error;

    fn try_from(
        (client, ctx): (&ClientProperty, &RuntimeContext),
    ) -> std::result::Result<Self, Self::Error> {
        let strategy = resolve_strategy(
            client
                .options
                .iter()
                .map(|(k, v)| Ok((k.clone(), serde_json::json!(v))))
                .collect::<Result<HashMap<_, _>>>()?,
            ctx,
        )?;
        Ok(Self {
            name: client.name.clone(),
            retry_policy: client.retry_policy.clone(),
            client_specs: strategy,
        })
    }
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for FallbackStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let properties = super::super::resolve_properties_walker(client, ctx)?;
        let strategy = resolve_strategy(properties, ctx)?;
        Ok(Self {
            name: client.item.elem.name.clone(),
            retry_policy: client.retry_policy().as_ref().map(String::from),
            client_specs: strategy,
        })
    }
}

impl IterOrchestrator for FallbackStrategy {
    fn iter_orchestrator<'a>(
        &self,
        state: &mut OrchestrationState,
        _previous: OrchestrationScope,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<crate::internal::llm_client::orchestrator::OrchestratorNodeIterator> {
        let items = self
            .client_specs
            .iter()
            .enumerate()
            .map(
                |(idx, client)| match client_lookup.get_llm_provider(client, ctx) {
                    Ok(client) => {
                        let client = client.clone();
                        Ok(client.iter_orchestrator(
                            state,
                            ExecutionScope::Fallback(self.name.clone(), idx).into(),
                            ctx,
                            client_lookup,
                        ))
                    }
                    Err(e) => Err(e),
                },
            )
            .flatten()
            .collect::<Result<Vec<_>>>()?
            .into_iter()
            .flatten()
            .collect();

        Ok(items)
    }
}
