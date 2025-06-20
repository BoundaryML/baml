use crate::generated_types::TypeAliasTS;
use crate::ir_to_ts;
use crate::package::CurrentRenderPackage;
use internal_baml_core::ir::TypeAlias;


pub fn ir_type_alias_to_ts<'a>(alias: &TypeAlias, pkg: &'a CurrentRenderPackage) -> TypeAliasTS<'a> {
    TypeAliasTS {
        name: alias.elem.name.clone(),
        target_type: ir_to_ts::type_to_ts(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias.elem.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_ts_stream<'a>(alias: &TypeAlias, pkg: &'a CurrentRenderPackage) -> TypeAliasTS<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasTS {
        name: alias.elem.name.clone(),
        target_type: ir_to_ts::stream_type_to_ts(&partialized, pkg.lookup()),
        docstring: alias.elem.docstring.clone().map(|docstring| docstring.0.clone()),
        pkg,
    }
}
