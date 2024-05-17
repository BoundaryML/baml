use anyhow::{Context, Result};

use internal_baml_core::ir::ClientWalker;

use crate::{
    internal::llm_client::orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState, OrchestratorNode,
    },
    runtime_interface::InternalClientLookup,
    RuntimeContext,
};

pub struct FallbackStrategy {
    pub name: String,
    pub(super) retry_policy: Option<String>,
    // TODO: We can add conditions to each client
    clients: Vec<String>,
}
impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for FallbackStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let properties = &client.item.elem.options;

        let mut unknown_keys = vec![];

        let mut strategy = None;
        for (key, value) in properties {
            match key.as_str() {
                "strategy" => {
                    let clients = ctx
                        .resolve_expression::<Vec<String>>(value)
                        .context("Failed to resolve strategy expression into string[]");
                    strategy = Some(clients);
                }
                other => unknown_keys.push(other),
            };
        }

        if !unknown_keys.is_empty() {
            let supported_keys = ["strategy"];
            anyhow::bail!(
                "Unknown keys: {}. Supported keys are: {}",
                unknown_keys.join(", "),
                supported_keys.join(", ")
            );
        }

        let strategy = match strategy {
            Some(Ok(strategy)) => {
                if strategy.is_empty() {
                    anyhow::bail!("Empty strategy array, at least one client is required");
                }
                strategy
            }
            Some(Err(e)) => return Err(e),
            None => anyhow::bail!("Missing a strategy field"),
        };

        Ok(Self {
            name: client.item.elem.name.clone(),
            retry_policy: client.retry_policy().as_ref().map(String::from),
            clients: strategy,
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
    ) -> crate::internal::llm_client::orchestrator::OrchestratorNodeIterator {
        let items = self
            .clients
            .iter()
            .enumerate()
            .flat_map(|(idx, client)| {
                let client = client_lookup.get_llm_provider(client, ctx).unwrap().clone();
                client.iter_orchestrator(
                    state,
                    ExecutionScope::Fallback(self.name.clone(), idx).into(),
                    ctx,
                    client_lookup,
                )
            })
            .collect::<Vec<_>>();

        items
    }
}
