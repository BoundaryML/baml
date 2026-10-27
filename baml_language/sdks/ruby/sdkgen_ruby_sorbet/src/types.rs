use std::collections::BTreeSet;

use baml_sdkgen_types::{Name, Ty};

use crate::names::NameTable;

pub(crate) fn ruby_type(ty: &Ty, supported: &BTreeSet<Name>, names: &NameTable) -> Option<String> {
    Some(match ty {
        // The pool also defines baml.String and baml.Float; do not resolve to those wrappers.
        Ty::Literal(baml_base::Literal::String(_), ..) => "::String".into(),
        Ty::Literal(baml_base::Literal::Int(_), ..) => "::Integer".into(),
        Ty::Literal(baml_base::Literal::Float(_), ..) => "::Float".into(),
        Ty::Literal(baml_base::Literal::Bool(_), ..) => "::T::Boolean".into(),
        Ty::String => "::String".into(),
        Ty::Int => "::Integer".into(),
        Ty::Float => "::Float".into(),
        Ty::Bool => "::T::Boolean".into(),
        Ty::Null => "::NilClass".into(),
        Ty::RustType => "::Baml::Handle".into(),
        Ty::Never => "::T.noreturn".into(),
        Ty::List(t) => format!("::T::Array[{}]", ruby_type(t, supported, names)?),
        Ty::Map { key, value } if matches!(key.as_ref(), Ty::String) => {
            format!(
                "::T::Hash[::String, {}]",
                ruby_type(value, supported, names)?
            )
        }
        Ty::Class(n, args) if args.is_empty() && supported.contains(n) => {
            format!("::{}", names.qualified(n))
        }
        Ty::Enum(n) if supported.contains(n) => format!("::{}", names.qualified(n)),
        Ty::Union(items) => match items.as_ref() {
            [Ty::Null, inner] | [inner, Ty::Null] => {
                format!("::T.nilable({})", ruby_type(inner, supported, names)?)
            }
            _ => return None,
        },
        _ => return None,
    })
}
