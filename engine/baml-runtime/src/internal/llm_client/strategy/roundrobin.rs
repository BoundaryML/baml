use anyhow::{Context, Result};
use std::sync::{atomic::AtomicUsize, Arc};

use internal_baml_core::ir::ClientWalker;

use crate::{
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

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for RoundRobinStrategy {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        let properties = &client.item.elem.options;

        let mut unknown_keys = vec![];

        let mut start = None;
        let mut strategy = None;
        for (key, value) in properties {
            match key.as_str() {
                "start" => {
                    let start_expr = ctx
                        .resolve_expression::<usize>(value)
                        .context("Invalid start index (not a number)");
                    start = Some(start_expr);
                }
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
            let supported_keys = ["start", "strategy"];
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

        let start = match start {
            Some(Ok(start)) => start % strategy.len(),
            Some(Err(e)) => return Err(e),
            None => {
                #[cfg(not(target = "wasm32"))]
                {
                    fastrand::usize(..strategy.len())
                }

                // For VSCode, we don't want a random start point,
                // as it can make rendering inconsistent
                #[cfg(target = "wasm32")]
                {
                    0
                }
            }
        };

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
