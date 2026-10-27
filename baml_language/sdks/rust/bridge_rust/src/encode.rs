//! Inbound (host → engine) wire construction.
//!
//! Every value crosses through an [`Encoder`], which owns what the encoding
//! creates (a clone of each engine handle the value holds, a registration of
//! each host callable) until the value is sent, and releases it if the
//! encoding fails. A value nested deeper than the wire carries is refused
//! here, before anything is sent: the engine's decoder reads at most
//! [`WIRE_NESTING_LIMIT`] nested protobuf messages, so the encoder measures
//! the value it built exactly as the decoder will.

use std::fmt;

use crate::{
    BamlValue, OptionalArg,
    wire::{self, inbound_map_entry::Key, inbound_value::Value as In},
};

/// How many nested protobuf messages the engine's decoder reads before it
/// refuses a message as too deep (prost's default recursion limit).
pub(crate) const WIRE_NESTING_LIMIT: usize = 100;

/// The messages that enclose a call argument's value: `CallFunctionArgs` →
/// `InboundMapEntry` → the value. The cancel token beside the arguments sits
/// as deep: `CallFunctionArgs` → `InvocationOptions` → the value.
pub(crate) const ARGUMENT_DEPTH: usize = 2;

/// The message that encloses a host callback's result or thrown value: the
/// `HostCallCompletion` whose outcome holds it.
pub(crate) const RESULT_DEPTH: usize = 1;

/// How many container levels a value may nest while it is encoded. Every
/// list level costs two wire messages and every map or class level three, so
/// a value nested deeper can never cross; the exact measure is taken of the
/// built value. The bound keeps the recursive encoder's stack finite however
/// deep the host value is.
const MAX_LEVELS: usize = WIRE_NESTING_LIMIT / 2;

/// Why a host value cannot be sent to the engine.
#[derive(Debug, Clone, PartialEq, Eq)]
#[non_exhaustive]
pub enum EncodeError {
    /// The value nests more protobuf messages than the engine's decoder
    /// reads (`limit`, counting the messages that enclose the value in its
    /// request).
    TooDeep {
        /// The decoder's nesting limit.
        limit: usize,
    },
    /// The engine could not clone a handle the value holds: the handle was
    /// released, or the runtime shut down.
    HandleClone,
    /// A type the value states names a declaration only the runtime can
    /// name (an anonymous label the engine sent beside a value it returned).
    AnonymousDeclaration,
    /// A container the engine returned without the type it has: the wire
    /// requires a container to state its type, and nothing else knows it.
    UntypedContainer,
    /// The value is one only the runtime builds (a `baml.errors.HostCallable`,
    /// a raw outbound wire value).
    RuntimeOnly {
        /// What the value is.
        what: &'static str,
    },
}

impl fmt::Display for EncodeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            EncodeError::TooDeep { limit } => write!(
                f,
                "the value is nested too deeply for the boundary (at most {limit} nested messages)"
            ),
            EncodeError::HandleClone => {
                write!(f, "a handle the value holds could not be cloned")
            }
            EncodeError::AnonymousDeclaration => write!(
                f,
                "the value's type names a declaration only the runtime can name"
            ),
            EncodeError::UntypedContainer => {
                write!(
                    f,
                    "the value holds a container the runtime sent without its type"
                )
            }
            EncodeError::RuntimeOnly { what } => {
                write!(f, "{what} is built by the runtime alone and cannot be sent")
            }
        }
    }
}

impl std::error::Error for EncodeError {}

/// The engine refused to clone a handle: it was released, or the runtime
/// shut down.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub(crate) struct CloneRefused;

impl From<CloneRefused> for EncodeError {
    fn from(CloneRefused: CloneRefused) -> Self {
        EncodeError::HandleClone
    }
}

impl From<CloneRefused> for crate::DecodeError {
    fn from(CloneRefused: CloneRefused) -> Self {
        crate::DecodeError::HandleClone
    }
}

/// An argument the bridge could not encode, refused before the call is sent.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ArgumentError {
    /// The parameter the argument fills.
    pub name: String,
    /// Why it could not be encoded.
    pub error: EncodeError,
}

impl fmt::Display for ArgumentError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "argument `{}`: {}", self.name, self.error)
    }
}

impl std::error::Error for ArgumentError {}

/// The state of one encoding: how deep the value being encoded is so far, and
/// what the encoding created that the engine takes over once the value is
/// sent. Dropped without [`commit`](Self::commit), it releases all of it.
#[doc(hidden)]
#[derive(Default)]
pub struct Encoder {
    levels: usize,
    /// Engine references cloned for the value, one per occurrence.
    clones: Vec<u64>,
    /// Host callables registered for the value.
    registrations: Vec<u64>,
}

impl Encoder {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// Encode one container level: its children are encoded by `body`. A
    /// value nested deeper than any value the wire carries is refused here,
    /// before the encoder recurses further.
    pub fn nested<T>(
        &mut self,
        body: impl FnOnce(&mut Self) -> Result<T, EncodeError>,
    ) -> Result<T, EncodeError> {
        if self.levels >= MAX_LEVELS {
            return Err(EncodeError::TooDeep {
                limit: WIRE_NESTING_LIMIT,
            });
        }
        self.levels += 1;
        let result = body(self);
        self.levels -= 1;
        result
    }

    /// A fresh engine reference to the object `key` names, owned by this
    /// encoding until the value is sent.
    pub(crate) fn clone_handle(&mut self, key: u64) -> Result<u64, CloneRefused> {
        let cloned = crate::handle::clone_engine_key(key).ok_or(CloneRefused)?;
        self.clones.push(cloned);
        Ok(cloned)
    }

    /// Record a host value registered for the value, unregistered if the
    /// value is never sent.
    pub(crate) fn registered(&mut self, key: u64) {
        self.registrations.push(key);
    }

    /// Take over what `other` created: one value is sent with both.
    pub(crate) fn adopt(&mut self, mut other: Encoder) {
        self.clones.append(&mut other.clones);
        self.registrations.append(&mut other.registrations);
    }

    /// The value was handed to the engine, which now owns what it holds.
    pub(crate) fn commit(mut self) {
        self.clones.clear();
        self.registrations.clear();
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        for key in self.clones.drain(..) {
            crate::handle::release_engine_key(key);
        }
        for key in self.registrations.drain(..) {
            crate::host_value::unregister(key);
        }
    }
}

/// Measure `value` against the wire's nesting limit, given the messages
/// `enclosing` it in its request.
pub(crate) fn fits_the_wire(
    value: wire::InboundValue,
    enclosing: usize,
) -> Result<wire::InboundValue, EncodeError> {
    if enclosing + inbound_value_depth(&value) > WIRE_NESTING_LIMIT {
        return Err(EncodeError::TooDeep {
            limit: WIRE_NESTING_LIMIT,
        });
    }
    Ok(value)
}

/// Encode a host callback's result or thrown value: `encode` builds it, and
/// it must fit the wire one message below the completion.
pub(crate) fn callback_value(
    encode: impl FnOnce(&mut Encoder) -> Result<wire::InboundValue, EncodeError>,
) -> Result<(wire::InboundValue, Encoder), EncodeError> {
    let mut encoder = Encoder::new();
    let value = fits_the_wire(encode(&mut encoder)?, RESULT_DEPTH)?;
    Ok((value, encoder))
}

/// Encode a value held for a call (a dynamic argument, a cancel token) when
/// the call is sent.
#[doc(hidden)]
pub type EncodeValue =
    dyn Fn(&mut Encoder) -> Result<wire::InboundValue, EncodeError> + Send + Sync;

/// The arguments of one call, encoded for the wire: each against the
/// parameter it fills, each measured against the wire's nesting limit.
#[doc(hidden)]
#[derive(Default)]
pub struct Arguments {
    encoder: Encoder,
    kwargs: Vec<wire::InboundMapEntry>,
}

impl Arguments {
    pub(crate) fn new() -> Self {
        Self::default()
    }

    /// The argument `name`, a value of the parameter's declared type `slot`.
    pub fn value<T: BamlValue>(
        &mut self,
        name: &str,
        value: &T,
        slot: &wire::BamlTy,
    ) -> Result<(), ArgumentError> {
        self.encoded(name, |encoder| value.to_baml_in(slot, encoder))
    }

    /// The receiver of an instance method, sent as the argument `self` under
    /// its own type.
    pub fn receiver<T: BamlValue>(&mut self, value: &T) -> Result<(), ArgumentError> {
        self.encoded("self", |encoder| value.to_baml(encoder))
    }

    /// An argument with a BAML default: omitted when it is
    /// [`Unset`](OptionalArg::Unset), so the engine evaluates the default.
    pub fn optional<T: BamlValue>(
        &mut self,
        name: &str,
        value: &OptionalArg<T>,
        slot: &wire::BamlTy,
    ) -> Result<(), ArgumentError> {
        match value {
            OptionalArg::Unset => Ok(()),
            OptionalArg::Set(value) => self.value(name, value, slot),
        }
    }

    /// A host callable argument: `callback` is registered with the bridge
    /// and sent as its handle, stating the function type its parameter
    /// declares.
    pub fn callable<Args, Ret, Marker, Cb>(
        &mut self,
        name: &str,
        callback: Cb,
        params: &'static [crate::HostParam],
        function_type: crate::host_value::CallableTy,
    ) -> Result<(), ArgumentError>
    where
        Ret: BamlValue,
        Cb: crate::HostCallback<Args, Ret, Marker>,
    {
        self.encoded(name, |encoder| {
            Ok(crate::host_value::callable_handle(
                callback,
                params,
                function_type,
                encoder,
            ))
        })
    }

    /// The argument `name`, built by `encode`, measured, and added to the
    /// call. Every argument goes through here.
    pub(crate) fn encoded(
        &mut self,
        name: &str,
        encode: impl FnOnce(&mut Encoder) -> Result<wire::InboundValue, EncodeError>,
    ) -> Result<(), ArgumentError> {
        let value = encode(&mut self.encoder)
            .and_then(|value| fits_the_wire(value, ARGUMENT_DEPTH))
            .map_err(|error| ArgumentError {
                name: name.to_string(),
                error,
            })?;
        self.kwargs.push(wire::InboundMapEntry {
            key: Some(Key::StringKey(name.to_string())),
            value: Some(value),
        });
        Ok(())
    }

    /// The encoder the arguments are built with, for a value that rides the
    /// same request outside the arguments (the cancel token).
    pub(crate) fn encoder(&mut self) -> &mut Encoder {
        &mut self.encoder
    }

    /// Take over values encoded ahead of the call (see
    /// [`CallOptions`](crate::CallOptions)).
    pub(crate) fn adopt(&mut self, encoder: Encoder) {
        self.encoder.adopt(encoder);
    }

    /// The encoded arguments, and the encoding whose creations the engine
    /// takes over once the call is sent.
    pub(crate) fn into_parts(self) -> (Vec<wire::InboundMapEntry>, Encoder) {
        (self.kwargs, self.encoder)
    }
}

/// Build the explicit `TypeVar` bindings for a generic call. Each entry
/// pairs a callee `TypeVar` name with the concrete [`wire::BamlTy`] the
/// Rust call site monomorphized it to; the engine seeds its entry frame
/// from them. Entries must be in the callee's `TypeVar` declaration order.
pub fn type_args(entries: Vec<(&str, wire::BamlTy)>) -> Vec<wire::BamlTyArg> {
    entries
        .into_iter()
        .map(|(name, ty)| wire::BamlTyArg {
            type_var: name.to_string(),
            type_value: Some(ty),
        })
        .collect()
}

/// Encode a class instance. `fqn` is the BAML class FQN the generated impl
/// bakes in; the engine binds the instance nominally through it.
/// `type_args` carries a generic instance's concrete type arguments in
/// declaration order, empty for a non-generic class. `fields` encodes each
/// field, one container level below the instance.
pub fn class(
    fqn: &str,
    type_args: Vec<wire::BamlTy>,
    encoder: &mut Encoder,
    fields: impl FnOnce(&mut Encoder) -> Result<Vec<(&'static str, wire::InboundValue)>, EncodeError>,
) -> Result<wire::InboundValue, EncodeError> {
    let fields = encoder.nested(fields)?;
    Ok(wire::InboundValue {
        value_type: Some(wire::BamlTy {
            ty: Some(wire::baml_ty::Ty::ClassTy(wire::BamlTyClass {
                head: Some(wire::baml_ty_class::Head::Name(fqn.to_string())),
                type_args,
            })),
        }),
        value: Some(In::ClassValue(wire::InboundClassValue {
            fields: fields
                .into_iter()
                .map(|(name, value)| wire::InboundMapEntry {
                    key: Some(Key::StringKey(name.to_string())),
                    value: Some(value),
                })
                .collect(),
        })),
    })
}

/// Encode an enum value. `variant_value` is the variant's wire value (not
/// necessarily its Rust variant name).
pub fn enum_value(fqn: &str, variant_value: &str) -> wire::InboundValue {
    wire::InboundValue {
        value_type: None,
        value: Some(In::EnumValue(wire::InboundEnumValue {
            name: fqn.to_string(),
            value: variant_value.to_string(),
        })),
    }
}

/// How many messages `value` nests below itself, counting its own type
/// statement and the payload of every leaf: the decoder's own accounting. A
/// scalar is `0`; a list of scalars is `2` (`InboundListValue`,
/// `InboundValue`).
pub(crate) fn inbound_value_depth(value: &wire::InboundValue) -> usize {
    let statement = value
        .value_type
        .as_ref()
        .map_or(0, |ty| 1 + baml_ty_depth(ty));
    let payload = match &value.value {
        None
        | Some(
            In::StringValue(_)
            | In::IntValue(_)
            | In::FloatValue(_)
            | In::BoolValue(_)
            | In::Uint8arrayValue(_)
            | In::BigintValue(_)
            | In::JsNumberValue(_),
        ) => 0,
        // `InboundEnumValue`, `BamlHandle`: one message, no nesting.
        Some(In::EnumValue(_) | In::Handle(_)) => 1,
        // `InboundListValue` → each `InboundValue`.
        Some(In::ListValue(list)) => {
            1 + list
                .values
                .iter()
                .map(|item| 1 + inbound_value_depth(item))
                .max()
                .unwrap_or(0)
        }
        // `InboundMapValue` → `InboundMapEntry` → `InboundValue`.
        Some(In::MapValue(map)) => 1 + entries_depth(&map.entries),
        // `InboundClassValue` → `InboundMapEntry` → `InboundValue`.
        Some(In::ClassValue(class)) => 1 + entries_depth(&class.fields),
        Some(In::TyValue(ty)) => 1 + baml_ty_depth(ty),
        Some(In::MediaValue(media)) => 1 + media_depth(media),
        Some(In::PromptAstValue(prompt)) => 1 + prompt_depth(prompt),
    };
    statement.max(payload)
}

/// The deepest of `entries`, each an `InboundMapEntry` holding its value or
/// an `InboundEnumValue` key (the one key kind that is a message).
fn entries_depth(entries: &[wire::InboundMapEntry]) -> usize {
    entries
        .iter()
        .map(|entry| {
            let key = usize::from(matches!(entry.key, Some(Key::EnumKey(_))));
            1 + entry
                .value
                .as_ref()
                .map_or(0, |value| 1 + inbound_value_depth(value))
                .max(key)
        })
        .max()
        .unwrap_or(0)
}

/// How many messages a `BamlTy` nests below itself. A primitive is `1`
/// (`BamlTyPrimitive`); `list<int>` is `3` (`BamlTyList`, `BamlTy`,
/// `BamlTyPrimitive`).
fn baml_ty_depth(ty: &wire::BamlTy) -> usize {
    use wire::baml_ty::Ty;
    fn nested(ty: &wire::BamlTy) -> usize {
        1 + baml_ty_depth(ty)
    }
    let deepest = |tys: &[wire::BamlTy]| tys.iter().map(nested).max().unwrap_or(0);
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
        Some(Ty::List(list)) => 1 + list.item.as_deref().map_or(0, nested),
        Some(Ty::Map(map)) => {
            1 + map
                .key
                .as_deref()
                .map_or(0, nested)
                .max(map.value.as_deref().map_or(0, nested))
        }
        Some(Ty::Optional(optional)) => 1 + optional.inner.as_deref().map_or(0, nested),
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
                .max(function.ret.as_deref().map_or(0, nested))
                .max(function.throws.as_deref().map_or(0, nested))
        }
        Some(Ty::Future(future)) => {
            1 + future
                .value
                .as_deref()
                .map_or(0, nested)
                .max(future.error.as_deref().map_or(0, nested))
        }
        Some(Ty::AssociatedTypeProjection(projection)) => {
            1 + projection
                .base
                .as_deref()
                .map_or(0, nested)
                .max(projection.interface.as_deref().map_or(0, nested))
        }
    }
}

fn media_depth(media: &wire::BamlValueMedia) -> usize {
    use wire::baml_value_media::Value;
    match &media.value {
        None | Some(Value::Url(_) | Value::Base64(_)) => 0,
        Some(Value::FileContent(_)) => 1,
    }
}

fn prompt_depth(prompt: &wire::BamlValuePromptAst) -> usize {
    use wire::baml_value_prompt_ast::Value;
    match &prompt.value {
        None => 0,
        Some(Value::Simple(simple)) => 1 + simple_depth(simple),
        Some(Value::Message(message)) => {
            1 + message
                .content
                .as_ref()
                .map_or(0, |content| 1 + simple_depth(content))
        }
        Some(Value::Multiple(multiple)) => {
            1 + multiple
                .items
                .iter()
                .map(|item| 1 + prompt_depth(item))
                .max()
                .unwrap_or(0)
        }
    }
}

fn simple_depth(simple: &wire::BamlValuePromptAstSimple) -> usize {
    use wire::baml_value_prompt_ast_simple::Value;
    match &simple.value {
        None | Some(Value::String(_)) => 0,
        Some(Value::Media(media)) => 1 + media_depth(media),
        Some(Value::Multiple(multiple)) => {
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
    use prost::Message as _;

    use super::*;

    fn nested_list(levels: usize) -> wire::InboundValue {
        (0..levels).fold(
            wire::InboundValue {
                value_type: None,
                value: Some(In::IntValue(1)),
            },
            |value, _| wire::InboundValue {
                value_type: None,
                value: Some(In::ListValue(wire::InboundListValue {
                    values: vec![value],
                })),
            },
        )
    }

    /// The measure is the decoder's: a value at the limit decodes, one
    /// message deeper is refused. This crate's prost is the engine's, at the
    /// same default recursion limit.
    #[test]
    fn the_measure_is_the_decoders() {
        let at_limit = nested_list(50);
        assert_eq!(inbound_value_depth(&at_limit), WIRE_NESTING_LIMIT);
        wire::InboundValue::decode(at_limit.encode_to_vec().as_slice())
            .expect("a value at the limit decodes");
        let past = nested_list(51);
        assert_eq!(inbound_value_depth(&past), WIRE_NESTING_LIMIT + 2);
        wire::InboundValue::decode(past.encode_to_vec().as_slice())
            .expect_err("a value past the limit is refused");
    }

    /// The enclosing messages count: a value that fits a standalone root does
    /// not fit as an argument or a callback result.
    #[test]
    fn enclosing_messages_count() {
        let completion = |value| wire::HostCallCompletion {
            outcome: Some(wire::host_call_completion::Outcome::Returned(value)),
        };
        assert!(fits_the_wire(nested_list(49), RESULT_DEPTH).is_ok());
        wire::HostCallCompletion::decode(completion(nested_list(49)).encode_to_vec().as_slice())
            .expect("a result that fits decodes");
        assert_eq!(
            fits_the_wire(nested_list(50), RESULT_DEPTH),
            Err(EncodeError::TooDeep {
                limit: WIRE_NESTING_LIMIT
            })
        );
        wire::HostCallCompletion::decode(completion(nested_list(50)).encode_to_vec().as_slice())
            .expect_err("a result past the limit is refused");

        let call = |value| wire::CallFunctionArgs {
            kwargs: vec![wire::InboundMapEntry {
                key: Some(Key::StringKey("x".to_string())),
                value: Some(value),
            }],
            ..Default::default()
        };
        assert!(fits_the_wire(nested_list(49), ARGUMENT_DEPTH).is_ok());
        wire::CallFunctionArgs::decode(call(nested_list(49)).encode_to_vec().as_slice())
            .expect("an argument that fits decodes");
        assert!(fits_the_wire(nested_list(50), ARGUMENT_DEPTH).is_err());
        wire::CallFunctionArgs::decode(call(nested_list(50)).encode_to_vec().as_slice())
            .expect_err("an argument past the limit is refused");
    }

    /// The encoder stops descending past the deepest value the wire could
    /// carry, so a host value of any depth is refused without exhausting the
    /// stack.
    #[test]
    fn the_encoder_stops_at_the_level_bound() {
        fn descend(encoder: &mut Encoder, levels: usize) -> Result<usize, EncodeError> {
            if levels == 0 {
                return Ok(0);
            }
            encoder.nested(|encoder| descend(encoder, levels - 1).map(|reached| reached + 1))
        }
        let mut encoder = Encoder::new();
        assert_eq!(descend(&mut encoder, MAX_LEVELS), Ok(MAX_LEVELS));
        assert_eq!(
            descend(&mut encoder, 1_000_000),
            Err(EncodeError::TooDeep {
                limit: WIRE_NESTING_LIMIT
            })
        );
        // A refusal leaves the encoder at the level it was entered at.
        assert_eq!(descend(&mut encoder, MAX_LEVELS), Ok(MAX_LEVELS));
    }
}
