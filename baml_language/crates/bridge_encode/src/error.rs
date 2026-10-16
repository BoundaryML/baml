//! What went wrong, and where in the value.
//!
//! Two things can go wrong. The value is not a member of the slot's type
//! ([`UnfitKind`]): a union tries its other members on that, and only what
//! escapes to the caller is rendered. Or the facts or the host are at fault
//! ([`FaultKind`]): final wherever it happens. A host's own exception is a
//! third outcome ([`Error::Host`]) and travels as the host's value, not as
//! text.

use std::fmt;

use baml_type::RuntimeTy;

use crate::host::Head;

/// One step from an argument down to the node an error is about.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PathSegment {
    /// The argument itself, by its declared name.
    Argument(String),
    /// A callback's result.
    Result,
    /// A class field or map entry, by key.
    Field(String),
    /// A list element.
    Index(usize),
    /// A map key (as opposed to the value under it).
    Key(String),
}

/// Where an encode starts: a named argument of a call, or a callback's result.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Root {
    Argument(String),
    Result,
}

impl From<Root> for PathSegment {
    fn from(root: Root) -> Self {
        match root {
            Root::Argument(name) => PathSegment::Argument(name),
            Root::Result => PathSegment::Result,
        }
    }
}

/// What an encode returns when it does not ship: the host's own exception
/// raised while the value was read, a refusal of the value, or the host's
/// exception raised while a decided plan was committed (a handle that could
/// not be cloned, a registration that failed), which is the bridge's failure,
/// not the value's.
#[derive(Debug)]
pub enum Error<E> {
    Host(E),
    Value(EncodeError),
    Commit(E),
}

impl<E> From<EncodeError> for Error<E> {
    fn from(error: EncodeError) -> Self {
        Error::Value(error)
    }
}

/// A value could not be encoded against its declared type.
///
/// `path` leads from the argument to the offending node. Every rendering is
/// bounded: a host's [`describe`](crate::HostValue::describe) is short by
/// contract, and the encoder never echoes a value itself.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("{}: {kind}", render_path(path))]
pub struct EncodeError {
    pub path: Vec<PathSegment>,
    /// Boxed so the `Err` arm stays small on the hot `Result` path; the
    /// kind carries whole types.
    pub kind: Box<EncodeErrorKind>,
}

#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum EncodeErrorKind {
    /// The value is not a member of the slot's type.
    #[error(transparent)]
    Unfit(UnfitKind<String>),
    /// A fault in the facts or the host.
    #[error(transparent)]
    Fault(FaultKind),
}

/// The value simply is not a member of the slot's type. `F` is how the value
/// is carried: a reference while a walk is deciding, its rendering once an
/// error is final; only the variants that show the value carry one.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum UnfitKind<F> {
    #[error("expected {expected}, found {found}")]
    Mismatch { expected: RuntimeTy, found: F },
    #[error("expected one of {}, found {found}", render_members(members))]
    NoMember { members: Vec<RuntimeTy>, found: F },
    /// The value fits several members as distinct types and nothing says
    /// which is meant. A host has no way to say yet (a typed wrapper is
    /// planned), so the message does not pretend it has.
    #[error(
        "{found} fits more than one of {} and the host cannot yet say which member is meant",
        render_members(members)
    )]
    Ambiguous { members: Vec<RuntimeTy>, found: F },
    #[error("{found} does not fit the literal type {expected}")]
    LiteralMismatch { expected: RuntimeTy, found: F },
    #[error("map keys must be {expected}, found {found}")]
    MapKey { expected: RuntimeTy, found: F },
    #[error("cannot encode {found}: it has no BAML type")]
    NoBamlType { found: F },
    #[error(
        "cannot encode {found}: a callable has no type of its own; declare the slot as a \
         function type"
    )]
    UntypedCallable { found: F },
    #[error("class {class} has no field `{field}`")]
    UnknownField { class: Head, field: String },
    #[error("class {class} requires field `{field}`")]
    MissingField { class: Head, field: String },
    #[error("enum {enumeration} has no variant `{variant}`")]
    UnknownVariant { enumeration: Head, variant: String },
    #[error("the fields of this {0} do not determine its type arguments; give them explicitly")]
    MissingTypeArgs(Head),
    #[error("the fields of this {class} disagree about its type argument `{param}`")]
    TypeArgConflict { class: Head, param: String },
    /// A type variable the call left open and nothing in the value binds: an
    /// empty container says nothing of its elements, a handle's type is the
    /// engine's, a callable's type is its slot's. As the compiler says of `[]`
    /// with no annotation, the caller names the type.
    #[error(
        "BAML could not infer a type for `{var}` from the value; pass the call's type arguments"
    )]
    UnboundTypeArgs { var: String },
}

impl<F> UnfitKind<F> {
    /// The same kind with the value carried differently.
    pub(crate) fn map_found<G>(self, f: impl FnOnce(F) -> G) -> UnfitKind<G> {
        match self {
            UnfitKind::Mismatch { expected, found } => UnfitKind::Mismatch {
                expected,
                found: f(found),
            },
            UnfitKind::NoMember { members, found } => UnfitKind::NoMember {
                members,
                found: f(found),
            },
            UnfitKind::Ambiguous { members, found } => UnfitKind::Ambiguous {
                members,
                found: f(found),
            },
            UnfitKind::LiteralMismatch { expected, found } => UnfitKind::LiteralMismatch {
                expected,
                found: f(found),
            },
            UnfitKind::MapKey { expected, found } => UnfitKind::MapKey {
                expected,
                found: f(found),
            },
            UnfitKind::NoBamlType { found } => UnfitKind::NoBamlType { found: f(found) },
            UnfitKind::UntypedCallable { found } => UnfitKind::UntypedCallable { found: f(found) },
            UnfitKind::UnknownField { class, field } => UnfitKind::UnknownField { class, field },
            UnfitKind::MissingField { class, field } => UnfitKind::MissingField { class, field },
            UnfitKind::UnknownVariant {
                enumeration,
                variant,
            } => UnfitKind::UnknownVariant {
                enumeration,
                variant,
            },
            UnfitKind::MissingTypeArgs(class) => UnfitKind::MissingTypeArgs(class),
            UnfitKind::TypeArgConflict { class, param } => {
                UnfitKind::TypeArgConflict { class, param }
            }
            UnfitKind::UnboundTypeArgs { var } => UnfitKind::UnboundTypeArgs { var },
        }
    }
}

/// A fault in the facts or the host: final wherever it happens.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FaultKind {
    #[error("the program declares no class {0}")]
    UnknownClass(Head),
    #[error("the program declares no enum {0}")]
    UnknownEnum(Head),
    /// A slot names a class with a different number of type arguments than
    /// the class declares: the signature and the declaration disagree.
    #[error("{class} declares {declared} type parameters, the slot states {stated}")]
    ClassArity {
        class: Head,
        declared: usize,
        stated: usize,
    },
    /// The value nests deeper than the wire carries
    /// ([`WIRE_NESTING_LIMIT`](bridge_ctypes::WIRE_NESTING_LIMIT) protobuf
    /// messages; a list level is two of them, a map or class level three), so
    /// the engine would refuse it unread.
    #[error("the value is nested too deeply for the boundary, which carries {limit} wire messages")]
    TooDeep { limit: usize },
    /// The value has more nodes than one request may carry (a shared or
    /// cyclic graph expands to a tree on the wire).
    #[error("the value has more than {budget} nodes, more than one call carries")]
    TooLarge { budget: usize },
    /// Planning the value against its slot took more steps than one request
    /// may spend (a value and a type whose trials multiply).
    #[error("planning the value took more than {budget} steps")]
    TooMuchPlanning { budget: usize },
    /// A host spelled a type argument that mentions a type variable. A host
    /// spells realized types only; the walker never binds a variable a host
    /// chose.
    #[error("type argument `{ty}` mentions a type variable; type arguments must be realized types")]
    UnrealizedTypeArgument { ty: String },
}

/// The facts a descriptor describes could not be read.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
pub enum FactsError {
    #[error("the program declares no type alias {0}")]
    UnknownAlias(Head),
    #[error("type alias {0} expands to itself")]
    AliasCycle(Head),
}

/// How much of a host's text an error shows.
const SHOWN: usize = 80;

/// `text` cut to what an error shows, with an ellipsis where it was cut:
/// no error carries an unbounded piece of the host's value.
pub(crate) fn bounded(text: String) -> String {
    if text.chars().count() <= SHOWN {
        return text;
    }
    let mut shown: String = text.chars().take(SHOWN).collect();
    shown.push('…');
    shown
}

fn render_path(path: &[PathSegment]) -> String {
    let mut out = String::new();
    for segment in path {
        match segment {
            PathSegment::Argument(name) => {
                out.push_str("argument `");
                out.push_str(name);
                out.push('`');
            }
            PathSegment::Result => out.push_str("result"),
            PathSegment::Field(name) => {
                out.push('.');
                out.push_str(name);
            }
            PathSegment::Index(index) => {
                out.push('[');
                out.push_str(&index.to_string());
                out.push(']');
            }
            PathSegment::Key(key) => {
                out.push_str("[key ");
                out.push_str(key);
                out.push(']');
            }
        }
    }
    if out.is_empty() {
        out.push_str("value");
    }
    out
}

fn render_members(members: &[RuntimeTy]) -> String {
    struct Members<'a>(&'a [RuntimeTy]);
    impl fmt::Display for Members<'_> {
        fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
            for (index, member) in self.0.iter().enumerate() {
                if index > 0 {
                    f.write_str(" | ")?;
                }
                write!(f, "{member}")?;
            }
            Ok(())
        }
    }
    Members(members).to_string()
}
