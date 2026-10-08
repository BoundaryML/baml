//! JavaScript's answers to the walker, shared by every JavaScript bridge.
//!
//! Node (napi) and the Web (wasm-bindgen) read JavaScript values through
//! different APIs but the language is one, so the rules live here once, over
//! the small [`JsView`] a bridge implements for its value type:
//!
//! - `null` and `undefined` are null.
//! - A `number` is a double. It fits an `int` slot as it is when it is
//!   integral and in range, and a `float` slot as it is when it is not
//!   integral and by conversion when it is (so `int | float` takes an
//!   integral number as an `int`, and `float | bigint` as a `float`). It
//!   never fits a `bigint` slot: JavaScript has a `bigint` type for that.
//!   Under an open slot an integral number is an `int` and any other a
//!   `float`.
//! - A `bigint` is a `bigint`; it fits an `int` slot by conversion when it
//!   fits the VM's integers.
//! - A `symbol` or any other value JavaScript has no BAML counterpart for
//!   fits nothing.
//! - A `string` is a `string`; in an enum slot it names a variant, by
//!   conversion, because a generated TypeScript enum member is a plain string.
//! - An `Array` is a list; a `Uint8Array` is bytes.
//! - An object whose constructor the typemap knows is an instance of that
//!   class, with its own enumerable fields minus functions and `$types`; any
//!   other object is a map of its own enumerable entries and stands in for a
//!   class when the slot asks for one.
//! - A function is a host callable bound to its slot, unless the runtime
//!   knows it as a BAML closure, which crosses as its handle.
//! - Handles, media, prompts and type values are leaves the runtime
//!   serializes itself.

use baml_type::{Int63, RuntimeTy, TypeName};
use bridge_ctypes::baml_bridge::cffi::InboundValue;
use num_bigint::BigInt;

use crate::{
    facts::EnumFacts,
    host::{Answer, HostError, HostValue, Instance, Leaf, Scalar, ScalarSlot},
};

/// What a JavaScript value is, as the bridge's value API reports it.
#[derive(Debug, Clone, PartialEq)]
pub enum JsKind {
    /// `null` or `undefined`.
    Null,
    Bool(bool),
    Number(f64),
    BigInt(BigInt),
    String(String),
    Array,
    /// A `Uint8Array` (a Node `Buffer` is one).
    Bytes(Vec<u8>),
    Function,
    /// Any other object.
    Object,
    /// A `symbol`, or a value the bridge's API cannot classify.
    Other,
}

/// A JavaScript value as one bridge's API exposes it.
pub trait JsView: Sized {
    fn kind(&self) -> Result<JsKind, HostError>;

    /// The elements of an `Array`.
    fn elements(&self) -> Result<Vec<Self>, HostError>;

    /// The own enumerable string-keyed entries of an object, in insertion
    /// order, as `Object.entries` gives them.
    fn entries(&self) -> Result<Vec<(String, Self)>, HostError>;

    /// The BAML class a generated-class instance is an instance of, by its
    /// constructor; `None` for any other object.
    fn class_name(&self) -> Result<Option<TypeName>, HostError>;

    /// The concrete type arguments a generic instance carries in `$types`,
    /// when it carries all of them.
    fn type_args(&self) -> Result<Option<Vec<RuntimeTy>>, HostError>;

    /// The wire leaf this value is, when the runtime owns its serialization:
    /// a handle (cloned for the wire), media, a prompt, a type value, or a
    /// BAML closure's handle.
    fn leaf(&self) -> Result<Option<Leaf>, HostError>;

    /// Register a function as a host callable bound to `signature` (the
    /// slot's function type, or `None` under an open slot) and return the
    /// wire handle.
    fn register_callable(&self, signature: Option<&RuntimeTy>) -> Result<InboundValue, HostError>;

    /// A short, bounded rendering for error messages.
    fn describe(&self) -> String;

    /// A string value in this value's realm: the walker encodes map keys as
    /// host values.
    #[must_use]
    fn string(&self, value: String) -> Self;
}

/// A JavaScript value with JavaScript's rules.
pub struct Js<J>(pub J);

impl<J: JsView> Js<J> {
    fn kind_or_no(&self) -> Option<JsKind> {
        self.0.kind().ok()
    }
}

/// `number` as the slot asks.
fn number(value: f64, slot: ScalarSlot) -> Answer<Scalar> {
    let integral = value.fract() == 0.0 && value.is_finite();
    let as_int = integral.then(|| exact_int(value)).flatten();
    match slot {
        ScalarSlot::Int => match as_int {
            Some(int) => Answer::AsIs(Scalar::Int(int)),
            None => Answer::No,
        },
        // An integral number is an `int` first; a `float` slot still takes
        // it, by conversion, so `int | float` prefers `int`.
        ScalarSlot::Float => {
            if integral {
                Answer::Converted(Scalar::Float(value))
            } else {
                Answer::AsIs(Scalar::Float(value))
            }
        }
        ScalarSlot::Open => match as_int {
            Some(int) => Answer::AsIs(Scalar::Int(int)),
            None => Answer::AsIs(Scalar::Float(value)),
        },
        // A `number` is a double; JavaScript's `bigint` is a different type.
        ScalarSlot::Bigint | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes => {
            Answer::No
        }
    }
}

/// An integral double as a VM integer, when the double is exactly one.
#[expect(
    clippy::cast_precision_loss,
    clippy::cast_possible_truncation,
    clippy::float_cmp,
    reason = "the round trip checks the cast was exact"
)]
fn exact_int(value: f64) -> Option<i64> {
    let int = value as i64;
    (int as f64 == value && Int63::new(int).is_some()).then_some(int)
}

/// `bigint` as the slot asks.
fn bigint(value: BigInt, slot: ScalarSlot) -> Answer<Scalar> {
    match slot {
        ScalarSlot::Bigint | ScalarSlot::Open => Answer::AsIs(Scalar::Bigint(value)),
        ScalarSlot::Int => match i64::try_from(&value) {
            Ok(int) if Int63::new(int).is_some() => Answer::Converted(Scalar::Int(int)),
            Ok(_) | Err(_) => Answer::No,
        },
        ScalarSlot::Float | ScalarSlot::Bool | ScalarSlot::String | ScalarSlot::Bytes => Answer::No,
    }
}

/// A value that is exactly one BAML scalar: it fits that slot or an open one.
fn only(slot: ScalarSlot, kind: ScalarSlot, scalar: Scalar) -> Answer<Scalar> {
    if slot == kind || slot == ScalarSlot::Open {
        Answer::AsIs(scalar)
    } else {
        Answer::No
    }
}

impl<J: JsView> HostValue for Js<J> {
    fn is_null(&self) -> bool {
        matches!(self.kind_or_no(), Some(JsKind::Null))
    }

    fn scalar(&self, slot: ScalarSlot) -> Answer<Scalar> {
        match self.kind_or_no() {
            Some(JsKind::Bool(bool)) => only(slot, ScalarSlot::Bool, Scalar::Bool(bool)),
            Some(JsKind::Number(value)) => number(value, slot),
            Some(JsKind::BigInt(value)) => bigint(value, slot),
            Some(JsKind::String(string)) => only(slot, ScalarSlot::String, Scalar::String(string)),
            Some(JsKind::Bytes(bytes)) => only(slot, ScalarSlot::Bytes, Scalar::Bytes(bytes)),
            Some(
                JsKind::Null | JsKind::Array | JsKind::Function | JsKind::Object | JsKind::Other,
            )
            | None => Answer::No,
        }
    }

    fn enum_member(&self) -> Option<(TypeName, String)> {
        // A generated enum member is a plain string; nothing says which enum.
        None
    }

    fn enum_variant(&self, _enumeration: &EnumFacts) -> Answer<String> {
        match self.kind_or_no() {
            Some(JsKind::String(string)) => Answer::Converted(string),
            Some(_) | None => Answer::No,
        }
    }

    fn list(&self) -> Option<Vec<Self>> {
        match self.kind_or_no()? {
            JsKind::Array => self
                .0
                .elements()
                .ok()
                .map(|items| items.into_iter().map(Js).collect()),
            _ => None,
        }
    }

    fn map(&self) -> Option<Vec<(Self, Self)>> {
        // A plain object is a map; an instance of a known class is not.
        match self.kind_or_no()? {
            JsKind::Object if self.0.class_name().ok().flatten().is_none() => {
                self.0.entries().ok().map(|entries| {
                    entries
                        .into_iter()
                        .map(|(key, value)| (Js(self.0.string(key)), Js(value)))
                        .collect()
                })
            }
            _ => None,
        }
    }

    fn instance(&self) -> Option<Instance<Self>> {
        let class = self.0.class_name().ok().flatten()?;
        let fields = self
            .0
            .entries()
            .ok()?
            .into_iter()
            .filter(|(name, value)| {
                name != "$types" && !matches!(value.kind(), Ok(JsKind::Function))
            })
            .map(|(name, value)| (name, Js(value)))
            .collect();
        Some(Instance {
            class,
            type_args: self.0.type_args().ok().flatten(),
            fields,
        })
    }

    fn callable(&self, signature: &RuntimeTy) -> Option<Result<InboundValue, HostError>> {
        match self.kind_or_no()? {
            JsKind::Function => {
                let signature =
                    matches!(signature, RuntimeTy::Function { .. }).then_some(signature);
                Some(self.0.register_callable(signature))
            }
            _ => None,
        }
    }

    fn leaf(&self) -> Option<Result<Leaf, HostError>> {
        self.0.leaf().transpose()
    }

    fn describe(&self) -> String {
        self.0.describe()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numbers_follow_javascripts_rules() {
        assert_eq!(number(3.0, ScalarSlot::Int), Answer::AsIs(Scalar::Int(3)));
        assert_eq!(number(3.5, ScalarSlot::Int), Answer::No);
        assert_eq!(
            number(3.0, ScalarSlot::Float),
            Answer::Converted(Scalar::Float(3.0))
        );
        assert_eq!(
            number(3.5, ScalarSlot::Float),
            Answer::AsIs(Scalar::Float(3.5))
        );
        assert_eq!(number(3.0, ScalarSlot::Open), Answer::AsIs(Scalar::Int(3)));
        assert_eq!(
            number(3.5, ScalarSlot::Open),
            Answer::AsIs(Scalar::Float(3.5))
        );
        // A number is never a bigint, so `float | bigint` takes it as a float.
        assert_eq!(number(3.0, ScalarSlot::Bigint), Answer::No);
        assert_eq!(number(f64::NAN, ScalarSlot::Int), Answer::No);
        assert_eq!(
            number(f64::INFINITY, ScalarSlot::Open),
            Answer::AsIs(Scalar::Float(f64::INFINITY))
        );
        // Beyond the VM's integers an integral double is still only a float.
        assert_eq!(number(1e19, ScalarSlot::Int), Answer::No);
        assert_eq!(
            number(1e19, ScalarSlot::Open),
            Answer::AsIs(Scalar::Float(1e19))
        );
        assert_eq!(number(3.0, ScalarSlot::String), Answer::No);
    }

    #[test]
    fn bigints_fit_int_slots_when_they_fit() {
        assert_eq!(
            bigint(BigInt::from(7), ScalarSlot::Int),
            Answer::Converted(Scalar::Int(7))
        );
        assert_eq!(
            bigint(BigInt::from(1u128 << 70), ScalarSlot::Int),
            Answer::No
        );
        assert_eq!(
            bigint(BigInt::from(7), ScalarSlot::Open),
            Answer::AsIs(Scalar::Bigint(BigInt::from(7)))
        );
        assert_eq!(bigint(BigInt::from(7), ScalarSlot::Float), Answer::No);
    }
}
