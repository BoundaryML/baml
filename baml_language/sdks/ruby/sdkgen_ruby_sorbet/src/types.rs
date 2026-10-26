use std::collections::BTreeSet;

use baml_sdkgen_types::{Name, Symbol, SymbolPool, Ty};

use crate::names::NameTable;

pub(crate) fn ruby_type(ty: &Ty, supported: &BTreeSet<Name>, names: &NameTable) -> Option<String> {
    ruby_type_with_params(ty, supported, names, &[])
}

pub(crate) fn ruby_type_with_params(
    ty: &Ty,
    supported: &BTreeSet<Name>,
    names: &NameTable,
    params: &[String],
) -> Option<String> {
    render(
        ty,
        supported,
        names,
        false,
        None,
        params,
        &mut BTreeSet::new(),
    )
}

pub(crate) fn ruby_input_type(
    ty: &Ty,
    supported: &BTreeSet<Name>,
    names: &NameTable,
    pool: &SymbolPool,
    params: &[String],
) -> Option<String> {
    render(
        ty,
        supported,
        names,
        true,
        Some(pool),
        params,
        &mut BTreeSet::new(),
    )
}

pub(crate) fn ruby_alias_type(
    ty: &Ty,
    pool: &SymbolPool,
    supported: &BTreeSet<Name>,
    names: &NameTable,
) -> Option<String> {
    render(
        ty,
        supported,
        names,
        false,
        Some(pool),
        &[],
        &mut BTreeSet::new(),
    )
}

fn render(
    ty: &Ty,
    supported: &BTreeSet<Name>,
    names: &NameTable,
    input: bool,
    aliases: Option<&SymbolPool>,
    params: &[String],
    visiting: &mut BTreeSet<Name>,
) -> Option<String> {
    let mut child = |ty: &Ty| render(ty, supported, names, input, aliases, params, visiting);
    Some(match ty {
        // The pool also defines baml.String and baml.Float; do not resolve to those wrappers.
        Ty::Literal(baml_base::Literal::String(_), ..) => "::String".into(),
        Ty::Literal(baml_base::Literal::Int(_) | baml_base::Literal::Bigint(_), ..) => {
            "::Integer".into()
        }
        Ty::Literal(baml_base::Literal::Float(_), ..) => "::Float".into(),
        Ty::Literal(baml_base::Literal::Bool(_), ..) => "::T::Boolean".into(),
        Ty::String => "::String".into(),
        Ty::Int | Ty::Bigint => "::Integer".into(),
        // Sorbet rejects Integer for Float; the engine widens integer inputs.
        Ty::Float if input => "::T.any(::Float, ::Integer)".into(),
        Ty::Float => "::Float".into(),
        Ty::Uint8Array => "::String".into(),
        Ty::Media(kind) => match kind {
            baml_base::MediaKind::Image => "::Baml::Image",
            baml_base::MediaKind::Audio => "::Baml::Audio",
            baml_base::MediaKind::Video => "::Baml::Video",
            baml_base::MediaKind::Pdf => "::Baml::Pdf",
            baml_base::MediaKind::Generic => "::Baml::Media",
        }
        .into(),
        Ty::Bool => "::T::Boolean".into(),
        Ty::Null => "::NilClass".into(),
        Ty::RustType => "::Baml::Handle".into(),
        // JSON is a recursive Ruby value, not a generated wrapper.
        Ty::TypeAlias(name) if name.to_string() == "baml.json.json" => "::T.untyped".into(),
        Ty::Never => "::T.noreturn".into(),
        Ty::Function { params, ret, .. } => {
            let params = params
                .iter()
                .enumerate()
                .map(|(index, param)| Some(format!("arg{index}: {}", child(&param.ty)?)))
                .collect::<Option<Vec<_>>>()?;
            let builder = if params.is_empty() {
                "::T.proc".into()
            } else {
                format!("::T.proc.params({})", params.join(", "))
            };
            format!(
                "::T.any({builder}.returns({}), ::Baml::Closure)",
                child(ret)?
            )
        }
        Ty::TypeVar(param) => params
            .get(param.index() as usize)
            .cloned()
            .unwrap_or_else(|| "::T.untyped".into()),
        Ty::List(t) => format!("::T::Array[{}]", child(t)?),
        Ty::Map { key, value } => format!("::T::Hash[{}, {}]", child(key)?, child(value)?),
        Ty::Class(n, args) if supported.contains(n) => {
            let base = format!("::{}", names.qualified(n));
            if args.is_empty() {
                base
            } else {
                format!(
                    "{base}[{}]",
                    args.iter()
                        .map(&mut child)
                        .collect::<Option<Vec<_>>>()?
                        .join(", ")
                )
            }
        }
        Ty::Enum(n) | Ty::EnumVariant(n, _) if supported.contains(n) => {
            format!("::{}", names.qualified(n))
        }
        Ty::TypeAlias(n) if supported.contains(n) => {
            if let Some(pool) = aliases {
                // Sorbet cannot resolve recursive aliases. Widen only the back-edge;
                // the runtime retains the exact recursive descriptor for encoding.
                if !visiting.insert(n.clone()) {
                    return Some("::T.untyped".into());
                }
                let Symbol::TypeAlias(alias) = pool.get(n)? else {
                    return None;
                };
                let result = render(
                    &alias.resolves_to,
                    supported,
                    names,
                    input,
                    aliases,
                    params,
                    visiting,
                );
                visiting.remove(n);
                result?
            } else {
                format!("::{}", names.qualified(n))
            }
        }
        Ty::Union(items) => {
            let mut types = items.iter().map(&mut child).collect::<Option<Vec<_>>>()?;
            // Literal unions often project several members to the same Ruby type.
            let mut seen = BTreeSet::new();
            types.retain(|t| seen.insert(t.clone()));
            if types.iter().any(|t| t == "::T.untyped") {
                return Some("::T.untyped".into());
            }
            match types.as_slice() {
                [] => "::T.noreturn".into(),
                [only] => only.clone(),
                [a, b] if a == "::NilClass" || b == "::NilClass" => {
                    format!("::T.nilable({})", if a == "::NilClass" { b } else { a })
                }
                _ => format!("::T.any({})", types.join(", ")),
            }
        }
        _ => return None,
    })
}
