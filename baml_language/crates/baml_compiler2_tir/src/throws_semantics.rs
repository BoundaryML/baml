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
    let got_resolved = resolve_alias_chain(got, aliases);
    let expected_resolved = resolve_alias_chain(expected, aliases);

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
