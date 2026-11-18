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
}

impl Ty {
    pub(crate) fn native<'db>(&self) -> NativeTy<'db> {
        match self {
            Self::Int => NativeTy::Int,
            Self::Float => NativeTy::Float,
            Self::Bool => NativeTy::Bool,
            Self::Null => NativeTy::Null,
            Self::String => NativeTy::String,
            Self::Array(element) => NativeTy::Array(Box::new(element.native())),
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
        let Some(kind) = self.kind else {
            return Err(Blocker::new(
                "standard library native that is not ported",
                self.path,
            ));
        };
        if self.type_args != 0 {
            return Err(Blocker::new("generic standard library native", self.path));
        }
        match &self.signature {
            Some(signature) => Ok((signature, kind)),
            None => Err(Blocker::new(
                "standard library native with an unsupported signature",
                self.path,
            )),
        }
    }
}
