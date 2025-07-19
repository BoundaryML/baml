use internal_baml_core::ir::repr::TypeAlias;

use crate::{generated_types::TypeAliasRb, ir_to_rb, package::CurrentRenderPackage};

pub fn ir_type_alias_to_rb<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasRb<'a> {
    TypeAliasRb {
        name: alias.name.clone(),
        type_: ir_to_rb::type_to_rb(
            &alias.r#type.elem.to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
        docstring: alias.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_rb_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasRb<'a> {
    let partialized = alias.r#type.elem.to_streaming_type(pkg.lookup());
    TypeAliasRb {
        name: alias.name.clone(),
        type_: ir_to_rb::stream_type_to_rb(&partialized, pkg.lookup()),
        docstring: alias.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}
