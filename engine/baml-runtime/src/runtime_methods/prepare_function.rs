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

use baml_types::{BamlMap, BamlValue, BamlValueWithMeta, Constraint, EvaluationContext, FieldType};
use indexmap::IndexMap;
use internal_baml_core::ir::repr::{Node, TypeBuilderEntry};
use internal_baml_core::ir::TestCase;
use internal_baml_core::{
    internal_baml_diagnostics::SourceFile,
    ir::{repr::IntermediateRepr, ArgCoercer, ExprFunctionWalker, FunctionWalker, IRHelper},
    validate,
};
use internal_baml_jinja::RenderedPrompt;
use internal_llm_client::{AllowedRoleMetadata, ClientSpec};

pub(crate) struct PreparedFunction<'ir> {
    pub function_name: String,
    pub func: FunctionWalker<'ir>,
    pub baml_args: PreparedFunctionArgs,
}

pub(crate) struct PreparedFunctionArgs {
    pub value: IndexMap<String, BamlValue>,
    pub value2: IndexMap<String, BamlValueWithMeta<FieldType>>,
}

pub(crate) enum PrepareFunctionError {
    FunctionNotFound {
        function_name: String,
        error: anyhow::Error,
    },
    InvalidParams {
        function_name: String,
        error: anyhow::Error,
    },
}

impl PrepareFunctionError {
    pub fn as_error(self) -> anyhow::Error {
        match self {
            PrepareFunctionError::FunctionNotFound {
                function_name,
                error,
            } => error.context(format!(
                "BAML function {function_name} does not exist in baml_src/ (did you typo it?)"
            )),
            PrepareFunctionError::InvalidParams {
                function_name,
                error,
            } => error.context(format!(
                "Invalid parameters for BAML function {function_name}"
            )),
        }
    }
}

impl From<PrepareFunctionError> for Result<FunctionResult> {
    fn from(error: PrepareFunctionError) -> Self {
        match error {
            PrepareFunctionError::FunctionNotFound {
                function_name,
                error,
            } => Err(error),
            PrepareFunctionError::InvalidParams {
                function_name,
                error,
            } => Err(error),
        }
    }
}

impl InternalBamlRuntime {
    // TODO: this is introduced so that tracing can hook into function calls
    // _after_ prepare_function but before call_function_impl, which is why
    // `prepare_function` is not used in `FunctionResultStream`.  We should try
    // to unify those two callpaths though.
    pub(crate) fn prepare_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
    ) -> Result<PreparedFunction, PrepareFunctionError> {
        let func = match self.get_function(&function_name) {
            Ok(func) => func,
            Err(error) => {
                return Err(PrepareFunctionError::FunctionNotFound {
                    function_name,
                    error,
                });
            }
        };

        let baml_args = match self.ir().check_function_params(
            &func.inputs(),
            params,
            ArgCoercer {
                span_path: None,
                allow_implicit_cast_to_string: false,
            },
        ) {
            Ok(baml_args) => baml_args,
            Err(error) => {
                return Err(PrepareFunctionError::InvalidParams {
                    function_name,
                    error,
                });
            }
        };

        Ok(PreparedFunction {
            function_name,
            func,
            baml_args: PreparedFunctionArgs {
                value: baml_args
                    .clone()
                    .into_iter()
                    .map(|(k, v)| (k, v.value()))
                    .collect(),
                value2: baml_args,
            },
        })
    }
}
