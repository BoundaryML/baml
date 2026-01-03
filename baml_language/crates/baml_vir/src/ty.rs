//! Type system for VIR.
//!
//! Types are fully resolved - no unresolved references. Class and Enum IDs
//! from TIR are resolved to their names during lowering.

use std::fmt;

use baml_base::Name;

/// A resolved type in BAML.
///
/// Unlike `baml_tir::Ty` which may contain `ClassId` and `EnumId` references,
/// this type is fully resolved with all names known.
#[derive(Debug, Clone, PartialEq, Eq, Hash)]
pub enum Ty {
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
    Union(Vec<Ty>),

    /// Function/arrow type: `(T1, T2, ...) -> R`
    Function {
        params: Vec<Ty>,
        ret: Box<Ty>,
    },

    // Special types
    /// Unknown type (for error recovery).
    Unknown,
    /// Error type.
    Error,
    /// Void/Unit type - the type of effectful expressions.
    Unit,
    /// Never type - the type of diverging expressions (return, break, continue).
    Never,

    /// Watch accessor type: represents `x.$watch` on a watched variable.
    WatchAccessor(Box<Ty>),
}

impl Ty {
    /// Check if this is the unit type.
    pub fn is_unit(&self) -> bool {
        matches!(self, Ty::Unit)
    }

    /// Check if this is the never type.
    pub fn is_never(&self) -> bool {
        matches!(self, Ty::Never)
    }

    /// Check if this type is unknown.
    pub fn is_unknown(&self) -> bool {
        matches!(self, Ty::Unknown)
    }

    /// Check if this type is an error type.
    pub fn is_error(&self) -> bool {
        matches!(self, Ty::Error)
    }

    /// Check if this is a primitive type.
    pub fn is_primitive(&self) -> bool {
        matches!(self, Ty::Int | Ty::Float | Ty::String | Ty::Bool | Ty::Null)
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

        // Never is a subtype of everything (diverging expressions)
        if self.is_never() {
            return true;
        }

        match (self, other) {
            // Null is a subtype of Optional<T>
            (Ty::Null, Ty::Optional(_)) => true,

            // T is a subtype of Optional<T>
            (inner, Ty::Optional(opt_inner)) => inner.is_subtype_of(opt_inner),

            // T is a subtype of T | U (union containing T)
            (inner, Ty::Union(types)) => types.iter().any(|t| inner.is_subtype_of(t)),

            // Union<T1, T2> is a subtype of U if all Ti are subtypes of U
            (Ty::Union(types), other) => types.iter().all(|t| t.is_subtype_of(other)),

            // List covariance
            (Ty::List(inner1), Ty::List(inner2)) => inner1.is_subtype_of(inner2),

            // Map covariance in value (key invariant)
            (Ty::Map { key: k1, value: v1 }, Ty::Map { key: k2, value: v2 }) => {
                k1 == k2 && v1.is_subtype_of(v2)
            }

            // Int is a subtype of Float (numeric widening)
            (Ty::Int, Ty::Float) => true,

            _ => false,
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
            Ty::Image => write!(f, "image"),
            Ty::Audio => write!(f, "audio"),
            Ty::Video => write!(f, "video"),
            Ty::Pdf => write!(f, "pdf"),
            Ty::Class(name) => write!(f, "{name}"),
            Ty::Enum(name) => write!(f, "{name}"),
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
            Ty::Unit => write!(f, "unit"),
            Ty::Never => write!(f, "never"),
            Ty::WatchAccessor(inner) => write!(f, "{inner}.$watch"),
        }
    }
}
