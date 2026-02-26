use crate::baml_value::BamlValue;
use crate::deserializer::coercer::{ParsingContext, ParsingError, TypeCoercer};
use crate::jsonish::CompletionState;
use crate::sap_model::{
    ArrayTy, ClassTy, EnumTy, Literal, LiteralTy, MapTy, PrimitiveTy, TyResolvedRef, TyWithMeta,
    TypeAnnotations, TypeIdent, UnionTy,
};

use super::array_helper::coerce_array_to_singular;
use crate::deserializer::{deserialize_flags::Flag, types::BamlValueWithFlags};

/// Dispatch methods for `TyResolvedRef` that delegate to the appropriate
/// `TypeCoercer` implementation based on the variant.
///
/// These are inherent methods (not trait impl) because `TyResolvedRef` is Copy
/// and returned by value from `resolve_with_meta`. The `TypeCoercer` trait
/// requires `&'t Self`, which would require a `'t`-lived reference to a local.
/// By taking `Self` by value (Copy), we avoid that lifetime issue entirely.
impl<'t, N: TypeIdent> TyResolvedRef<'t, N> {
    pub fn try_cast(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Option<BamlValueWithFlags<'t, N>> {
        match target.ty {
            TyResolvedRef::Primitive(p) => {
                PrimitiveTy::try_cast(ctx, TyWithMeta::new(p.as_static_ref(), target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Literal(l) => {
                LiteralTy::try_cast(ctx, TyWithMeta::new(l, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Array(a) => {
                ArrayTy::try_cast(ctx, TyWithMeta::new(a, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Map(m) => MapTy::try_cast(ctx, TyWithMeta::new(m, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            TyResolvedRef::Class(c) => {
                ClassTy::try_cast(ctx, TyWithMeta::new(c, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::Enum(e) => EnumTy::try_cast(ctx, TyWithMeta::new(e, target.meta), value)
                .map(|v| v.map_value(Into::into)),
            TyResolvedRef::Union(u) => {
                UnionTy::try_cast(ctx, TyWithMeta::new(u, target.meta), value)
                    .map(|v| v.map_value(Into::into))
            }
            TyResolvedRef::StreamState(_s) => todo!(),
        }
    }

    pub fn coerce(
        ctx: &ParsingContext<'t, N>,
        target: TyWithMeta<Self, &'t TypeAnnotations<'t, N>>,
        value: Option<&crate::jsonish::Value>,
    ) -> Result<BamlValueWithFlags<'t, N>, ParsingError> {
        // Extract fields - both are Copy.
        let target_ty = target.ty;
        let target_meta = target.meta;

        let result = match value {
            Some(crate::jsonish::Value::AnyOf(candidates, primitive)) => {
                log::debug!(
                    "scope: {scope} :: coercing AnyOf to: {name:?} (current: {current})",
                    name = target_ty,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                if matches!(
                    target_ty,
                    TyResolvedRef::Primitive(PrimitiveTy::String(_))
                        | TyResolvedRef::Enum(_)
                        | TyResolvedRef::Literal(LiteralTy::String(_))
                ) {
                    Self::coerce(
                        ctx,
                        TyWithMeta::new(target_ty, target_meta),
                        Some(&crate::jsonish::Value::String(
                            primitive.clone(),
                            CompletionState::Complete,
                        )),
                    )
                } else {
                    coerce_array_to_singular(
                        ctx,
                        TyWithMeta::new(target_ty.clone(), target_meta),
                        &candidates.iter().collect::<Vec<_>>(),
                        &|val| {
                            Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(val))
                        },
                    )
                }
            }
            Some(crate::jsonish::Value::Markdown(_t, v, _completion)) => {
                log::debug!(
                    "scope: {scope} :: coercing Markdown to: {name:?} (current: {current})",
                    name = target_ty,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(v)).map(|mut v| {
                    v.add_flag(Flag::ObjectFromMarkdown(
                        if matches!(target_ty, TyResolvedRef::Primitive(PrimitiveTy::String(_))) {
                            1
                        } else {
                            0
                        },
                    ));
                    v
                })
            }
            Some(crate::jsonish::Value::FixedJson(v, fixes)) => {
                log::debug!(
                    "scope: {scope} :: coercing FixedJson to: {name:?} (current: {current})",
                    name = target_ty,
                    scope = ctx.display_scope(),
                    current = value.map(|v| v.r#type()).unwrap_or("<null>".into())
                );
                let mut v = Self::coerce(ctx, TyWithMeta::new(target_ty, target_meta), Some(v))?;
                v.add_flag(Flag::ObjectFromFixedJson(fixes.to_vec()));
                Ok(v)
            }
            _ => {
                // try_cast is a way to exit early for exact-match cases
                if let Some(v) = Self::try_cast(ctx, TyWithMeta::new(target_ty, target_meta), value)
                {
                    Ok(v)
                } else {
                    match target_ty {
                        TyResolvedRef::Primitive(p) => PrimitiveTy::coerce(
                            ctx,
                            TyWithMeta::new(p.as_static_ref(), target_meta),
                            value,
                        )
                        .map(|v| v.map_value(Into::into)),
                        TyResolvedRef::Literal(l) => {
                            LiteralTy::coerce(ctx, TyWithMeta::new(l, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::Array(a) => {
                            ArrayTy::coerce(ctx, TyWithMeta::new(a, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::Map(m) => {
                            MapTy::coerce(ctx, TyWithMeta::new(m, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::Class(c) => {
                            ClassTy::coerce(ctx, TyWithMeta::new(c, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::Enum(e) => {
                            EnumTy::coerce(ctx, TyWithMeta::new(e, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::Union(u) => {
                            UnionTy::coerce(ctx, TyWithMeta::new(u, target_meta), value)
                                .map(|v| v.map_value(Into::into))
                        }
                        TyResolvedRef::StreamState(_s) => todo!(),
                    }
                }
            }
        };

        // Handle incomplete streaming state
        if let Some(CompletionState::Incomplete) = value.map(|v| v.completion_state()) {
            match result {
                Ok(mut v) => {
                    // If in_progress is Never, the field must be complete before use.
                    if matches!(target_meta.in_progress, Some(Literal::Never)) {
                        return Err(ctx.error_internal("Streaming field is not done"));
                    }
                    v.add_flag(Flag::Incomplete);
                    return Ok(v);
                }
                Err(e) => return Err(e),
            }
        }
        result
    }
}

// TODO: Implement validate_asserts once Assertion/Constraint types are fully defined.
// pub fn validate_asserts(constraints: &[(Constraint, bool)]) -> Result<(), ParsingError> { ... }

// TODO: Implement DefaultValue for AnnotatedTy once Assertion type is fully defined.
// The old implementation matched on AnnotatedTy variants (Enum, List, Class, etc.)
// and provided default values (empty list, null, empty map) for optional types.
// It also validated constraints/asserts on the default values.
