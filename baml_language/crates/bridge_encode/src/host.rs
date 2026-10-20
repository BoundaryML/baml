//! What a host value is, and what a host does for a value it ships.
//!
//! A bridge implements [`HostValue`] for its language's value type and
//! [`Host`] for its encode context. The walker never looks inside a host
//! value itself: it asks [`HostValue::shape`] once per value, which is the
//! only moment host code runs, and the host's language-specific judgments
//! live in one table, [`Rules`]. Everything a host *does* for the wire
//! (registering a callable, cloning a handle, copying a media payload)
//! happens through [`Host`], and only once the walk has decided what ships.

use std::borrow::Cow;

use baml_type::{Int63, MediaKind, RuntimeTy, TypeName};
use bridge_ctypes::{
    HostKey, TableKey,
    baml_bridge::cffi::{BamlHandle, BamlTyDef, BamlValueMedia, BamlValuePromptAst},
};
use num_bigint::BigInt;

use crate::rules::Rules;

/// A reference to a declaration (class, enum, interface, alias). A name for
/// now; the identity work replaces this one type with the declaration's index
/// or handle, so nothing else in the crate spells a head as text.
pub type Head = TypeName;

/// A language's value, as the walker sees it.
pub trait HostValue: Sized {
    /// The host's own exception type (`PyErr`, `napi::Error`, `JsValue`),
    /// raised to the caller as it is.
    type Error;

    /// The language whose conversions apply to this value.
    const RULES: Rules;

    /// What the value is, one level deep, in the language's terms. Asked
    /// exactly once per value: this is where host code runs (iteration,
    /// attribute access, getters).
    fn shape(&self) -> Result<Shape<Self>, Self::Error>;

    /// The text of a [`Scalar::String`] value. Borrowed where the host object
    /// is immutable UTF-8 (a Python `str`), owned where the API or the
    /// boundary forces a copy. Asked only to compare, and once to write the
    /// wire.
    fn text(&self) -> Result<Cow<'_, str>, Self::Error>;

    /// The bytes of a [`Scalar::Bytes`] value, likewise.
    fn bytes(&self) -> Result<Cow<'_, [u8]>, Self::Error>;

    /// A short rendering for an error message: the host type and at most a
    /// few dozen characters of the value. Bounded in work as well as length,
    /// and asked only when an error is rendered.
    fn describe(&self) -> String;
}

/// What a host value is. Exactly one answer per value; `C` is the type of
/// its children (the host's own value type when the host answers, the read
/// snapshot once the walker holds it).
#[derive(Debug, Clone, PartialEq)]
pub enum Shape<C> {
    Null,
    Scalar(Scalar),
    /// A member of an enum the language represents as a value of its own (a
    /// Python `Enum` member). JavaScript has none: a generated enum member is
    /// a plain string.
    EnumMember {
        enumeration: Head,
        variant: String,
    },
    List(Vec<C>),
    /// Entries in the host's order; keys are host values too.
    Map(Vec<(C, C)>),
    /// An instance of a named class. `type_args` are the arguments the value
    /// knows (a pydantic `Box[int]`, a `$types` record); `None` when it knows
    /// only its head.
    Instance {
        class: Head,
        type_args: Option<Vec<RuntimeTy>>,
        /// Fields under their declared (wire) names.
        fields: Vec<(String, C)>,
    },
    /// A function the engine could invoke.
    Callable,
    /// A value the host already knows how to put on the wire exactly.
    Leaf(LeafKind),
    /// No BAML counterpart: a symbol, a set, an arbitrary object.
    Other,
}

/// A scalar value. The host classifies by the VM's integer range: an integer
/// beyond `Int63` is a `BigInt`. String and byte payloads stay in the host
/// object until the wire is written ([`HostValue::text`], [`HostValue::bytes`]).
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Bool(bool),
    Int(Int63),
    BigInt(BigInt),
    Float(f64),
    String,
    Bytes,
}

/// A host object the host serializes itself; the walker only checks that the
/// slot can hold that kind, and the payload is fetched at commit.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    /// A rooted engine object; the engine checks it live against the slot.
    Handle,
    Media(MediaKind),
    Prompt,
    TypeValue,
}

/// What a host creates for the wire and still holds until the engine takes
/// it over. Released if the request never reaches the engine.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Owner {
    /// A host callable the host registered under this key.
    Callable(HostKey),
    /// An engine reference the host cloned under this key.
    Handle(TableKey),
}

/// A leaf as the wire carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum LeafWire {
    Handle(BamlHandle),
    Media(BamlValueMedia),
    Prompt(BamlValuePromptAst),
    TypeValue(BamlTyDef),
}

/// The effects of shipping a value. Only the commit of a decided plan calls
/// these, each exactly once per shipped node.
pub trait Host<V: HostValue> {
    /// Register `callable` as a host callable bound to `signature` (always a
    /// `RuntimeTy::Function`) and return its registry key.
    fn register(&self, callable: &V, signature: &RuntimeTy) -> Result<HostKey, V::Error>;

    /// The wire form of a leaf, and the owner created for it if any (a
    /// handle clone).
    fn leaf(&self, leaf: &V, kind: LeafKind) -> Result<(LeafWire, Option<Owner>), V::Error>;

    /// Let go of an owner the engine will never take over.
    fn release(&self, owner: Owner);
}
