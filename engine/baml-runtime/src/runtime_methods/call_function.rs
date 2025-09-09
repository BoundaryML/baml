use std::sync::Arc;

use anyhow::Result;
use baml_types::BamlValue;

use super::prepare_function::PreparedFunction;
use crate::{
    internal::{
        llm_client::orchestrator::orchestrate_call,
        prompt_renderer::PromptRenderer,
    },
    runtime::InternalBamlRuntime,
    FunctionResult, InternalRuntimeInterface,
    RuntimeContext, TripWire,
};

impl InternalBamlRuntime {
    pub(crate) async fn call_function_impl<'ir>(
        &'ir self,
        prepared_func_call: PreparedFunction<'ir>,
        ctx: RuntimeContext,
        cancel_tripwire: Arc<TripWire>,
    ) -> Result<crate::FunctionResult> {
        let future = async {
            let renderer =
                PromptRenderer::from_function(&prepared_func_call.func, self.ir(), &ctx)?;
            let orchestrator = self.orchestration_graph(renderer.client_spec(), &ctx)?;

            let baml_args = BamlValue::Map(prepared_func_call.baml_args.value);

            // Now actually execute the code.
            let (history, _) = orchestrate_call(
                orchestrator,
                self.ir(),
                &ctx,
                &renderer,
                &baml_args,
                |s| renderer.parse(self.ir(), &ctx, s, false),
                cancel_tripwire.trip_wire(),
            )
            .await;

            FunctionResult::new_chain(history)
        };

        future.await
    }
}
