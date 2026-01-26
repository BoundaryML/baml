//! Type system BAML exposes for code-gen (this is both for streaming and non-streaming types).
//!
//! Types are fully resolved - no unresolved references. Class and Enum IDs
//! from VIR are resolved to their names during lowering.

use std::fmt;

#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Namespace {
    Types,
    StreamTypes,
}

impl std::fmt::Display for Namespace {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{}",
            match self {
                Namespace::StreamTypes => "stream_types",
                Namespace::Types => "types",
            }
        )
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

    // Media types
    Media(baml_base::MediaKind),

    /// Class type with resolved name.
    Class(Name),

    /// Enum type with resolved name.
    Enum(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },

    /// Assumes no unions within unions
    Union(Vec<Ty>),
    Checked(Box<Ty>, Vec<String>),
    StreamState(Box<Ty>),

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
            | Ty::Media(_)
            | Ty::Literal(_) => None,
            Ty::Class(name) | Ty::Enum(name) => Some(name.namespace.clone()),
            Ty::Optional(ty) => ty.namespace(),
            Ty::List(ty) => ty.namespace(),
            Ty::Map { key: _, value } => value.namespace(),
            Ty::Union(items) => items.iter().fold(None, |acc, ty| match &acc {
                Some(Namespace::StreamTypes) => acc,
                other => match (other, ty.namespace()) {
                    (None, ns) => ns,
                    (_, None) => acc,
                    (Some(_), Some(Namespace::StreamTypes)) => Some(Namespace::StreamTypes),
                    (_, other) => other,
                },
            }),
            Ty::Checked(ty, _) => ty.namespace().or(Some(Namespace::Types)),
            Ty::StreamState(_) => Some(Namespace::StreamTypes),
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
            Ty::Media(_) => None,
            Ty::Class(_) => None,
            Ty::Enum(_) => None,
            Ty::List(_) => None,
            Ty::Map { .. } => None,
            Ty::Union(_) => None,
            Ty::Unit => None,
            Ty::Checked(_, _) => None,
            Ty::StreamState(_) => None,
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
            | Ty::Media(_)
            | Ty::Class(_)
            | Ty::Enum(_) => Ok(()),
            Ty::Null => Ok(()),
            Ty::Checked(ty, _) => ty.validate(),
            Ty::StreamState(ty) => ty.validate(),
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
            Ty::Media(kind) => write!(f, "{kind}"),
            Ty::Class(name) | Ty::Enum(name) => write!(f, "{name}"),
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
                baml_base::Literal::String(v) => write!(f, "string({v:?})"),
                baml_base::Literal::Bool(v) => write!(f, "bool({v})"),
            },
            Ty::Unit => write!(f, "void"),
            Ty::Checked(inner, checks) => write!(f, "Checked<{inner}, {}>", checks.join(" | ")),
            Ty::StreamState(inner) => write!(f, "StreamState<{inner}>"),
        }
    }
}
