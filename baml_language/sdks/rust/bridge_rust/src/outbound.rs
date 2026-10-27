//! Values the engine sent, held as they arrived: released when let go,
//! cloned to be decoded again, and sent back.
//!
//! Each occurrence of an engine key in a value the engine sent is one
//! reference this process owns: a handle's key, and a `live` head in a type
//! value. A host key (a value this process registered) is the engine's
//! reference, never cloned or released here.

use crate::{
    encode::{CloneRefused, EncodeError, Encoder},
    handle::KeySpace,
    wire::{
        self, baml_outbound_value::Value as Out, inbound_map_entry::Key, inbound_value::Value as In,
    },
};

/// Release every engine reference `value` holds.
pub(crate) fn release(value: wire::BamlOutboundValue) {
    let Some(value) = value.value else {
        return;
    };
    match value {
        Out::HandleValue(handle) => match KeySpace::of(handle.handle_type) {
            KeySpace::Engine => crate::handle::release_engine_key(handle.key),
            KeySpace::Host => {}
        },
        Out::TyValue(ty) => {
            // A type value's annotations hold nothing; its `live` heads are
            // one reference each.
            let _ = map_heads(&ty, &mut |head| {
                if let Head::Live(key) = head {
                    crate::handle::release_engine_key(key);
                }
                Ok::<_, std::convert::Infallible>(head)
            });
        }
        Out::ListValue(list) => list.items.into_iter().for_each(release),
        Out::MapValue(map) => map
            .entries
            .into_iter()
            .filter_map(|entry| entry.value)
            .for_each(release),
        Out::ClassValue(class) => class
            .fields
            .into_iter()
            .filter_map(|entry| entry.value)
            .for_each(release),
        Out::UnionVariantValue(union) => {
            if let Some(value) = union.value {
                release(*value);
            }
        }
        Out::NullValue(_)
        | Out::StringValue(_)
        | Out::IntValue(_)
        | Out::FloatValue(_)
        | Out::BoolValue(_)
        | Out::EnumValue(_)
        | Out::LiteralValue(_)
        | Out::MediaValue(_)
        | Out::PromptAstValue(_)
        | Out::Uint8arrayValue(_)
        | Out::BigintValue(_) => {}
    }
}

/// A copy of `value` holding references of its own, which `encoder` owns
/// until the copy is taken over: decoding a held value twice must not hand
/// out the one reference twice.
pub(crate) fn clone(
    value: &wire::BamlOutboundValue,
    encoder: &mut Encoder,
) -> Result<wire::BamlOutboundValue, CloneRefused> {
    let Some(inner) = &value.value else {
        return Ok(wire::BamlOutboundValue { value: None });
    };
    let cloned = match inner {
        Out::HandleValue(handle) => Out::HandleValue(wire::BamlOutboundHandle {
            key: match KeySpace::of(handle.handle_type) {
                KeySpace::Engine => encoder.clone_handle(handle.key)?,
                KeySpace::Host => handle.key,
            },
            handle_type: handle.handle_type,
            ty: handle.ty.clone(),
        }),
        Out::TyValue(ty) => Out::TyValue(map_heads(ty, &mut |head| match head {
            Head::Live(key) => encoder.clone_handle(key).map(Head::Live),
            Head::Name(_) | Head::Label(_) => Ok(head),
        })?),
        Out::ListValue(list) => Out::ListValue(wire::BamlValueList {
            item_type: list.item_type.clone(),
            items: list
                .items
                .iter()
                .map(|item| clone(item, encoder))
                .collect::<Result<_, _>>()?,
        }),
        Out::MapValue(map) => Out::MapValue(wire::BamlValueMap {
            key_type: map.key_type.clone(),
            value_type: map.value_type.clone(),
            entries: map
                .entries
                .iter()
                .map(|entry| {
                    Ok(wire::BamlOutboundMapEntry {
                        key: entry.key.clone(),
                        value: entry
                            .value
                            .as_ref()
                            .map(|value| clone(value, encoder))
                            .transpose()?,
                    })
                })
                .collect::<Result<_, CloneRefused>>()?,
        }),
        Out::ClassValue(class) => Out::ClassValue(wire::BamlValueClass {
            name: class.name.clone(),
            type_args: class.type_args.clone(),
            fields: class
                .fields
                .iter()
                .map(|entry| {
                    Ok(wire::BamlOutboundMapEntry {
                        key: entry.key.clone(),
                        value: entry
                            .value
                            .as_ref()
                            .map(|value| clone(value, encoder))
                            .transpose()?,
                    })
                })
                .collect::<Result<_, CloneRefused>>()?,
        }),
        Out::UnionVariantValue(union) => {
            Out::UnionVariantValue(Box::new(wire::BamlValueUnionVariant {
                value: union
                    .value
                    .as_deref()
                    .map(|value| clone(value, encoder).map(Box::new))
                    .transpose()?,
                ..(**union).clone()
            }))
        }
        Out::NullValue(_)
        | Out::StringValue(_)
        | Out::IntValue(_)
        | Out::FloatValue(_)
        | Out::BoolValue(_)
        | Out::EnumValue(_)
        | Out::LiteralValue(_)
        | Out::MediaValue(_)
        | Out::PromptAstValue(_)
        | Out::Uint8arrayValue(_)
        | Out::BigintValue(_) => inner.clone(),
    };
    Ok(wire::BamlOutboundValue {
        value: Some(cloned),
    })
}

/// `value` sent back as it arrived: every engine reference cloned (each
/// occurrence transfers one), a container stating the type the engine sent
/// beside it, a host callable the function type this process registered it
/// under. A type the engine labeled (a runtime declaration it would not name)
/// cannot be stated, nor can a container it sent without a type.
pub(crate) fn to_inbound(
    value: &wire::BamlOutboundValue,
    encoder: &mut Encoder,
) -> Result<wire::InboundValue, EncodeError> {
    let Some(inner) = &value.value else {
        return Ok(wire::InboundValue {
            value_type: None,
            value: None,
        });
    };
    let (value_type, value) = match inner {
        Out::NullValue(_) => (None, None),
        Out::StringValue(value) => (None, Some(In::StringValue(value.clone()))),
        Out::IntValue(value) => (None, Some(In::IntValue(*value))),
        Out::FloatValue(value) => (None, Some(In::FloatValue(*value))),
        Out::BoolValue(value) => (None, Some(In::BoolValue(*value))),
        Out::Uint8arrayValue(value) => (None, Some(In::Uint8arrayValue(value.clone()))),
        Out::BigintValue(value) => (None, Some(In::BigintValue(value.clone()))),
        Out::MediaValue(value) => (None, Some(In::MediaValue(value.clone()))),
        Out::PromptAstValue(value) => (None, Some(In::PromptAstValue(value.clone()))),
        Out::TyValue(ty) => (None, Some(In::TyValue(restate(ty, encoder)?))),
        Out::EnumValue(value) => (
            None,
            Some(In::EnumValue(wire::InboundEnumValue {
                name: value.name.clone(),
                value: value.value.clone(),
            })),
        ),
        Out::HandleValue(handle) => {
            // An engine object implies its own type: the engine knows it
            // from the key. Only a host callable has no type of its own; it
            // goes back under the function type this process registered it
            // with.
            let (key, value_type) = match KeySpace::of(handle.handle_type) {
                KeySpace::Engine => (encoder.clone_handle(handle.key)?, None),
                KeySpace::Host => (
                    handle.key,
                    match wire::BamlHandleType::try_from(handle.handle_type) {
                        Ok(wire::BamlHandleType::HostValueCallable) => {
                            crate::host_value::registered_function_type(handle.key)
                        }
                        _ => None,
                    },
                ),
            };
            (
                value_type,
                Some(In::Handle(wire::BamlHandle {
                    key,
                    handle_type: handle.handle_type,
                })),
            )
        }
        Out::ClassValue(class) => {
            let type_args = class
                .type_args
                .iter()
                .map(|ty| restate(ty, encoder))
                .collect::<Result<_, _>>()?;
            let fields = encoder.nested(|encoder| {
                class
                    .fields
                    .iter()
                    .map(|entry| {
                        Ok(wire::InboundMapEntry {
                            key: Some(Key::StringKey(entry.key.clone())),
                            value: entry
                                .value
                                .as_ref()
                                .map(|value| to_inbound(value, encoder))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_, EncodeError>>()
            })?;
            (
                Some(crate::baml_value::internal::class_ty(
                    &class.name,
                    type_args,
                )),
                Some(In::ClassValue(wire::InboundClassValue { fields })),
            )
        }
        Out::ListValue(list) => {
            let item = list
                .item_type
                .as_ref()
                .ok_or(EncodeError::UntypedContainer)?;
            let value_type = crate::baml_value::internal::list_ty(restate(item, encoder)?);
            let values = encoder.nested(|encoder| {
                list.items
                    .iter()
                    .map(|item| to_inbound(item, encoder))
                    .collect::<Result<_, _>>()
            })?;
            (
                Some(value_type),
                Some(In::ListValue(wire::InboundListValue { values })),
            )
        }
        Out::MapValue(map) => {
            let (Some(key_type), Some(map_value_type)) = (&map.key_type, &map.value_type) else {
                return Err(EncodeError::UntypedContainer);
            };
            let value_type = crate::baml_value::internal::map_ty(
                restate(key_type, encoder)?,
                restate(map_value_type, encoder)?,
            );
            // Map keys cross as strings only: the engine sends every key as
            // its string, and its decoder refuses any other key kind.
            let entries = encoder.nested(|encoder| {
                map.entries
                    .iter()
                    .map(|entry| {
                        Ok(wire::InboundMapEntry {
                            key: Some(Key::StringKey(entry.key.clone())),
                            value: entry
                                .value
                                .as_ref()
                                .map(|value| to_inbound(value, encoder))
                                .transpose()?,
                        })
                    })
                    .collect::<Result<_, EncodeError>>()
            })?;
            (
                Some(value_type),
                Some(In::MapValue(wire::InboundMapValue { entries })),
            )
        }
        // A union envelope says which member a value came out of; the value
        // itself implies or states its type, so the envelope adds nothing.
        Out::UnionVariantValue(union) => {
            return match union.value.as_deref() {
                Some(value) => to_inbound(value, encoder),
                None => Ok(wire::InboundValue {
                    value_type: None,
                    value: None,
                }),
            };
        }
        // A literal is the one value of its base type it is; the base scalar
        // implies it.
        Out::LiteralValue(literal) => {
            return to_inbound(
                &crate::decode::unwrap(wire::BamlOutboundValue {
                    value: Some(Out::LiteralValue(literal.clone())),
                }),
                encoder,
            );
        }
    };
    Ok(wire::InboundValue { value_type, value })
}

/// The class FQN of `value`, through any union envelopes.
pub(crate) fn class_name(value: &wire::BamlOutboundValue) -> Option<&str> {
    match &value.value {
        Some(Out::ClassValue(class)) => Some(&class.name),
        Some(Out::UnionVariantValue(union)) => union.value.as_deref().and_then(class_name),
        _ => None,
    }
}

/// `ty` as a statement this process sends: a `live` head cloned (each
/// occurrence transfers one reference), a labeled head refused.
fn restate(ty: &wire::BamlTy, encoder: &mut Encoder) -> Result<wire::BamlTy, EncodeError> {
    map_heads(ty, &mut |head| match head {
        Head::Name(_) => Ok(head),
        Head::Live(key) => Ok(Head::Live(encoder.clone_handle(key)?)),
        Head::Label(_) => Err(EncodeError::AnonymousDeclaration),
    })
}

/// A nominal head of a wire type, whichever declaration kind it heads.
enum Head {
    Name(String),
    Live(u64),
    Label(String),
}

macro_rules! head_conversions {
    ($($module:ident),* $(,)?) => {
        $(
            impl From<wire::$module::Head> for Head {
                fn from(head: wire::$module::Head) -> Self {
                    match head {
                        wire::$module::Head::Name(name) => Head::Name(name),
                        wire::$module::Head::Live(key) => Head::Live(key),
                        wire::$module::Head::AnonymousLabel(label) => Head::Label(label),
                    }
                }
            }

            impl From<Head> for wire::$module::Head {
                fn from(head: Head) -> Self {
                    match head {
                        Head::Name(name) => wire::$module::Head::Name(name),
                        Head::Live(key) => wire::$module::Head::Live(key),
                        Head::Label(label) => wire::$module::Head::AnonymousLabel(label),
                    }
                }
            }
        )*
    };
}

head_conversions!(
    baml_ty_class,
    baml_ty_type_alias,
    baml_ty_enum,
    baml_ty_enum_variant,
    baml_ty_interface,
);

/// `ty` with every nominal head passed through `head`.
fn map_heads<E>(
    ty: &wire::BamlTy,
    head: &mut impl FnMut(Head) -> Result<Head, E>,
) -> Result<wire::BamlTy, E> {
    use wire::baml_ty::Ty;
    fn each<E>(
        tys: &[wire::BamlTy],
        head: &mut impl FnMut(Head) -> Result<Head, E>,
    ) -> Result<Vec<wire::BamlTy>, E> {
        tys.iter().map(|ty| map_heads(ty, head)).collect()
    }
    fn boxed<E>(
        ty: Option<&wire::BamlTy>,
        head: &mut impl FnMut(Head) -> Result<Head, E>,
    ) -> Result<Option<Box<wire::BamlTy>>, E> {
        ty.map(|ty| map_heads(ty, head).map(Box::new)).transpose()
    }
    fn mapped<H, E>(
        current: Option<&H>,
        head: &mut impl FnMut(Head) -> Result<Head, E>,
    ) -> Result<Option<H>, E>
    where
        H: Clone + Into<Head> + From<Head>,
    {
        current
            .map(|current| head(current.clone().into()).map(H::from))
            .transpose()
    }
    let Some(inner) = &ty.ty else {
        return Ok(wire::BamlTy { ty: None });
    };
    let mapped_ty = match inner {
        Ty::ClassTy(class) => Ty::ClassTy(wire::BamlTyClass {
            head: mapped(class.head.as_ref(), head)?,
            type_args: each(&class.type_args, head)?,
        }),
        Ty::TypeAlias(alias) => Ty::TypeAlias(wire::BamlTyTypeAlias {
            head: mapped(alias.head.as_ref(), head)?,
            type_args: each(&alias.type_args, head)?,
        }),
        Ty::Enum(enum_ty) => Ty::Enum(wire::BamlTyEnum {
            head: mapped(enum_ty.head.as_ref(), head)?,
        }),
        Ty::EnumVariant(variant) => Ty::EnumVariant(wire::BamlTyEnumVariant {
            head: mapped(variant.head.as_ref(), head)?,
            variant: variant.variant.clone(),
        }),
        Ty::Interface(interface) => Ty::Interface(wire::BamlTyInterface {
            head: mapped(interface.head.as_ref(), head)?,
            type_args: each(&interface.type_args, head)?,
            bindings: interface
                .bindings
                .iter()
                .map(|binding| {
                    Ok(wire::BamlTyAssociatedBinding {
                        name: binding.name.clone(),
                        ty: binding
                            .ty
                            .as_ref()
                            .map(|ty| map_heads(ty, head))
                            .transpose()?,
                    })
                })
                .collect::<Result<_, E>>()?,
        }),
        Ty::List(list) => Ty::List(Box::new(wire::BamlTyList {
            item: boxed(list.item.as_deref(), head)?,
        })),
        Ty::Map(map) => Ty::Map(Box::new(wire::BamlTyMap {
            key: boxed(map.key.as_deref(), head)?,
            value: boxed(map.value.as_deref(), head)?,
        })),
        Ty::Optional(optional) => Ty::Optional(Box::new(wire::BamlTyOptional {
            inner: boxed(optional.inner.as_deref(), head)?,
        })),
        Ty::Union(union) => Ty::Union(wire::BamlTyUnion {
            options: each(&union.options, head)?,
        }),
        Ty::Function(function) => Ty::Function(Box::new(wire::BamlTyFunction {
            generic_params: function.generic_params.clone(),
            params: function
                .params
                .iter()
                .map(|param| {
                    Ok(wire::BamlTyFunctionParam {
                        name: param.name.clone(),
                        ty: param
                            .ty
                            .as_ref()
                            .map(|ty| map_heads(ty, head))
                            .transpose()?,
                        mode: param.mode,
                    })
                })
                .collect::<Result<_, E>>()?,
            ret: boxed(function.ret.as_deref(), head)?,
            throws: boxed(function.throws.as_deref(), head)?,
        })),
        Ty::Future(future) => Ty::Future(Box::new(wire::BamlTyFuture {
            value: boxed(future.value.as_deref(), head)?,
            error: boxed(future.error.as_deref(), head)?,
        })),
        Ty::AssociatedTypeProjection(projection) => {
            Ty::AssociatedTypeProjection(Box::new(wire::BamlTyAssociatedTypeProjection {
                base: boxed(projection.base.as_deref(), head)?,
                interface: boxed(projection.interface.as_deref(), head)?,
                ..(**projection).clone()
            }))
        }
        Ty::Primitive(_)
        | Ty::Literal(_)
        | Ty::Unknown(_)
        | Ty::Media(_)
        | Ty::RustType(_)
        | Ty::MetaType(_)
        | Ty::Resource(_)
        | Ty::PromptAst(_)
        | Ty::TypeVar(_)
        | Ty::Never(_) => inner.clone(),
    };
    Ok(wire::BamlTy {
        ty: Some(mapped_ty),
    })
}
