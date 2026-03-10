use std::borrow::Cow;

use anyhow::Result;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::{
    baml_value::{BamlArray, BamlValue},
    deserializer::{
        deserialize_flags::{DeserializerConditions, Flag},
        types::{BamlValueWithFlags, DeserializerMeta, ValueWithFlags},
    },
    jsonish::CompletionState,
    sap_model::{
        ArrayTy, AttrLiteral, FromLiteral, TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent,
    },
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

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for ArrayTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let element_type = &*target.ty.ty;
        let element_type = ctx.db.resolve_with_meta(element_type.as_ref()).ok()?;

        // Only handle array values
        let crate::jsonish::Value::Array(arr, completion_state) = value else {
            return None;
        };

        let flags = match (completion_state, target.meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return target
                    .ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value))),
                                ty: TyWithMeta::new(TyResolvedRef::Array(target.ty), target.meta),
                            },
                        )
                    })
                    .ok();
            }
            (CompletionState::Incomplete, None) => {
                DeserializerConditions::new().with_flag(Flag::Incomplete)
            }
            (CompletionState::Complete, _) => DeserializerConditions::new(),
        };

        // For empty arrays, we can return immediately
        if arr.is_empty() {
            return Some(ValueWithFlags::new(
                BamlArray { value: Vec::new() },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Array(target.ty), target.meta),
                },
            ));
        }

        // Try to cast all elements, tracking union hints for optimization
        let mut items = Vec::with_capacity(arr.len());
        let mut last_union_hint: Option<usize> = None;
        for (i, item) in arr.iter().enumerate() {
            let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
            let et_ref = TyWithMeta::new(element_type.ty, element_type.meta);
            let v = TyResolvedRef::try_cast(&child_ctx, et_ref, item)?;

            // Extract winning variant index for the next iteration's hint
            last_union_hint = extract_union_winner_index(&v);
            items.push(v);
        }

        Some(ValueWithFlags::new(
            BamlArray { value: items },
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Array(target.ty), target.meta),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        let element_type = &*target.ty.ty;
        let element_type = ctx
            .db
            .resolve_with_meta(element_type.as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;

        let mut items = vec![];
        let mut flags = DeserializerConditions::new();

        match value {
            crate::jsonish::Value::Array(arr, completion_state) => {
                if *completion_state == CompletionState::Incomplete {
                    match &target.meta.in_progress {
                        Some(AttrLiteral::Never) => return Ok(None),
                        Some(lit) => {
                            let ret = target.ty.from_literal(lit, ctx)?;
                            let ret = ValueWithFlags::new(
                                ret,
                                DeserializerMeta {
                                    flags: DeserializerConditions::new().with_flag(
                                        Flag::DefaultFromInProgress(Cow::Borrowed(value)),
                                    ),
                                    ty: target.map_ty(TyResolvedRef::Array),
                                },
                            );
                            return Ok(Some(ret));
                        }
                        None => {
                            flags.add_flag(Flag::Incomplete);
                        }
                    }
                }
                // Track the winning union variant from the previous element to hint the next
                let mut last_union_hint: Option<usize> = None;
                for (i, item) in arr.iter().enumerate() {
                    let child_ctx = ctx.enter_scope_with_hint(&format!("{i}"), last_union_hint);
                    let et_ref = TyWithMeta::new(element_type.ty, element_type.meta);
                    match TyResolvedRef::coerce(&child_ctx, et_ref, item) {
                        Ok(Some(v)) => {
                            // Extract winning variant index for the next iteration's hint
                            last_union_hint = extract_union_winner_index(&v);
                            items.push(v);
                        }
                        Ok(None) => {
                            // child is incomplete with `in_progress = never`
                            debug_assert_eq!(
                                *completion_state,
                                CompletionState::Incomplete,
                                "Array should be incomplete if an item is."
                            );
                        }
                        // TODO(vbv): document why we penalize in proportion to how deep into an array a parse error is
                        Err(e) => flags.add_flag(Flag::ArrayItemParseError(i, e)),
                    }
                }
            }
            // Not an array: try and make it a single-value array
            v => {
                flags.add_flag(Flag::SingleToArray);
                let et_ref = TyWithMeta::new(element_type.ty, element_type.meta);
                match TyResolvedRef::coerce(&ctx.enter_scope("<implied>"), et_ref, v) {
                    Ok(Some(v)) => items.push(v),
                    Ok(None) => {
                        flags.add_flag(Flag::Incomplete);
                    }
                    Err(e) => flags.add_flag(Flag::ArrayItemParseError(0, e)),
                }
            }
        }

        let ret = BamlValue::Array(BamlArray { value: items });
        target.meta.expect_asserts(&ret, ctx)?;
        let BamlValue::Array(ret) = ret else {
            unreachable!("we just wrapped it in a BamlValue::Array");
        };

        Ok(Some(ValueWithFlags::new(
            ret,
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Array(target.ty), target.meta),
            },
        )))
    }
}
