use std::borrow::Cow;
use std::collections::VecDeque;
use std::ops::Deref;

use crate::baml_value::{BamlMap, BamlValue};
use crate::deserializer::types::{DeserializerMeta, ValueWithFlags};
use crate::jsonish::CompletionState;
use crate::sap_model::{
    AttrLiteral, FromLiteral as _, MapTy, Ty, TyResolved, TyResolvedRef, TyWithMeta,
    TypeAnnotations, TypeIdent,
};
use anyhow::Result;
use indexmap::IndexMap;

use super::{ParsingContext, ParsingError, TypeCoercer};
use crate::{
    deserializer::{
        deserialize_flags::{DeserializerConditions, Flag},
        types::BamlValueWithFlags,
    },
    jsonish,
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for MapTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // Only handle object values
        let crate::jsonish::Value::Object(obj, completion_state) = value else {
            return None;
        };

        let mut flags = match (completion_state, target.meta.in_progress.as_ref()) {
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
                                ty: target.map_ty(TyResolvedRef::Map),
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
        flags.add_flag(Flag::ObjectToMap(Cow::Borrowed(value)));

        let map_ty = target.ty;
        let meta = target.meta;

        // For empty objects, we can return immediately
        if obj.is_empty() {
            return Some(ValueWithFlags::new(
                BamlMap {
                    value: IndexMap::new(),
                },
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::Map(map_ty), meta),
                },
            ));
        }

        let value_ty_with_meta = ctx
            .db
            .resolve_with_meta(map_ty.value.deref().as_ref())
            .ok()?;

        // Try to cast all values
        let items: IndexMap<Cow<'s, str>, BamlValueWithFlags<'s, 'v, 't, N>> = obj
            .iter()
            .map(|(key, value)| {
                let target_ref = TyWithMeta::new(value_ty_with_meta.ty, value_ty_with_meta.meta);
                TyResolvedRef::try_cast(ctx, target_ref, value)
                    .map(|cast_value| (key.clone(), cast_value))
            })
            .collect::<Option<_>>()?;

        let map = BamlMap { value: items };
        Some(ValueWithFlags::new(
            map,
            DeserializerMeta {
                flags,
                ty: TyWithMeta::new(TyResolvedRef::Map(map_ty), meta),
            },
        ))
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target,
            scope = ctx.display_scope(),
            current = value.r#type()
        );

        if matches!(value, crate::jsonish::Value::Null) {
            return Err(ctx.error_unexpected_null(&target));
        }

        let key_type = ctx
            .db
            .resolve_with_meta(target.ty.key.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let value_type = ctx
            .db
            .resolve_with_meta(target.ty.value.deref().as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;

        // TODO: Do we actually need to check the key type here in the coercion
        // logic? Can the user pass a "type" here at runtime? Can we pass the wrong
        // type from our own code or is this guaranteed to be a valid map key type?
        // If we can determine that the type is always valid then we can get rid of
        // this logic and skip the loops & allocs in the the union branch.
        match key_type.ty {
            // String, enum or just one literal string, OK.
            TyResolvedRef::String(_) | TyResolvedRef::Enum(_) | TyResolvedRef::LiteralString(_) => {
            }

            // For unions we need to check if all the items are literal strings.
            TyResolvedRef::Union(sub_union) => {
                let mut queue = VecDeque::from_iter(&sub_union.variants);
                while let Some(item) = queue.pop_front() {
                    match &item.ty {
                        Ty::ResolvedRef(TyResolvedRef::LiteralString(_))
                        | Ty::Resolved(TyResolved::LiteralString(_)) => continue,
                        Ty::ResolvedRef(TyResolvedRef::Union(nested)) => {
                            queue.extend(&nested.variants);
                        }
                        Ty::Resolved(TyResolved::Union(nested)) => {
                            queue.extend(&nested.variants);
                        }
                        _ => return Err(ctx.error_map_must_have_supported_key(&item.ty)),
                    }
                }
            }

            // Key type not allowed.
            other => return Err(ctx.error_map_must_have_supported_key(&other)),
        }

        let mut flags = DeserializerConditions::new();
        flags.add_flag(Flag::ObjectToMap(Cow::Borrowed(value)));

        let ret = match (&value, target.meta.in_progress.as_ref()) {
            (jsonish::Value::Object(_, CompletionState::Incomplete), Some(AttrLiteral::Never)) => {
                return Ok(None);
            }
            (jsonish::Value::Object(_, CompletionState::Incomplete), Some(lit)) => {
                flags.add_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value)));
                target.ty.from_literal(lit, ctx)?
            }
            (jsonish::Value::Object(obj, completion_state), _) => {
                let mut items = IndexMap::new();
                for (idx, (key, value)) in obj.iter().enumerate() {
                    let vt_ref = TyWithMeta::new(value_type.ty, value_type.meta);
                    let coerced_value =
                        match TyResolvedRef::coerce(&ctx.enter_scope(key), vt_ref, value) {
                            Ok(Some(v)) => v,
                            Ok(None) => {
                                // Value type with `in_progress = never` means we ignore this entry until it is complete.
                                continue;
                            }
                            Err(e) => {
                                flags.add_flag(Flag::MapValueParseError(key.clone(), e));
                                // Could not coerce value, nothing else to do here.
                                continue;
                            }
                        };

                    // Keys are just strings but since we suport enums and literals
                    // we have to check that the key we are reading is actually a
                    // valid enum member or expected literal value. The coercion
                    // logic already does that so we'll just coerce the key.
                    //
                    // TODO: Is it necessary to check that values match here? This
                    // is also checked at `coerce_arg` in
                    // baml-lib/baml-core/src/ir/ir_helpers/to_baml_arg.rs
                    // TODO: Is it Ok that we assume keys are complete?
                    let key_as_jsonish =
                        jsonish::Value::String(key.to_owned(), CompletionState::Complete);
                    match TyResolvedRef::coerce(ctx, key_type.clone(), &key_as_jsonish) {
                        Ok(None) => {
                            unreachable!("key_as_jsonish is defined to be complete");
                        }
                        Ok(Some(_)) => {
                            // Both the value and the key were successfully
                            // coerced, add the key to the map.
                            items.insert(key.clone(), coerced_value);
                        }
                        // Couldn't coerce key, this is either not a valid enum
                        // variant or it doesn't match any of the literal values
                        // expected.
                        Err(e) => flags.add_flag(Flag::MapKeyParseError(idx, e)),
                    }
                }
                if *completion_state == CompletionState::Incomplete {
                    flags.add_flag(Flag::Incomplete);
                }
                BamlMap { value: items }
            }
            // TODO: first map in an array that matches
            _ => return Err(ctx.error_unexpected_type(&target, value)),
        };

        let ret = BamlValue::Map(ret);
        target.meta.expect_asserts(&ret, ctx)?;
        let BamlValue::Map(ret) = ret else {
            unreachable!("we just wrapped it in a BamlValue::Map");
        };
        Ok(Some(
            ValueWithFlags::new(ret, DeserializerMeta::new(target)).with_flags(flags.flags),
        ))
    }
}

#[cfg(test)]
mod tests {
    use crate::sap_model::{NullTy, PrimitiveTy, StringTy, TypeRefDb};

    use super::*;

    #[test]
    fn test_empty_map() {
        let target_ty: MapTy<'_, &'static str> = MapTy::new(
            TyWithMeta::new(
                PrimitiveTy::String(StringTy).into(),
                TypeAnnotations::default(),
            ),
            TyWithMeta::new(PrimitiveTy::Null(NullTy).into(), TypeAnnotations::default()),
        );
        let db = TypeRefDb::new();

        let ctx = ParsingContext::new(&db);
        let annotations = TypeAnnotations::default();
        let target_ty = TyWithMeta::new(&target_ty, &annotations);

        let parsed = jsonish::Value::Object(vec![], CompletionState::Complete);
        let casted = MapTy::try_cast(&ctx, target_ty, &parsed).unwrap();
        assert_eq!(casted.value.value.len(), 0);
    }
}
