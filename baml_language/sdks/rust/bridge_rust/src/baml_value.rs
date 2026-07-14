//! The bidirectional value-conversion boundary between Rust and BAML.
//!
//! [`BamlValue`] is the single public trait: one bound covers both
//! directions (host → engine arguments, engine → host results).
//!
//! Decode is driven by the *expected static type*: the impl on a declared
//! return type interprets the wire value, and mismatches are loud
//! [`DecodeError`]s. There is no runtime typemap in the Rust SDK.

use std::convert::Infallible;

use num_bigint::BigInt;

use crate::{
    DecodeError,
    decode::unwrap,
    wire::{self, baml_outbound_value::Value as Out, inbound_value::Value as In},
};

/// Implemented by types that can be converted to and from BAML
/// across the FFI boundary.
///
/// Should only be implemented in the generated SDK and `baml_bridge`;
/// implementing [`internal::__BamlValuePrivate`] yields this trait via a
/// blanket impl.
pub trait BamlValue: internal::__BamlValuePrivate {}

impl<T: internal::__BamlValuePrivate> BamlValue for T {}

#[doc(hidden)]
pub mod internal {
    use crate::{DecodeError, wire};

    /// Public only to allow generated SDK to reference it.
    /// Do NOT implement it yourself.
    ///
    /// Carries the actual conversion methods so the wire types stay off
    /// the public [`BamlValue`](super::BamlValue) trait.
    pub trait __BamlValuePrivate: Sized {
        /// Encode as an inbound (host → engine) wire value.
        fn to_baml(&self) -> wire::InboundValue;
        /// Decode from an outbound (engine → host) wire value.
        ///
        /// Implementations must tolerate `union_variant` / `literal`
        /// envelopes by peeling with [`crate::decode::unwrap`] first —
        /// the provided impls and generated impls all do.
        fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError>;
    }
}

use internal::__BamlValuePrivate;

/// Shorthand for building an inbound value from a oneof arm.
fn inbound(value: In) -> wire::InboundValue {
    wire::InboundValue { value: Some(value) }
}

/// The inbound null value (absent oneof = null on the wire).
fn inbound_null() -> wire::InboundValue {
    wire::InboundValue { value: None }
}

/// Bounded name of the wire variant that arrived, for `WrongType` errors.
pub(crate) fn wire_variant_kind(v: &wire::BamlOutboundValue) -> &'static str {
    match &v.value {
        None => "null",
        Some(Out::NullValue(_)) => "null",
        Some(Out::StringValue(_)) => "string",
        Some(Out::IntValue(_)) => "int",
        Some(Out::FloatValue(_)) => "float",
        Some(Out::BoolValue(_)) => "bool",
        Some(Out::ClassValue(_)) => "class",
        Some(Out::EnumValue(_)) => "enum",
        Some(Out::LiteralValue(_)) => "literal",
        Some(Out::ListValue(_)) => "list",
        Some(Out::MapValue(_)) => "map",
        Some(Out::UnionVariantValue(_)) => "union variant",
        Some(Out::HandleValue(_)) => "handle",
        Some(Out::MediaValue(_)) => "media",
        Some(Out::PromptAstValue(_)) => "prompt ast",
        Some(Out::Uint8arrayValue(_)) => "uint8array",
        Some(Out::BigintValue(_)) => "bigint",
        Some(Out::TyValue(_)) => "type",
    }
}

fn wrong_type(expected: &'static str, v: &wire::BamlOutboundValue) -> DecodeError {
    DecodeError::WrongType {
        expected,
        got: wire_variant_kind(v),
    }
}

impl __BamlValuePrivate for i64 {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::IntValue(*self))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::IntValue(i)) => Ok(i),
            _ => Err(wrong_type("int", &v)),
        }
    }
}

impl __BamlValuePrivate for f64 {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::FloatValue(*self))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::FloatValue(f)) => Ok(f),
            _ => Err(wrong_type("float", &v)),
        }
    }
}

impl __BamlValuePrivate for bool {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::BoolValue(*self))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::BoolValue(b)) => Ok(b),
            _ => Err(wrong_type("bool", &v)),
        }
    }
}

impl __BamlValuePrivate for String {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::StringValue(self.clone()))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::StringValue(s)) => Ok(s),
            _ => Err(wrong_type("string", &v)),
        }
    }
}

/// BAML `null` and `void` both map to `()`: null rides the wire as an
/// absent value, and a void function returns null.
impl __BamlValuePrivate for () {
    fn to_baml(&self) -> wire::InboundValue {
        inbound_null()
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            None | Some(Out::NullValue(_)) => Ok(()),
            _ => Err(wrong_type("null", &v)),
        }
    }
}

/// BAML `uint8array`. There is deliberately no `u8: BamlValue` impl, so
/// this stays coherent with the element-wise `Vec<T>` impl below.
impl __BamlValuePrivate for Vec<u8> {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::Uint8arrayValue(self.clone()))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::Uint8arrayValue(bytes)) => Ok(bytes),
            _ => Err(wrong_type("uint8array", &v)),
        }
    }
}

/// BAML `bigint` rides the wire as a sign-preserving base sixteen string
/// (the engine's own encoding; power-of-two-base parsing is
/// SIMD-friendly).
impl __BamlValuePrivate for BigInt {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::BigintValue(format!("{self:x}")))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::BigintValue(s)) => BigInt::parse_bytes(s.as_bytes(), 16)
                .ok_or(DecodeError::InvalidBigint { len: s.len() }),
            _ => Err(wrong_type("bigint", &v)),
        }
    }
}

/// BAML `T?`: `None` is the explicit null value.
impl<T: __BamlValuePrivate> __BamlValuePrivate for Option<T> {
    fn to_baml(&self) -> wire::InboundValue {
        match self {
            None => inbound_null(),
            Some(v) => v.to_baml(),
        }
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            None | Some(Out::NullValue(_)) => Ok(None),
            _ => T::from_baml(v).map(Some),
        }
    }
}

/// BAML `T[]`.
impl<T: __BamlValuePrivate> __BamlValuePrivate for Vec<T> {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::ListValue(wire::InboundListValue {
            values: self.iter().map(__BamlValuePrivate::to_baml).collect(),
        }))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::ListValue(list)) => list.items.into_iter().map(T::from_baml).collect(),
            _ => Err(wrong_type("list", &v)),
        }
    }
}

/// A key of a BAML `map`. The language currently restricts map keys to
/// strings (E0067), so [`String`] is the only impl; the trait is the
/// extension point should richer key types land. Like [`BamlValue`] it is
/// bidirectional, but keys have their own wire shape: a typed key oneof
/// inbound, a stringified key outbound.
pub trait BamlMapKey: Sized + Eq + std::hash::Hash {
    /// Encode as an inbound map-entry key.
    fn to_baml_key(&self) -> wire::inbound_map_entry::Key;
    /// Decode from an outbound (stringified) map key.
    fn from_baml_key(key: &str) -> Result<Self, DecodeError>;
}

impl BamlMapKey for String {
    fn to_baml_key(&self) -> wire::inbound_map_entry::Key {
        wire::inbound_map_entry::Key::StringKey(self.clone())
    }

    fn from_baml_key(key: &str) -> Result<Self, DecodeError> {
        Ok(key.to_string())
    }
}

/// BAML `map<K, V>`. [`crate::Map`] (`IndexMap`) is the declared type in
/// generated signatures — it preserves the engine's entry order.
impl<K: BamlMapKey, V: __BamlValuePrivate> __BamlValuePrivate for indexmap::IndexMap<K, V> {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::MapValue(wire::InboundMapValue {
            entries: self
                .iter()
                .map(|(k, v)| wire::InboundMapEntry {
                    key: Some(k.to_baml_key()),
                    value: Some(v.to_baml()),
                })
                .collect(),
        }))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::MapValue(map)) => map
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        K::from_baml_key(&entry.key)?,
                        V::from_baml(entry.value.unwrap_or_default())?,
                    ))
                })
                .collect(),
            _ => Err(wrong_type("map", &v)),
        }
    }
}

/// `HashMap` convenience: accepted anywhere a BAML `map` is expected, at
/// the cost of dropping the engine's entry order. Declared types in
/// generated code are always [`crate::Map`].
impl<K: BamlMapKey, V: __BamlValuePrivate> __BamlValuePrivate for std::collections::HashMap<K, V> {
    fn to_baml(&self) -> wire::InboundValue {
        inbound(In::MapValue(wire::InboundMapValue {
            entries: self
                .iter()
                .map(|(k, v)| wire::InboundMapEntry {
                    key: Some(k.to_baml_key()),
                    value: Some(v.to_baml()),
                })
                .collect(),
        }))
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        let v = unwrap(v);
        match v.value {
            Some(Out::MapValue(map)) => map
                .entries
                .into_iter()
                .map(|entry| {
                    Ok((
                        K::from_baml_key(&entry.key)?,
                        V::from_baml(entry.value.unwrap_or_default())?,
                    ))
                })
                .collect(),
            _ => Err(wrong_type("map", &v)),
        }
    }
}

/// Forwarding impl for boxed values: recursive class fields are boxed at
/// cycle sites by codegen, and the emitted field conversions go through
/// this impl unchanged.
impl<T: __BamlValuePrivate> __BamlValuePrivate for Box<T> {
    fn to_baml(&self) -> wire::InboundValue {
        self.as_ref().to_baml()
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        T::from_baml(v).map(Box::new)
    }
}

/// [`Infallible`] occupies the `throws` slot of contract-free functions;
/// no value ever decodes as it (that is the point), and encoding one is
/// statically impossible.
impl __BamlValuePrivate for Infallible {
    fn to_baml(&self) -> wire::InboundValue {
        match *self {}
    }

    fn from_baml(v: wire::BamlOutboundValue) -> Result<Self, DecodeError> {
        Err(wrong_type("never", &v))
    }
}

/// Three-state wrapper for arguments with BAML defaults: [`Unset`] omits
/// the argument entirely so the engine evaluates the default — distinct
/// from an explicit null (`Set(None)` on an optional-typed parameter).
///
/// Generated functions accept `impl Into<OptionalArg<T>>`, so call sites
/// pass a plain value, `None` for explicit null, or `Unset`.
///
/// [`Unset`]: OptionalArg::Unset
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OptionalArg<T> {
    /// Omit the argument; the engine applies the BAML default.
    Unset,
    /// Pass the wrapped value.
    Set(T),
}

impl<T> From<T> for OptionalArg<T> {
    fn from(value: T) -> Self {
        OptionalArg::Set(value)
    }
}

impl<T: BamlValue> OptionalArg<T> {
    /// Encode for the kwargs builder: `None` means "omit this argument".
    pub fn to_baml_opt(&self) -> Option<wire::InboundValue> {
        match self {
            OptionalArg::Unset => None,
            OptionalArg::Set(v) => Some(v.to_baml()),
        }
    }
}
