// TODO: DO NOT EXPOSE THIS
use internal_baml_core::ir::FunctionNode;

use super::{stream_type_to_rust, type_to_rust};
use crate::{functions::FunctionRust, package::CurrentRenderPackage};

pub fn ir_function_to_rust(function: &FunctionNode, pkg: &CurrentRenderPackage) -> FunctionRust {
    // Function arguments and return types don't participate in class cycles
    // (they reference classes but aren't themselves recursive)
    FunctionRust {
        documentation: None,
        name: function.elem.name().to_string(),
        args: function
            .elem
            .inputs()
            .iter()
            .map(|(name, field_type)| {
                (
                    name.clone(),
                    type_to_rust(
                        &field_type.to_non_streaming_type(pkg.lookup()),
                        pkg.lookup(),
                        None, // No cycle context for function args
                    ),
                )
            })
            .collect(),
        return_type: type_to_rust(
            &function.elem.output().to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
            None, // No cycle context for return types
        ),
        stream_return_type: stream_type_to_rust(
            &function.elem.output().to_streaming_type(pkg.lookup()),
            pkg.lookup(),
            None, // No cycle context for stream return types
        ),
    }
}
