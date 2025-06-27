// TODO: DO NOT EXPOSE THIS
use internal_baml_core::ir::FunctionNode;

use super::{stream_type_to_ts, type_to_ts};
use crate::{functions::FunctionTS, package::CurrentRenderPackage};

pub fn ir_function_to_ts(function: &FunctionNode, pkg: &CurrentRenderPackage) -> FunctionTS {
    FunctionTS {
        name: function.elem.name().to_string(),
        args: function
            .elem
            .inputs()
            .iter()
            .map(|(name, field_type)| (name.clone(), type_to_ts(field_type, pkg.lookup())))
            .collect(),
        return_type: type_to_ts(function.elem.output(), pkg.lookup()),
        stream_return_type: stream_type_to_ts(
            &function.elem.output().partialize(pkg.lookup()),
            pkg.lookup(),
        ),
    }
}
