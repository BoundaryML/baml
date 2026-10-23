use baml_base::Literal;
use baml_sdkgen_types::Ty;

use crate::names::quote;

/// Keep the wire shape independent of which Sorbet types are supported.
pub(crate) fn descriptor(ty: &Ty) -> String {
    let body = match ty {
        Ty::Int => ":int".into(),
        Ty::Float => ":float".into(),
        Ty::String => ":string".into(),
        Ty::Bool => ":bool".into(),
        Ty::Null => ":null".into(),
        Ty::Never => ":never".into(),
        Ty::RustType => ":handle".into(),
        Ty::Literal(value, _) => format!(
            ":literal, {}",
            match value {
                Literal::String(s) => quote(s),
                Literal::Int(v) => v.to_string(),
                Literal::Bigint(v) => v.to_string(),
                Literal::Float(v) => v.clone(),
                Literal::Bool(v) => v.to_string(),
            }
        ),
        Ty::Class(name, _) => format!(":class, {}", quote(&name.to_string())),
        Ty::Enum(name) => format!(":enum, {}", quote(&name.to_string())),
        Ty::List(inner) => format!(":list, {}", descriptor(inner)),
        Ty::Map { key, value } if matches!(key.as_ref(), Ty::String) => {
            format!(":map, {}, {}", descriptor(key), descriptor(value))
        }
        Ty::Union(items) if matches!(items.as_ref(), [Ty::Null, _] | [_, Ty::Null]) => {
            format!(":union, {}", descriptors(items))
        }
        _ => ":other".into(),
    };
    format!("[{body}].freeze")
}

fn descriptors(items: &[Ty]) -> String {
    format!(
        "[{}].freeze",
        items.iter().map(descriptor).collect::<Vec<_>>().join(", ")
    )
}
