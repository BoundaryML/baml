use internal_baml_core::ir::TypeAlias;

use crate::{generated_types::{TypeAliasTS, TypeAliasInterfaceTS}, ir_to_ts, package::CurrentRenderPackage};

pub fn ir_type_alias_to_ts<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasTS<'a> {
    TypeAliasTS {
        name: alias.elem.name.clone(),
        target_type: ir_to_ts::type_to_ts(&alias.elem.r#type.elem, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

pub fn ir_type_alias_to_ts_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasTS<'a> {
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    TypeAliasTS {
        name: alias.elem.name.clone(),
        target_type: ir_to_ts::stream_type_to_ts(&partialized, pkg.lookup()),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
        pkg,
    }
}

/// Convert a map-type alias to an interface to break circular references
pub fn ir_type_alias_to_ts_interface<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> Option<TypeAliasInterfaceTS<'a>> {
    use baml_types::ir_type::TypeGeneric;
    
    match &alias.elem.r#type.elem {
        TypeGeneric::Map(_, value_type, _) => {
            Some(TypeAliasInterfaceTS {
                name: alias.elem.name.clone(),
                value_type: ir_to_ts::type_to_ts(value_type, pkg.lookup()),
                docstring: alias
                    .elem
                    .docstring
                    .clone()
                    .map(|docstring| docstring.0.clone()),
                pkg,
            })
        }
        _ => None,
    }
}

/// Convert a map-type alias to an interface for streaming to break circular references
pub fn ir_type_alias_to_ts_interface_stream<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> Option<TypeAliasInterfaceTS<'a>> {
    use baml_types::ir_type::TypeGeneric;
    
    let partialized = alias.elem.r#type.elem.partialize(pkg.lookup());
    match &partialized {
        TypeGeneric::Map(_, value_type, _) => {
            Some(TypeAliasInterfaceTS {
                name: alias.elem.name.clone(),
                value_type: ir_to_ts::stream_type_to_ts(value_type, pkg.lookup()),
                docstring: alias
                    .elem
                    .docstring
                    .clone()
                    .map(|docstring| docstring.0.clone()),
                pkg,
            })
        }
        _ => None,
    }
}
