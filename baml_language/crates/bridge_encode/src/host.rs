//! The questions the walker asks a host value, and a language's answers.
//!
//! A bridge implements [`HostValue`] for its language's value type. The
//! walker never looks inside a host value itself: it asks, slot by slot, and
//! the answers carry the language's rules. A Python `int` asked for a `float`
//! answers [`Answer::Converted`]; a JavaScript `number` asked for an `int`
//! answers [`Answer::AsIs`] when it is integral and [`Answer::No`] otherwise.
//! The walker only needs to know that a value fits as it is, fits after the
//! language converted it, or does not fit.

use baml_type::{MediaKind, RuntimeTy, TypeName};
use bridge_ctypes::baml_bridge::cffi::InboundValue;
use num_bigint::BigInt;

use crate::facts::EnumFacts;

/// How a value relates to the slot it was asked about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Answer<T> {
    /// The value is a member of the slot's type as it is.
    AsIs(T),
    /// The language turned the value into a member of the slot's type.
    Converted(T),
    /// The value is not a member and the language does not convert it.
    No,
}

impl<T> Answer<T> {
    pub fn map<U>(self, f: impl FnOnce(T) -> U) -> Answer<U> {
        match self {
            Answer::AsIs(value) => Answer::AsIs(f(value)),
            Answer::Converted(value) => Answer::Converted(f(value)),
            Answer::No => Answer::No,
        }
    }
}

/// The scalar type a slot asks for. [`ScalarSlot::Open`] asks for the
/// scalar the value naturally is, when the slot fixes nothing.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ScalarSlot {
    Int,
    Bigint,
    Float,
    Bool,
    String,
    Bytes,
    Open,
}

/// A scalar as the wire carries it.
#[derive(Debug, Clone, PartialEq)]
pub enum Scalar {
    Int(i64),
    Bigint(BigInt),
    Float(f64),
    Bool(bool),
    String(String),
    Bytes(Vec<u8>),
}

/// A host object the host already knows how to put on the wire exactly: a
/// handle to an engine object, media, a prompt, a type value. The walker only
/// checks that the slot can hold that kind.
#[derive(Debug, Clone, PartialEq)]
pub struct Leaf {
    pub value: InboundValue,
    pub kind: LeafKind,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum LeafKind {
    /// A rooted engine object; the engine checks it live against the slot.
    Handle,
    Media(MediaKind),
    Prompt,
    TypeValue,
}

/// A host object that is an instance of a named class.
#[derive(Debug)]
pub struct Instance<V> {
    pub class: TypeName,
    /// The instance's concrete type arguments when the host knows them;
    /// `None` for a non-generic class or an instance that carries none.
    pub type_args: Option<Vec<RuntimeTy>>,
    /// Fields under their declared (wire) names.
    pub fields: Vec<(String, V)>,
}

/// A failure the host hit while answering (registering a callable, reading an
/// attribute). Rendered as given; keep it short.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct HostError(pub String);

/// A language's value, as the walker sees it.
///
/// Answers are per slot: the same value may answer [`Answer::AsIs`] for one
/// slot and [`Answer::Converted`] for another. Containers and instances are
/// not slot-directed; the walker descends into them with the slot's
/// structure.
pub trait HostValue: Sized {
    /// The value is the language's null.
    fn is_null(&self) -> bool;

    /// The value as the scalar `slot` asks for.
    fn scalar(&self, slot: ScalarSlot) -> Answer<Scalar>;

    /// The enum and variant this value is, when the language represents enum
    /// members as values of their own (a Python `Enum` member). Asked under
    /// an open slot, where no enum is expected in particular.
    fn enum_member(&self) -> Option<(TypeName, String)>;

    /// The variant of `enumeration` this value denotes, by variant name.
    /// [`Answer::AsIs`] for the language's own representation of that enum,
    /// [`Answer::Converted`] when the language lets another value (a string)
    /// name a variant. The default accepts only the value's own
    /// [`enum_member`](Self::enum_member).
    fn enum_variant(&self, enumeration: &EnumFacts) -> Answer<String> {
        match self.enum_member() {
            Some((name, variant)) if name == enumeration.name => Answer::AsIs(variant),
            Some(_) | None => Answer::No,
        }
    }

    /// The elements, when the value is a sequence.
    fn list(&self) -> Option<Vec<Self>>;

    /// The entries, when the value is a mapping. Keys are host values; the
    /// walker encodes them against the slot's key type. A mapping with string
    /// keys can also stand in for a class.
    fn map(&self) -> Option<Vec<(Self, Self)>>;

    /// The value as an instance of a named class, when it is one.
    fn instance(&self) -> Option<Instance<Self>>;

    /// The value is a function the engine could invoke. Asked under a slot
    /// that is no function type, where a callable has no type to take.
    fn is_callable(&self) -> bool;

    /// Register the value as a callable the engine may invoke, bound to the
    /// slot's function type `signature` (always a [`RuntimeTy::Function`]),
    /// and return its wire handle. `None` when the value is not callable. The
    /// host records `signature` so the callable's result can later be encoded
    /// against its return type.
    fn callable(&self, signature: &RuntimeTy) -> Option<Result<InboundValue, HostError>>;

    /// The value as a wire leaf the host owns outright, when it is one.
    fn leaf(&self) -> Option<Result<Leaf, HostError>>;

    /// A short, bounded rendering for error messages: the host type and at
    /// most a few dozen characters of the value.
    fn describe(&self) -> String;
}
