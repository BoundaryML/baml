use internal_baml_core::ir::TypeAlias;

use crate::{
    generated_types::{TypeAliasInterfaceTS, TypeAliasTS},
    ir_to_ts,
    package::CurrentRenderPackage,
};

pub fn ir_type_alias_to_ts<'a>(
    alias: &TypeAlias,
    pkg: &'a CurrentRenderPackage,
) -> TypeAliasTS<'a> {
    TypeAliasTS {
        name: alias.elem.name.clone(),
        target_type: ir_to_ts::type_to_ts(
            &alias.elem.r#type.elem.to_non_streaming_type(pkg.lookup()),
            pkg.lookup(),
        ),
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
    let partialized = alias.elem.r#type.elem.to_streaming_type(pkg.lookup());
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
        TypeGeneric::Map(_, value_type, _) => Some(TypeAliasInterfaceTS {
            name: alias.elem.name.clone(),
            value_type: ir_to_ts::type_to_ts(
                &value_type.to_non_streaming_type(pkg.lookup()),
                pkg.lookup(),
            ),
            docstring: alias
                .elem
                .docstring
                .clone()
                .map(|docstring| docstring.0.clone()),
            pkg,
        }),
        TypeGeneric::Union(union_type, _) => {
            // Check if this union contains a map that we can extract as an interface
            for variant in union_type.iter_skip_null() {
                if let TypeGeneric::Map(_, value_type, _) = variant {
                    // Found a map in the union - create an interface that extends the union but as an index signature
                    return Some(TypeAliasInterfaceTS {
                        name: alias.elem.name.clone(),
                        value_type: ir_to_ts::type_to_ts(
                            &value_type.to_non_streaming_type(pkg.lookup()),
                            pkg.lookup(),
                        ),
                        docstring: alias
                            .elem
                            .docstring
                            .clone()
                            .map(|docstring| docstring.0.clone()),
                        pkg,
                    });
                }
            }
            None
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

    let partialized = alias.elem.r#type.elem.to_streaming_type(pkg.lookup());
    match &partialized {
        TypeGeneric::Map(_, value_type, _) => Some(TypeAliasInterfaceTS {
            name: alias.elem.name.clone(),
            value_type: ir_to_ts::stream_type_to_ts(value_type, pkg.lookup()),
            docstring: alias
                .elem
                .docstring
                .clone()
                .map(|docstring| docstring.0.clone()),
            pkg,
        }),
        TypeGeneric::Union(union_type, _) => {
            // Check if this union contains a map that we can extract as an interface
            for variant in union_type.iter_skip_null() {
                if let TypeGeneric::Map(_, _value_type, _) = variant {
                    // Found a map in the union - create an interface that extends the union but as an index signature
                    return Some(TypeAliasInterfaceTS {
                        name: alias.elem.name.clone(),
                        value_type: ir_to_ts::stream_type_to_ts(&partialized, pkg.lookup()),
                        docstring: alias
                            .elem
                            .docstring
                            .clone()
                            .map(|docstring| docstring.0.clone()),
                        pkg,
                    });
                }
            }
            None
        }
        _ => None,
    }
}
