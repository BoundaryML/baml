//! What a link refuses, and why.

use baml_base::Name;
use bex_vm_types::DeclPath;

/// An error raised while linking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a declaration the bound package does not export.
    UnresolvedImport { package: Name, path: DeclPath },
    /// A dependency entry named an edge its parent's edge table lacks.
    UnknownDependency { package: Name, edge: Name },
    /// A package exports the same declaration twice.
    DuplicateExport { package: Name, path: DeclPath },
    /// A package's edge table reaches two packages under one name, so the
    /// name would resolve to neither.
    DuplicateEdge { package: Name, edge: Name },
    /// A type switch's hash table could not be solved over the tags the link
    /// assigned its keys (see `MatchHashTable::solve` in `bex_vm_types`).
    UnsolvableSwitch { package: Name, keys: usize },
    /// A unit is malformed: an index outside its declared space, a table that
    /// contradicts its buckets, an entry that violates the format's laws.
    InvalidUnit(String),
}

impl LinkError {
    pub(super) fn invalid(message: impl Into<String>) -> Self {
        Self::InvalidUnit(message.into())
    }
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::UnresolvedImport { package, path } => {
                write!(
                    f,
                    "unresolved import: package `{package}` exports no {path}"
                )
            }
            Self::UnknownDependency { package, edge } => {
                write!(f, "package `{package}` depends on unknown edge `{edge}`")
            }
            Self::DuplicateExport { package, path } => {
                write!(f, "package `{package}` exports {path} twice")
            }
            Self::DuplicateEdge { package, edge } => {
                write!(f, "package `{package}` reaches two packages as `{edge}`")
            }
            Self::UnsolvableSwitch { package, keys } => write!(
                f,
                "package `{package}`: no perfect hash separates the {keys} keys of a type switch"
            ),
            Self::InvalidUnit(message) => write!(f, "invalid unit: {message}"),
        }
    }
}

impl std::error::Error for LinkError {}
