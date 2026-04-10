//! Type system BAML exposes for code-gen (this is both for streaming and non-streaming types).
//!
//! Types are fully resolved - no unresolved references. Class and Enum IDs
//! from VIR are resolved to their names during lowering.

use std::fmt;

use smol_str::SmolStr;

#[derive(Debug, Clone, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub enum Namespace {
    Types { path: Vec<SmolStr> },
    StreamTypes { path: Vec<SmolStr> },
}

impl Namespace {
    /// Convenience constructor for a Types namespace with no path.
    pub fn types() -> Self {
        Namespace::Types { path: Vec::new() }
    }

    /// Convenience constructor for a StreamTypes namespace with no path.
    pub fn stream_types() -> Self {
        Namespace::StreamTypes { path: Vec::new() }
    }
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Namespace::Types { path } if path.is_empty() => write!(f, "types"),
            Namespace::StreamTypes { path } if path.is_empty() => write!(f, "stream_types"),
            Namespace::Types { path } => {
                write!(f, "types")?;
                for seg in path {
                    write!(f, ".{seg}")?;
                }
                Ok(())
            }
            Namespace::StreamTypes { path } => {
                write!(f, "stream_types")?;
                for seg in path {
                    write!(f, ".{seg}")?;
                }
                Ok(())
            }
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub struct Name {
    pub name: baml_base::Name,
    pub namespace: Namespace,
}

impl std::fmt::Display for Name {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}.{}", self.namespace, self.name)
    }
}

pub enum DefaultValue {
    Null,
    Literal(baml_base::Literal),
}

/// A resolved type in BAML.
///
/// Unlike `baml_compiler_vir::Ty` which may contain `Unknown`, `Error`, `Never` references,
/// this type is guaranteed to be valid.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,
    Literal(baml_base::Literal),

    // Binary data
    Uint8Array,

    // Media types
    Media(baml_base::MediaKind),

    /// Class type with resolved name.
    Class(Name),

    /// Enum type with resolved name.
    Enum(Name),

    /// Type alias with resolved name (used for recursive type aliases).
    TypeAlias(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },

    /// Assumes no unions within unions
    Union(Vec<Ty>),

    /// BAML's `unknown` type — the top type where every type is a subtype.
    /// Renders to `typing.Any` in Python.
    BuiltinUnknown,

    /// Callable type, e.g. `callable<[int, string], bool>`.
    Callable {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },

    // Special types
    /// Void/Unit type - the type of effectful expressions.
    Unit,
    BamlOptions,
}

impl Ty {
    pub fn namespace(&self) -> Option<Namespace> {
        match self {
            Ty::Unit
            | Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Null
            | Ty::Uint8Array
            | Ty::Media(_)
            | Ty::Literal(_)
            | Ty::BuiltinUnknown => None,
            Ty::Class(name) | Ty::Enum(name) | Ty::TypeAlias(name) => Some(name.namespace.clone()),
            Ty::Optional(ty) => ty.namespace(),
            Ty::List(ty) => ty.namespace(),
            Ty::Map { key: _, value } => value.namespace(),
            Ty::Union(items) => items.iter().fold(None, |acc, ty| match &acc {
                Some(Namespace::StreamTypes { .. }) => acc,
                other => match (other, ty.namespace()) {
                    (None, ns) => ns,
                    (_, None) => acc,
                    (Some(_), Some(ns @ Namespace::StreamTypes { .. })) => Some(ns),
                    (_, other) => other,
                },
            }),
            Ty::Callable { params, ret } => {
                params.iter().fold(ret.namespace(), |acc, ty| match acc {
                    Some(ns @ Namespace::StreamTypes { .. }) => Some(ns),
                    other => ty.namespace().or(other),
                })
            }
            Ty::BamlOptions => None,
        }
    }

    pub fn default_value(&self) -> Option<DefaultValue> {
        match self {
            Ty::BamlOptions => None,
            Ty::Int => None,
            Ty::Float => None,
            Ty::String => None,
            Ty::Bool => None,
            Ty::Uint8Array => None,
            Ty::Media(_) => None,
            Ty::Class(_) => None,
            Ty::Enum(_) => None,
            Ty::TypeAlias(_) => None,
            Ty::List(_) => None,
            Ty::Map { .. } => None,
            Ty::Union(_) => None,
            Ty::BuiltinUnknown => None,
            Ty::Callable { .. } => None,
            Ty::Unit => None,
            Ty::Literal(lit) => Some(DefaultValue::Literal(lit.clone())),
            Ty::Optional(_) | Ty::Null => Some(DefaultValue::Null),
        }
    }

    pub(crate) fn validate(&self) -> Result<(), super::CodegenTypeError> {
        match self {
            Ty::BamlOptions => Ok(()),
            Ty::Int
            | Ty::Float
            | Ty::String
            | Ty::Bool
            | Ty::Uint8Array
            | Ty::Media(_)
            | Ty::Class(_)
            | Ty::Enum(_)
            | Ty::TypeAlias(_)
            | Ty::BuiltinUnknown => Ok(()),
            Ty::Callable { params, ret } => {
                params.iter().try_for_each(Ty::validate)?;
                ret.validate()
            }
            Ty::Null => Ok(()),
            Ty::Optional(ty) => {
                ty.validate()?;
                if matches!(ty.as_ref(), Ty::Optional(_) | Ty::Null | Ty::Unit) {
                    Err(super::CodegenTypeError::InvalidOptionalUsage(self.clone()))
                } else {
                    Ok(())
                }
            }
            Ty::Literal(_) => Ok(()),
            Ty::List(ty) => {
                if matches!(ty.as_ref(), Ty::Unit) {
                    return Err(super::CodegenTypeError::InvalidUnit(*ty.clone()));
                }
                ty.validate()
            }
            Ty::Map { key, value } => {
                if !matches!(key.as_ref(), Ty::String | Ty::Enum(_)) {
                    return Err(super::CodegenTypeError::InvalidMapKey(*key.clone()));
                }
                if matches!(value.as_ref(), Ty::Unit) {
                    return Err(super::CodegenTypeError::InvalidUnit(*value.clone()));
                }
                value.validate()
            }
            Ty::Union(items) => {
                if items.is_empty() {
                    return Err(super::CodegenTypeError::InvalidUnionUsage(self.clone()));
                }
                items
                    .iter()
                    .map(Ty::validate)
                    .reduce(|acc, res| match (acc, res) {
                        (Ok(()), Ok(())) => Ok(()),
                        (Err(err), _) => Err(err),
                        (_, Err(err)) => Err(err),
                    })
                    .expect("Union is guaranteed to have atleast 1 item")?;

                // Check if any inner type is a union or a null, if so, nope
                if items
                    .iter()
                    .any(|ty| matches!(ty, Ty::Union(_) | Ty::Optional(_) | Ty::Null | Ty::Unit))
                {
                    Err(super::CodegenTypeError::InvalidUnionUsage(self.clone()))
                } else {
                    Ok(())
                }
            }
            Ty::Unit => Ok(()),
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::BamlOptions => write!(f, "baml.Options"),
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Uint8Array => write!(f, "uint8array"),
            Ty::Media(kind) => write!(f, "{kind}"),
            Ty::Class(name) | Ty::Enum(name) | Ty::TypeAlias(name) => write!(f, "{name}"),
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::List(inner) => write!(f, "{inner}[]"),
            Ty::Map { key, value } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types) => {
                let parts: Vec<std::string::String> =
                    types.iter().map(std::string::ToString::to_string).collect();
                write!(f, "({})", parts.join(" | "))
            }
            Ty::Literal(lit) => match lit {
                baml_base::Literal::Int(v) => write!(f, "int({v})"),
                baml_base::Literal::Float(s) => write!(f, "float({s})"),
                baml_base::Literal::String(v) => write!(f, "string({v:?})"),
                baml_base::Literal::Bool(v) => write!(f, "bool({v})"),
            },
            Ty::Unit => write!(f, "void"),
            Ty::BuiltinUnknown => write!(f, "unknown"),
            Ty::Callable { params, ret } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                write!(f, "callable<[{}], {ret}>", param_strs.join(", "))
            }
        }
    }
}
