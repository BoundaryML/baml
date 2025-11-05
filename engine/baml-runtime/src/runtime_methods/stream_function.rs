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
use stream_cancel::Tripwire;

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
    pub(crate) fn stream_function_impl(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
        tracer: Arc<BamlTracer>,
        ctx: RuntimeContext,
        #[cfg(not(target_arch = "wasm32"))] tokio_runtime: Arc<tokio::runtime::Runtime>,
        collectors: Vec<Arc<Collector>>,
        cancel_tripwire: Arc<TripWire>,
        tags: Option<&HashMap<String, String>>,
    ) -> Result<FunctionResultStream> {
        let is_expr_fn = self.get_expr_function(&function_name, &ctx).is_ok();
        if is_expr_fn {
            // TODO: this likely breaks something, the expr_fn eval logic is now unreferenced
            let func = self.get_expr_function(&function_name, &ctx)?;
            // let renderer = PromptRenderer::mk_fake();
            // let orchestrator = vec![];
            // let baml_args = self
            //     .ir
            //     .check_function_params(
            //         &func.inputs(),
            //         params,
            //         ArgCoercer {
            //             span_path: None,
            //             allow_implicit_cast_to_string: false,
            //         },
            //     )?
            //     .as_map_owned()
            //     .ok_or(anyhow::anyhow!("Failed to check function params."))?;
            let prepared = self
                .prepare_function(function_name, params)
                .map_err(|e| e.into_error())?;

            Ok(FunctionResultStream {
                function_name: prepared.function_name,
                prepared_func: prepared.baml_args,
                ir: self.ir.clone(),
                orchestrator: vec![],
                tracer,
                renderer: PromptRenderer::mk_fake(),
                #[cfg(not(target_arch = "wasm32"))]
                tokio_runtime,
                collectors,
                tags: tags.cloned(),
                cancel_tripwire,
            })
        } else {
            let prepared = self
                .prepare_function(function_name, params)
                .map_err(|e| e.into_error())?;

            // let func = self.get_function(&function_name)?;
            let func = prepared.func.as_ref().ok_or_else(|| {
                anyhow::anyhow!("Cannot stream expr function through this code path")
            })?;
            let renderer = PromptRenderer::from_function(func, self.ir(), &ctx)?;
            let orchestrator = self.orchestration_graph(renderer.client_spec(), &ctx)?;
            Ok(FunctionResultStream {
                function_name: prepared.function_name,
                ir: self.ir.clone(),
                prepared_func: prepared.baml_args,
                orchestrator,
                tracer,
                renderer,
                #[cfg(not(target_arch = "wasm32"))]
                tokio_runtime,
                collectors,
                tags: tags.cloned(),
                cancel_tripwire,
            })
        }
    }
}
