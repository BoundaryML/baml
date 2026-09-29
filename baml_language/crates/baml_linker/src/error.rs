//! What a link refuses, and why.

use baml_base::Name;
use baml_linker_types::Digest;
use bex_vm_types::DeclPath;

/// An error raised while linking.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum LinkError {
    /// An import named a declaration the bound package does not export.
    UnresolvedImport { package: Name, path: DeclPath },
    /// A dependency entry named an edge its parent's edge table lacks.
    UnknownDependency { package: Name, edge: Name },
    /// A unit was compiled against one interface of a dependency and linked
    /// against a package carrying another: the entry's recorded fingerprint
    /// is not the digest of the bound package's interface payload.
    InterfaceMismatch {
        package: Name,
        edge: Name,
        expected: Digest,
        found: Digest,
    },
    /// A dependency entry carries a fingerprint but the package bound to it
    /// has no interface payload to check it against.
    MissingInterface { package: Name, edge: Name },
    /// A package exports the same declaration twice.
    DuplicateExport { package: Name, path: DeclPath },
    /// A package's edge table reaches two packages under one name, so the
    /// name would resolve to neither.
    DuplicateEdge { package: Name, edge: Name },
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
            Self::InterfaceMismatch {
                package,
                edge,
                expected,
                found,
            } => write!(
                f,
                "package `{package}` was compiled against an interface of `{edge}` with digest \
                 {} but is linked against one with digest {}",
                hex(expected),
                hex(found)
            ),
            Self::MissingInterface { package, edge } => write!(
                f,
                "package `{package}` depends on `{edge}`, whose package carries no interface \
                 payload to verify against"
            ),
            Self::DuplicateExport { package, path } => {
                write!(f, "package `{package}` exports {path} twice")
            }
            Self::DuplicateEdge { package, edge } => {
                write!(f, "package `{package}` reaches two packages as `{edge}`")
            }
            Self::InvalidUnit(message) => write!(f, "invalid unit: {message}"),
        }
    }
}

impl std::error::Error for LinkError {}

fn hex(digest: &Digest) -> String {
    use std::fmt::Write as _;
    digest
        .iter()
        .fold(String::with_capacity(64), |mut out, byte| {
            let _ = write!(out, "{byte:02x}");
            out
        })
}
