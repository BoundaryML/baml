//! Turning a decided plan into the wire, with every effect exactly once.
//!
//! Commit is the only phase that touches the host for anything but reading:
//! a callable is registered, a handle cloned, a payload copied, each once per
//! node the plan ships. Everything created is recorded for the caller, who
//! releases it if the request never reaches the engine. Before any of that,
//! the plan's own depth is measured so a value the wire cannot carry is
//! refused with nothing created.

use bridge_ctypes::{
    baml_bridge::cffi::{
        BamlHandle, BamlHandleType, InboundClassValue, InboundEnumValue, InboundListValue,
        InboundMapEntry, InboundMapValue, InboundValue, inbound_map_entry::Key,
        inbound_value::Value,
    },
    baml_ty_depth, runtime_ty_to_proto_ty,
};
use num_bigint::BigInt;

use crate::{
    facts::Facts,
    host::{Host, HostValue, LeafWire, Owner, Scalar},
    plan::{KeyNode, Node},
};

/// How many wire messages `node` will nest below itself, counted as the
/// decoder counts them, from the plan alone. Exact for everything but a
/// leaf's payload, whose nesting the host produces at commit and the final
/// measure catches; this one refuses the common case before any effect.
pub(crate) fn depth<V>(facts: &Facts, node: &Node<'_, V>) -> usize {
    let statement = |ty: &baml_type::RuntimeTy| {
        1 + baml_ty_depth(&runtime_ty_to_proto_ty(&facts.stated_form(ty.clone())))
    };
    match node {
        Node::Null | Node::Scalar { .. } => 0,
        // `InboundEnumValue`, `BamlHandle`: one message, no nesting.
        Node::EnumVariant { .. } | Node::Leaf { .. } => 1,
        Node::Callable { signature, .. } => statement(signature).max(1),
        // `InboundListValue` → each `InboundValue`.
        Node::List { ty, items } => statement(ty).max(
            1 + items
                .iter()
                .map(|item| 1 + depth(facts, item))
                .max()
                .unwrap_or(0),
        ),
        // `InboundMapValue` → `InboundMapEntry` → `InboundValue`.
        Node::Map { ty, entries } => statement(ty).max(
            1 + entries
                .iter()
                .map(|(_, item)| 2 + depth(facts, item))
                .max()
                .unwrap_or(0),
        ),
        // `InboundClassValue` → `InboundMapEntry` → `InboundValue`.
        Node::Class { ty, fields } => statement(ty).max(
            1 + fields
                .iter()
                .map(|(_, item)| 2 + depth(facts, item))
                .max()
                .unwrap_or(0),
        ),
    }
}

/// Lower `node` to the wire through `host`, recording what was created in
/// `created`. A host failure leaves `created` holding what preceded it, for
/// the caller to release.
pub(crate) fn commit<V: HostValue, H: Host<V>>(
    facts: &Facts,
    node: &Node<'_, V>,
    host: &H,
    created: &mut Vec<Owner>,
) -> Result<InboundValue, V::Error> {
    match node {
        Node::Null => Ok(InboundValue::default()),
        Node::Scalar { value, scalar } => Ok(InboundValue {
            value_type: None,
            value: Some(match scalar {
                Scalar::Bool(bool) => Value::BoolValue(*bool),
                Scalar::Int(int) => Value::IntValue(int.get()),
                Scalar::BigInt(big) => Value::BigintValue(hex(big)),
                Scalar::Float(float) => Value::FloatValue(*float),
                Scalar::String => Value::StringValue(value.value.text()?.into_owned()),
                Scalar::Bytes => Value::Uint8arrayValue(value.value.bytes()?.into_owned()),
            }),
        }),
        Node::EnumVariant {
            enumeration,
            variant,
        } => Ok(InboundValue {
            value_type: None,
            value: Some(Value::EnumValue(InboundEnumValue {
                name: enumeration.render_dotted(false),
                value: variant.clone(),
            })),
        }),
        Node::Leaf { value, kind } => {
            let (wire, owner) = host.leaf(&value.value, *kind)?;
            created.extend(owner);
            Ok(InboundValue {
                value_type: None,
                value: Some(match wire {
                    LeafWire::Handle(handle) => Value::Handle(handle),
                    LeafWire::Media(media) => Value::MediaValue(media),
                    LeafWire::Prompt(prompt) => Value::PromptAstValue(prompt),
                    LeafWire::TypeValue(def) => Value::TyDefValue(def),
                }),
            })
        }
        Node::Callable { value, signature } => {
            let key = host.register(&value.value, signature)?;
            created.push(Owner::Callable(key));
            Ok(InboundValue {
                value_type: Some(runtime_ty_to_proto_ty(
                    &facts.stated_form(signature.clone()),
                )),
                value: Some(Value::Handle(BamlHandle {
                    key: key.get(),
                    handle_type: BamlHandleType::HostValueCallable as i32,
                })),
            })
        }
        Node::List { ty, items } => {
            let values = items
                .iter()
                .map(|item| commit(facts, item, host, created))
                .collect::<Result<Vec<_>, _>>()?;
            Ok(InboundValue {
                value_type: Some(runtime_ty_to_proto_ty(&facts.stated_form(ty.clone()))),
                value: Some(Value::ListValue(InboundListValue { values })),
            })
        }
        Node::Map { ty, entries } => {
            let entries = entries
                .iter()
                .map(|(key, item)| {
                    let key = match key {
                        KeyNode::String(tree) => Key::StringKey(tree.value.text()?.into_owned()),
                    };
                    Ok(InboundMapEntry {
                        key: Some(key),
                        value: Some(commit(facts, item, host, created)?),
                    })
                })
                .collect::<Result<Vec<_>, V::Error>>()?;
            Ok(InboundValue {
                value_type: Some(runtime_ty_to_proto_ty(&facts.stated_form(ty.clone()))),
                value: Some(Value::MapValue(InboundMapValue { entries })),
            })
        }
        Node::Class { ty, fields } => {
            let fields = fields
                .iter()
                .map(|(name, item)| {
                    Ok(InboundMapEntry {
                        key: Some(Key::StringKey(name.clone())),
                        value: Some(commit(facts, item, host, created)?),
                    })
                })
                .collect::<Result<Vec<_>, V::Error>>()?;
            Ok(InboundValue {
                value_type: Some(runtime_ty_to_proto_ty(&facts.stated_form(ty.clone()))),
                value: Some(Value::ClassValue(InboundClassValue { fields })),
            })
        }
    }
}

/// Bigints cross the wire in base sixteen with a leading minus.
fn hex(value: &BigInt) -> String {
    format!("{value:x}")
}
