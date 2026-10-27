//! The nominal head of a type as it crosses the host boundary.
//!
//! A type on the wire names a declaration in one of two ways. One compiled
//! with the program is spelled by its name and resolved against the program's
//! tables alone. One the engine created at run time has no name any lookup
//! could resolve; the host references it by its own row to the declaration,
//! which is followed, never resolved. A third form is display only: a label
//! beside a value that crosses by reference, for a declaration the host
//! cannot name. It is never a reference, and a decoder refuses it.

use crate::{Name, RealizedTy, RuntimeTy, TypeName};

/// A nominal head on the wire. `L` is how a live declaration is referenced:
/// a host's table key before the engine's decoder runs, a heap handle after.
#[derive(Clone, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum WireHead<L> {
    /// A declaration compiled with the program, by name.
    Static(TypeName),
    /// A declaration the engine holds, by the host's reference to it.
    Live(L),
    /// What an unnameable declaration is called, beside a value that crosses
    /// by reference. Display only; refused where a type is stated.
    Anonymous(Name),
}

impl<L> WireHead<L> {
    /// The head with its live reference carried by `map`.
    pub fn map_live<M>(self, map: impl FnOnce(L) -> M) -> WireHead<M> {
        match self {
            Self::Static(name) => WireHead::Static(name),
            Self::Live(live) => WireHead::Live(map(live)),
            Self::Anonymous(label) => WireHead::Anonymous(label),
        }
    }

    /// The head with its live reference carried by `map`, or `map`'s error.
    pub fn try_map_live<M, E>(self, map: impl FnOnce(L) -> Result<M, E>) -> Result<WireHead<M>, E> {
        Ok(match self {
            Self::Static(name) => WireHead::Static(name),
            Self::Live(live) => WireHead::Live(map(live)?),
            Self::Anonymous(label) => WireHead::Anonymous(label),
        })
    }
}

impl<L> From<TypeName> for WireHead<L> {
    fn from(name: TypeName) -> Self {
        Self::Static(name)
    }
}

impl<L: std::fmt::Debug> crate::HeadDisplay for WireHead<L> {
    fn head_display_name(&self) -> String {
        match self {
            Self::Static(name) => name.display_name().to_string(),
            Self::Live(_) => "<runtime declaration>".to_string(),
            Self::Anonymous(label) => label.to_string(),
        }
    }
}

impl<L: std::fmt::Debug> std::fmt::Display for WireHead<L> {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Static(name) => write!(f, "{name}"),
            Self::Live(_) => f.write_str("<runtime declaration>"),
            Self::Anonymous(label) => write!(f, "{label}"),
        }
    }
}

/// A type on the wire: a realized type over a [`WireHead`]. Every type a
/// value crosses with is realized, so a type variable or an unresolved
/// projection cannot be spelled on the wire at all; a host's declared
/// signatures, which have them, are host-side data and never sent.
pub type WireTy<L> = RealizedTy<WireHead<L>>;

impl<L: Clone> RealizedTy<WireHead<L>> {
    /// `Class(name)` for a compiled class under the local module path, no
    /// type arguments.
    pub fn static_class(name: &str) -> Self {
        RealizedTy::Class(WireHead::Static(TypeName::local(name.into())), Box::new([]))
    }

    /// `Class(name, args)` for a compiled generic class.
    pub fn static_class_with_args(name: TypeName, args: Box<[Self]>) -> Self {
        RealizedTy::Class(WireHead::Static(name), args)
    }

    /// `Enum(name)` for a compiled enum under the local module path.
    pub fn static_enum(name: &str) -> Self {
        RealizedTy::Enum(WireHead::Static(TypeName::local(name.into())))
    }

    /// `Interface(name)` for a compiled interface, no type arguments and no
    /// associated-type bindings.
    pub fn static_interface(name: TypeName) -> Self {
        RealizedTy::Interface(WireHead::Static(name), Box::new([]), Box::new([]))
    }

    /// Whether any head is a live reference.
    pub fn names_live_declaration(&self) -> bool {
        let mut live = false;
        self.visit_heads(&mut |head: &WireHead<L>| live |= matches!(head, WireHead::Live(_)));
        live
    }

    /// The type with every live reference carried by `map`.
    pub fn map_live<M: Clone>(&self, map: &mut impl FnMut(&L) -> M) -> RealizedTy<WireHead<M>> {
        self.map_heads(&mut |head: &WireHead<L>| head.clone().map_live(|live| map(&live)))
    }

    /// The type with every live reference carried by `map`, or `map`'s first
    /// error.
    pub fn try_map_live<M: Clone, E>(
        &self,
        map: &mut impl FnMut(&L) -> Result<M, E>,
    ) -> Result<RealizedTy<WireHead<M>>, E> {
        self.try_map_heads(&mut |head: &WireHead<L>| head.clone().try_map_live(|live| map(&live)))
    }
}

impl RealizedTy<WireHead<std::convert::Infallible>> {
    /// This annotation as a wire type of any reference: it holds none.
    pub fn as_stated<L: Clone>(&self) -> WireTy<L> {
        self.map_live(&mut |never: &std::convert::Infallible| match *never {})
    }
}

impl RealizedTy<TypeName> {
    /// This name-headed type as a wire type: every head static.
    pub fn into_wire<L: Clone>(self) -> WireTy<L> {
        self.map_heads(&mut |name: &TypeName| WireHead::Static(name.clone()))
    }
}

impl RuntimeTy<TypeName> {
    /// This name-headed type as a wire type, when it is realized: a type
    /// variable or an unresolved projection is not a type a value crosses
    /// with, and is the error.
    pub fn try_into_wire<L: Clone>(&self) -> Result<WireTy<L>, crate::NotRealizedTy> {
        RealizedTy::try_from(self).map(RealizedTy::into_wire)
    }
}

impl<L: Clone> WireTy<L> {
    /// This type with every head spelled by name, or the first head that is
    /// not a compiled declaration.
    pub fn try_into_named(&self) -> Result<RealizedTy<TypeName>, WireHead<L>> {
        self.try_map_heads(&mut |head: &WireHead<L>| match head {
            WireHead::Static(name) => Ok(name.clone()),
            WireHead::Live(_) | WireHead::Anonymous(_) => Err(head.clone()),
        })
    }
}
