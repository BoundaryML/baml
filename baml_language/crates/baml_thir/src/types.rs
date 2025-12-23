//! Type system for BAML.

use std::fmt;

use baml_base::Name;
use baml_hir::{ClassId, EnumId};

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
pub enum Ty<'db> {
    // Primitive types
    Int,
    Float,
    String,
    Bool,
    Null,

    // Media types
    Image,
    Audio,
    Video,
    Pdf,

    /// Literal type: a type inhabited by exactly one value (also called singleton type).
    /// Used for exhaustiveness checking of literal unions like `200 | 201 | 204`.
    /// The literal type `{42}` contains only the value `42`.
    ///
    /// Note: `Ty::Null` is also a singleton type but is kept as a separate variant
    /// for convenience, since null is fundamental to optional types.
    Literal(LiteralValue),

    // User-defined types (fully resolved with IDs)
    Class(ClassId<'db>),
    Enum(EnumId<'db>),

    /// Named type (unresolved class/enum by name).
    /// Used when we know the type name but haven't resolved it to an ID yet.
    Named(Name),

    // Type constructors
    Optional(Box<Ty<'db>>),
    List(Box<Ty<'db>>),
    Map {
        key: Box<Ty<'db>>,
        value: Box<Ty<'db>>,
    },
    Union(Vec<Ty<'db>>),

    /// Function/arrow type: `(T1, T2, ...) -> R`
    Function {
        params: Vec<Ty<'db>>,
        ret: Box<Ty<'db>>,
    },

    // Special types
    Unknown,
    Error,
    Void,
}

impl Ty<'_> {
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
    /// - Zero-variant enums: `Ty::Enum(id)` where the enum has no variants defined
    ///   (would require access to the database to check variant count)
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
            // TODO(exhaustiveness): Check for zero-variant enums. This requires database
            // access to look up enum definitions. Currently only empty unions are detected.
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
        matches!(self, Ty::Image | Ty::Audio | Ty::Video | Ty::Pdf)
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

    /// Check if this type is a subtype of another.
    ///
    /// Returns true if `self` can be used where `other` is expected.
    pub fn is_subtype_of(&self, other: &Ty) -> bool {
        // Same types are subtypes
        if self == other {
            return true;
        }

        // Unknown is compatible with everything (for error recovery)
        if self.is_unknown() || other.is_unknown() {
            return true;
        }

        // Error type is compatible with everything (for error recovery)
        if self.is_error() || other.is_error() {
            return true;
        }

        match (self, other) {
            // Null is a subtype of Optional<T>
            (Ty::Null, Ty::Optional(_)) => true,

            // T is a subtype of Optional<T>
            (inner, Ty::Optional(opt_inner)) => inner.is_subtype_of(opt_inner),

            // Optional<T> is a subtype of T | null (Union containing null)
            (Ty::Optional(inner), Ty::Union(types)) => {
                types.contains(&Ty::Null) && types.iter().any(|t| inner.is_subtype_of(t))
            }

            // T is a subtype of T | U (union containing T)
            (inner, Ty::Union(types)) => types.iter().any(|t| inner.is_subtype_of(t)),

            // Union<T1, T2> is a subtype of U if all Ti are subtypes of U
            (Ty::Union(types), other) => types.iter().all(|t| t.is_subtype_of(other)),

            // List covariance: List<T> is a subtype of List<U> if T is a subtype of U
            (Ty::List(inner1), Ty::List(inner2)) => inner1.is_subtype_of(inner2),

            // Map covariance in value: Map<K, V1> is a subtype of Map<K, V2> if V1 is a subtype of V2
            // Key types must be equal (invariant)
            (Ty::Map { key: k1, value: v1 }, Ty::Map { key: k2, value: v2 }) => {
                k1 == k2 && v1.is_subtype_of(v2)
            }

            // Int is a subtype of Float (numeric widening)
            (Ty::Int, Ty::Float) => true,

            _ => false,
        }
    }

    /// Get the element type if this is a list type.
    #[allow(dead_code)]
    pub fn list_element(&self) -> Option<&Ty<'_>> {
        match self {
            Ty::List(inner) => Some(inner),
            _ => None,
        }
    }

    /// Get the key and value types if this is a map type.
    #[allow(dead_code)]
    pub fn map_types(&self) -> Option<(&Ty<'_>, &Ty<'_>)> {
        match self {
            Ty::Map { key, value } => Some((key, value)),
            _ => None,
        }
    }

    /// Unwrap optional type.
    #[allow(dead_code)]
    pub fn unwrap_optional(&self) -> &Ty<'_> {
        match self {
            Ty::Optional(inner) => inner,
            _ => self,
        }
    }
}

impl fmt::Display for Ty<'_> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Ty::Int => write!(f, "int"),
            Ty::Float => write!(f, "float"),
            Ty::String => write!(f, "string"),
            Ty::Bool => write!(f, "bool"),
            Ty::Null => write!(f, "null"),
            Ty::Image => write!(f, "image"),
            Ty::Audio => write!(f, "audio"),
            Ty::Video => write!(f, "video"),
            Ty::Pdf => write!(f, "pdf"),
            Ty::Literal(val) => write!(f, "{val}"),
            Ty::Class(_) => write!(f, "<class>"),
            Ty::Enum(_) => write!(f, "<enum>"),
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
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_subtype_same() {
        assert!(Ty::<'_>::Int.is_subtype_of(&Ty::Int));
        assert!(Ty::<'_>::String.is_subtype_of(&Ty::String));
    }

    #[test]
    fn test_subtype_numeric_widening() {
        assert!(Ty::<'_>::Int.is_subtype_of(&Ty::Float));
        assert!(!Ty::<'_>::Float.is_subtype_of(&Ty::Int));
    }

    #[test]
    fn test_subtype_optional() {
        let opt_int: Ty<'_> = Ty::Optional(Box::new(Ty::Int));
        assert!(Ty::<'_>::Int.is_subtype_of(&opt_int));
        assert!(Ty::<'_>::Null.is_subtype_of(&opt_int));
        assert!(!Ty::<'_>::String.is_subtype_of(&opt_int));
    }

    #[test]
    fn test_subtype_union() {
        let union: Ty<'_> = Ty::Union(vec![Ty::Int, Ty::String]);
        assert!(Ty::<'_>::Int.is_subtype_of(&union));
        assert!(Ty::<'_>::String.is_subtype_of(&union));
        assert!(!Ty::<'_>::Bool.is_subtype_of(&union));
    }

    #[test]
    fn test_subtype_list_covariance() {
        let list_int: Ty<'_> = Ty::List(Box::new(Ty::Int));
        let list_float: Ty<'_> = Ty::List(Box::new(Ty::Float));
        assert!(list_int.is_subtype_of(&list_float));
        assert!(!list_float.is_subtype_of(&list_int));
    }

    #[test]
    fn test_display() {
        assert_eq!(Ty::<'_>::Int.to_string(), "int");
        assert_eq!(
            Ty::<'_>::Optional(Box::new(Ty::String)).to_string(),
            "string?"
        );
        assert_eq!(Ty::<'_>::List(Box::new(Ty::Int)).to_string(), "int[]");
        assert_eq!(
            Ty::<'_>::Union(vec![Ty::Int, Ty::String]).to_string(),
            "int | string"
        );
    }

    #[test]
    fn test_is_uninhabited() {
        // Unknown and Error are treated as uninhabited for error recovery
        assert!(Ty::<'_>::Unknown.is_uninhabited());
        assert!(Ty::<'_>::Error.is_uninhabited());

        // Empty union is uninhabited (no possible values)
        assert!(Ty::<'_>::Union(vec![]).is_uninhabited());

        // Non-empty union is inhabited
        assert!(!Ty::<'_>::Union(vec![Ty::Int]).is_uninhabited());
        assert!(!Ty::<'_>::Union(vec![Ty::Int, Ty::String]).is_uninhabited());

        // Regular types are inhabited
        assert!(!Ty::<'_>::Int.is_uninhabited());
        assert!(!Ty::<'_>::String.is_uninhabited());
        assert!(!Ty::<'_>::Bool.is_uninhabited());
        assert!(!Ty::<'_>::Null.is_uninhabited());
        assert!(!Ty::<'_>::Void.is_uninhabited());
        assert!(!Ty::<'_>::List(Box::new(Ty::Int)).is_uninhabited());
        assert!(!Ty::<'_>::Optional(Box::new(Ty::Int)).is_uninhabited());
    }
}
