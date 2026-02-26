use crate::baml_value::{BamlEnum, BamlString, BamlValue};
use crate::deserializer::deserialize_flags::DeserializerConditions;
use crate::deserializer::types::{DeserializerMeta, ValueWithFlags};
use crate::jsonish::{self, CompletionState};
use crate::sap_model::{
    AnnotatedEnumVariant, EnumTy, FromLiteral, TyResolvedRef, TyWithMeta, TypeAnnotations,
    TypeIdent,
};
use anyhow::Result;

use super::ParsingContext;
use crate::deserializer::{
    coercer::{ParsingError, TypeCoercer, match_string::match_string},
    deserialize_flags::Flag,
};

fn enum_match_candidates<'t, N: TypeIdent>(enm: &EnumTy<'t, N>) -> Vec<(&'t str, Vec<String>)> {
    // TODO: Extract variant names from EnumTy.variants (Vec<AnnotatedTy>).
    // The old code extracted (name, aliases) from each enum variant Name + description.
    // With the new model, variants are AnnotatedTy values whose names need to be
    // extracted differently.
    todo!("Extract variant names from EnumTy variants")
}

impl<'t, N: TypeIdent + 't> TypeCoercer<'t, N> for EnumTy<'t, N> {
    fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Option<ValueWithFlags<'t, Self::Value, N>> {
        let enum_ty = target.ty;
        let meta = target.meta;

        // Enums can only be cast from string values
        let Some(value) = value else {
            return None;
        };
        let jsonish::Value::String(s, completion) = value else {
            return None;
        };

        if completion == &CompletionState::Incomplete {
            return if let Some(ref in_progress) = meta.in_progress {
                let in_progress = enum_ty.from_literal(in_progress, ctx).ok()?;
                Some(ValueWithFlags::new(
                    in_progress,
                    DeserializerMeta {
                        flags: DeserializerConditions::new()
                            .with_flag(Flag::DefaultFromInProgress(value.clone())),
                        ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
                    },
                ))
            } else {
                None
            };
        }

        // assumes no name or alias can have the same value as another name or alias
        for AnnotatedEnumVariant { name, aliases } in enum_ty.variants.iter() {
            if name == s {
                let value = BamlEnum {
                    name: &enum_ty.name,
                    value: name.to_string(),
                };
                if !meta
                    .check_asserts(&BamlValue::Enum(value.clone()), ctx)
                    .ok()?
                {
                    return None;
                }
                return Some(ValueWithFlags::new(
                    value,
                    DeserializerMeta {
                        flags: DeserializerConditions::new(),
                        ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
                    },
                ));
            }
        }

        None
    }

    fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&jsonish::Value>,
    ) -> Result<ValueWithFlags<'t, Self::Value, N>, ParsingError> {
        log::debug!(
            "scope: {scope} :: coercing to: {name} (current: {current})",
            name = target.ty.name,
            scope = ctx.display_scope(),
            current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
        );

        // Enums can only be cast from string values
        let value = match value {
            None | Some(jsonish::Value::Null) => {
                return Err(ctx.error_unexpected_null(&target));
            }
            Some(v) => v,
        };

        let enum_ty = target.ty;
        let meta = target.meta;

        if value.completion_state() == &CompletionState::Incomplete
            && let Some(ref in_progress) = meta.in_progress
        {
            let in_progress = enum_ty.from_literal(in_progress, ctx)?;
            return Ok(ValueWithFlags::new(
                in_progress,
                DeserializerMeta {
                    flags: DeserializerConditions::new()
                        .with_flag(Flag::DefaultFromInProgress(value.clone())),
                    ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
                },
            ));
        }

        let variant_match = match_string(
            ctx,
            TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
            Some(value),
            &enum_match_candidates(enum_ty),
            true,
        )?;

        let value = BamlEnum {
            name: &enum_ty.name,
            value: variant_match.value.value.clone(),
        };
        if !meta.check_asserts(&BamlValue::Enum(value), ctx)? {
            return Err(ctx.error_assertion_failure());
        }

        Ok(variant_match.map_value(|BamlString { value }| BamlEnum {
            name: &enum_ty.name,
            value,
        }))
    }
}
