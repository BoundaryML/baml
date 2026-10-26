//! Owning dynamic values and closed invocation targets for generated SDK roots.
use std::{collections::BTreeMap, sync::Arc};

use crate::{
    BamlValue, DecodeError, Error, SdkError,
    baml_value::internal::__BamlValuePrivate,
    encode::{EncodeError, Encoder},
    wire,
};

/// An argument of a dynamic invocation: a value encoded when the call is
/// sent, under its own type.
#[derive(Clone)]
pub struct Input(Arc<crate::encode::EncodeValue>);
impl Input {
    pub fn new<T: BamlValue + Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(move |encoder| value.to_baml(encoder)))
    }
    fn encode(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        (self.0)(encoder)
    }
}
impl<T: BamlValue + Send + Sync + 'static> From<T> for Input {
    fn from(value: T) -> Self {
        Self::new(value)
    }
}
pub type Arguments = BTreeMap<String, Input>;
#[derive(Clone)]
pub struct Type(wire::BamlTy);
impl Type {
    pub fn of<T: BamlValue>() -> Self {
        Self(T::baml_ty())
    }
}
pub type TypeBindings = BTreeMap<String, Type>;

#[derive(Clone)]
pub enum Target {
    Named(String),
    Callable(CallableTarget),
}
#[derive(Clone)]
pub struct CallableTarget(pub(crate) Arc<crate::function::FunctionHandle>);
impl Target {
    pub fn named(name: impl Into<String>) -> Self {
        Self::Named(name.into())
    }
}
impl<A, R, E> From<&crate::BamlFunction<A, R, E>> for Target {
    fn from(function: &crate::BamlFunction<A, R, E>) -> Self {
        Self::Callable(CallableTarget(Arc::clone(&function.handle)))
    }
}

/// A value the engine returned, held as it arrived: decoded on demand, sent
/// back as it arrived, and released when the last clone is dropped.
#[derive(Clone)]
pub struct Value(Arc<OwnedValue>);
struct OwnedValue(wire::BamlOutboundValue);
impl Drop for OwnedValue {
    fn drop(&mut self) {
        crate::outbound::release(std::mem::take(&mut self.0));
    }
}
impl std::fmt::Debug for Value {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Value").field(&self.0.0).finish()
    }
}
impl Value {
    /// Take ownership of a value the engine sent.
    pub(crate) fn own(value: wire::BamlOutboundValue) -> Self {
        Self(Arc::new(OwnedValue(value)))
    }

    /// Decode the value as a `T`, which owns references of its own.
    pub fn decode<T: BamlValue>(&self) -> Result<T, DecodeError> {
        let mut encoder = Encoder::new();
        let cloned = crate::outbound::clone(&self.0.0, &mut encoder)?;
        // The decoded value owns the clones now.
        encoder.commit();
        T::from_baml(cloned)
    }

    /// The FQN of the value's class, when it is a class instance.
    pub fn class_name(&self) -> Option<&str> {
        crate::outbound::class_name(&self.0.0)
    }

    pub fn callable_target(&self) -> Result<Target, SdkError> {
        let value = crate::decode::unwrap(self.0.0.clone());
        let Some(wire::baml_outbound_value::Value::HandleValue(handle)) = value.value else {
            return Err(SdkError::new("value is not a BAML callable"));
        };
        if handle.handle_type != wire::BamlHandleType::FunctionRef as i32 {
            return Err(SdkError::new("value is not a BAML callable"));
        }
        let key = crate::handle::clone_engine_key(handle.key)
            .ok_or_else(|| SdkError::new("released BAML callable"))?;
        Ok(Target::Callable(CallableTarget(Arc::new(
            crate::function::FunctionHandle {
                key,
                #[cfg(test)]
                release: None,
            },
        ))))
    }
}
impl __BamlValuePrivate for Value {
    fn from_baml(value: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        Ok(Self::own(value))
    }
    fn to_baml(&self, encoder: &mut Encoder) -> Result<wire::InboundValue, EncodeError> {
        crate::outbound::to_inbound(&self.0.0, encoder)
    }
    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::Unknown(wire::BamlTyUnknown {})),
        }
    }
}
fn dispatch<E>(
    target: &Target,
    arguments: Arguments,
    types: TypeBindings,
    baml: crate::invocation::InvocationOptions,
) -> Result<crate::completion::Receiver, Error<E>> {
    let target = match target {
        Target::Named(name) if !name.is_empty() && !name.contains('\0') => {
            wire::call_function_args::CallTarget::FunctionName(name.clone())
        }
        Target::Named(_) => {
            return Err(Error::Sdk(SdkError::new("invalid BAML invocation target")));
        }
        Target::Callable(function) => {
            if !types.is_empty() {
                return Err(Error::Sdk(SdkError::new(
                    "specialized callable rejects type bindings",
                )));
            }
            wire::call_function_args::CallTarget::FunctionHandle(function.0.key)
        }
    };
    crate::runtime::dispatch_controlled(
        target,
        |args| {
            for (name, input) in arguments {
                args.encoded(&name, |encoder| input.encode(encoder))?;
            }
            Ok(types
                .into_iter()
                .map(|(name, ty)| wire::BamlTyArg {
                    type_var: name,
                    type_value: Some(ty.0),
                })
                .collect())
        },
        baml,
    )
}
pub fn invoke(
    target: Target,
    arguments: Arguments,
    types: TypeBindings,
    baml: crate::invocation::InvocationOptions,
) -> Result<Value, Error<Value>> {
    if tokio::runtime::Handle::try_current().is_ok() && !crate::host_value::in_blocking_dispatch() {
        return Err(Error::CalledSyncFromAsync);
    }
    let receiver = dispatch(&target, arguments, types, baml)?;
    let result = crate::decode::decode_result(&receiver.wait_blocking());
    drop(target);
    result
}
pub async fn invoke_async(
    target: Target,
    arguments: Arguments,
    types: TypeBindings,
    baml: crate::invocation::InvocationOptions,
) -> Result<Value, Error<Value>> {
    let receiver = dispatch(&target, arguments, types, baml)?;
    let result = crate::decode::decode_result(&receiver.wait().await);
    drop(target);
    result
}

#[cfg(test)]
mod tests {
    use wire::{baml_outbound_value::Value as Out, baml_ty::Ty, inbound_value::Value as In};

    use super::*;

    fn int_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(Ty::Primitive(wire::BamlTyPrimitive {
                kind: wire::BamlTyPrimitiveKind::BamlTyPrimitiveInt as i32,
            })),
        }
    }

    fn string_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(Ty::Primitive(wire::BamlTyPrimitive {
                kind: wire::BamlTyPrimitiveKind::BamlTyPrimitiveString as i32,
            })),
        }
    }

    /// The class the engine labels rather than names: a runtime declaration
    /// in an annotation beside a value.
    fn labeled_class_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(Ty::ClassTy(wire::BamlTyClass {
                head: Some(wire::baml_ty_class::Head::AnonymousLabel(
                    "<runtime declaration>".to_string(),
                )),
                type_args: Vec::new(),
            })),
        }
    }

    fn out(value: Out) -> wire::BamlOutboundValue {
        wire::BamlOutboundValue { value: Some(value) }
    }

    fn resent(value: Out) -> Result<wire::InboundValue, EncodeError> {
        crate::outbound::to_inbound(&out(value), &mut Encoder::new())
    }

    /// A list or map goes back under the type the engine gave it.
    #[test]
    fn containers_state_the_type_they_arrived_with() {
        let list = resent(Out::ListValue(wire::BamlValueList {
            item_type: Some(int_ty()),
            items: vec![out(Out::IntValue(1))],
        }))
        .unwrap();
        assert!(matches!(
            list.value_type.as_ref().and_then(|ty| ty.ty.as_ref()),
            Some(Ty::List(inner)) if inner.item.as_deref() == Some(&int_ty())
        ));
        assert!(matches!(list.value, Some(In::ListValue(_))));

        let map = resent(Out::MapValue(wire::BamlValueMap {
            key_type: Some(string_ty()),
            value_type: Some(int_ty()),
            entries: vec![wire::BamlOutboundMapEntry {
                key: "a".into(),
                value: Some(out(Out::IntValue(1))),
            }],
        }))
        .unwrap();
        assert!(matches!(
            map.value_type.as_ref().and_then(|ty| ty.ty.as_ref()),
            Some(Ty::Map(inner))
                if inner.key.as_deref() == Some(&string_ty()) && inner.value.as_deref() == Some(&int_ty())
        ));
        let Some(In::MapValue(map)) = map.value else {
            panic!("a map goes back as a map");
        };
        assert_eq!(
            map.entries[0].key,
            Some(wire::inbound_map_entry::Key::StringKey("a".to_string()))
        );
    }

    /// The wire requires a container to state its type: one the engine sent
    /// without its type cannot go back, and is refused here rather than by
    /// the engine.
    #[test]
    fn untyped_containers_are_refused() {
        assert_eq!(
            resent(Out::ListValue(wire::BamlValueList {
                item_type: None,
                items: vec![],
            })),
            Err(EncodeError::UntypedContainer)
        );
        assert_eq!(
            resent(Out::MapValue(wire::BamlValueMap {
                key_type: Some(string_ty()),
                value_type: None,
                entries: vec![],
            })),
            Err(EncodeError::UntypedContainer)
        );
    }

    /// A label is display text for a declaration the engine would not name:
    /// a container whose type carries one cannot be stated back.
    #[test]
    fn labeled_annotations_are_refused() {
        assert_eq!(
            resent(Out::ListValue(wire::BamlValueList {
                item_type: Some(labeled_class_ty()),
                items: vec![],
            })),
            Err(EncodeError::AnonymousDeclaration)
        );
        assert_eq!(
            resent(Out::ClassValue(wire::BamlValueClass {
                name: "user.Box".to_string(),
                type_args: vec![labeled_class_ty()],
                fields: vec![],
            })),
            Err(EncodeError::AnonymousDeclaration)
        );
        assert_eq!(
            resent(Out::TyValue(labeled_class_ty())),
            Err(EncodeError::AnonymousDeclaration)
        );
    }

    /// A host callable has no type of its own: it goes back under the
    /// function type it was registered with.
    #[test]
    fn host_callables_state_their_registered_type() {
        let function_type = crate::host_value::CallableTy::new(
            vec![crate::baml_value::internal::function_param(None, int_ty())],
            int_ty(),
            crate::baml_value::internal::leaf_ty(Ty::Unknown(wire::BamlTyUnknown {})),
        );
        let stated = function_type.to_wire();
        let mut encoder = Encoder::new();
        let sent = crate::host_value::callable_handle(
            |x: i64| x + 1,
            &[crate::HostParam {
                name: "x",
                optional: false,
            }],
            function_type,
            &mut encoder,
        );
        let Some(In::Handle(handle)) = sent.value else {
            panic!("a host callable is sent as a handle: {sent:?}");
        };
        let back = crate::outbound::to_inbound(
            &out(Out::HandleValue(wire::BamlOutboundHandle {
                key: handle.key,
                handle_type: handle.handle_type,
                ty: None,
            })),
            &mut Encoder::new(),
        )
        .unwrap();
        assert_eq!(back.value_type, Some(stated));
        // Never sent: dropping the encoding unregisters the callable.
        drop(encoder);
        assert_eq!(
            crate::host_value::registered_function_type(handle.key),
            None
        );
    }

    /// A union envelope says which member a value came out of; the value
    /// itself implies or states its type, so re-encoding adds nothing to a
    /// scalar and keeps a container's own statement.
    #[test]
    fn union_envelopes_add_nothing() {
        let scalar = resent(Out::UnionVariantValue(Box::new(
            wire::BamlValueUnionVariant {
                self_type: Some(wire::BamlTy {
                    ty: Some(Ty::Union(wire::BamlTyUnion {
                        options: vec![int_ty(), string_ty()],
                    })),
                }),
                selected_option_index: Some(1),
                value: Some(Box::new(out(Out::StringValue("x".into())))),
                ..Default::default()
            },
        )))
        .unwrap();
        assert!(scalar.value_type.is_none());
        assert!(matches!(scalar.value, Some(In::StringValue(ref s)) if s == "x"));

        let list_ty = wire::BamlTy {
            ty: Some(Ty::List(Box::new(wire::BamlTyList {
                item: Some(Box::new(int_ty())),
            }))),
        };
        let list = resent(Out::UnionVariantValue(Box::new(
            wire::BamlValueUnionVariant {
                self_type: Some(wire::BamlTy {
                    ty: Some(Ty::Union(wire::BamlTyUnion {
                        options: vec![list_ty.clone(), string_ty()],
                    })),
                }),
                selected_option_index: Some(0),
                value: Some(Box::new(out(Out::ListValue(wire::BamlValueList {
                    item_type: Some(int_ty()),
                    items: vec![],
                })))),
                ..Default::default()
            },
        )))
        .unwrap();
        assert_eq!(list.value_type, Some(list_ty));
    }
}
