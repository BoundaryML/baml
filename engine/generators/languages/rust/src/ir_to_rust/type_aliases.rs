use baml_types::baml_value::TypeLookups;
use internal_baml_core::ir::TypeAlias;

use crate::{generated_types::TypeAliasRust, ir_to_rust, package::CurrentRenderPackage};

struct LookupWithDrop<'a, T> {
    lookup: &'a T,
    drop_type: Option<&'a String>,
}

impl<'a, T: TypeLookups> TypeLookups for LookupWithDrop<'a, T> {
    fn expand_recursive_type(&self, type_alias: &str) -> anyhow::Result<&baml_types::TypeIR> {
        if self.drop_type.is_some() && self.drop_type.unwrap() == type_alias {
            Err(anyhow::anyhow!(
                "Recursive type alias {type_alias} is not supported in Rust"
            ))
        } else {
            self.lookup.expand_recursive_type(type_alias)
        }
    }
}

pub fn ir_type_alias_to_rust(
    alias: &TypeAlias,
    pkg: &CurrentRenderPackage,
    drop_type: Option<&String>,
) -> TypeAliasRust {
    let non_streaming = alias.elem.r#type.elem.to_non_streaming_type(pkg.lookup());
    let lookup = LookupWithDrop {
        lookup: pkg.lookup(),
        drop_type,
    };
    TypeAliasRust {
        name: alias.elem.name.clone(),
        // Type aliases don't participate in class cycles directly
        type_: ir_to_rust::type_to_rust(&non_streaming, &lookup, None),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
    }
}

pub fn ir_type_alias_to_rust_stream(
    alias: &TypeAlias,
    pkg: &CurrentRenderPackage,
    drop_type: Option<&String>,
) -> TypeAliasRust {
    let partialized = alias.elem.r#type.elem.to_streaming_type(pkg.lookup());

    let lookup = LookupWithDrop {
        lookup: pkg.lookup(),
        drop_type,
    };
    TypeAliasRust {
        name: alias.elem.name.clone(),
        // Type aliases don't participate in class cycles directly
        type_: ir_to_rust::stream_type_to_rust(&partialized, &lookup, None),
        docstring: alias
            .elem
            .docstring
            .clone()
            .map(|docstring| docstring.0.clone()),
    }
}
