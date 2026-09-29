//! A declaration's coordinates within its package: the identity the
//! executable's package tables key on, and the path a unit imports or
//! exports a declaration under. The variant IS the kind, so a reference of
//! one kind can never bind a declaration of another.

use baml_base::Name;
use baml_type::{PathName, wire::EdgePath};
use borsh::{BorshDeserialize, BorshSerialize};

use crate::types::LocalName;

/// A package-relative item coordinate: namespace path plus item name. The
/// owning package's own coordinates — never a consumer's spelling of them.
pub type ItemPath = LocalName;

/// A function's path within its package.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum FnPath {
    /// A free function.
    Free(ItemPath),
    /// A class-inherent method (static or instance), reached through the
    /// class's method table.
    Method { class: ItemPath, name: Name },
}

/// The interface an impl-provided body belongs to, located from the body's
/// own package by edge path: empty for an interface of that package, the
/// edge names that reach its package otherwise.
///
/// This is an identity COMPONENT of the body, not a reference the linker
/// binds (a body is reached rule-relatively; the direct-call import resolves
/// the body itself). Edges are manifest-fixed, so every submission of a
/// session locates the same interface the same way — which a dependency-table
/// slot, numbered per table in first-use order, would not — and no name a
/// program gives a package takes part.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct InterfaceKey {
    pub package: EdgePath,
    pub path: ItemPath,
}

/// The identity of an interface-machinery body. A body has no runtime name:
/// an adopted default is reached through its interface object, a provided
/// method through its impl rule. These keys exist so a later session
/// submission can call an earlier submission's body directly.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum BodyKey {
    /// An interface's default-method body; the interface is declared by the
    /// body's own package.
    Default { interface: ItemPath, method: Name },
    /// A method an `implements` block provides. Importable only from the
    /// unit's own package (`DepSlot::SELF` in `baml_linker_types`): an
    /// impl-provided body is never addressed across packages (dispatch is
    /// virtual). Boxed: the coherence key carries type templates, and a
    /// [`DeclPath`] is a map key everywhere else.
    ImplMethod(Box<ImplBodyKey>),
}

/// The identity of an impl-provided body: the interface, the impl's coherence
/// identity (injective over admitted impls), and the method name.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImplBodyKey {
    pub interface: InterfaceKey,
    pub coherence: ImplBodyCoherence,
    pub method: Name,
}

/// An `implements` block's coherence identity as a wire key: its for-pattern,
/// interface arguments, and constraint set — everything coherence's
/// admissibility check discriminates on, the same discriminant as the
/// runtime's [`ImplCoherenceKey`](crate::types::ImplCoherenceKey) — with every
/// head located from the body's own package by edge path, like
/// [`InterfaceKey`] is. A unit's type heads are that unit's operands, so a
/// head-typed key would differ between the unit that declares the block and a
/// later session submission that imports its body; edge paths are
/// manifest-fixed, so every unit of a package agrees on these, whatever the
/// program calls the packages involved.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct ImplBodyCoherence {
    pub for_ty_pattern: baml_type::TyTemplate<PathName>,
    pub interface_args: Vec<baml_type::TyTemplate<PathName>>,
    /// Per impl-frame param in frame order, that param's bounds canonically
    /// sorted.
    pub generic_param_bounds: Vec<Vec<SpelledBound>>,
}

/// One interface bound of an [`ImplBodyCoherence`], located from the body's
/// package.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub struct SpelledBound {
    pub interface: PathName,
    pub args: Vec<baml_type::TyTemplate<PathName>>,
    pub assoc: Vec<(Name, baml_type::TyTemplate<PathName>)>,
}

/// A declaration's coordinates within its package. The variant IS the kind,
/// so an import of one kind can never bind an export of another.
#[derive(Clone, Debug, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum DeclPath {
    Class(ItemPath),
    Enum(ItemPath),
    Interface(ItemPath),
    /// A recursive alias (the only kind pooled; others expand at lowering).
    TypeAlias(ItemPath),
    /// A top-level `let`. Owns a global slot and no pool object.
    Let(ItemPath),
    /// A named function. Owns a pool object and a global slot.
    Function(FnPath),
    /// An interface-machinery body. Owns a pool object and a global slot,
    /// and appears in no runtime name table.
    InterfaceBody(BodyKey),
}

impl DeclPath {
    /// Is this a type declaration — one whose references bake a type tag?
    #[must_use]
    pub fn is_type(&self) -> bool {
        matches!(
            self,
            Self::Class(_) | Self::Enum(_) | Self::Interface(_) | Self::TypeAlias(_)
        )
    }

    /// Does this declaration own a global slot?
    #[must_use]
    pub fn owns_global_slot(&self) -> bool {
        matches!(
            self,
            Self::Let(_) | Self::Function(_) | Self::InterfaceBody(_)
        )
    }

    /// Does this declaration own a pool object?
    #[must_use]
    pub fn owns_object(&self) -> bool {
        !matches!(self, Self::Let(_))
    }
}

impl std::fmt::Display for DeclPath {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Class(path) => write!(f, "class {path}"),
            Self::Enum(path) => write!(f, "enum {path}"),
            Self::Interface(path) => write!(f, "interface {path}"),
            Self::TypeAlias(path) => write!(f, "type alias {path}"),
            Self::Let(path) => write!(f, "let {path}"),
            Self::Function(FnPath::Free(path)) => write!(f, "function {path}"),
            Self::Function(FnPath::Method { class, name }) => {
                write!(f, "method {class}.{name}")
            }
            Self::InterfaceBody(BodyKey::Default { interface, method }) => {
                write!(f, "default body {interface}.{method}")
            }
            Self::InterfaceBody(BodyKey::ImplMethod(body)) => write!(
                f,
                "impl body <(_ as {}.{})>.{}",
                body.interface.package, body.interface.path, body.method
            ),
        }
    }
}
