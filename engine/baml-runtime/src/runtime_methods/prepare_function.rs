use std::{collections::HashMap, path::PathBuf, sync::Arc};

use anyhow::{Context, Result};
use baml_types::{
    tracing::events::{FunctionEnd, FunctionStart, TraceData, TraceEvent},
    BamlMap, BamlValue, BamlValueWithMeta, Constraint, EvaluationContext, TypeIR,
};
use indexmap::IndexMap;
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
    RenderCurlSettings, RuntimeContext, RuntimeContextManager,
};

pub(crate) struct PreparedFunction<'ir> {
    pub function_name: String,
    /// If the function is an expr_fn, it won't have a `FunctionWalker`.
    pub func: Option<FunctionWalker<'ir>>,
    pub baml_args: PreparedFunctionArgs,
}

pub(crate) struct PreparedFunctionArgs {
    pub value: IndexMap<String, BamlValue>,
    pub value2: IndexMap<String, BamlValueWithMeta<TypeIR>>,
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
    pub fn into_error(self) -> anyhow::Error {
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

impl BamlRuntime {
    // TODO: this is introduced so that tracing can hook into function calls
    // _after_ prepare_function but before call_function_impl, which is why
    // `prepare_function` is not used in `FunctionResultStream`.  We should try
    // to unify those two callpaths though.
    pub(crate) fn prepare_function(
        &self,
        function_name: String,
        params: &BamlMap<String, BamlValue>,
    ) -> Result<PreparedFunction<'_>, PrepareFunctionError> {
        // Try LLM function first
        let func = match self.get_function(&function_name) {
            Ok(func) => func,
            Err(llm_error) => {
                // Try expr function
                match self.ir().find_expr_fn(&function_name) {
                    Ok(expr_fn) => {
                        // For expr functions, validate params and return
                        let baml_args = match self.ir().check_function_params(
                            expr_fn.inputs(),
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

                        // For expr functions, return PreparedFunction with None for func
                        return Ok(PreparedFunction {
                            function_name,
                            func: None,
                            baml_args: PreparedFunctionArgs {
                                value: baml_args
                                    .clone()
                                    .into_iter()
                                    .map(|(k, v)| (k, v.value()))
                                    .collect(),
                                value2: baml_args,
                            },
                        });
                    }
                    Err(_) => {
                        return Err(PrepareFunctionError::FunctionNotFound {
                            function_name,
                            error: llm_error,
                        });
                    }
                }
            }
        };

        let baml_args = match self.ir().check_function_params(
            func.inputs(),
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
            func: Some(func),
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
