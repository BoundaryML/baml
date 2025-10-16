// TODO: DO NOT EXPOSE THIS
use internal_baml_core::ir::{ExprFunctionNode, FunctionNode};

use super::{stream_type_to_py, type_to_py};
use crate::{
    functions::{FunctionArgPy, FunctionPy},
    package::CurrentRenderPackage,
};

pub fn ir_function_to_py(function: &FunctionNode, pkg: &CurrentRenderPackage) -> FunctionPy {
    FunctionPy {
        documentation: None,
        name: function.elem.name().to_string(),
        args: {
            let mut args = function
                .elem
                .inputs()
                .iter()
                .map(|(name, field_type)| FunctionArgPy {
                    name: name.clone(),
                    type_: type_to_py(
                        &field_type.to_non_streaming_type(pkg.lookup()),
                        pkg.lookup(),
                    ),
                    default_value: None,
                })
                .collect::<Vec<_>>();

            // all optional args on the right side of the function
            // should have a default value of None
            args.iter_mut()
                .rev()
                .take_while(|arg| arg.type_.default_value().is_some() && arg.type_.is_optional())
                .for_each(|arg| {
                    arg.default_value = arg.type_.default_value();
                });
            args
        },
        return_type: type_to_py(
            &function.elem.output().to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        stream_return_type: stream_type_to_py(
            &function.elem.output().to_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
    }
}

pub fn ir_expr_fn_to_py(function: &ExprFunctionNode, pkg: &CurrentRenderPackage) -> FunctionPy {
    FunctionPy {
        documentation: None,
        name: function.elem.name.to_string(),
        args: {
            let mut args = function
                .elem
                .inputs()
                .iter()
                .map(|(name, field_type)| FunctionArgPy {
                    name: name.clone(),
                    type_: type_to_py(
                        &field_type.to_non_streaming_type(pkg.lookup()),
                        pkg.lookup(),
                    ),
                    default_value: None,
                })
                .collect::<Vec<_>>();

            // all optional args on the right side of the function
            // should have a default value of None
            args.iter_mut()
                .rev()
                .take_while(|arg| arg.type_.default_value().is_some() && arg.type_.is_optional())
                .for_each(|arg| {
                    arg.default_value = arg.type_.default_value();
                });
            args
        },
        return_type: type_to_py(
            &function.elem.output.to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        stream_return_type: stream_type_to_py(
            &function.elem.output.to_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
    }
}
