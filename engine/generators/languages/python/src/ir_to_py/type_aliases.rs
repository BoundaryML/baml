use internal_baml_core::ir::TypeAlias;

use crate::{generated_types::TypeAliasPy, ir_to_py, package::CurrentRenderPackage};

pub fn ir_type_alias_to_py<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasPy<'a> {
    TypeAliasPy {
        name: alias.elem.name.clone(),
        type_: ir_to_py::type_to_py(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_py_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasPy<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasPy {
        name: alias.elem.name.clone(),
        type_: ir_to_py::stream_type_to_py(&partialized, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}
