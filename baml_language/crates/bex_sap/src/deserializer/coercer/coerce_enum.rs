use std::borrow::Cow;

use super::ParsingContext;
use crate::{
    baml_value::BamlEnum,
    deserializer::{
        coercer::{ParsingError, TypeCoercer, match_string::match_string},
        deserialize_flags::Flag,
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish::{self, CompletionState},
    sap_model::{AnnotatedEnumVariant, EnumTy, EnumVariantTy, TyResolvedRef, TypeIdent, TypeValue},
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
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        enum_ty: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // Enums can only be cast from complete string values: a prefix of a variant name is
        // not a variant.
        let jsonish::Value::String(s, CompletionState::Complete) = value else {
            return None;
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
                    DeserializerMeta::new(TyResolvedRef::Enum(enum_ty)),
                ));
            }
        }

        None
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        // Enums can only be cast from string values
        if matches!(value, jsonish::Value::Null) {
            return Err(ctx.error_unexpected_null(target));
        }
        // An enum has no partial parse: a prefix of a variant name is not a variant.
        if value.completion_state() == &CompletionState::Incomplete {
            return Ok(None);
        }

        Self::coerce_from_cow(ctx, target, Cow::Borrowed(value), [])
    }
}

impl<'s, 'v, 't, N: TypeIdent> EnumTy<'t, N> {
    #[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
    pub fn coerce_from_cow(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: Cow<'v, jsonish::Value<'s>>,
        add_flags: impl IntoIterator<Item = Flag<'s, 'v, 't, N>>,
    ) -> Result<
        Option<ValueWithFlags<'s, 'v, 't, <EnumTy<'t, N> as TypeValue<'s, 'v, 't>>::Value, N>>,
        ParsingError,
    > {
        match_string(
            ctx,
            TyResolvedRef::Enum(target),
            value,
            &enum_match_candidates(target),
            true,
        )
        .map(|v| {
            v.map_value(|val| BamlEnum {
                name: &target.name,
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
        _ctx: &ParsingContext<'s, 'v, 't, N>,
        ev_ty: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // A prefix of the variant name is not the variant.
        let jsonish::Value::String(s, CompletionState::Complete) = value else {
            return None;
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
                DeserializerMeta::new(TyResolvedRef::EnumVariant(ev_ty)),
            ));
        }

        None
    }

    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
        value: &'v jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        if matches!(value, jsonish::Value::Null) {
            return Err(ctx.error_unexpected_null(target));
        }
        // A prefix of the variant name is not the variant.
        if value.completion_state() == &CompletionState::Incomplete {
            return Ok(None);
        }

        Self::coerce_from_cow(ctx, target, Cow::Borrowed(value), [])
    }
}

impl<'s, 'v, 't, N: TypeIdent> EnumVariantTy<'t, N> {
    #[allow(clippy::type_complexity, clippy::needless_pass_by_value)]
    pub fn coerce_from_cow(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: &'t Self,
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
            TyResolvedRef::EnumVariant(target),
            value,
            &enum_variant_match_candidates(target),
            true,
        )
        .map(|v| {
            v.map_value(|val| BamlEnum {
                name: &target.name,
                value: val,
            })
            .with_flags(add_flags)
        })
        .map(Some)
    }
}
