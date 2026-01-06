//! Type intermediate representation.
//!
//! TypeIR represents the types in BAML's type system. This is a simplified
//! version that will be replaced by the full HIR/TIR types.

use serde::{Deserialize, Serialize};
use crate::Constraint;

/// Streaming mode for types.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum StreamingMode {
    /// Non-streaming context
    NonStreaming,
    /// Streaming context
    Streaming,
}

impl Default for StreamingMode {
    fn default() -> Self {
        StreamingMode::NonStreaming
    }
}

/// Literal values that can appear in types.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum LiteralValue {
    String(String),
    Int(i64),
    Bool(bool),
}

impl std::fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            LiteralValue::String(s) => write!(f, "\"{}\"", s),
            LiteralValue::Int(i) => write!(f, "{}", i),
            LiteralValue::Bool(b) => write!(f, "{}", b),
        }
    }
}

/// Primitive type values.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum TypeValue {
    String,
    Int,
    Float,
    Bool,
    Null,
    Media(MediaTypeValue),
}

impl std::fmt::Display for TypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            TypeValue::String => write!(f, "string"),
            TypeValue::Int => write!(f, "int"),
            TypeValue::Float => write!(f, "float"),
            TypeValue::Bool => write!(f, "bool"),
            TypeValue::Null => write!(f, "null"),
            TypeValue::Media(m) => write!(f, "{}", m),
        }
    }
}

/// Media type variants.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum MediaTypeValue {
    Image,
    Audio,
    Video,
    Pdf,
}

impl std::fmt::Display for MediaTypeValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaTypeValue::Image => write!(f, "image"),
            MediaTypeValue::Audio => write!(f, "audio"),
            MediaTypeValue::Video => write!(f, "video"),
            MediaTypeValue::Pdf => write!(f, "pdf"),
        }
    }
}

/// Type metadata for streaming behavior.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct TypeMeta {
    pub streaming_behavior: StreamingBehavior,
}

/// Streaming behavior configuration.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct StreamingBehavior {
    /// Whether this type should show streaming state in serialization.
    pub state: bool,
}

/// The intermediate representation of a BAML type.
///
/// This enum represents all possible types in the BAML type system.
/// It's used for type checking, coercion, and output format generation.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum TypeIR {
    /// Primitive types (string, int, float, bool, null, media)
    Primitive(TypeValue, TypeMeta),

    /// Literal types (specific values)
    Literal(LiteralValue, TypeMeta),

    /// Optional type (T?)
    Optional(Box<TypeIR>, TypeMeta),

    /// List/array type (T[])
    List(Box<TypeIR>, TypeMeta),

    /// Map type (map<string, T>)
    Map(Box<TypeIR>, Box<TypeIR>, TypeMeta),

    /// Union type (T | U | ...)
    Union(Vec<TypeIR>, TypeMeta),

    /// Named class type
    Class(String, TypeMeta),

    /// Named enum type
    Enum(String, TypeMeta),

    /// Type alias reference
    Alias(String, Box<TypeIR>, TypeMeta),

    /// Constrained type (with @assert/@check)
    Constrained {
        base: Box<TypeIR>,
        constraints: Vec<Constraint>,
        meta: TypeMeta,
    },
}

impl TypeIR {
    /// Create a string type.
    pub fn string() -> Self {
        TypeIR::Primitive(TypeValue::String, TypeMeta::default())
    }

    /// Create an int type.
    pub fn int() -> Self {
        TypeIR::Primitive(TypeValue::Int, TypeMeta::default())
    }

    /// Create a float type.
    pub fn float() -> Self {
        TypeIR::Primitive(TypeValue::Float, TypeMeta::default())
    }

    /// Create a bool type.
    pub fn bool() -> Self {
        TypeIR::Primitive(TypeValue::Bool, TypeMeta::default())
    }

    /// Create a null type.
    pub fn null() -> Self {
        TypeIR::Primitive(TypeValue::Null, TypeMeta::default())
    }

    /// Create an optional type.
    pub fn optional(inner: TypeIR) -> Self {
        TypeIR::Optional(Box::new(inner), TypeMeta::default())
    }

    /// Create a list type.
    pub fn list(element: TypeIR) -> Self {
        TypeIR::List(Box::new(element), TypeMeta::default())
    }

    /// Create a map type.
    pub fn map(key: TypeIR, value: TypeIR) -> Self {
        TypeIR::Map(Box::new(key), Box::new(value), TypeMeta::default())
    }

    /// Create a union type.
    pub fn union(variants: Vec<TypeIR>) -> Self {
        TypeIR::Union(variants, TypeMeta::default())
    }

    /// Create a class type reference.
    pub fn class(name: impl Into<String>) -> Self {
        TypeIR::Class(name.into(), TypeMeta::default())
    }

    /// Create an enum type reference.
    pub fn enum_type(name: impl Into<String>) -> Self {
        TypeIR::Enum(name.into(), TypeMeta::default())
    }

    /// Get the type metadata.
    pub fn meta(&self) -> &TypeMeta {
        match self {
            TypeIR::Primitive(_, m) => m,
            TypeIR::Literal(_, m) => m,
            TypeIR::Optional(_, m) => m,
            TypeIR::List(_, m) => m,
            TypeIR::Map(_, _, m) => m,
            TypeIR::Union(_, m) => m,
            TypeIR::Class(_, m) => m,
            TypeIR::Enum(_, m) => m,
            TypeIR::Alias(_, _, m) => m,
            TypeIR::Constrained { meta, .. } => meta,
        }
    }

    /// Check if this is a primitive string type.
    pub fn is_string(&self) -> bool {
        matches!(self, TypeIR::Primitive(TypeValue::String, _))
    }

    /// Check if this is an optional type.
    pub fn is_optional(&self) -> bool {
        matches!(self, TypeIR::Optional(_, _))
    }

    /// Get a display name for this type.
    pub fn display_name(&self) -> String {
        match self {
            TypeIR::Primitive(v, _) => v.to_string(),
            TypeIR::Literal(v, _) => v.to_string(),
            TypeIR::Optional(inner, _) => format!("{}?", inner.display_name()),
            TypeIR::List(elem, _) => format!("{}[]", elem.display_name()),
            TypeIR::Map(k, v, _) => format!("map<{}, {}>", k.display_name(), v.display_name()),
            TypeIR::Union(variants, _) => {
                let names: Vec<_> = variants.iter().map(|v| v.display_name()).collect();
                names.join(" | ")
            }
            TypeIR::Class(name, _) => name.clone(),
            TypeIR::Enum(name, _) => name.clone(),
            TypeIR::Alias(name, _, _) => name.clone(),
            TypeIR::Constrained { base, .. } => base.display_name(),
        }
    }
}

impl std::fmt::Display for TypeIR {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.display_name())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_type_ir_construction() {
        let string_type = TypeIR::string();
        assert!(string_type.is_string());

        let optional_string = TypeIR::optional(TypeIR::string());
        assert!(optional_string.is_optional());

        let list_of_ints = TypeIR::list(TypeIR::int());
        assert_eq!(list_of_ints.display_name(), "int[]");
    }

    #[test]
    fn test_type_ir_display() {
        assert_eq!(TypeIR::string().to_string(), "string");
        assert_eq!(TypeIR::int().to_string(), "int");
        assert_eq!(TypeIR::optional(TypeIR::string()).to_string(), "string?");
        assert_eq!(TypeIR::list(TypeIR::int()).to_string(), "int[]");
        assert_eq!(TypeIR::class("Person").to_string(), "Person");
    }
}
