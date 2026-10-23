use baml_base::Literal;
use baml_sdkgen_types::Ty;

use crate::names::quote;

/// Keep the wire shape independent of which Sorbet types are supported.
pub(crate) fn descriptor(ty: &Ty) -> String {
    let body = match ty {
        Ty::Int => ":int".into(),
        Ty::Bigint => ":bigint".into(),
        Ty::Float => ":float".into(),
        Ty::String => ":string".into(),
        Ty::Bool => ":bool".into(),
        Ty::Null => ":null".into(),
        Ty::Uint8Array => ":bytes".into(),
        Ty::Never => ":never".into(),
        Ty::RustType => ":handle".into(),
        Ty::Media(kind) => format!(
            ":media, :{}",
            if matches!(kind, baml_base::MediaKind::Generic) {
                "generic"
            } else {
                kind.tag_str()
            }
        ),
        Ty::Literal(value, _) => format!(
            ":literal, {}",
            match value {
                Literal::String(s) => quote(s),
                Literal::Int(v) => v.to_string(),
                // Sorbet's parser rejects integer literals outside signed 64-bit.
                Literal::Bigint(v) => format!("::Kernel.Integer({})", quote(&v.to_string())),
                Literal::Float(v) => v.clone(),
                Literal::Bool(v) => v.to_string(),
            }
        ),
        Ty::Class(name, args) => format!(
            ":class, {}, {}",
            quote(&name.to_string()),
            descriptors(args)
        ),
        Ty::Enum(name) | Ty::EnumVariant(name, _) => format!(":enum, {}", quote(&name.to_string())),
        Ty::List(inner) => format!(":list, {}", descriptor(inner)),
        Ty::Map { key, value } => format!(":map, {}, {}", descriptor(key), descriptor(value)),
        Ty::Union(items) => format!(":union, {}", descriptors(items)),
        Ty::TypeAlias(name) => format!(":alias, {}", quote(&name.to_string())),
        Ty::TypeVar(param) => format!(":param, {}, {}", param.index(), quote(param.as_str())),
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
