//! Type-directed encoding of host values for the engine's inbound wire.
//!
//! A host hands the engine a value for a slot the program declares: a
//! parameter, a class field, an element, a callback's result. This crate
//! walks the host value against that declared type and produces the exact
//! wire value the slot takes, so the engine only has to check and hydrate it.
//! Every conversion a language allows (an `int` where a `float` is declared, a
//! string naming an enum variant, a dictionary standing in for a class) is
//! decided here, on the host's side, by the host's own rules.
//!
//! The walk is the same for every language. What differs is how a language's
//! values answer the walker's questions, and that is the [`HostValue`] trait a
//! bridge implements for its value type. The declared types come from the
//! generated SDK's [`ProgramDescriptor`](baml_sdkgen_types::ProgramDescriptor),
//! read once into [`Facts`].
//!
//! Where a value's type is not fixed by its slot (an `unknown`, a union, an
//! unbound type variable) the wire states it: a list or map node under such a
//! slot carries a `value_type`, and the engine takes that type as the node's.
//! A class node always carries one, since the wire's `class_value` has no
//! name of its own and every typed SDK states it the same way. Scalars, nulls,
//! enums and handles are self-evident and never carry one. That is the whole
//! contract between this encoder and the engine.

mod encode;
mod error;
mod facts;
mod host;

pub use encode::{Encoded, Fit, encode, encode_arguments};
pub use error::{EncodeError, EncodeErrorKind, PathSegment};
pub use facts::{ClassFacts, EnumFacts, Facts, FunctionSig, Param};
pub use host::{Answer, HostError, HostValue, Instance, Leaf, LeafKind, Scalar, ScalarSlot};
