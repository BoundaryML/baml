use borsh::{BorshDeserialize, BorshSerialize};

/// How a declaration is named, at the moment a name is wanted.
///
/// A declaration stores only its path inside its package. This is that path
/// rendered: for a declaration a package owns, [`Declared`](Self::Declared) —
/// the path under its package's display label, which is not an identity. A
/// runtime-created declaration (`reflect.Class.new`,
/// `reflect.Enum.new`, the class builder, host authoring) is
/// [`Anonymous`](Self::Anonymous): it has an item name for display, but no
/// package and no namespace — no source world can spell it, so nothing may
/// key on it or resolve by it. Its identity is its declaration object and the
/// counter [`TypeTag`](crate::typetag::TypeTag) that travels with it; the
/// only ways to reach it are a `TypeHead` and the mount surfaces that a
/// compile explicitly names it through.
///
/// Deliberately implements no equality and no `Hash`: a declaration's name is
/// never its identity, so there is nothing sound to compare. Code that needs
/// "same declaration?" compares `TypeTag`s (or pointers); code that genuinely
/// needs "same declared spelling?" unwraps [`declared`](Self::declared) and
/// compares the `TypeName`s it gets — visibly scoped to compiled declarations.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum DeclarationName {
    /// Declared in source: its path, spelled under its package's display
    /// label. The label is not an identity, so neither is this name.
    Declared(crate::TypeName),
    /// Created at runtime: an item name with no package or namespace path.
    Anonymous(crate::Name),
}

impl DeclarationName {
    /// The package-qualified name, if the declaration was compiled from
    /// source. `None` for a runtime-created declaration: there is no spelling
    /// to resolve, and callers must treat that as "not addressable by name" —
    /// never invent one.
    #[must_use]
    pub fn declared(&self) -> Option<&crate::TypeName> {
        match self {
            Self::Declared(qtn) => Some(qtn),
            Self::Anonymous(_) => None,
        }
    }

    /// The bare item name (`Person`), present for every declaration. Display
    /// and schema rendering only — it is not unique, even within one heap.
    #[must_use]
    pub fn item_name(&self) -> &crate::Name {
        match self {
            Self::Declared(qtn) => qtn.name(),
            Self::Anonymous(name) => name,
        }
    }

    /// The user-facing display string (`ai.PromptMessage`, `Person`). An
    /// anonymous declaration displays as its bare item name.
    #[must_use]
    pub fn display_name(&self) -> crate::Name {
        match self {
            Self::Declared(qtn) => qtn.display_name(),
            Self::Anonymous(name) => name.clone(),
        }
    }
}

/// The canonical rendering: a declared name renders fully qualified, an
/// anonymous one as its bare item name (there is no qualification to render).
impl std::fmt::Display for DeclarationName {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Declared(qtn) => qtn.fmt(f),
            Self::Anonymous(name) => name.fmt(f),
        }
    }
}
