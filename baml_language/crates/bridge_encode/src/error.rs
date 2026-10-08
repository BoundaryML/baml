//! What went wrong, and where in the value.

use std::fmt;

use baml_type::{RuntimeTy, TypeName};

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
    #[error("expected {expected}, found {found}")]
    Mismatch { expected: RuntimeTy, found: String },
    #[error("expected one of {}, found {found}", render_members(members))]
    NoMember {
        members: Vec<RuntimeTy>,
        found: String,
    },
    #[error(
        "{found} fits more than one of {}; pass the value as the member you mean",
        render_members(members)
    )]
    Ambiguous {
        members: Vec<RuntimeTy>,
        found: String,
    },
    #[error("class {class} has no field `{field}`")]
    UnknownField { class: TypeName, field: String },
    #[error("class {class} requires field `{field}`")]
    MissingField { class: TypeName, field: String },
    #[error("enum {enumeration} has no variant `{variant}`")]
    UnknownVariant {
        enumeration: TypeName,
        variant: String,
    },
    #[error("{found} does not fit the literal type {expected}")]
    LiteralMismatch { expected: RuntimeTy, found: String },
    #[error("{found} cannot be represented exactly as {expected}")]
    NotRepresentable { expected: RuntimeTy, found: String },
    #[error("map keys must be {expected}, found {found}")]
    MapKey { expected: RuntimeTy, found: String },
    #[error("the program declares no class {0}")]
    UnknownClass(TypeName),
    #[error("the program declares no enum {0}")]
    UnknownEnum(TypeName),
    #[error("the program declares no type alias {0}")]
    UnknownAlias(TypeName),
    #[error("type alias {0} expands to itself")]
    AliasCycle(TypeName),
    #[error("the fields of this {0} do not determine its type arguments; give them explicitly")]
    MissingTypeArgs(TypeName),
    #[error("the fields of this {class} disagree about its type argument `{param}`")]
    TypeArgConflict { class: TypeName, param: String },
    #[error("cannot encode {found}: it has no BAML type")]
    NoBamlType { found: String },
    #[error("{0}")]
    Host(String),
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
