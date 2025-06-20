use internal_baml_core::ir::TypeAlias;

use crate::{generated_types::TypeAliasGo, ir_to_go, package::CurrentRenderPackage};

pub fn ir_type_alias_to_go<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasGo<'a> {
    TypeAliasGo {
        name: alias.elem.name.clone(),
        type_: ir_to_go::type_to_go(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_go_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasGo<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasGo {
        name: alias.elem.name.clone(),
        type_: ir_to_go::stream_type_to_go(&partialized, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}
