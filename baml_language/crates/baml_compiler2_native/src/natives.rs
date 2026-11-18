//! The standard library builtins written in Rust, and how compiled code
//! calls the native runtime's implementation of each.
//!
//! The table is generated from the standard library's declarations and the
//! runtime's lists of what it implements, so a builtin the runtime has no
//! implementation of is known here by name and reported as a blocker rather
//! than left for the linker to miss.

use crate::{Blocker, types::NativeTy};

/// A type a native takes or returns.
#[derive(Debug)]
pub(crate) enum Ty {
    Int,
    Float,
    Bool,
    Null,
    String,
    Array(&'static Ty),
    /// A map from the builtin's first type parameter to its second.
    Map,
    /// A type or `null`.
    Optional(&'static Ty),
    /// The type a call names for the builtin's type parameter at this
    /// position: passed and returned as the one word an array holds it in.
    Elem(usize),
    /// A value of any type, as a union holds it: a `json`.
    Any,
    /// An `unknown` parameter of `baml.json`: the id of the description
    /// of the argument's static type, then the value as a union holds it.
    Typed,
    /// The type a call names for the type parameter at this position, as a
    /// union holds it: what `baml.json` returns a value it decoded as.
    BoxedElem(usize),
}

impl Ty {
    /// Whether a value is an optional `int`, `float` or `bool`: two words,
    /// its bits and whether there is a value.
    pub(crate) fn is_pair(&self) -> bool {
        matches!(
            self,
            Self::Optional(Self::Int | Self::Float | Self::Bool | Self::Null)
        )
    }

    /// Whether a value is an optional value of a type parameter, which a
    /// native returns as two words whatever the type: the value's word, and
    /// whether there is a value.
    pub(crate) fn is_optional_elem(&self) -> bool {
        matches!(self, Self::Optional(Self::Elem(_)))
    }
}

impl Ty {
    /// The type at the type arguments `types` of a call.
    pub(crate) fn native<'db>(&self, types: &[NativeTy<'db>]) -> NativeTy<'db> {
        match self {
            Self::Elem(index) | Self::BoxedElem(index) => types[*index].clone(),
            Self::Any | Self::Typed => NativeTy::Unknown,
            // `null` is already one of the values of these.
            Self::Optional(Self::Elem(index))
                if matches!(
                    types[*index],
                    NativeTy::Optional(_) | NativeTy::Union(_) | NativeTy::Unknown
                ) =>
            {
                types[*index].clone()
            }
            Self::Int => NativeTy::Int,
            Self::Float => NativeTy::Float,
            Self::Bool => NativeTy::Bool,
            Self::Null => NativeTy::Null,
            Self::String => NativeTy::String,
            Self::Array(element) => NativeTy::Array(Box::new(element.native(types))),
            Self::Map => NativeTy::Map(Box::new(types[0].clone()), Box::new(types[1].clone())),
            Self::Optional(inner) => NativeTy::Optional(Box::new(inner.native(types))),
        }
    }
}

#[derive(Debug)]
pub(crate) struct Signature {
    /// The receiver, when the builtin is a method, then its parameters.
    pub(crate) params: &'static [Ty],
    pub(crate) result: Ty,
}

/// How a native may be called. It selects the stub a call goes through.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub(crate) enum Kind {
    /// Reads its arguments and returns: it takes no worker, allocates
    /// nothing and throws nothing.
    Leaf,
    /// Takes the worker first. May allocate, and so collect, and may throw.
    Reentrant,
    /// Reserved: holds addresses of objects across a call that may collect.
    #[expect(dead_code, reason = "no native is listed as pinned yet")]
    Pinned,
    /// Reserved: may park the task that calls it.
    #[expect(dead_code, reason = "no native is listed as async yet")]
    Async,
}

/// The native `map[key]` is a call of where it is not a place.
pub(crate) const MAP_INDEX: &str = "baml.Map.index";

/// One Rust-declared builtin of the standard library.
#[derive(Debug)]
pub(crate) struct Native {
    /// The builtin's name, as a call of it names it.
    pub(crate) path: &'static str,
    /// The runtime symbol implementing it, when it is ported.
    pub(crate) symbol: &'static str,
    /// `None` when the signature has a type natives cannot take yet.
    pub(crate) signature: Option<Signature>,
    /// Whether the symbol returns a value and a thrown object, in two
    /// registers, instead of the bare value.
    pub(crate) throws: bool,
    /// `None` when the runtime does not implement it yet.
    pub(crate) kind: Option<Kind>,
    /// How many type ids a call passes before the declared parameters.
    pub(crate) type_args: usize,
}

include!(concat!(env!("OUT_DIR"), "/native_table.rs"));

/// The Rust-declared builtin called `name`.
pub(crate) fn named(name: &str) -> Option<&'static Native> {
    NATIVES
        .binary_search_by(|native| native.path.cmp(name))
        .ok()
        .map(|index| &NATIVES[index])
}

impl Native {
    /// How to call it, or why compiled code cannot yet.
    pub(crate) fn callable(&self) -> Result<(&Signature, Kind), Blocker> {
        // `map[key]` is compiled as a probe and a panic, and never called:
        // the runtime has no symbol for it.
        let inline = (self.path == MAP_INDEX).then_some(Kind::Leaf);
        let Some(kind) = self.kind.or(inline) else {
            return Err(Blocker::new(
                "standard library native that is not ported",
                self.path,
            ));
        };
        match &self.signature {
            Some(signature) => Ok((signature, kind)),
            None => Err(Blocker::new(
                "standard library native with an unsupported signature",
                self.path,
            )),
        }
    }
}
