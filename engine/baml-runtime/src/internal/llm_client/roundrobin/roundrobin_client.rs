use anyhow::Context;
use anyhow::Result;
use internal_baml_core::ir::ClientWalker;

use std::sync::{Arc, Mutex};

struct RoundRobinState {
    idx: usize,
}

use crate::internal::llm_client::{
    llm_provider::LLMProvider,
    retry_policy::CallablePolicy,
    traits::{WithCallable, WithPrompt, WithRetryPolicy, WithSingleCallable},
    LLMResponse, RenderedPrompt,
};
use crate::{internal::prompt_renderer::PromptRenderer, RuntimeContext};
use internal_baml_jinja::BamlArgType;

pub(crate) type FnGetClientConfig<'a> =
    Box<dyn Fn(&str) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)> + Send + Sync + 'a>;

pub struct RoundRobinClient<'a> {
    // client: ClientWalker<'ir>,
    retry_policy: Option<String>,
    clients: Vec<String>,
    get_client_config_callback: FnGetClientConfig<'a>,
    internal_state: Arc<Mutex<RoundRobinState>>,
}

impl<'a> RoundRobinClient<'a> {
    pub fn new(
        client: &ClientWalker,
        ctx: &RuntimeContext,
        get_client_config_cb: FnGetClientConfig<'a>,
    ) -> Result<RoundRobinClient<'a>> {
        let mut properties = (&client.item.elem.options);
        let properties = &client.item.elem.options;

        // Extract the start expression if it exists
        let start_expr = properties
            .iter()
            .find(|(key, _)| key == "start")
            .map(|(_, value)| value);

        // Resolve the start expression and parse it as a usize, default to 0 if not found
        let start_value = if let Some(expr) = start_expr {
            ctx.resolve_expression(expr)
                .context("Failed to resolve start expression")?
                .as_str()
                .context("Start value should be a string")?
                .parse::<usize>()
                .context("Invalid start index")?
        } else {
            0
        };
        // Extract and resolve the strategy option
        let strategy_option = properties
            .iter()
            .find(|(key, _)| key == "strategy")
            .context("strategy option not found")?;

        let strategy_expr = &strategy_option.1;

        let resolved_expression = ctx
            .resolve_expression(strategy_expr)
            .context("Failed to resolve strategy expression")?;
        let strategy_value: Option<&Vec<serde_json::Value>> = resolved_expression.as_array();

        let clients: Vec<String> = strategy_value
            .clone()
            .iter()
            .flat_map(|v| {
                v.iter()
                    .filter_map(|json_value| json_value.as_str().map(|s| s.to_string()))
            })
            .collect();

        Ok(RoundRobinClient {
            clients,
            retry_policy: client
                .elem()
                .retry_policy_id
                .as_ref()
                .map(|s| s.to_string()),
            internal_state: Arc::new(Mutex::new(RoundRobinState { idx: start_value })),
            get_client_config_callback: get_client_config_cb,
        })
    }
    // used to render the prompt. Choose the current one from list of clients
    // TODO: pass a callback or something to lookup clients
    fn get_client(&self) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)> {
        let state = self.internal_state.lock().unwrap();
        let idx = state.idx;
        let client = self.clients.get(idx).unwrap();
        (self.get_client_config_callback)(client)
    }

    fn get_client_and_increment(&self) -> Result<(Arc<LLMProvider>, Option<CallablePolicy>)> {
        let mut state = self.internal_state.lock().unwrap();
        let idx = state.idx;
        let client = self.clients.get(idx).unwrap();
        state.idx = (idx + 1) % self.clients.len();
        (self.get_client_config_callback)(client)
    }
}

impl<'a> WithRetryPolicy for RoundRobinClient<'a> {
    fn retry_policy_name(&self) -> Option<&str> {
        self.retry_policy.as_deref()
    }
}

// WithClient for roundrobin

impl<'ir> WithPrompt<'ir> for RoundRobinClient {
    fn render_prompt(
        &'ir self,
        renderer: &PromptRenderer,
        ctx: &RuntimeContext,
        params: &BamlArgType,
    ) -> Result<RenderedPrompt> {
        let res = self.get_client();
        match res {
            Ok((client, _)) => client.render_prompt(renderer, ctx, params),
            Err(e) => Err(e),
        }
    }
}

impl WithSingleCallable for RoundRobinClient {
    async fn single_call(
        &self,
        ctx: &RuntimeContext,
        prompt_renderer: &PromptRenderer, // TODO: PromptRenderer should be this. We should call render_prompt at the latest step, in the callable + with chat + withcompletion.
        baml_args: &BamlArgType,
    ) -> Result<LLMResponse> {
        let res = self.get_client();
        match res {
            Ok((client, retry_policy)) => Ok(client
                .call(retry_policy, ctx, prompt_renderer, baml_args)
                .await),
            Err(e) => Err(e),
        }
    }
}
