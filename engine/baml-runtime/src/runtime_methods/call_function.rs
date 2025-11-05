use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{FunctionEnd, FunctionStart, TraceData, TraceEvent},
    BamlMap, BamlValue, Constraint, EvaluationContext,
};
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{
        repr::{IntermediateRepr, Node, TypeBuilderEntry},
        ArgCoercer, ExprFunctionWalker, FunctionWalker, IRHelper, TestCase,
    },
    validate,
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

use super::prepare_function::PreparedFunction;
use crate::{
    client_registry::ClientProperty,
    internal::{
        ir_features::{IrFeatures, WithInternal},
        llm_client::{
            llm_provider::LLMProvider,
            orchestrator::{
                orchestrate_call, IterOrchestrator, OrchestrationScope, OrchestratorNode,
            },
            primitive::LLMPrimitiveProvider,
            retry_policy::CallablePolicy,
            traits::{WithClientProperties, WithPrompt, WithRenderRawCurl},
            LLMResponse,
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::RuntimeConstructor,
    tracing::BamlTracer,
    tracingv2::storage::storage::{Collector, BAML_TRACER},
    type_builder::TypeBuilder,
    BamlRuntime, FunctionResult, FunctionResultStream, InternalRuntimeInterface,
    RenderCurlSettings, RuntimeContext, RuntimeContextManager, TripWire,
};

impl BamlRuntime {
    pub(crate) async fn call_function_impl<'ir>(
        &'ir self,
        prepared_func_call: PreparedFunction<'ir>,
        ctx: RuntimeContext,
        cancel_tripwire: Arc<TripWire>,
    ) -> Result<crate::FunctionResult> {
        let future = async {
            let func = prepared_func_call.func.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Cannot call expr function through call_function_impl")
            })?;
            let renderer = PromptRenderer::from_function(func, self.ir(), &ctx)?;
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
