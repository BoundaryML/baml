use baml_base::Literal;
use baml_sdkgen_types::{CodegenFunctionParamMode, Ty};

use crate::names::quote;

/// Keep the wire shape independent of which Sorbet types are supported.
pub(crate) fn descriptor(ty: &Ty) -> String {
    let body = match ty {
        Ty::Int => ":int".into(),
        Ty::Bigint => ":bigint".into(),
        Ty::Uint8Array => ":bytes".into(),
        Ty::Media(kind) => format!(
            ":media, :{}",
            if matches!(kind, baml_base::MediaKind::Generic) {
                "generic"
            } else {
                kind.tag_str()
            }
        ),
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
                Literal::Bigint(v) =>
                    format!(":bigint, ::Kernel.Integer({})", quote(&v.to_string())),
                Literal::Float(v) => v.clone(),
                Literal::Bool(v) => v.to_string(),
            }
        ),
        Ty::Class(name, args) => format!(
            ":class, {}, {}",
            quote(&name.to_string()),
            descriptors(args)
        ),
        Ty::Enum(name) => format!(":enum, {}", quote(&name.to_string())),
        Ty::EnumVariant(name, variant) => format!(
            ":enum_variant, {}, {}",
            quote(&name.to_string()),
            quote(variant.as_str())
        ),
        Ty::TypeAlias(name) => format!(":alias, {}", quote(&name.to_string())),
        Ty::Unknown => ":unknown".into(),
        Ty::Interface(name, ..) => format!(":interface, {}", quote(&name.to_string())),
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => format!(
            ":function, [{}].freeze, {}, {}",
            params
                .iter()
                .map(|param| format!(
                    "[{}, {}, {}].freeze",
                    param
                        .name
                        .as_ref()
                        .map_or_else(|| "nil".into(), |name| quote(name.as_str())),
                    descriptor(&param.ty),
                    param.mode == CodegenFunctionParamMode::Optional
                ))
                .collect::<Vec<_>>()
                .join(", "),
            descriptor(ret),
            descriptor(throws)
        ),
        Ty::List(inner) => format!(":list, {}", descriptor(inner)),
        Ty::Map { key, value } => {
            format!(":map, {}, {}", descriptor(key), descriptor(value))
        }
        Ty::Union(items) => {
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
