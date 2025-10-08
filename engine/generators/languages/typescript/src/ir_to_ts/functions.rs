// TODO: DO NOT EXPOSE THIS
use internal_baml_core::ir::{ExprFunctionNode, FunctionNode};

use super::{stream_type_to_ts, type_to_ts};
use crate::{functions::FunctionTS, package::CurrentRenderPackage};

pub fn ir_function_to_ts(function: &FunctionNode, pkg: &CurrentRenderPackage) -> FunctionTS {
    FunctionTS {
        name: function.elem.name().to_string(),
        args: function
            .elem
            .inputs()
            .iter()
            .map(|(name, field_type)| {
                (
                    name.clone(),
                    type_to_ts(
                        &field_type.to_non_streaming_type(pkg.lookup()),
                        pkg.lookup(),
                    ),
                )
            })
            .collect(),
        return_type: type_to_ts(
            &function.elem.output().to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        stream_return_type: stream_type_to_ts(
            &function.elem.output().to_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        event_collector_type: None,
    }
}

pub fn ir_expr_fn_to_ts(function: &ExprFunctionNode, pkg: &CurrentRenderPackage) -> FunctionTS {
    FunctionTS {
        name: function.elem.name.to_string(),
        args: function
            .elem
            .inputs()
            .iter()
            .map(|(name, field_type)| {
                (
                    name.clone(),
                    type_to_ts(
                        &field_type.to_non_streaming_type(pkg.lookup()),
                        pkg.lookup(),
                    ),
                )
            })
            .collect(),
        return_type: type_to_ts(
            &function.elem.output.to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        stream_return_type: stream_type_to_ts(
            &function.elem.output.to_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        event_collector_type: Some(format!("{}EventCollector", function.elem.name)),
    }
}
