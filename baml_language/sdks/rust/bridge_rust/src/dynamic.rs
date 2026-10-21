//! Owning dynamic values and closed invocation targets for generated SDK roots.
use std::{collections::BTreeMap, sync::Arc};

use crate::{
    BamlValue, DecodeError, Error, SdkError, baml_value::internal::__BamlValuePrivate, wire,
};

#[derive(Clone)]
pub struct Input(Arc<dyn Fn() -> wire::InboundValue + Send + Sync>);
impl Input {
    pub fn new<T: BamlValue + Send + Sync + 'static>(value: T) -> Self {
        Self(Arc::new(move || value.to_baml()))
    }
    fn encode(&self) -> wire::InboundValue {
        (self.0)()
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

#[derive(Clone)]
pub struct Value(Arc<OwnedValue>);
struct OwnedValue(wire::BamlOutboundValue);
impl Drop for OwnedValue {
    fn drop(&mut self) {
        if let Ok(api) = crate::capi::api() {
            crate::host_value::release_callback_value(api, std::mem::take(&mut self.0));
        }
    }
}
impl std::fmt::Debug for Value {
    fn fmt(&self, formatter: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        formatter.debug_tuple("Value").field(&self.0.0).finish()
    }
}
impl Value {
    pub fn decode<T: BamlValue>(&self) -> Result<T, DecodeError> {
        T::from_baml(crate::host_value::clone_callback_value(&self.0.0))
    }
    pub fn callable_target(&self) -> Result<Target, SdkError> {
        let value = crate::decode::unwrap(self.0.0.clone());
        let Some(wire::baml_outbound_value::Value::HandleValue(handle)) = value.value else {
            return Err(SdkError::new("value is not a BAML callable"));
        };
        if handle.handle_type != wire::BamlHandleType::FunctionRef as i32 {
            return Err(SdkError::new("value is not a BAML callable"));
        }
        let api = crate::capi::api()?;
        let mut key = 0;
        // SAFETY: this value owns the source key for the entire clone operation.
        #[expect(unsafe_code)]
        let status = unsafe { (api.handle_clone)(handle.key, &raw mut key) };
        if status != 0 {
            return Err(SdkError::new("released BAML callable"));
        }
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
        Ok(Self(Arc::new(OwnedValue(value))))
    }
    fn to_baml(&self) -> wire::InboundValue {
        outbound_to_inbound(crate::host_value::clone_callback_value(&self.0.0))
    }
    fn baml_ty() -> wire::BamlTy {
        wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::Unknown(wire::BamlTyUnknown {})),
        }
    }
}
fn outbound_to_inbound(value: wire::BamlOutboundValue) -> wire::InboundValue {
    use wire::{baml_outbound_value::Value as Out, inbound_value::Value as In};
    let mut value_type = None;
    let value = match value.value {
        None | Some(Out::NullValue(_)) => None,
        Some(Out::StringValue(value)) => Some(In::StringValue(value)),
        Some(Out::IntValue(value)) => Some(In::IntValue(value)),
        Some(Out::FloatValue(value)) => Some(In::FloatValue(value)),
        Some(Out::BoolValue(value)) => Some(In::BoolValue(value)),
        Some(Out::Uint8arrayValue(value)) => Some(In::Uint8arrayValue(value)),
        Some(Out::BigintValue(value)) => Some(In::BigintValue(value)),
        Some(Out::TyValue(value)) => Some(In::TyValue(value)),
        Some(Out::TyDefValue(value)) => Some(In::TyDefValue(value)),
        Some(Out::MediaValue(value)) => Some(In::MediaValue(value)),
        Some(Out::PromptAstValue(value)) => Some(In::PromptAstValue(value)),
        Some(Out::HandleValue(handle)) => {
            value_type = handle.ty;
            Some(In::Handle(wire::BamlHandle {
                key: handle.key,
                handle_type: handle.handle_type,
            }))
        }
        Some(Out::EnumValue(value)) => Some(In::EnumValue(wire::InboundEnumValue {
            name: value.name,
            value: value.value,
        })),
        Some(Out::ClassValue(class)) => {
            value_type = Some(crate::baml_value::internal::class_ty(
                &class.name,
                class.type_args,
            ));
            Some(In::ClassValue(wire::InboundClassValue {
                fields: class
                    .fields
                    .into_iter()
                    .map(|entry| wire::InboundMapEntry {
                        key: Some(wire::inbound_map_entry::Key::StringKey(entry.key)),
                        value: entry.value.map(outbound_to_inbound),
                    })
                    .collect(),
            }))
        }
        Some(Out::ListValue(list)) => {
            value_type = Some(wire::BamlTy {
                ty: Some(wire::baml_ty::Ty::List(Box::new(wire::BamlTyList {
                    item: list.item_type.map(Box::new),
                }))),
            });
            Some(In::ListValue(wire::InboundListValue {
                values: list.items.into_iter().map(outbound_to_inbound).collect(),
            }))
        }
        Some(Out::MapValue(map)) => {
            let key_type = map.key_type.clone();
            value_type = Some(wire::BamlTy {
                ty: Some(wire::baml_ty::Ty::Map(Box::new(wire::BamlTyMap {
                    key: map.key_type.map(Box::new),
                    value: map.value_type.map(Box::new),
                }))),
            });
            Some(In::MapValue(wire::InboundMapValue {
                entries: map
                    .entries
                    .into_iter()
                    .map(|entry| wire::InboundMapEntry {
                        key: Some(map_key(entry.key, key_type.as_ref())),
                        value: entry.value.map(outbound_to_inbound),
                    })
                    .collect(),
            }))
        }
        Some(Out::UnionVariantValue(union)) => {
            let selected = union.selected_option_index.and_then(|index| {
                match union.self_type.as_ref()?.ty.as_ref()? {
                    wire::baml_ty::Ty::Union(union) => union.options.get(index as usize).cloned(),
                    _ => None,
                }
            });
            let mut encoded =
                outbound_to_inbound(union.value.map(|value| *value).unwrap_or_default());
            if encoded.value_type.is_none() {
                encoded.value_type = selected;
            }
            return encoded;
        }
        Some(Out::LiteralValue(literal)) => {
            let literal_type = wire::BamlTy {
                ty: Some(wire::baml_ty::Ty::Literal(wire::BamlTyLiteral {
                    literal: literal.literal.clone().map(|value| match value {
                        wire::baml_literal_value::Literal::StringValue(value) => {
                            wire::baml_ty_literal::Literal::StringValue(value)
                        }
                        wire::baml_literal_value::Literal::IntValue(value) => {
                            wire::baml_ty_literal::Literal::IntValue(value)
                        }
                        wire::baml_literal_value::Literal::BoolValue(value) => {
                            wire::baml_ty_literal::Literal::BoolValue(value)
                        }
                        wire::baml_literal_value::Literal::BigintValue(value) => {
                            wire::baml_ty_literal::Literal::BigintValue(value)
                        }
                        wire::baml_literal_value::Literal::FloatValue(value) => {
                            wire::baml_ty_literal::Literal::FloatValue(value)
                        }
                    }),
                })),
            };
            let mut encoded = outbound_to_inbound(crate::decode::unwrap(wire::BamlOutboundValue {
                value: Some(Out::LiteralValue(literal)),
            }));
            encoded.value_type = Some(literal_type);
            return encoded;
        }
    };
    wire::InboundValue { value_type, value }
}
fn map_key(key: String, ty: Option<&wire::BamlTy>) -> wire::inbound_map_entry::Key {
    use wire::{BamlTyPrimitiveKind, baml_ty::Ty, inbound_map_entry::Key};
    match ty.and_then(|ty| ty.ty.as_ref()) {
        Some(Ty::Primitive(value))
            if value.kind == BamlTyPrimitiveKind::BamlTyPrimitiveInt as i32 =>
        {
            Key::IntKey(key.parse().expect("runtime integer map key"))
        }
        Some(Ty::Primitive(value))
            if value.kind == BamlTyPrimitiveKind::BamlTyPrimitiveBool as i32 =>
        {
            Key::BoolKey(key.parse().expect("runtime boolean map key"))
        }
        Some(Ty::Enum(value)) => Key::EnumKey(wire::InboundEnumValue {
            name: value.name.clone(),
            value: key,
        }),
        _ => Key::StringKey(key),
    }
}
fn dispatch(
    target: &Target,
    arguments: Arguments,
    types: TypeBindings,
    baml: crate::invocation::InvocationOptions,
) -> Result<crate::completion::Receiver, SdkError> {
    let target = match target {
        Target::Named(name) if !name.is_empty() && !name.contains('\0') => {
            wire::call_function_args::CallTarget::FunctionName(name.clone())
        }
        Target::Named(_) => return Err(SdkError::new("invalid BAML invocation target")),
        Target::Callable(function) => {
            if !types.is_empty() {
                return Err(SdkError::new("specialized callable rejects type bindings"));
            }
            wire::call_function_args::CallTarget::FunctionHandle(function.0.key)
        }
    };
    crate::runtime::dispatch_controlled(
        target,
        || {
            Ok((
                arguments
                    .into_iter()
                    .map(|(name, value)| wire::InboundMapEntry {
                        key: Some(wire::inbound_map_entry::Key::StringKey(name)),
                        value: Some(value.encode()),
                    })
                    .collect(),
                types
                    .into_iter()
                    .map(|(name, ty)| wire::BamlTyArg {
                        type_var: name,
                        type_value: Some(ty.0),
                        type_definition: None,
                    })
                    .collect(),
            ))
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
    let receiver = dispatch(&target, arguments, types, baml).map_err(Error::Sdk)?;
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
    let receiver = dispatch(&target, arguments, types, baml).map_err(Error::Sdk)?;
    let result = crate::decode::decode_result(&receiver.wait().await);
    drop(target);
    result
}
