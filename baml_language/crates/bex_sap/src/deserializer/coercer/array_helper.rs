use crate::baml_value::BamlValue;
use crate::sap_model::{TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent};
use anyhow::Result;

use super::{ParsingContext, ParsingError};
use crate::deserializer::{deserialize_flags::Flag, types::BamlValueWithFlags};

/// Tries to pick one of the items in the array and returns it.
pub(super) fn coerce_array_to_singular<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    target: TyWithMeta<TyResolvedRef<'t, N>, &TypeAnnotations<'t, N>>,
    items: impl IntoIterator<Item = &'v crate::jsonish::Value<'s>>,
    coercion: &dyn Fn(
        &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError>,
) -> Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError> {
    let parsed = items
        .into_iter()
        .filter_map(|item| coercion(item).transpose())
        .collect::<Vec<_>>();

    let mut best = pick_best(ctx, target, parsed);

    if let Ok(ref mut f) = best {
        // Store empty vec - the full results are only used for debugging display
        // TODO: Restore if detailed debugging is needed:
        // f.add_flag(Flag::FirstMatch(0, parsed.to_vec()))
        f.add_flag(Flag::FirstMatch(0, vec![]))
    }

    best
}

/// Picks the best value to return for the target type+annotations.
pub(super) fn pick_best<'s, 'v, 't, 'a, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    target: TyWithMeta<TyResolvedRef<'t, N>, &TypeAnnotations<'t, N>>,
    res: Vec<Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError>>,
) -> Result<BamlValueWithFlags<'s, 'v, 't, N>, ParsingError> {
    if res.is_empty() {
        return Err(ctx.error_unexpected_empty_array(&target.ty));
    };
    if res.len() == 1 {
        return res.into_iter().next().unwrap();
    }

    let res_index = (0..res.len())
        .map(|i| match res[i] {
            Ok(ref v) => (i, v.score()),
            Err(_) => (i, i32::MAX),
        })
        .collect::<Vec<_>>();

    // Pick the best one, but in case of picking "default" values like null or empty list, prefer picking the first one
    let all_valid_scores = res_index
        .iter()
        .filter_map(|&(i, score)| match res.get(i) {
            Some(Ok(r)) => Some((
                i,
                score,
                match &r.value {
                    BamlValue::Array(arr) => {
                        arr.value.is_empty()
                            && r.meta
                                .flags
                                .flags
                                .iter()
                                .any(|f| matches!(f, Flag::SingleToArray))
                    }
                    _ => false,
                },
                r,
            )),
            _ => None,
        })
        .collect::<Vec<_>>();

    // Sort by (false, score, index)
    let best = all_valid_scores.into_iter().max_by(
        |&(a, a_score, a_default, a_val), &(b, b_score, b_default, b_val)| {
            // TODO: This is a bit of a hack. We should likely use some is_subtype_of logic here
            // to ensure that we're accepting the "best" type.
            // E.g. if a is a subtype of b, we should prefer a over b. (empty list is a subtype of any list)
            if matches!(&a_val.value, BamlValue::Array(..))
                && matches!(&b_val.value, BamlValue::Array(..))
            {
                let a_is_single = a_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::SingleToArray));
                let b_is_single = b_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::SingleToArray));

                match (a_is_single, b_is_single) {
                    // Return B
                    (true, false) => return std::cmp::Ordering::Greater,
                    // Return A
                    (false, true) => return std::cmp::Ordering::Less,
                    _ => {
                        if let (BamlValue::Array(arr_a), BamlValue::Array(arr_b)) =
                            (&a_val.value, &b_val.value)
                        {
                            // Prefer lists with properly parsed content over lists containing raw markdown strings
                            // NOTE: inner items are BamlValue (no metadata), so per-item
                            // condition checks cannot be expressed here yet.
                            let a_has_markdown_string = false;
                            let b_has_markdown_string = false;

                            match (a_has_markdown_string, b_has_markdown_string) {
                                // If a has markdown string but b doesn't, prefer b
                                (true, false) => return std::cmp::Ordering::Greater,
                                // If b has markdown string but a doesn't, prefer a
                                (false, true) => return std::cmp::Ordering::Less,
                                _ => {}
                            }

                            let unparseables_a = a_val
                                .conditions()
                                .flags
                                .iter()
                                .filter(|f| matches!(f, Flag::ArrayItemParseError(..)))
                                .count();
                            let unparseables_b = b_val
                                .conditions()
                                .flags
                                .iter()
                                .filter(|f| matches!(f, Flag::ArrayItemParseError(..)))
                                .count();
                            match (unparseables_a, unparseables_b) {
                                // If A has no unparseables and B has unparseables and B is empty, prefer A
                                (0, b) if b > 0 && arr_b.value.is_empty() => {
                                    return std::cmp::Ordering::Less;
                                }
                                // If A has unparseables and B has no unparseables and A is empty, prefer B
                                (a, 0) if a > 0 && arr_a.value.is_empty() => {
                                    return std::cmp::Ordering::Greater;
                                }
                                _ => {}
                            }
                        }
                    }
                }
            }

            // De-value default values when comparing
            if let (BamlValue::Class(cls_a), BamlValue::Class(cls_b)) = (&a_val.value, &b_val.value)
            {
                let a_props = &cls_a.value;
                let b_props = &cls_b.value;

                // If matching on a union, and one of the choices is picking an object that only
                // had a single string coerced from JSON, prefer the other one
                // (since string cost is low, its better to pick the other one if possible)
                if matches!(target.ty, TyResolvedRef::Union(_)) {
                    let a_is_coerced_string = a_props.len() == 1
                        && a_props
                            .iter()
                            .all(|(_, val)| matches!(&val.value, BamlValue::String(..)));

                    let b_is_coerced_string = b_props.len() == 1
                        && b_props
                            .iter()
                            .all(|(_, val)| matches!(&val.value, BamlValue::String(..)));

                    match (a_is_coerced_string, b_is_coerced_string) {
                        // Return B
                        (true, false) => return std::cmp::Ordering::Greater,
                        // Return A
                        (false, true) => return std::cmp::Ordering::Less,
                        _ => {}
                    }
                }

                // NOTE: per-field condition checks (default detection) cannot be expressed
                // on inner BamlValue items which lack metadata. Using top-level flags instead.
                let a_is_default = a_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::DefaultFromNoValue));
                let b_is_default = b_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::DefaultFromNoValue));

                match (a_is_default, b_is_default) {
                    // Return B
                    (true, false) => return std::cmp::Ordering::Greater,
                    // Return A
                    (false, true) => return std::cmp::Ordering::Less,
                    _ => {}
                }
            }

            // Devalue strings that were cast from objects.
            if !a_val.is_composite()
                && b_val.is_composite()
                && a_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::JsonToString(..) | Flag::FirstMatch(_, _)))
            {
                return std::cmp::Ordering::Greater;
            }

            if a_val.is_composite()
                && !b_val.is_composite()
                && b_val
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::JsonToString(..) | Flag::FirstMatch(_, _)))
            {
                return std::cmp::Ordering::Less;
            }

            match a_default.cmp(&b_default) {
                std::cmp::Ordering::Equal => match a_score.cmp(&b_score) {
                    std::cmp::Ordering::Equal => a.cmp(&b),
                    std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                    std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
                },
                std::cmp::Ordering::Less => std::cmp::Ordering::Less,
                std::cmp::Ordering::Greater => std::cmp::Ordering::Greater,
            }
        },
    );

    // log::trace!(
    //     "Picking {} from {:?} items. Picked({:?}):\n{}",
    //     target,
    //     res_index,
    //     first,
    //     res.as_ref()
    //         .iter()
    //         .enumerate()
    //         .map(|(idx, r)| match r {
    //             Ok(r) => format!("{idx} {r:#}"),
    //             Err(e) => format!("{idx} {e:#}"),
    //         })
    //         .collect::<Vec<_>>()
    //         .join("\n")
    // );

    // Take the best one
    match best {
        Some((i, _, _, v)) => {
            let mut v = v.clone();
            if res.len() > 1 {
                // Only add flag if one doesn't already exist.
                // This allows callers (like try_cast_union) to pre-add the flag with the
                // correct original index, avoiding bugs where the filtered list index differs
                // from the original union variant index.
                let has_union_match = v
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::UnionMatch(_, _)));
                let has_first_match = v
                    .conditions()
                    .flags
                    .iter()
                    .any(|f| matches!(f, Flag::FirstMatch(_, _)));

                if !has_union_match && !has_first_match {
                    // Store empty vec - the full results are only used for debugging display
                    // TODO: Restore if detailed debugging is needed:
                    // v.add_flag(if matches!(target.ty, TyResolvedRef::Union(_)) {
                    //     Flag::UnionMatch(i, res.to_vec())
                    // } else {
                    //     Flag::FirstMatch(i, res.to_vec())
                    // });
                    v.add_flag(if matches!(target.ty, TyResolvedRef::Union(_)) {
                        Flag::UnionMatch(i, vec![])
                    } else {
                        Flag::FirstMatch(i, vec![])
                    });
                }
            }
            Ok(v)
        }
        None => {
            if !res.is_empty() {
                let errors = res.iter().filter_map(|r| r.as_ref().err());
                Err(ctx.error_merge_multiple(
                    &format!("Failed to find any {} in {} items", target, res.len()),
                    errors,
                ))
            } else {
                Err(ctx.error_internal("Index out of bounds"))
            }
        }
    }
}
