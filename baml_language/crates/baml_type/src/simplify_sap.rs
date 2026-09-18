//! Type simplification for SAP.
//!
//! Rewrites a [`RuntimeTy`] into the flat form the SAP model converter
//! requires, without changing the set of values it denotes:
//!
//! - Non-recursive type aliases are inlined; a recursive alias stays named,
//!   as the indirection that ties its knot (`json[]` keeps its `json`).
//! - Unions are flattened, so a member is never itself a union. For a parse
//!   target this includes a recursive union alias used directly as a member:
//!   `json | null` splices `json`'s own members in, while below a list or map
//!   the alias stays named.
//! - A member covered by another is dropped: the same type, a literal under
//!   its primitive (`5 | int` is `int`), or `int` under `bigint` — the one
//!   lossless widening. `int → float` is not a subsumption here: SAP scores
//!   those as distinct candidates.
//! - `null` moves to the last position, and a singleton union unwraps.
//!
//! No attribute takes part: attributes belong to declarations (BEP-075), not
//! to the types being simplified, so nothing here can drop or merge one.
use std::collections::{HashMap, HashSet};

use crate::{Literal, RuntimeTy};

/// Simplify a type for SAP processing.
///
/// - `aliases`: map from alias name to its definition.
/// - `recursive_aliases`: aliases that are recursive (left as `RuntimeTy::TypeAlias`).
pub fn simplify<N: crate::Head>(
    ty: RuntimeTy<N>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive_aliases: &HashSet<N>,
) -> RuntimeTy<N> {
    simplify_impl(ty, aliases, recursive_aliases, false)
}

/// Simplify a runtime-materialized SAP parse target.
///
/// In addition to [`simplify`], this expands a recursive union alias when it
/// appears directly as another union's member. This turns `json | null` into a
/// flat union while retaining recursive `json` references below lists/maps.
pub fn simplify_parse_target<N: crate::Head>(
    ty: RuntimeTy<N>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive_aliases: &HashSet<N>,
) -> RuntimeTy<N> {
    simplify_impl(ty, aliases, recursive_aliases, true)
}

fn simplify_impl<N: crate::Head>(
    ty: RuntimeTy<N>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive: &HashSet<N>,
    expand_recursive_alias_unions: bool,
) -> RuntimeTy<N> {
    match ty {
        RuntimeTy::TypeAlias(ref name) => {
            if recursive.contains(name) {
                // Recursive alias: keep as TypeAlias, don't inline.
                ty
            } else if let Some(target) = aliases.get(name) {
                // Non-recursive: expand and simplify the result.
                simplify_impl(
                    target.clone(),
                    aliases,
                    recursive,
                    expand_recursive_alias_unions,
                )
            } else {
                // Unknown alias — leave as-is.
                ty
            }
        }

        RuntimeTy::Union(variants) => {
            simplify_union(variants, aliases, recursive, expand_recursive_alias_unions)
        }

        // Recurse into compound types.
        RuntimeTy::List(inner) => RuntimeTy::List(Box::new(simplify_impl(
            *inner,
            aliases,
            recursive,
            expand_recursive_alias_unions,
        ))),
        RuntimeTy::Map { key, value } => RuntimeTy::Map {
            key: Box::new(simplify_impl(
                *key,
                aliases,
                recursive,
                expand_recursive_alias_unions,
            )),
            value: Box::new(simplify_impl(
                *value,
                aliases,
                recursive,
                expand_recursive_alias_unions,
            )),
        },

        // Leaf types pass through unchanged.
        _ => ty,
    }
}

// ---------------------------------------------------------------------------
// Union simplification
// ---------------------------------------------------------------------------

fn simplify_union<N: crate::Head>(
    variants: Box<[RuntimeTy<N>]>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive: &HashSet<N>,
    expand_recursive_alias_unions: bool,
) -> RuntimeTy<N> {
    // 1. Simplify each variant recursively.
    let variants: Vec<RuntimeTy<N>> = variants
        .into_iter()
        .map(|v| simplify_impl(v, aliases, recursive, expand_recursive_alias_unions))
        .collect();

    // 2. A recursive alias may itself be a union. It must stay named under an
    // indirection such as `json[]`, but when used directly as another union's
    // member (`json | null`) its immediate variants must be spliced into this
    // union so SAP never receives an alias-hidden nested union.
    let variants = if expand_recursive_alias_unions {
        expand_recursive_union_aliases(variants, aliases, recursive)
    } else {
        variants
    };

    // 3. Flatten nested unions.
    let variants = flatten_union(variants);

    // 4. Deduplicate.
    let variants = dedup_variants(variants);

    // 5. Push null to end.
    let variants = null_to_end(variants);

    // 6. Unwrap singleton.
    if variants.len() == 1 {
        variants
            .into_iter()
            .next()
            .unwrap_or_else(|| unreachable!("a length-1 vec has an element"))
    } else {
        RuntimeTy::Union(variants.into())
    }
}

fn expand_recursive_union_aliases<N: crate::Head>(
    variants: Vec<RuntimeTy<N>>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive: &HashSet<N>,
) -> Vec<RuntimeTy<N>> {
    let mut out = Vec::new();
    let mut expanding = HashSet::new();
    for variant in variants {
        expand_recursive_union_alias_variant(variant, aliases, recursive, &mut expanding, &mut out);
    }
    out
}

fn expand_recursive_union_alias_variant<N: crate::Head>(
    variant: RuntimeTy<N>,
    aliases: &HashMap<N, RuntimeTy<N>>,
    recursive: &HashSet<N>,
    expanding: &mut HashSet<N>,
    out: &mut Vec<RuntimeTy<N>>,
) {
    let RuntimeTy::TypeAlias(name) = variant else {
        out.push(variant);
        return;
    };

    let Some(RuntimeTy::Union(alias_variants)) = aliases.get(&name) else {
        out.push(RuntimeTy::TypeAlias(name));
        return;
    };
    if !recursive.contains(&name) || !expanding.insert(name.clone()) {
        out.push(RuntimeTy::TypeAlias(name));
        return;
    }

    for member in alias_variants {
        let member = simplify_impl(member.clone(), aliases, recursive, true);
        expand_recursive_union_alias_variant(member, aliases, recursive, expanding, out);
    }
    expanding.remove(&name);
}

/// Flatten nested unions into a single level.
fn flatten_union<N: crate::Head>(variants: Vec<RuntimeTy<N>>) -> Vec<RuntimeTy<N>> {
    let mut out = Vec::new();
    for v in variants {
        match v {
            RuntimeTy::Union(inner_variants) => out.extend(inner_variants),
            other => out.push(other),
        }
    }
    out
}

/// Remove variants that are subtypes of other variants.
///
/// If variant A is a subtype of variant B, A is dropped and B is kept.
fn dedup_variants<N: crate::Head>(variants: Vec<RuntimeTy<N>>) -> Vec<RuntimeTy<N>> {
    let mut result: Vec<RuntimeTy<N>> = Vec::new();
    for candidate in variants {
        if result
            .iter()
            .any(|existing| is_sap_structural_subtype(&candidate, existing))
        {
            // candidate is already covered by something in result — skip it.
            continue;
        }
        // Remove any existing variants that the candidate now covers.
        result.retain(|existing| !is_sap_structural_subtype(existing, &candidate));
        result.push(candidate);
    }
    result
}

/// Push `null` variants to the end.
fn null_to_end<N: crate::Head>(variants: Vec<RuntimeTy<N>>) -> Vec<RuntimeTy<N>> {
    let mut non_null = Vec::new();
    let mut nulls = Vec::new();
    for v in variants {
        if matches!(v, RuntimeTy::Null) {
            nulls.push(v);
        } else {
            non_null.push(v);
        }
    }
    non_null.extend(nulls);
    non_null
}

// ---------------------------------------------------------------------------
// Subtyping (for deduplication)
// ---------------------------------------------------------------------------

/// Restricted structural subtyping for SAP dedup.
///
/// Returns `true` when `sub` and `sup` are the same type, or when `sub` is a
/// literal whose base primitive matches `sup`.
fn is_sap_structural_subtype<N: crate::Head>(sub: &RuntimeTy<N>, sup: &RuntimeTy<N>) -> bool {
    if sub == sup {
        return true;
    }

    matches!(
        (sub, sup),
        (RuntimeTy::Literal(Literal::Int(_), _), RuntimeTy::Int)
            | (RuntimeTy::Literal(Literal::Int(_), _), RuntimeTy::Bigint)
            | (RuntimeTy::Literal(Literal::Bigint(_), _), RuntimeTy::Bigint)
            | (RuntimeTy::Literal(Literal::Float(_), _), RuntimeTy::Float)
            | (RuntimeTy::Literal(Literal::String(_), _), RuntimeTy::String)
            | (RuntimeTy::Literal(Literal::Bool(_), _), RuntimeTy::Bool)
            // The one cross-type widening allowed by SAP: `int → bigint`
            // is lossless, unlike `int → float` which loses precision past
            // 2^53.
            | (RuntimeTy::Int, RuntimeTy::Bigint)
    )
}
