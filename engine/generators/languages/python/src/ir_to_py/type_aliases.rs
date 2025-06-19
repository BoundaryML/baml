use crate::generated_types::TypeAliasPy;
use crate::ir_to_py;
use crate::package::CurrentRenderPackage;
use internal_baml_core::ir::TypeAlias;


pub fn ir_type_alias_to_py<'a>(alias: &TypeAlias, pkg: &'a CurrentRenderPackage) -> TypeAliasPy<'a> {
    TypeAliasPy {
        name: alias.elem.name.clone(),
        type_: ir_to_py::type_to_py(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias.elem.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_py_stream<'a>(alias: &TypeAlias, pkg: &'a CurrentRenderPackage) -> TypeAliasPy<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasPy {
        name: alias.elem.name.clone(),
        type_: ir_to_py::stream_type_to_py(&partialized, pkg.lookup()),
        docstring: alias.elem.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}
