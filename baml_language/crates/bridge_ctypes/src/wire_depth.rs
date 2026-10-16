//! How deep the wire nests, measured as the decoder measures it.
//!
//! prost refuses a message nested more than [`WIRE_NESTING_LIMIT`] messages
//! deep. An encoder that ships a deeper value gets a protobuf decode error
//! back from the engine, so an encoder measures a value the same way before
//! shipping it. The measure is of the generated structs themselves, so it
//! cannot drift from the schema: every nested message is counted where the
//! struct nests it.

use crate::baml_bridge::cffi::{
    BamlTy, BamlTyDef, BamlValueMedia, BamlValuePromptAst, BamlValuePromptAstSimple,
    InboundMapEntry, InboundValue, baml_ty::Ty, baml_value_media, baml_value_prompt_ast,
    baml_value_prompt_ast_simple, inbound_map_entry, inbound_value::Value,
};

/// What an `InboundMapEntry` nests: its value, or an `InboundEnumValue` key,
/// the one key kind that is a message.
fn entry_depth(entry: &InboundMapEntry) -> usize {
    let key = usize::from(matches!(
        entry.key,
        Some(inbound_map_entry::Key::EnumKey(_))
    ));
    entry
        .value
        .as_ref()
        .map_or(0, |v| 1 + inbound_value_depth(v))
        .max(key)
}

/// How many nested protobuf messages the decoder reads before it refuses a
/// message as too deep: prost's recursion limit, which this crate leaves at
/// its default.
pub const WIRE_NESTING_LIMIT: usize = 100;

/// The messages that enclose a call argument's `InboundValue`:
/// `CallFunctionArgs` → `InboundMapEntry` → the value.
pub const ARGUMENT_DEPTH: usize = 2;

/// A host callback's result or thrown value is a standalone `InboundValue`.
pub const RESULT_DEPTH: usize = 0;

/// How many messages `value` nests below itself, counting its own type
/// statement and the payload of every leaf. A scalar is `0`; a list of
/// scalars is `2` (`InboundListValue`, `InboundValue`).
pub fn inbound_value_depth(value: &InboundValue) -> usize {
    let statement = value
        .value_type
        .as_ref()
        .map_or(0, |ty| 1 + baml_ty_depth(ty));
    let payload = match &value.value {
        None
        | Some(
            Value::StringValue(_)
            | Value::IntValue(_)
            | Value::FloatValue(_)
            | Value::BoolValue(_)
            | Value::Uint8arrayValue(_)
            | Value::BigintValue(_)
            | Value::JsNumberValue(_),
        ) => 0,
        // `InboundEnumValue`, `BamlHandle`: one message, no nesting.
        Some(Value::EnumValue(_) | Value::Handle(_)) => 1,
        // `InboundListValue` → each `InboundValue`.
        Some(Value::ListValue(list)) => {
            1 + list
                .values
                .iter()
                .map(|item| 1 + inbound_value_depth(item))
                .max()
                .unwrap_or(0)
        }
        // `InboundMapValue` → `InboundMapEntry` → `InboundValue`.
        Some(Value::MapValue(map)) => {
            1 + map
                .entries
                .iter()
                .map(|entry| 1 + entry_depth(entry))
                .max()
                .unwrap_or(0)
        }
        // `InboundClassValue` → `InboundMapEntry` → `InboundValue`.
        Some(Value::ClassValue(class)) => {
            1 + class
                .fields
                .iter()
                .map(|entry| 1 + entry_depth(entry))
                .max()
                .unwrap_or(0)
        }
        Some(Value::TyValue(ty)) => 1 + baml_ty_depth(ty),
        Some(Value::TyDefValue(def)) => 1 + baml_ty_def_depth(def),
        Some(Value::MediaValue(media)) => 1 + media_depth(media),
        Some(Value::PromptAstValue(prompt)) => 1 + prompt_depth(prompt),
    };
    statement.max(payload)
}

/// How many messages a `BamlTy` nests below itself. A primitive is `1`
/// (`BamlTyPrimitive`); `list<int>` is `3` (`BamlTyList`, `BamlTy`,
/// `BamlTyPrimitive`).
pub fn baml_ty_depth(ty: &BamlTy) -> usize {
    fn nested(ty: &BamlTy) -> usize {
        1 + baml_ty_depth(ty)
    }
    let deepest = |tys: &[BamlTy]| tys.iter().map(nested).max().unwrap_or(0);
    match &ty.ty {
        None => 0,
        Some(
            Ty::Primitive(_)
            | Ty::Enum(_)
            | Ty::Literal(_)
            | Ty::Unknown(_)
            | Ty::Media(_)
            | Ty::EnumVariant(_)
            | Ty::RustType(_)
            | Ty::MetaType(_)
            | Ty::Resource(_)
            | Ty::PromptAst(_)
            | Ty::TypeVar(_)
            | Ty::Never(_),
        ) => 1,
        Some(Ty::ClassTy(class)) => 1 + deepest(&class.type_args),
        Some(Ty::TypeAlias(alias)) => 1 + deepest(&alias.type_args),
        Some(Ty::List(list)) => 1 + list.item.as_ref().map_or(0, |ty| nested(ty)),
        Some(Ty::Map(map)) => {
            1 + map
                .key
                .as_ref()
                .map_or(0, |ty| nested(ty))
                .max(map.value.as_ref().map_or(0, |ty| nested(ty)))
        }
        Some(Ty::Optional(optional)) => 1 + optional.inner.as_ref().map_or(0, |ty| nested(ty)),
        Some(Ty::Union(union)) => 1 + deepest(&union.options),
        Some(Ty::Interface(interface)) => {
            1 + deepest(&interface.type_args).max(
                interface
                    .bindings
                    .iter()
                    .map(|binding| 1 + binding.ty.as_ref().map_or(0, nested))
                    .max()
                    .unwrap_or(0),
            )
        }
        Some(Ty::Function(function)) => {
            1 + function
                .params
                .iter()
                .map(|param| 1 + param.ty.as_ref().map_or(0, nested))
                .max()
                .unwrap_or(0)
                .max(function.ret.as_ref().map_or(0, |ty| nested(ty)))
                .max(function.throws.as_ref().map_or(0, |ty| nested(ty)))
        }
        Some(Ty::Future(future)) => {
            1 + future
                .value
                .as_ref()
                .map_or(0, |ty| nested(ty))
                .max(future.error.as_ref().map_or(0, |ty| nested(ty)))
        }
        Some(Ty::AssociatedTypeProjection(projection)) => {
            1 + projection
                .base
                .as_ref()
                .map_or(0, |ty| nested(ty))
                .max(projection.interface.as_ref().map_or(0, |ty| nested(ty)))
        }
    }
}

/// `BamlTyDef` → `BamlClassDef` → `BamlClassFieldDef` → `BamlTy`, and the
/// root type and witnesses alongside.
fn baml_ty_def_depth(def: &BamlTyDef) -> usize {
    let root = def.root.as_ref().map_or(0, |ty| 1 + baml_ty_depth(ty));
    // Each declaration and variant may carry a `BamlTyMetadata` message.
    let classes = def
        .classes
        .iter()
        .map(|class| {
            1 + class
                .fields
                .iter()
                .map(|field| 1 + field.ty.as_ref().map_or(0, |ty| 1 + baml_ty_depth(ty)))
                .max()
                .unwrap_or(0)
                .max(usize::from(class.metadata.is_some()))
        })
        .max()
        .unwrap_or(0);
    let enums = def
        .enums
        .iter()
        .map(|enumeration| {
            1 + enumeration
                .variants
                .iter()
                .map(|variant| 1 + usize::from(variant.metadata.is_some()))
                .max()
                .unwrap_or(0)
                .max(usize::from(enumeration.metadata.is_some()))
        })
        .max()
        .unwrap_or(0);
    let witnesses = def
        .witnesses
        .iter()
        .map(|witness| {
            1 + witness
                .interface_args
                .iter()
                .map(|ty| 1 + baml_ty_depth(ty))
                .max()
                .unwrap_or(0)
                .max(
                    witness
                        .associated_types
                        .iter()
                        .map(|binding| {
                            1 + binding.ty.as_ref().map_or(0, |ty| 1 + baml_ty_depth(ty))
                        })
                        .max()
                        .unwrap_or(0),
                )
                .max(usize::from(!witness.field_links.is_empty()))
        })
        .max()
        .unwrap_or(0);
    root.max(classes).max(enums).max(witnesses)
}

fn media_depth(media: &BamlValueMedia) -> usize {
    match &media.value {
        None | Some(baml_value_media::Value::Url(_) | baml_value_media::Value::Base64(_)) => 0,
        Some(baml_value_media::Value::FileContent(_)) => 1,
    }
}

fn prompt_depth(prompt: &BamlValuePromptAst) -> usize {
    match &prompt.value {
        None => 0,
        Some(baml_value_prompt_ast::Value::Simple(simple)) => 1 + simple_depth(simple),
        Some(baml_value_prompt_ast::Value::Message(message)) => {
            1 + message
                .content
                .as_ref()
                .map_or(0, |content| 1 + simple_depth(content))
        }
        Some(baml_value_prompt_ast::Value::Multiple(multiple)) => {
            1 + multiple
                .items
                .iter()
                .map(|item| 1 + prompt_depth(item))
                .max()
                .unwrap_or(0)
        }
    }
}

fn simple_depth(simple: &BamlValuePromptAstSimple) -> usize {
    match &simple.value {
        None | Some(baml_value_prompt_ast_simple::Value::String(_)) => 0,
        Some(baml_value_prompt_ast_simple::Value::Media(media)) => 1 + media_depth(media),
        Some(baml_value_prompt_ast_simple::Value::Multiple(multiple)) => {
            1 + multiple
                .items
                .iter()
                .map(|item| 1 + simple_depth(item))
                .max()
                .unwrap_or(0)
        }
    }
}

#[cfg(test)]
mod tests {
    use prost::Message;

    use super::*;
    use crate::baml_bridge::cffi::{InboundListValue, InboundMapEntry, InboundMapValue};

    fn nested_list(levels: usize) -> InboundValue {
        let mut value = InboundValue {
            value_type: None,
            value: Some(Value::IntValue(1)),
        };
        for _ in 0..levels {
            value = InboundValue {
                value_type: None,
                value: Some(Value::ListValue(InboundListValue {
                    values: vec![value],
                })),
            };
        }
        value
    }

    /// The limit is prost's: a value measured exactly at the limit decodes,
    /// one message deeper is refused. This pins `WIRE_NESTING_LIMIT` and the
    /// per-message accounting to the real decoder.
    #[test]
    fn the_measure_is_the_decoders() {
        // A standalone `InboundValue` is at depth 0; 50 list levels nest 100
        // messages below it.
        let at_limit = nested_list(50);
        assert_eq!(inbound_value_depth(&at_limit), WIRE_NESTING_LIMIT);
        InboundValue::decode(at_limit.encode_to_vec().as_slice())
            .expect("a value at the limit decodes");
        let past = nested_list(51);
        assert_eq!(inbound_value_depth(&past), WIRE_NESTING_LIMIT + 2);
        InboundValue::decode(past.encode_to_vec().as_slice())
            .expect_err("a value past the limit is refused");
    }

    #[test]
    fn statements_and_entries_count() {
        let stated = InboundValue {
            value_type: Some(crate::runtime_ty_to_proto_ty(&baml_type::RuntimeTy::List(
                Box::new(baml_type::RuntimeTy::Int),
            ))),
            value: Some(Value::ListValue(InboundListValue { values: vec![] })),
        };
        // `BamlTy` → `BamlTyList` → `BamlTy` → `BamlTyPrimitive` = 4, deeper
        // than the empty list's 1.
        assert_eq!(inbound_value_depth(&stated), 4);
        let map = InboundValue {
            value_type: None,
            value: Some(Value::MapValue(InboundMapValue {
                entries: vec![InboundMapEntry {
                    key: None,
                    value: Some(nested_list(1)),
                }],
            })),
        };
        // `InboundMapValue` → `InboundMapEntry` → `InboundValue` → list(2).
        assert_eq!(inbound_value_depth(&map), 5);
    }
}
