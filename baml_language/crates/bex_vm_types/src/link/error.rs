//! What a link refuses, and why.

use baml_base::Name;
use baml_type::typetag::TypeTag;

use crate::unit::DeclPath;

/// An error raised while linking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a declaration the bound package does not export.
    UnresolvedImport { package: Name, path: DeclPath },
    /// A dependency entry named an edge its parent's edge table lacks.
    UnknownDependency { package: Name, edge: Name },
    /// A package exports the same declaration twice.
    DuplicateExport { package: Name, path: DeclPath },
    /// Two declarations render to one key of the flat name maps, or two
    /// packages share one spelling.
    DuplicateLinkName(String),
    /// Two declarations of the linked image carry one type tag.
    TagCollision(Box<TagCollision>),
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
            Self::DuplicateLinkName(name) => write!(f, "duplicate link name `{name}`"),
            Self::TagCollision(collision) => write!(
                f,
                "type tag {:?} is carried by both `{}` {} and `{}` {}",
                collision.tag,
                collision.first.0,
                collision.first.1,
                collision.second.0,
                collision.second.1
            ),
            Self::InvalidUnit(message) => write!(f, "invalid unit: {message}"),
        }
    }
}

impl std::error::Error for LinkError {}

/// The payload of [`LinkError::TagCollision`]: the tag and the two
/// declarations (package, path) that carry it.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct TagCollision {
    pub tag: TypeTag,
    pub first: (Name, DeclPath),
    pub second: (Name, DeclPath),
}
