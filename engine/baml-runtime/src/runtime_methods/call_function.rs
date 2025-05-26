use crate::runtime::InternalBamlRuntime;
use std::{collections::HashMap, path::PathBuf, sync::Arc};

use crate::internal::llm_client::traits::WithClientProperties;
use crate::internal::llm_client::LLMResponse;
use crate::tracingv2::storage::storage::{Collector, BAML_TRACER};
use crate::type_builder::TypeBuilder;
use crate::RuntimeContextManager;
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
            traits::{WithPrompt, WithRenderRawCurl},
        },
        prompt_renderer::PromptRenderer,
    },
    runtime_interface::{InternalClientLookup, RuntimeConstructor},
    tracing::BamlTracer,
    FunctionResult, FunctionResultStream, InternalRuntimeInterface, RenderCurlSettings,
    RuntimeContext,
};
use anyhow::{Context, Result};
use baml_types::tracing::events::{FunctionEnd, FunctionStart, TraceData, TraceEvent};

use baml_types::{BamlMap, BamlValue, Constraint, EvaluationContext};
use internal_baml_core::ir::repr::{Node, TypeBuilderEntry};
use internal_baml_core::ir::TestCase;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ArgCoercer, ExprFunctionWalker, FunctionWalker, IRHelper},
    validate,
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

use super::prepare_function::PreparedFunction;

impl InternalBamlRuntime {
    pub(crate) async fn call_function_impl<'ir>(
        &'ir self,
        prepared_func_call: PreparedFunction<'ir>,
        ctx: RuntimeContext,
    ) -> Result<crate::FunctionResult> {
        let future = async {
            let renderer =
                PromptRenderer::from_function(&prepared_func_call.func, self.ir(), &ctx)?;
            let orchestrator = self.orchestration_graph(renderer.client_spec(), &ctx)?;

            let baml_args = BamlValue::Map(prepared_func_call.baml_args.value);

            // Now actually execute the code.
            let (history, _) =
                orchestrate_call(orchestrator, self.ir(), &ctx, &renderer, &baml_args, |s| {
                    renderer.parse(self.ir(), &ctx, s, false)
                })
                .await;

            FunctionResult::new_chain(history)
        };

        future.await
    }
}
