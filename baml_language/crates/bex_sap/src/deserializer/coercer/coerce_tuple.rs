//! Coercion into fixed-arity tuples `(T1, T2, ...)`.
//!
//! A tuple is written as a JSON array with exactly one item per position. Unlike a list,
//! positions have different types, so each item is coerced on its own (scope `"{i}"`, no
//! union hint carried between positions) and a failure at any position fails the tuple, the
//! way a failing required field fails a class.
//!
//! Lenient handling of the wrong arity:
//! - Too many items: the first `n` are kept and each dropped item is flagged
//!   [`Flag::TupleExtraItem`] (scored like an extra class key).
//! - Too few items in a complete array: each missing position takes
//!   [`crate::sap_model::TypeRefDb::missing_default`] (`null` for optional, `[]` / `{}` for
//!   lists and maps), flagged like an omitted class field; otherwise the tuple fails.
//! - Too few items in an incomplete (streaming) array: each position not yet reached takes
//!   [`crate::sap_model::TypeRefDb::partial_default`] (flagged [`Flag::Pending`]). If any such
//!   position has no default, the tuple has no partial parse yet (`Ok(None)`).
//! - A non-array value becomes a 1-tuple the way it would become a one-item list
//!   ([`Flag::SingleToArray`]); for larger arities it is an error. Objects are not read as
//!   tuples.

use std::borrow::Cow;

use super::{ParsingContext, ParsingError, TypeCoercer, VisitedType};
use crate::{
    baml_value::BamlTuple,
    deserializer::{
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
    },
    jsonish::{self, CompletionState},
    sap_model::{DefaultValue, TupleTy, TyResolvedRef, TypeIdent},
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for TupleTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // Strict: an array of exactly the tuple's arity, every item casting as-is.
        let jsonish::Value::Array(arr, completion_state) = value else {
            return None;
        };
        if arr.len() != target.items.len() {
            return None;
        }

        let flags = match completion_state {
            CompletionState::Incomplete => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            CompletionState::Complete => DeserializerConditions::new(),
        };

        let mut items = Vec::with_capacity(arr.len());
        for (i, (item_ty, item)) in target.items.iter().zip(arr).enumerate() {
            let item_ty = ctx.db.resolve(item_ty).ok()?;
            let child_ctx = ctx.enter_scope(&format!("{i}"));
            items.push(TyResolvedRef::try_cast(&child_ctx, item_ty, item)?);
        }

        Some(ValueWithFlags::new(
            BamlTuple { value: items },
            DeserializerMeta {
                flags,
                ty: TyResolvedRef::Tuple(target),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        match value {
            jsonish::Value::Array(arr, completion_state) => coerce_from_items(
                ctx,
                target,
                arr,
                completion_state == &CompletionState::Incomplete,
            ),
            // A single value becomes a 1-tuple, as it would become a one-item list.
            v if target.items.len() == 1 => {
                // A recursive alias (`type T = int | (T,)`) reaches this tuple again with the
                // same value through the element type; that attempt is already in progress.
                let visited = VisitedType::Tuple(::core::ptr::from_ref(target).cast());
                if ctx.is_visiting(&visited, v, true) {
                    return Ok(None);
                }
                let ctx = ctx.visit(visited, v, true).enter_scope("<implied>");
                let item_ty = ctx
                    .db
                    .resolve(&target.items[0])
                    .map_err(|ident| ctx.error_type_resolution(ident))?;
                let Some(item) = TyResolvedRef::coerce(&ctx, item_ty, v)? else {
                    return Ok(None);
                };
                let mut flags = DeserializerConditions::new();
                if v.completion_state() == &CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                flags.add_flag(Flag::SingleToArray);
                Ok(Some(ValueWithFlags::new(
                    BamlTuple { value: vec![item] },
                    DeserializerMeta {
                        flags,
                        ty: TyResolvedRef::Tuple(target),
                    },
                )))
            }
            _ => Err(ctx.error_unexpected_type(target, value)),
        }
    }
}

/// Builds a tuple from the items of a JSON array, applying the arity rules in the module docs.
fn coerce_from_items<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    target: &'t TupleTy<'t, N>,
    arr: &'v [jsonish::Value<'s>],
    is_incomplete: bool,
) -> Result<Option<ValueWithFlags<'s, 'v, 't, BamlTuple<'s, 'v, 't, N>, N>>, ParsingError>
where
    't: 's,
    's: 'v,
{
    let mut flags = DeserializerConditions::new();
    if is_incomplete {
        flags.add_flag(Flag::Incomplete);
    }

    let mut items = Vec::with_capacity(target.items.len());
    let mut errors = Vec::new();
    for (i, item_ty) in target.items.iter().enumerate() {
        let child_ctx = ctx.enter_scope(&format!("{i}"));
        let resolved = child_ctx
            .db
            .resolve(item_ty)
            .map_err(|ident| child_ctx.error_type_resolution(ident))?;
        let item = match arr.get(i) {
            Some(raw) => match TyResolvedRef::coerce(&child_ctx, resolved, raw) {
                Ok(Some(parsed)) => parsed,
                // Incomplete without a partial parse: the element's default stands in, or the
                // tuple waits.
                Ok(None) => {
                    let default = child_ctx
                        .db
                        .partial_default(item_ty)
                        .map_err(|ident| child_ctx.error_type_resolution(ident))?;
                    let Some(value) = from_default(&child_ctx, resolved, &default)? else {
                        return Ok(None);
                    };
                    value.with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(raw)))
                }
                Err(e) => {
                    errors.push(e);
                    continue;
                }
            },
            // Pending: the streaming array has not reached this position yet.
            None if is_incomplete => {
                let default = child_ctx
                    .db
                    .partial_default(item_ty)
                    .map_err(|ident| child_ctx.error_type_resolution(ident))?;
                let Some(value) = from_default(&child_ctx, resolved, &default)? else {
                    return Ok(None);
                };
                value.with_flag(Flag::Pending)
            }
            // Missing from a complete array.
            None => {
                let default = child_ctx
                    .db
                    .missing_default(item_ty)
                    .map_err(|ident| child_ctx.error_type_resolution(ident))?;
                let Some(value) = from_default(&child_ctx, resolved, &default)? else {
                    errors.push(ParsingError {
                        scope: child_ctx.scope.clone(),
                        reason: format!("Missing required tuple element {i}"),
                        causes: Vec::new(),
                    });
                    continue;
                };
                let flag = if resolved.is_optional(ctx.db) {
                    Flag::OptionalDefaultFromNoValue
                } else {
                    Flag::DefaultFromNoValue
                };
                value.with_flag(flag)
            }
        };
        items.push(item);
    }

    if !errors.is_empty() {
        return Err(ParsingError {
            reason: format!(
                "Failed to parse {} of {} tuple elements (got {} items)",
                errors.len(),
                target.items.len(),
                arr.len()
            ),
            scope: ctx.scope.clone(),
            causes: errors,
        });
    }

    for (i, extra) in arr.iter().enumerate().skip(target.items.len()) {
        flags.add_flag(Flag::TupleExtraItem(i, Cow::Borrowed(extra)));
    }

    Ok(Some(ValueWithFlags::new(
        BamlTuple { value: items },
        DeserializerMeta {
            flags,
            ty: TyResolvedRef::Tuple(target),
        },
    )))
}

/// Materializes an element default, or `None` when the element has none (`never`).
fn from_default<'s, 'v, 't, N: TypeIdent>(
    ctx: &ParsingContext<'s, 'v, 't, N>,
    ty: TyResolvedRef<'t, N>,
    default: &DefaultValue<N>,
) -> Result<Option<BamlValueWithFlags<'s, 'v, 't, N>>, ParsingError>
where
    't: 's,
    's: 'v,
{
    if default == &DefaultValue::Never {
        return Ok(None);
    }
    Ok(Some(ValueWithFlags::new(
        ty.from_literal(default, ctx)?,
        DeserializerMeta::new(ty),
    )))
}

#[cfg(test)]
mod tests {
    use crate::{
        baml_db, baml_ty,
        baml_value::BamlValue,
        deserializer::coercer::{ParsingContext, array_helper},
        jsonish::{CompletionState, Value},
        sap_model::{Ty, TyResolvedRef, TypeRefDb},
    };

    /// With equal scores, an exact-arity tuple beats a list that was built by wrapping a single
    /// value, whichever comes first.
    #[test]
    fn exact_tuple_beats_single_to_array_list() {
        let db: TypeRefDb<'_, &str> = baml_db! {};
        let tuple_ty: Ty<'_, &str> = baml_ty!(tuple(int));
        let list_ty: Ty<'_, &str> = baml_ty!([int]);
        let union_ty: Ty<'_, &str> = baml_ty!(([int] | (tuple(int))));
        let ctx = ParsingContext::new(&db);

        // `[5.5]` as `(int,)`: FloatToInt (1). `5` as `int[]`: SingleToArray (1).
        let as_array = Value::Array(
            vec![Value::Number(
                serde_json::Number::from_f64(5.5).unwrap(),
                CompletionState::Complete,
            )],
            CompletionState::Complete,
        );
        let as_scalar = Value::Number(5.into(), CompletionState::Complete);
        let tuple = TyResolvedRef::coerce(&ctx, db.resolve(&tuple_ty).unwrap(), &as_array)
            .unwrap()
            .unwrap();
        let list = TyResolvedRef::coerce(&ctx, db.resolve(&list_ty).unwrap(), &as_scalar)
            .unwrap()
            .unwrap();
        assert_eq!(tuple.score(), 1);
        assert_eq!(list.score(), 1, "the tie-break is what's under test");

        let best = array_helper::pick_best(
            &ctx,
            db.resolve(&union_ty).unwrap(),
            vec![Ok(Some(list)), Ok(Some(tuple))],
        )
        .unwrap()
        .unwrap();
        assert!(matches!(best.value, BamlValue::Tuple(_)));
    }
}
