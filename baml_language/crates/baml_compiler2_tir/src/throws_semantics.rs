use std::collections::{BTreeSet, HashMap};

use baml_base::Name;

use crate::{
    normalize,
    ty::{PrimitiveType, QualifiedTypeName, Ty, TyAttr},
};

#[derive(Debug, Clone, PartialEq, Eq)]
pub(crate) struct ThrowsContractDiff {
    pub uncovered_effective: Vec<Ty>,
    pub extraneous_declared: Vec<Ty>,
}

pub(crate) fn resolve_alias_chain(ty: &Ty, aliases: &HashMap<QualifiedTypeName, Ty>) -> Ty {
    let mut resolved = ty.clone();
    for _ in 0..64 {
        match &resolved {
            Ty::TypeAlias(qtn, _) => match aliases.get(qtn) {
                Some(expanded) => resolved = expanded.clone(),
                None => break,
            },
            _ => break,
        }
    }
    resolved
}

pub(crate) fn function_throws_facts(
    ty: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> Option<BTreeSet<Ty>> {
    match resolve_callable_function_ty(ty, aliases)? {
        Ty::Function { throws, .. } => Some(flatten_ty_to_facts(&throws)),
        _ => None,
    }
}

fn resolve_callable_function_ty(ty: &Ty, aliases: &HashMap<QualifiedTypeName, Ty>) -> Option<Ty> {
    let mut resolved = resolve_alias_chain(ty, aliases);
    loop {
        match resolved {
            Ty::Function { .. } => return Some(resolved),
            Ty::Optional(inner, _) => {
                resolved = resolve_alias_chain(inner.as_ref(), aliases);
            }
            _ => return None,
        }
    }
}

/// Flatten a compound `Ty` into its leaf throw facts.
/// Unions and optionals are decomposed; leaf types are kept as-is.
pub fn flatten_ty_to_facts(ty: &Ty) -> BTreeSet<Ty> {
    let mut out = BTreeSet::new();
    collect_leaf_types(ty, &mut out);
    out
}

fn collect_leaf_types(ty: &Ty, out: &mut BTreeSet<Ty>) {
    match ty {
        Ty::Optional(inner, _) => {
            collect_leaf_types(inner, out);
            out.insert(Ty::Primitive(PrimitiveType::Null, TyAttr::default()));
        }
        Ty::Union(members, _) => {
            for member in members {
                collect_leaf_types(member, out);
            }
        }
        Ty::Literal(lit, _, _) => {
            out.insert(Ty::Primitive(
                PrimitiveType::from_literal(lit),
                TyAttr::default(),
            ));
        }
        Ty::Never { .. } | Ty::Void { .. } => {}
        _ => {
            out.insert(ty.clone());
        }
    }
}

pub(crate) fn throws_contract_diff(
    declared_ty: &Ty,
    effective_facts: &BTreeSet<Ty>,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> ThrowsContractDiff {
    let declared_facts = flatten_ty_to_facts(declared_ty);

    let uncovered_effective = effective_facts
        .iter()
        .filter(|fact| {
            !declared_facts
                .iter()
                .any(|declared| declared_covers_fact(declared, fact, aliases))
        })
        .cloned()
        .collect();

    let extraneous_declared = declared_facts
        .iter()
        .filter(|declared| {
            !effective_facts
                .iter()
                .any(|fact| declared_covers_fact(declared, fact, aliases))
        })
        .cloned()
        .collect();

    ThrowsContractDiff {
        uncovered_effective,
        extraneous_declared,
    }
}

pub(crate) fn type_covers_throw_fact(
    pattern_ty: &Ty,
    fact: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> bool {
    if matches!(
        fact,
        Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
    ) {
        let resolved = resolve_alias_chain(pattern_ty, aliases);
        return matches!(
            resolved,
            Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
        );
    }

    normalize::is_subtype_of(fact, pattern_ty, aliases)
}

pub fn combine_effect_vars_with_body_throws(
    synthetic_effect_vars: &[Name],
    body_throws_facts: BTreeSet<Ty>,
) -> Ty {
    let mut all_throws: Vec<Ty> = synthetic_effect_vars
        .iter()
        .map(|v| Ty::TypeVar(v.clone(), TyAttr::default()))
        .collect();
    all_throws.extend(body_throws_facts);
    all_throws.retain(|t| !matches!(t, Ty::Never { .. } | Ty::Void { .. }));

    match all_throws.len() {
        0 => Ty::Never {
            attr: TyAttr::default(),
        },
        1 => all_throws.remove(0),
        _ => Ty::Union(all_throws, TyAttr::default()),
    }
}

pub fn concrete_throws_ty_from_facts(facts: BTreeSet<Ty>) -> Ty {
    let mut concrete = BTreeSet::new();
    for fact in facts {
        if matches!(fact, Ty::Never { .. } | Ty::Void { .. } | Ty::TypeVar(_, _)) {
            continue;
        }
        let widened = match fact {
            Ty::Literal(lit, _, _) => {
                Ty::Primitive(PrimitiveType::from_literal(&lit), TyAttr::default())
            }
            other => other,
        };
        concrete.insert(widened);
    }

    match concrete.len() {
        0 => Ty::Never {
            attr: TyAttr::default(),
        },
        1 => concrete.into_iter().next().unwrap_or(Ty::Never {
            attr: TyAttr::default(),
        }),
        _ => Ty::Union(concrete.into_iter().collect(), TyAttr::default()),
    }
}

pub(crate) fn function_shape_matches_ignoring_outer_throws(
    got: &Ty,
    expected: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> bool {
    let mut got_resolved = resolve_alias_chain(got, aliases);
    let mut expected_resolved = resolve_alias_chain(expected, aliases);

    while let (Ty::Optional(got_inner, _), Ty::Optional(expected_inner, _)) =
        (&got_resolved, &expected_resolved)
    {
        got_resolved = resolve_alias_chain(got_inner.as_ref(), aliases);
        expected_resolved = resolve_alias_chain(expected_inner.as_ref(), aliases);
    }

    match (got_resolved, expected_resolved) {
        (
            Ty::Function {
                params: got_params,
                ret: got_ret,
                attr: got_attr,
                ..
            },
            Ty::Function {
                params: expected_params,
                ret: expected_ret,
                attr: expected_attr,
                ..
            },
        ) => normalize::is_subtype_of(
            &Ty::Function {
                params: got_params,
                ret: got_ret,
                throws: Box::new(Ty::Never {
                    attr: TyAttr::default(),
                }),
                attr: got_attr,
            },
            &Ty::Function {
                params: expected_params,
                ret: expected_ret,
                throws: Box::new(Ty::Never {
                    attr: TyAttr::default(),
                }),
                attr: expected_attr,
            },
            aliases,
        ),
        _ => false,
    }
}

pub(crate) fn format_throw_fact_for_diagnostic(ty: &Ty) -> String {
    match ty {
        Ty::TypeVar(name, _) if name.as_str().starts_with("__throws_") => {
            format!(
                "throws from callback parameter `{}`",
                &name.as_str()["__throws_".len()..]
            )
        }
        _ => format_ty_for_diagnostic(ty),
    }
}

pub(crate) fn format_throws_ty_for_diagnostic(ty: &Ty) -> String {
    match ty {
        Ty::Union(members, _) => members
            .iter()
            .map(format_throw_fact_for_diagnostic)
            .collect::<Vec<_>>()
            .join(" | "),
        other => format_throw_fact_for_diagnostic(other),
    }
}

fn needs_postfix_parens_for_diagnostic(ty: &Ty) -> bool {
    matches!(ty, Ty::Union(..) | Ty::Function { .. })
}

fn format_postfix_base_for_diagnostic(ty: &Ty) -> String {
    let rendered = format_ty_for_diagnostic(ty);
    if needs_postfix_parens_for_diagnostic(ty) {
        format!("({rendered})")
    } else {
        rendered
    }
}

pub(crate) fn format_ty_for_diagnostic(ty: &Ty) -> String {
    match ty {
        Ty::Class(qn, _) | Ty::Enum(qn, _) | Ty::TypeAlias(qn, _) => qn.to_string(),
        Ty::EnumVariant(qn, variant, _) => format!("{qn}.{variant}"),
        Ty::Primitive(p, _) => p.to_string(),
        Ty::List(inner, _) => format!("{}[]", format_postfix_base_for_diagnostic(inner)),
        Ty::Map(key, value, _) => {
            format!(
                "map<{}, {}>",
                format_ty_for_diagnostic(key),
                format_ty_for_diagnostic(value)
            )
        }
        Ty::Union(members, _) => members
            .iter()
            .map(format_ty_for_diagnostic)
            .collect::<Vec<_>>()
            .join(" | "),
        Ty::Optional(inner, _) => format!("{}?", format_postfix_base_for_diagnostic(inner)),
        Ty::Literal(lit, _, _) => lit.to_string(),
        Ty::EvolvingList(inner, _) => {
            if matches!(inner.as_ref(), Ty::Never { .. }) {
                "_[]".to_string()
            } else {
                format!("{}[] (evolving)", format_postfix_base_for_diagnostic(inner))
            }
        }
        Ty::EvolvingMap(key, value, _) => {
            if matches!(key.as_ref(), Ty::Never { .. })
                && matches!(value.as_ref(), Ty::Never { .. })
            {
                "map<_, _>".to_string()
            } else {
                format!(
                    "map<{}, {}> (evolving)",
                    format_ty_for_diagnostic(key),
                    format_ty_for_diagnostic(value)
                )
            }
        }
        Ty::Function {
            params,
            ret,
            throws,
            ..
        } => {
            let rendered_params = params
                .iter()
                .map(|(name, ty)| match name {
                    Some(name) => format!("{name}: {}", format_ty_for_diagnostic(ty)),
                    None => format_ty_for_diagnostic(ty),
                })
                .collect::<Vec<_>>()
                .join(", ");
            let mut rendered = format!("({rendered_params}) -> {}", format_ty_for_diagnostic(ret));
            if !matches!(throws.as_ref(), Ty::Never { .. }) {
                rendered.push_str(" throws ");
                rendered.push_str(&format_throws_ty_for_diagnostic(throws));
            }
            rendered
        }
        Ty::TypeVar(name, _) if name.as_str().starts_with("__throws_") => {
            format!(
                "throws from callback parameter `{}`",
                &name.as_str()["__throws_".len()..]
            )
        }
        Ty::TypeVar(name, _) => name.to_string(),
        Ty::Never { .. } => "never".to_string(),
        Ty::Void { .. } => "void".to_string(),
        Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } => "unknown".to_string(),
        Ty::RustType { .. } => "$rust_type".to_string(),
        Ty::Type { .. } => "type".to_string(),
        Ty::Error { .. } => "!error".to_string(),
    }
}

fn declared_covers_fact(
    declared: &Ty,
    fact: &Ty,
    aliases: &HashMap<QualifiedTypeName, Ty>,
) -> bool {
    match fact {
        Ty::Unknown { .. } | Ty::Error { .. } => {
            let resolved = resolve_alias_chain(declared, aliases);
            matches!(
                resolved,
                Ty::Unknown { .. } | Ty::BuiltinUnknown { .. } | Ty::Error { .. }
            )
        }
        Ty::BuiltinUnknown { .. } => {
            let resolved = resolve_alias_chain(declared, aliases);
            matches!(
                resolved,
                Ty::BuiltinUnknown { .. } | Ty::Unknown { .. } | Ty::Error { .. }
            )
        }
        _ => normalize::is_subtype_of(fact, declared, aliases),
    }
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::function_shape_matches_ignoring_outer_throws;
    use crate::ty::{PrimitiveType, Ty, TyAttr};

    #[test]
    fn optional_function_shapes_match_ignoring_outer_throws() {
        let aliases = HashMap::new();
        let got = Ty::Optional(
            Box::new(Ty::Function {
                params: vec![(None, Ty::Primitive(PrimitiveType::Int, TyAttr::default()))],
                ret: Box::new(Ty::Primitive(PrimitiveType::String, TyAttr::default())),
                throws: Box::new(Ty::Primitive(PrimitiveType::String, TyAttr::default())),
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );
        let expected = Ty::Optional(
            Box::new(Ty::Function {
                params: vec![(None, Ty::Primitive(PrimitiveType::Int, TyAttr::default()))],
                ret: Box::new(Ty::Primitive(PrimitiveType::String, TyAttr::default())),
                throws: Box::new(Ty::Primitive(PrimitiveType::Bool, TyAttr::default())),
                attr: TyAttr::default(),
            }),
            TyAttr::default(),
        );

        assert!(function_shape_matches_ignoring_outer_throws(
            &got, &expected, &aliases
        ));
    }
}
