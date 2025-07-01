use std::sync::Arc;

use anyhow::Result;
use internal_baml_core::ir::ClientWalker;
use internal_baml_jinja::RenderedChatMessage;

use super::{
    orchestrator::{
        ExecutionScope, IterOrchestrator, OrchestrationScope, OrchestrationState,
        OrchestratorNodeIterator,
    },
    primitive::LLMPrimitiveProvider,
    strategy::LLMStrategyProvider,
    traits::WithRetryPolicy,
    LLMResponse,
};
use crate::{
    client_registry::ClientProperty, runtime_interface::InternalClientLookup, RuntimeContext,
};

pub enum LLMProvider {
    Primitive(Arc<LLMPrimitiveProvider>),
    Strategy(LLMStrategyProvider),
}

impl std::fmt::Debug for LLMProvider {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LLMProvider::Primitive(provider) => write!(f, "Primitive({provider})"),
            LLMProvider::Strategy(provider) => write!(f, "Strategy({provider})"),
        }
    }
}

impl LLMProvider {
    /// Returns the "prompt" as required by the LLM API.
    ///
    /// It's some sort of JSON like this:
    ///
    /// ```json
    /// {
    ///     "messages": [
    ///         {
    ///             "role": "system",
    ///             "content": [
    ///                 {
    ///                     "type": "text",
    ///                     "text": "You are a helpful assistant"
    ///                 }
    ///             ]
    ///         },
    ///         {
    ///             "role": "user",
    ///             "content": [
    ///                 {
    ///                     "type": "text",
    ///                     "text": "What is the capital of France?"
    ///                 }
    ///             ]
    ///         }
    ///     ]
    /// }
    /// ```
    ///
    /// This JSON object can then be converted into Python or TS objects or
    /// whatever we need.
    pub fn chat_to_message<'a>(
        &self,
        chat: &[RenderedChatMessage],
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        match self {
            LLMProvider::Primitive(provider) => provider.chat_to_message(chat),

            // Return the first node's provider implementation.
            LLMProvider::Strategy(provider) => {
                let orchestrator = provider.iter_orchestrator(
                    &mut Default::default(),
                    Default::default(),
                    ctx,
                    client_lookup,
                )?;

                orchestrator
                    .first()
                    .ok_or(anyhow::anyhow!("Strategy provider is empty: {}", provider))?
                    .provider
                    .chat_to_message(chat)
            }
        }
    }

    pub fn completion_to_provider_body<'a>(
        &self,
        prompt: &str,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<serde_json::Map<String, serde_json::Value>> {
        match self {
            LLMProvider::Primitive(provider) => provider.completion_to_provider_body(prompt),

            LLMProvider::Strategy(provider) => {
                let orchestrator = provider.iter_orchestrator(
                    &mut Default::default(),
                    Default::default(),
                    ctx,
                    client_lookup,
                )?;

                orchestrator
                    .first()
                    .ok_or(anyhow::anyhow!("Strategy provider is empty: {}", provider))?
                    .provider
                    .completion_to_provider_body(prompt)
            }
        }
    }

    pub async fn build_request<'a>(
        &self,
        prompt: either::Either<&String, &[RenderedChatMessage]>,
        allow_proxy: bool,
        stream: bool,
        ctx: &RuntimeContext,
        client_lookup: &'a impl InternalClientLookup<'a>,
    ) -> Result<reqwest::RequestBuilder> {
        match self {
            LLMProvider::Primitive(provider) => {
                provider.build_request(prompt, allow_proxy, stream).await
            }

            LLMProvider::Strategy(provider) => {
                let orchestrator = provider.iter_orchestrator(
                    &mut Default::default(),
                    Default::default(),
                    ctx,
                    client_lookup,
                )?;

                orchestrator
                    .first()
                    .ok_or(anyhow::anyhow!("Strategy provider is empty: {}", provider))?
                    .provider
                    .build_request(prompt, allow_proxy, stream)
                    .await
            }
        }
    }
}

impl WithRetryPolicy for LLMProvider {
    fn retry_policy_name(&self) -> Option<&str> {
        match self {
            LLMProvider::Primitive(provider) => provider.retry_policy_name(),
            LLMProvider::Strategy(provider) => provider.retry_policy_name(),
        }
    }
}

impl TryFrom<(&ClientWalker<'_>, &RuntimeContext)> for LLMProvider {
    type Error = anyhow::Error;

    fn try_from((client, ctx): (&ClientWalker, &RuntimeContext)) -> Result<Self> {
        match &client.elem().provider {
            internal_llm_client::ClientProvider::Strategy(_) => {
                LLMStrategyProvider::try_from((client, ctx)).map(LLMProvider::Strategy)
            }
            _ => LLMPrimitiveProvider::try_from((client, ctx))
                .map(Arc::new)
                .map(LLMProvider::Primitive),
        }
    }
}

impl TryFrom<(&ClientProperty, &RuntimeContext)> for LLMProvider {
    type Error = anyhow::Error;

    fn try_from(value: (&ClientProperty, &RuntimeContext)) -> Result<Self> {
        match &value.0.provider {
            internal_llm_client::ClientProvider::Strategy(_) => {
                LLMStrategyProvider::try_from(value).map(LLMProvider::Strategy)
            }
            _ => LLMPrimitiveProvider::try_from(value)
                .map(Arc::new)
                .map(LLMProvider::Primitive),
        }
    }
}

impl IterOrchestrator for Arc<LLMProvider> {
    fn iter_orchestrator<'a>(
        &self,
        state: &mut OrchestrationState,
        previous: OrchestrationScope,
        ctx: &RuntimeContext,
        client_lookup: &'a dyn InternalClientLookup<'a>,
    ) -> Result<OrchestratorNodeIterator> {
        if let Some(retry_policy) = self.retry_policy_name() {
            let policy = client_lookup.get_retry_policy(retry_policy, ctx)?;
            Ok(policy
                .into_iter()
                .enumerate()
                .map(move |(idx, node)| {
                    previous
                        .clone()
                        .extend(ExecutionScope::Retry(retry_policy.into(), idx, node))
                })
                .map(|scope| {
                    // repeat the same provider for each retry policy

                    // We can pass in empty previous.
                    match self.as_ref() {
                        LLMProvider::Primitive(provider) => provider.iter_orchestrator(
                            state,
                            Default::default(),
                            ctx,
                            client_lookup,
                        ),
                        LLMProvider::Strategy(provider) => provider.iter_orchestrator(
                            state,
                            Default::default(),
                            ctx,
                            client_lookup,
                        ),
                    }
                    .map(|nodes| {
                        nodes
                            .into_iter()
                            .map(move |node| node.prefix(scope.clone()))
                            .collect::<Vec<_>>()
                    })
                })
                .collect::<Result<Vec<_>>>()?
                .into_iter()
                .flatten()
                .collect::<Vec<_>>())
        } else {
            Ok(match self.as_ref() {
                LLMProvider::Primitive(provider) => {
                    provider.iter_orchestrator(state, Default::default(), ctx, client_lookup)
                }
                LLMProvider::Strategy(provider) => {
                    provider.iter_orchestrator(state, Default::default(), ctx, client_lookup)
                }
            }?
            .into_iter()
            .map(|node| node.prefix(previous.clone()))
            .collect())
        }
    }
}
