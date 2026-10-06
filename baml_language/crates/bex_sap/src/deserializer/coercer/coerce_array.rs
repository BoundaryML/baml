use super::{ParsingContext, ParsingError, TypeCoercer, VisitedType};
use crate::{
    baml_value::BamlArray,
    deserializer::{
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
    },
    jsonish::CompletionState,
    sap_model::{ArrayTy, TyResolvedRef, TypeIdent},
};

/// Extract the winning union variant index from a coerced value's flags.
/// Returns None if the value wasn't from a union coercion.
///
/// IMPORTANT: We iterate in REVERSE to get the LAST (outermost) `UnionMatch` flag.
/// When coercing nested unions like `(A | B)[]` where `B = (C | D)`, the inner
/// union's flag is added first, then the outer union's flag. We want the outer
/// union's index for the array hint, not the inner one.
fn extract_union_winner_index<N: TypeIdent>(
    value: &BamlValueWithFlags<'_, '_, '_, N>,
) -> Option<usize> {
    value
        .conditions()
        .flags()
        .iter()
        .rev()
        .find_map(|flag| match flag {
            Flag::UnionMatch(idx, _) => Some(*idx),
            _ => None,
        })
}

/// Whether `item`, which does not fit `element_type`, is left out of the list
/// rather than failing it, whatever list it stands in.
///
/// - An item that is still arriving is not known to be unfit: a partial parse
///   shows the list without it, and the complete parse judges it.
/// - Media is never read from text (the caller takes it from the reply's
///   media parts), so the text holds no item of a media list. That list is
///   empty, not an error.
fn leaves_out_unfit_item<N: TypeIdent>(
    element_type: TyResolvedRef<'_, N>,
    item: &crate::jsonish::Value<'_>,
) -> bool {
    item.completion_state() == &CompletionState::Incomplete
        || matches!(element_type, TyResolvedRef::Media(_))
}

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for ArrayTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let element_type = ctx.db.resolve(&target.ty).ok()?;

        // Only handle array values
        let crate::jsonish::Value::Array(arr, completion_state) = value else {
            return None;
        };

        let flags = match completion_state {
            CompletionState::Incomplete => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            CompletionState::Complete => DeserializerConditions::new(),
        };

        // For empty arrays, we can return immediately
        if arr.is_empty() {
            return Some(ValueWithFlags::new(
                BamlArray { value: Vec::new() },
                DeserializerMeta {
                    flags,
                    ty: TyResolvedRef::Array(target),
                },
            ));
        }

        // Try to cast all elements, tracking union hints for optimization
        let mut items = Vec::with_capacity(arr.len());
        let mut last_union_hint: Option<usize> = None;
        for (i, item) in arr.iter().enumerate() {
            let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
            let v = TyResolvedRef::try_cast(&child_ctx, element_type, item)?;

            // Extract winning variant index for the next iteration's hint
            last_union_hint = extract_union_winner_index(&v);
            items.push(v);
        }

        Some(ValueWithFlags::new(
            BamlArray { value: items },
            DeserializerMeta {
                flags,
                ty: TyResolvedRef::Array(target),
            },
        ))
    }

    /// A list holds only values of its element type. An item that does not fit
    /// fails the list, with an error that names the item and carries the item's
    /// own failure: a shorter list, or an empty one, would read as a good reply.
    ///
    /// Four cases leave the item out instead (see `leaves_out_unfit_item` and
    /// the single-value arm below): the item is still arriving; the parser
    /// inferred the list itself from separate values in the text; the element
    /// type is media; and `null` stands where the list should be.
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        let element_type = ctx
            .db
            .resolve(&target.ty)
            .map_err(|ident| ctx.error_type_resolution(ident))?;

        let mut items = vec![];
        let mut flags = DeserializerConditions::new();

        match value {
            crate::jsonish::Value::Array(arr, c) => {
                if matches!(c, CompletionState::Incomplete) {
                    flags.add_flag(Flag::Incomplete);
                }
                let inferred = ctx.is_inferred_array(value);

                // Track the winning union variant from the previous element to hint the next
                let mut last_union_hint: Option<usize> = None;
                for (i, item) in arr.iter().enumerate() {
                    let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
                    match TyResolvedRef::coerce(&child_ctx, element_type, item) {
                        Ok(Some(v)) => {
                            // Extract winning variant index for the next iteration's hint
                            last_union_hint = extract_union_winner_index(&v);
                            items.push(v);
                        }
                        Ok(None) => {
                            // The item is incomplete and has no partial parse yet.
                        }
                        // TODO(vbv): document why we penalize in proportion to how deep into an array a parse error is
                        Err(e) if inferred || leaves_out_unfit_item(element_type, item) => {
                            flags.add_flag(Flag::ArrayItemParseError(i, e));
                        }
                        Err(e) => return Err(ctx.error_list_item(&element_type, i, e)),
                    }
                }
            }
            // Not an array: try and make it a single-value array
            v => {
                // A recursive alias (`type J = int | J[]`) reaches this array again with the
                // same value through the element type; that attempt is already in progress
                // and has nothing new to offer.
                let visited = VisitedType::Array(::core::ptr::from_ref(target).cast());
                if ctx.is_visiting(&visited, v, true) {
                    return Ok(None);
                }
                flags.add_flag(Flag::SingleToArray);
                let item_ctx = ctx.visit(visited, v, true).enter_scope("<implied>");
                match TyResolvedRef::coerce(&item_ctx, element_type, v) {
                    Ok(Some(v)) => items.push(v),
                    // The implied item has no partial parse yet, so neither does the array we
                    // are guessing around it.
                    Ok(None) => return Ok(None),
                    // `null` is the absence of a list, as a missing field is: an empty list.
                    Err(e)
                        if matches!(v, crate::jsonish::Value::Null)
                            || leaves_out_unfit_item(element_type, v) =>
                    {
                        flags.add_flag(Flag::ArrayItemParseError(0, e));
                    }
                    // Neither a list nor one item of it.
                    Err(e) => return Err(ctx.error_unexpected_type(target, v).with_cause(e)),
                }
            }
        }

        let ret = BamlArray { value: items };

        Ok(Some(ValueWithFlags::new(
            ret,
            DeserializerMeta {
                flags,
                ty: TyResolvedRef::Array(target),
            },
        )))
    }
}
