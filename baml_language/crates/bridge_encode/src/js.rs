//! JavaScript's answers to the walker, shared by every JavaScript bridge.
//!
//! Node (napi) and the Web (wasm-bindgen) read JavaScript values through
//! different APIs but the language is one, so the reading lives here once,
//! over the small [`JsView`] a bridge implements for its value type:
//!
//! - `null` and `undefined` are null.
//! - A `number` is a double ([`Scalar::Float`]); the language's rules
//!   ([`Rules::JavaScript`]) make an integral one an `int` where an `int` is
//!   asked for. A `bigint` is a `bigint`.
//! - A `string` is a string; in an enum slot it names a variant, by
//!   conversion, because a generated TypeScript enum member is a plain string.
//! - An `Array` is a list; a `Uint8Array` is bytes.
//! - A plain object (prototype `Object.prototype` or none) is a map of its own
//!   enumerable entries and stands in for a class when the slot asks for one;
//!   an object whose constructor the typemap knows is an instance of that
//!   class, with its own enumerable fields minus `$types`; any
//!   other object (a `Map`, a `Date`, a `Promise`) has no BAML counterpart.
//! - A function is a host callable, bound to its slot at commit, unless the
//!   runtime knows it as a BAML closure, which crosses as its handle.
//! - Handles, media, prompts and type values are leaves the runtime
//!   serializes itself, at commit.

use std::borrow::Cow;

use baml_type::TypeName;
use num_bigint::BigInt;

use crate::{
    host::{HostShape, HostValue, LeafKind, Scalar, StatedTy},
    rules::Rules,
};

/// What a JavaScript value is, as the bridge's value API reports it.
#[derive(Debug, Clone, PartialEq)]
pub enum JsKind {
    /// `null` or `undefined`.
    Null,
    Bool(bool),
    Number(f64),
    BigInt(BigInt),
    /// A `string`; its text is read through [`JsView::text`].
    String,
    Array,
    /// A `Uint8Array` (a Node `Buffer` is one); read through [`JsView::bytes`].
    Bytes,
    Function,
    /// A plain object: prototype `Object.prototype` or none.
    PlainObject,
    /// An instance of a generated class the typemap knows, by its BAML name.
    Instance(TypeName),
    /// A value the runtime serializes itself.
    Leaf(LeafKind),
    /// Any other object, a `symbol`, or a value the API cannot classify.
    Other,
}

/// The elements of a JavaScript array, one at a time as the walker pulls them.
pub type JsElements<'a, J> = Box<dyn Iterator<Item = Result<J, <J as JsView>::Error>> + 'a>;

/// The own enumerable entries of a JavaScript object, one at a time.
pub type JsEntries<'a, J> =
    Box<dyn Iterator<Item = Result<(String, J), <J as JsView>::Error>> + 'a>;

/// A JavaScript value as one bridge's API exposes it. Every method reads;
/// the effects of shipping a value (registering a function, cloning a handle,
/// serializing a leaf's payload) belong to the bridge's [`Host`](crate::Host).
pub trait JsView: Sized {
    /// The bridge's error type for a read that failed (`napi::Error`, `JsValue`).
    type Error;

    fn kind(&self) -> Result<JsKind, Self::Error>;

    /// The elements of an `Array`, one at a time as the walker pulls them: a
    /// sparse array's `length` is never trusted to size anything.
    fn elements(&self) -> Result<JsElements<'_, Self>, Self::Error>;

    /// The own enumerable string-keyed entries of an object, in insertion
    /// order, as `Object.entries` gives them.
    fn entries(&self) -> Result<JsEntries<'_, Self>, Self::Error>;

    /// The concrete type arguments a generic instance carries in `$types`,
    /// when it carries all of them.
    fn type_args(&self) -> Result<Option<Vec<StatedTy>>, Self::Error>;

    /// The text of a `string`. The engine's APIs copy a JavaScript string
    /// into Rust, so this is owned.
    fn text(&self) -> Result<Cow<'_, str>, Self::Error>;

    /// The bytes of a `Uint8Array`.
    fn bytes(&self) -> Result<Cow<'_, [u8]>, Self::Error>;

    /// A short, bounded rendering for error messages.
    fn describe(&self) -> String;

    /// A string value in this value's realm: a map's keys are host values.
    #[must_use]
    fn string(&self, value: String) -> Self;
}

/// A JavaScript value with JavaScript's rules.
pub struct Js<J>(pub J);

impl<J: JsView> HostValue for Js<J> {
    type Error = J::Error;

    const RULES: Rules = Rules::JavaScript;

    fn shape(&self) -> Result<HostShape<'_, Self>, J::Error> {
        Ok(match self.0.kind()? {
            JsKind::Null => HostShape::Null,
            JsKind::Bool(bool) => HostShape::Scalar(Scalar::Bool(bool)),
            JsKind::Number(number) => HostShape::Scalar(Scalar::Float(number)),
            JsKind::BigInt(big) => HostShape::Scalar(Scalar::BigInt(big)),
            JsKind::String => HostShape::Scalar(Scalar::String),
            JsKind::Bytes => HostShape::Scalar(Scalar::Bytes),
            JsKind::Array => HostShape::List(Box::new(self.0.elements()?.map(|item| item.map(Js)))),
            JsKind::PlainObject => {
                HostShape::Map(Box::new(self.0.entries()?.map(|entry| {
                    entry.map(|(key, value)| (Js(self.0.string(key)), Js(value)))
                })))
            }
            JsKind::Instance(class) => {
                let type_args = self.0.type_args()?;
                // Own enumerable entries are the instance's fields (methods
                // live on the prototype); `$types` is its type arguments.
                let fields = self
                    .0
                    .entries()?
                    .filter(|entry| !matches!(entry, Ok((name, _)) if name == "$types"))
                    .map(|entry| entry.map(|(name, value)| (name, Js(value))));
                HostShape::Instance {
                    class,
                    type_args,
                    fields: Box::new(fields),
                }
            }
            JsKind::Function => HostShape::Callable,
            JsKind::Leaf(kind) => HostShape::Leaf(kind),
            JsKind::Other => HostShape::Other,
        })
    }

    fn text(&self) -> Result<Cow<'_, str>, J::Error> {
        self.0.text()
    }

    fn bytes(&self) -> Result<Cow<'_, [u8]>, J::Error> {
        self.0.bytes()
    }

    fn describe(&self) -> String {
        self.0.describe()
    }
}
