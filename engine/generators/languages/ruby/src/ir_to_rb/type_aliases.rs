use internal_baml_core::ir::TypeAlias;

use crate::{generated_types::TypeAliasRb, ir_to_rb, package::CurrentRenderPackage};

pub fn ir_type_alias_to_rb<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasRb<'a> {
    TypeAliasRb {
        name: alias.elem.name.clone(),
        type_: ir_to_rb::type_to_rb(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_rb_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasRb<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasRb {
        name: alias.elem.name.clone(),
        type_: ir_to_rb::stream_type_to_rb(&partialized, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}
