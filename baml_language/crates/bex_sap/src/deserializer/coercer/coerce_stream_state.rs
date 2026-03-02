use crate::{
    baml_value::BamlStreamState,
    deserializer::{
        coercer::{ParsingContext, ParsingError, TypeCoercer},
        types::{DeserializerMeta, ValueWithFlags},
    },
    jsonish::CompletionState,
    sap_model::{
        AttrLiteral, StreamStateTy, TyResolvedRef, TyWithMeta, TypeAnnotations, TypeIdent,
    },
};

impl<'s, 'v, 't, N: TypeIdent> TypeCoercer<'s, 'v, 't, N> for StreamStateTy<'t, N>
where
    't: 's,
    's: 'v,
{
    fn coerce(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Result<Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>>, ParsingError> {
        // StreamState cannot have attributes as it is not a user-provided type.
        // It is only created through attributes, so any other attributes would be on the inner type.
        debug_assert!(
            target.meta.asserts.is_empty(),
            "StreamState should not have attributes"
        );
        debug_assert!(
            target.meta.in_progress.is_none(),
            "StreamState should not have attributes"
        );
        debug_assert!(
            !matches!(target.meta.on_error, AttrLiteral::Never),
            "StreamState should not have attributes"
        );

        let inner_ty = ctx
            .db
            .resolve_with_meta((*target.ty.value).as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))?;
        let Some(inner) = TyResolvedRef::coerce(ctx, inner_ty, value)? else {
            return Ok(None);
        };
        let value = match value.completion_state() {
            CompletionState::Complete => BamlStreamState::Complete(Box::new(inner)),
            CompletionState::Incomplete => BamlStreamState::Incomplete(Box::new(inner)),
        };
        Ok(Some(ValueWithFlags::new(
            value,
            DeserializerMeta {
                flags: Default::default(),
                ty: target.map_ty(TyResolvedRef::StreamState),
            },
        )))
    }
    fn try_cast(
        ctx: &ParsingContext<'s, 'v, 't, N>,
        target: TyWithMeta<&'t Self, &'t TypeAnnotations<'t, N>>,
        value: &'v crate::jsonish::Value<'s>,
    ) -> Option<ValueWithFlags<'s, 'v, 't, Self::Value, N>> {
        // StreamState cannot have attributes as it is not a user-provided type.
        // It is only created through attributes, so any other attributes would be on the inner type.
        debug_assert!(
            target.meta.asserts.is_empty(),
            "StreamState should not have attributes"
        );
        debug_assert!(
            target.meta.in_progress.is_none(),
            "StreamState should not have attributes"
        );
        debug_assert!(
            !matches!(target.meta.on_error, AttrLiteral::Never),
            "StreamState should not have attributes"
        );

        let inner_ty = ctx
            .db
            .resolve_with_meta((*target.ty.value).as_ref())
            .map_err(|ident| ctx.error_type_resolution(ident))
            .ok()?;
        let inner = TyResolvedRef::try_cast(ctx, inner_ty, value)?;
        let value = match value.completion_state() {
            CompletionState::Complete => BamlStreamState::Complete(Box::new(inner)),
            CompletionState::Incomplete => BamlStreamState::Incomplete(Box::new(inner)),
        };
        Some(ValueWithFlags::new(
            value,
            DeserializerMeta {
                flags: Default::default(),
                ty: target.map_ty(TyResolvedRef::StreamState),
            },
        ))
    }
}
