
use anyhow::Result;
use baml_types::{
    BamlMap, BamlValue, BamlValueWithMeta, TypeIR,
};
use indexmap::IndexMap;
use internal_baml_core::ir::{
        ArgCoercer, FunctionWalker, IRHelper,
    };

use crate::{
    runtime::InternalBamlRuntime,
    FunctionResult, InternalRuntimeInterface,
};

pub(crate) struct PreparedFunction<'ir> {
    pub function_name: String,
    pub func: FunctionWalker<'ir>,
    pub baml_args: PreparedFunctionArgs,
}

pub(crate) struct PreparedFunctionArgs {
    pub value: IndexMap<String, BamlValue>,
    #[allow(dead_code)]
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
                function_name: _,
                error,
            } => Err(error),
            PrepareFunctionError::InvalidParams {
                function_name: _,
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
    ) -> Result<PreparedFunction<'_>, PrepareFunctionError> {
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
