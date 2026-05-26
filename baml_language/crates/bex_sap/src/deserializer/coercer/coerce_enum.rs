use std::borrow::Cow;

use super::ParsingContext;
use crate::{
    baml_value::BamlEnum,
    deserializer::{
        coercer::{ParsingError, TypeCoercer, match_string::match_string},
        deserialize_flags::{DeserializerConditions, Flag},
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish::{self, CompletionState},
    sap_model::{
        AnnotatedEnumVariant, AttrLiteral, EnumTy, EnumVariantTy, FromLiteral, TyResolvedRef,
        TyWithMeta, TypeAnnotations, TypeIdent, TypeValue,
    },
};

/// Produces a list of (name, candidates) tuples for each enum variant.
/// When aliases exist, only aliases are used as candidates (original name excluded).
/// When no aliases, the name itself is the sole candidate.
fn enum_match_candidates<'t, N: TypeIdent>(ty: &'t EnumTy<'t, N>) -> Vec<(&'t str, Vec<&'t str>)> {
    ty.variants
        .iter()
        .map(|v| {
            let candidates = if v.aliases.is_empty() {
                vec![v.name.trim()]
            } else {
                v.aliases.iter().map(|a| a.trim()).collect()
            };
            (v.name.as_ref(), candidates)
        })
        .collect()
}

impl<'s, 'v, 't, N: TypeIdent + 't> TypeCoercer<'s, 'v, 't, N> for EnumTy<'t, N>
where
    't: 's,
    's: 'v,
{
    /// Strict: does not use aliases, just the name.
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let enum_ty = target.ty;
        let meta = target.meta;

        // Enums can only be cast from string values
        let jsonish::Value::String(s, completion) = value else {
            return None;
        };

        let flags = match (completion, target.meta.in_progress.as_ref()) {
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
                                ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
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

        // assumes no name or alias can have the same value as another name or alias
        // When aliases exist, only aliases are valid for matching (name is excluded)
        for AnnotatedEnumVariant { name, aliases } in &enum_ty.variants {
            let matches = if aliases.is_empty() {
                name == s
            } else {
                aliases.iter().any(|a| a == s)
            };
            if matches {
                let value = BamlEnum {
                    name: &enum_ty.name,
                    value: name,
                };
                return Some(ValueWithFlags::new(
                    value,
                    DeserializerMeta {
                        flags,
                        ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
                    },
                ));
            }
        }

        None
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        // Enums can only be cast from string values
        if matches!(value, jsonish::Value::Null) {
            return Err(ctx.error_unexpected_null(&target));
        }

        let enum_ty = target.ty;
        let meta = target.meta;
        let mut add_flags = Vec::new();

        if value.completion_state() == &CompletionState::Incomplete {
            match &meta.in_progress {
                Some(AttrLiteral::Never) => return Ok(None),
                Some(lit) => {
                    let in_progress = enum_ty.from_literal(lit, ctx)?;
                    return Ok(Some(ValueWithFlags::new(
                        in_progress,
                        DeserializerMeta {
                            flags: DeserializerConditions::new()
                                .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value))),
                            ty: TyWithMeta::new(TyResolvedRef::Enum(enum_ty), meta),
                        },
                    )));
                }
                None => {
                    add_flags.push(Flag::Incomplete);
                }
            }
        }

        Self::coerce_from_cow(ctx, target, Cow::Borrowed(value), add_flags)
    }
}

impl<'s, 'v, 't, N: TypeIdent> EnumTy<'t, N> {
    #[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
    pub fn coerce_from_cow(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Cow<'v, jsonish::Value<'s>>,
        add_flags: impl IntoIterator<Item = Flag<'s, 'v, 't, N>>,
    ) -> Result<
        Option<ValueWithFlags<'s, 'v, 't, <EnumTy<'t, N> as TypeValue<'s, 'v, 't>>::Value, N>>,
        ParsingError,
    > {
        match_string(
            ctx,
            target.clone().map_ty(TyResolvedRef::Enum),
            value,
            &enum_match_candidates(target.ty),
            true,
        )
        .map(|v| {
            v.map_value(|val| BamlEnum {
                name: &target.ty.name,
                value: val,
            })
            .with_flags(add_flags)
        })
        .map(Some)
    }
}

/// Produces match candidates for a single enum variant.
fn enum_variant_match_candidates<'t, N: TypeIdent>(
    ty: &'t EnumVariantTy<'t, N>,
) -> Vec<(&'t str, Vec<&'t str>)> {
    let v = &ty.value;
    let candidates = if v.aliases.is_empty() {
        vec![v.name.trim()]
    } else {
        v.aliases.iter().map(|a| a.trim()).collect()
    };
    vec![(v.name.as_ref(), candidates)]
}

impl<'s, 'v, 't, N: TypeIdent + 't> TypeCoercer<'s, 'v, 't, N> for EnumVariantTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        let ev_ty = target.ty;
        let meta = target.meta;

        let jsonish::Value::String(s, completion) = value else {
            return None;
        };

        let flags = match (completion, meta.in_progress.as_ref()) {
            (CompletionState::Incomplete, Some(AttrLiteral::Never)) => return None,
            (CompletionState::Incomplete, Some(lit)) => {
                return ev_ty
                    .from_literal(lit, ctx)
                    .map(|ret| {
                        ValueWithFlags::new(
                            ret,
                            DeserializerMeta {
                                flags: DeserializerConditions::new()
                                    .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value))),
                                ty: TyWithMeta::new(TyResolvedRef::EnumVariant(ev_ty), meta),
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

        let AnnotatedEnumVariant { name, aliases } = &ev_ty.value;
        let matches = if aliases.is_empty() {
            name == s
        } else {
            aliases.iter().any(|a| a == s)
        };
        if matches {
            let value = BamlEnum {
                name: &ev_ty.name,
                value: name,
            };
            return Some(ValueWithFlags::new(
                value,
                DeserializerMeta {
                    flags,
                    ty: TyWithMeta::new(TyResolvedRef::EnumVariant(ev_ty), meta),
                },
            ));
        }

        None
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        if matches!(value, jsonish::Value::Null) {
            return Err(ctx.error_unexpected_null(&target));
        }

        let ev_ty = target.ty;
        let meta = target.meta;
        let mut add_flags = Vec::new();

        if value.completion_state() == &CompletionState::Incomplete {
            match &meta.in_progress {
                Some(AttrLiteral::Never) => return Ok(None),
                Some(lit) => {
                    let in_progress = ev_ty.from_literal(lit, ctx)?;
                    return Ok(Some(ValueWithFlags::new(
                        in_progress,
                        DeserializerMeta {
                            flags: DeserializerConditions::new()
                                .with_flag(Flag::DefaultFromInProgress(Cow::Borrowed(value))),
                            ty: TyWithMeta::new(TyResolvedRef::EnumVariant(ev_ty), meta),
                        },
                    )));
                }
                None => {
                    add_flags.push(Flag::Incomplete);
                }
            }
        }

        Self::coerce_from_cow(ctx, target, Cow::Borrowed(value), add_flags)
    }
}

impl<'s, 'v, 't, N: TypeIdent> EnumVariantTy<'t, N> {
    #[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
    pub fn coerce_from_cow(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: Cow<'v, jsonish::Value<'s>>,
        add_flags: impl IntoIterator<Item = Flag<'s, 'v, 't, N>>,
    ) -> Result<
        Option<
            ValueWithFlags<'s, 'v, 't, <EnumVariantTy<'t, N> as TypeValue<'s, 'v, 't>>::Value, N>,
        >,
        ParsingError,
    > {
        match_string(
            ctx,
            target.clone().map_ty(TyResolvedRef::EnumVariant),
            value,
            &enum_variant_match_candidates(target.ty),
            true,
        )
        .map(|v| {
            v.map_value(|val| BamlEnum {
                name: &target.ty.name,
                value: val,
            })
            .with_flags(add_flags)
        })
        .map(Some)
    }
}
