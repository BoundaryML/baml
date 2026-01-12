//! Type system for BAML.

use std::fmt;

use baml_base::Name;

/// The value component of a literal type.
///
/// In type theory, literal types (also called singleton types) are types
/// inhabited by exactly one value. For example, `LiteralValue::Int(42)`
/// represents the value `42` which defines the literal type `{42}` — the
/// type whose only inhabitant is `42`.
///
/// Used for exhaustiveness checking of literal unions like `200 | 201 | 204`.
///
/// Note: Float values are stored as strings because floating-point
/// equality is problematic (NaN != NaN, -0.0 == 0.0, etc.).
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum LiteralValue {
    Int(i64),
    /// Float literal stored as string to avoid f64's lack of Eq/Hash.
    Float(std::string::String),
    String(std::string::String),
    Bool(bool),
}

impl fmt::Display for LiteralValue {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            LiteralValue::Int(v) => write!(f, "{v}"),
            LiteralValue::Float(v) => write!(f, "{v}"),
            LiteralValue::String(v) => write!(f, "\"{v}\""),
            LiteralValue::Bool(v) => write!(f, "{v}"),
        }
    }
}

/// A resolved type in BAML.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,

    // Media types
    Media(baml_base::MediaKind),

    /// Literal type: a type inhabited by exactly one value (also called singleton type).
    /// Used for exhaustiveness checking of literal unions like `200 | 201 | 204`.
    /// The literal type `{42}` contains only the value `42`.
    ///
    /// Note: `Ty::Null` is also a singleton type but is kept as a separate variant
    /// for convenience, since null is fundamental to optional types.
    Literal(LiteralValue),

    // User-defined types (resolved by name)
    Class(Name),
    Enum(Name),

    /// Named type (unresolved class/enum by name).
    /// Used when we know the type name but haven't resolved it to an ID yet.
    Named(Name),

    // Type constructors
    Optional(Box<Ty>),
    List(Box<Ty>),
    Map {
        key: Box<Ty>,
        value: Box<Ty>,
    },
    Union(Vec<Ty>),

    /// Function/arrow type: `(T1, T2, ...) -> R`
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },

    // Special types
    Unknown,
    Error,
    Void,

    /// Watch accessor type: represents `x.$watch` on a watched variable.
    /// Contains the inner type being watched for method resolution.
    WatchAccessor(Box<Ty>),
}

impl Ty {
    /// Check if this type is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Check if this type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }

    /// Check if this type is void.
    pub fn is_void(&self) -> bool {
        matches!(self, Ty::Void)
    }

    /// Check if this type is uninhabited (has no possible values).
    ///
    /// An empty match on an uninhabited type is actually correct and exhaustive—there are
    /// no cases to handle because there are no possible values.
    ///
    /// Currently handled cases:
    /// - `Ty::Unknown` and `Ty::Error`: Treated as uninhabited for error recovery
    ///   (we don't want to emit additional errors when type inference already failed)
    /// - `Ty::Union(vec![])`: An empty union has no members, so no values
    ///
    /// Possible future cases to consider:
    /// - Zero-variant enums: `Ty::Enum(name)` where the enum has no variants defined
    ///   (would require access to the enum variants map to check variant count)
    /// - Recursive uninhabited types: e.g., `List<Never>` is inhabited (by empty list),
    ///   but some recursive structures could be uninhabited
    /// - Intersection of incompatible types (if the type system supports intersections)
    pub fn is_uninhabited(&self) -> bool {
        match self {
            // Error recovery: don't emit additional errors when type inference failed
            Ty::Unknown | Ty::Error => true,
            // Empty union has no members, therefore no possible values
            Ty::Union(types) => types.is_empty(),
            // All other types are inhabited
            // TODO(exhaustiveness): Check for zero-variant enums. This requires access
            // to enum variants map. Currently only empty unions are detected.
            _ => false,
        }
    }

    /// Check if this type is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Null)
    }

    /// Check if this type is a media type.
    #[allow(dead_code)]
    pub fn is_media(&self) -> bool {
        matches!(self, Ty::Media(_))
    }

    /// Make this type optional.
    #[must_use]
    #[allow(dead_code)]
    pub fn into_optional(self) -> Self {
        Ty::Optional(Box::new(self))
    }

    /// Make a list of this type.
    #[must_use]
    #[allow(dead_code)]
    pub fn into_list(self) -> Self {
        Ty::List(Box::new(self))
    }

    /// Get the element type if this is a list type.
    #[allow(dead_code)]
    pub fn list_element(&self) -> Option<&Ty> {
        match self {
            Ty::List(inner) => Some(inner),
            _ => None,
        }
    }

    /// Get the key and value types if this is a map type.
    #[allow(dead_code)]
    pub fn map_types(&self) -> Option<(&Ty, &Ty)> {
        match self {
            Ty::Map { key, value } => Some((key, value)),
            _ => None,
        }
    }

    /// Unwrap optional type.
    #[allow(dead_code)]
    pub fn unwrap_optional(&self) -> &Ty {
        match self {
            Ty::Optional(inner) => inner,
            _ => self,
        }
    }
}

impl fmt::Display for Ty {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Media(kind) => write!(f, "{kind}"),
            Ty::Literal(val) => write!(f, "{val}"),
            Ty::Class(name) => write!(f, "{name}"),
            Ty::Enum(name) => write!(f, "{name}"),
            Ty::Named(name) => write!(f, "{name}"),
            Ty::Optional(inner) => write!(f, "{inner}?"),
            Ty::List(inner) => write!(f, "{inner}[]"),
            Ty::Map { key, value } => write!(f, "map<{key}, {value}>"),
            Ty::Union(types) => {
                let parts: Vec<std::string::String> =
                    types.iter().map(std::string::ToString::to_string).collect();
                write!(f, "{}", parts.join(" | "))
            }
            Ty::Function { params, ret } => {
                let param_strs: Vec<std::string::String> = params
                    .iter()
                    .map(std::string::ToString::to_string)
                    .collect();
                write!(f, "({}) -> {}", param_strs.join(", "), ret)
            }
            Ty::Unknown => write!(f, "unknown"),
            Ty::Error => write!(f, "error"),
            Ty::Void => write!(f, "void"),
            Ty::WatchAccessor(inner) => write!(f, "{inner}.$watch"),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // NOTE: Subtyping tests are now in normalize.rs which handles
    // type alias resolution and equirecursive subtyping.

    #[test]
    fn test_display() {
        assert_eq!(Ty::Int.to_string(), "int");
        assert_eq!(Ty::Optional(Box::new(Ty::String)).to_string(), "string?");
        assert_eq!(Ty::List(Box::new(Ty::Int)).to_string(), "int[]");
        assert_eq!(
            Ty::Union(vec![Ty::Int, Ty::String]).to_string(),
            "int | string"
        );
    }

    #[test]
    fn test_is_uninhabited() {
        // Unknown and Error are treated as uninhabited for error recovery
        assert!(Ty::Unknown.is_uninhabited());
        assert!(Ty::Error.is_uninhabited());

        // Empty union is uninhabited (no possible values)
        assert!(Ty::Union(vec![]).is_uninhabited());

        // Non-empty union is inhabited
        assert!(!Ty::Union(vec![Ty::Int]).is_uninhabited());
        assert!(!Ty::Union(vec![Ty::Int, Ty::String]).is_uninhabited());

        // Regular types are inhabited
        assert!(!Ty::Int.is_uninhabited());
        assert!(!Ty::String.is_uninhabited());
        assert!(!Ty::Bool.is_uninhabited());
        assert!(!Ty::Null.is_uninhabited());
        assert!(!Ty::Void.is_uninhabited());
        assert!(!Ty::List(Box::new(Ty::Int)).is_uninhabited());
        assert!(!Ty::Optional(Box::new(Ty::Int)).is_uninhabited());
    }
}
