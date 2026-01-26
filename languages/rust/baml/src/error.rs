use std::collections::HashMap;

/// BAML runtime errors
///
/// Note: This is intentionally minimal. Expand with specific variants
/// (`InitError`, `CallError`, etc.) once the core functionality works.
#[derive(Debug, thiserror::Error, Clone)]
pub enum BamlError {
    /// Internal/unexpected errors - bugs in BAML that should never happen
    #[error("internal error: {0}")]
    Internal(String),

    /// Type check errors - expected runtime failures (type mismatches)
    #[error("type error: expected {expected}, got {got}")]
    TypeCheck { expected: String, got: String },
}

/// Trait for types that can report their full type name for error messages.
/// Used by `BamlValue` variants to provide descriptive "got" values.
pub trait FullTypeName {
    fn full_type_name(&self) -> String;
}

impl BamlError {
    /// Create an internal error for unexpected bugs
    pub fn internal(msg: impl Into<String>) -> Self {
        BamlError::Internal(msg.into())
    }

    /// Create a type check error for expected runtime type mismatches.
    ///
    /// - `T`: The expected type (must implement `BamlTypeName`)
    /// - `got`: The actual value (must implement `FullTypeName`)
    ///
    /// This enforces good practice at compile time - you can't accidentally
    /// use hardcoded strings for type names.
    ///
    /// # Example
    /// ```ignore
    /// // Instead of: BamlError::type_check("String", other.full_type_name())
    /// // Use:        BamlError::type_check::<String>(&other)
    /// match value {
    ///     BamlValue::String(s) => Ok(s),
    ///     other => Err(BamlError::type_check::<String>(&other)),
    /// }
    /// ```
    pub fn type_check<T: BamlTypeName>(got: &impl FullTypeName) -> Self {
        BamlError::TypeCheck {
            expected: T::baml_type_name(),
            got: got.full_type_name(),
        }
    }
}

/// Trait for types that have a BAML type name.
/// This provides consistent type names for error messages.
///
/// Type names should be descriptive:
/// - Primitives: "String", "Int", "Float", "Bool", "Null"
/// - Containers: "List<String>", "Map<String, Int>", "Optional<Person>"
/// - Wrappers: "Checked<String>", "`StreamState`<Person>"
/// - Dynamic: "DynamicClass(PersonInfo)", "DynamicEnum(Sentiment)",
///   "DynamicUnion(unknown)"
pub trait BamlTypeName {
    /// The base BAML type name (e.g., "String", "Int", "List", "Map")
    /// For primitives this is the full name; for containers it's just the
    /// container name.
    const BASE_TYPE_NAME: &'static str;

    /// Get the full type name including generic parameters.
    /// Default implementation returns `BASE_TYPE_NAME` for non-generic types.
    fn baml_type_name() -> String {
        Self::BASE_TYPE_NAME.to_string()
    }
}

// Primitive type names (base name = full name)
impl BamlTypeName for String {
    const BASE_TYPE_NAME: &'static str = "String";
}
impl BamlTypeName for i64 {
    const BASE_TYPE_NAME: &'static str = "Int";
}
impl BamlTypeName for f64 {
    const BASE_TYPE_NAME: &'static str = "Float";
}
impl BamlTypeName for bool {
    const BASE_TYPE_NAME: &'static str = "Bool";
}
impl BamlTypeName for () {
    const BASE_TYPE_NAME: &'static str = "Null";
}

// Reference types delegate to their inner type
impl BamlTypeName for &str {
    const BASE_TYPE_NAME: &'static str = "String";
}

// Container type names (include element type)
impl<T: BamlTypeName> BamlTypeName for Vec<T> {
    const BASE_TYPE_NAME: &'static str = "List";

    fn baml_type_name() -> String {
        format!("List<{}>", T::baml_type_name())
    }
}

impl<T: BamlTypeName> BamlTypeName for Option<T> {
    const BASE_TYPE_NAME: &'static str = "Optional";

    fn baml_type_name() -> String {
        format!("Optional<{}>", T::baml_type_name())
    }
}

impl<V: BamlTypeName> BamlTypeName for HashMap<String, V> {
    const BASE_TYPE_NAME: &'static str = "Map";

    fn baml_type_name() -> String {
        format!("Map<String, {}>", V::baml_type_name())
    }
}

// Wrapper types (include inner type)
impl<T: BamlTypeName> BamlTypeName for crate::types::Checked<T> {
    const BASE_TYPE_NAME: &'static str = "Checked";

    fn baml_type_name() -> String {
        format!("Checked<{}>", T::baml_type_name())
    }
}

impl<T: BamlTypeName> BamlTypeName for crate::types::StreamState<T> {
    const BASE_TYPE_NAME: &'static str = "StreamState";

    fn baml_type_name() -> String {
        format!("StreamState<{}>", T::baml_type_name())
    }
}

// Box delegates to inner type
impl<T: BamlTypeName> BamlTypeName for Box<T> {
    const BASE_TYPE_NAME: &'static str = T::BASE_TYPE_NAME;

    fn baml_type_name() -> String {
        T::baml_type_name()
    }
}

// NOTE: BamlValue does NOT implement BamlTypeName because:
// 1. Its type name is runtime-determined (depends on the actual variant)
// 2. It would create a circular dependency (error.rs -> codec -> error.rs)
// Instead, BamlValue implements FullTypeName (instance method) in baml_value.rs

/// Marker type for unknown/dynamic inner types.
/// Use this when the inner type of a container is not known at compile time.
/// Example: `Checked<Unknown>` represents "Checked<?>"
pub(crate) struct Unknown;

impl BamlTypeName for Unknown {
    const BASE_TYPE_NAME: &'static str = "?";
}

// Slice reference type name (for raw list access)
impl<T> BamlTypeName for &[T] {
    const BASE_TYPE_NAME: &'static str = "List";

    fn baml_type_name() -> String {
        // We don't know the element type at compile time for raw BamlValue slices
        "List<?>".to_string()
    }
}

// HashMap reference type name (for raw map access)
impl<K, V> BamlTypeName for &HashMap<K, V> {
    const BASE_TYPE_NAME: &'static str = "Map";

    fn baml_type_name() -> String {
        // We don't know the value type at compile time for raw BamlValue maps
        "Map<String, ?>".to_string()
    }
}

/// Panics with a user-friendly error message for internal/unreachable errors.
///
/// This macro is used for situations that should never occur in practice -
/// bugs in the FFI boundary, protocol mismatches, etc. The error message
/// guides users to report the issue.
///
/// # Examples
/// ```ignore
/// // Simple message
/// baml_unreachable!("unexpected null pointer");
///
/// // With format args
/// baml_unreachable!("unknown object type: {:?}", obj_type);
/// ```
#[macro_export]
macro_rules! baml_unreachable {
    ($($arg:tt)*) => {{
        panic!(
            "\n\n\
            ========================================\n\
            BAML Internal Error\n\
            ========================================\n\n\
            {}\n\n\
            This is a bug in BAML. Please report it:\n\
            - GitHub: https://github.com/BoundaryML/baml/issues\n\
            - Discord: https://boundaryml.com/discord\n\n\
            Include this error message and steps to reproduce.\n\
            ========================================\n",
            format_args!($($arg)*)
        )
    }};
}
