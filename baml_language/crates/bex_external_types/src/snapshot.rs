//! Snapshot type for deep-copied value trees.
//!
//! `Snapshot` is a fully-owned value tree with no heap references.
//! Use when you need to traverse and convert the entire object graph,
//! such as for FFI conversion to Python/JS objects.
//!
//! Unlike `ExternalValue` which uses `Handle` for heap objects,
//! `Snapshot` contains owned copies of all data.
//!
//! # Typed Snapshots
//!
//! `TypedSnapshot` pairs a `Snapshot` value with its declared type from
//! the BAML schema. This is useful when the declared type is a union -
//! the caller can see what alternatives the value could have been.
//!
//! ```ignore
//! // Function returns Success | Failure
//! let result: TypedSnapshot = engine.call_function("GetStatus", &[]).await?;
//!
//! // result.value = Snapshot::Instance { class_name: "Success", ... }
//! // result.declared_type = Ty::Union([Ty::Class("Success"), Ty::Class("Failure")])
//! ```

// Re-export Ty from baml_snapshot for convenience
pub use baml_snapshot::Ty;
use indexmap::IndexMap;

/// A snapshot paired with its declared type from the BAML schema.
///
/// This is the primary return type from `BexEngine::call_function`. It contains
/// both the runtime value and the declared type, which is useful when the type
/// is a union - callers can see what alternatives the value could have been.
///
/// # Example
///
/// ```ignore
/// // Function signature: fn GetStatus() -> Success | Failure
/// let result: TypedSnapshot = engine.call_function("GetStatus", &[]).await?;
///
/// match &result.value {
///     Snapshot::Instance { class_name, .. } => {
///         println!("Got: {}", class_name);
///         // Check what else it could have been:
///         if let Ty::Union(alternatives) = &result.declared_type {
///             println!("Could have been: {:?}", alternatives);
///         }
///     }
///     _ => {}
/// }
/// ```
#[derive(Clone, Debug, PartialEq)]
pub struct TypedSnapshot {
    /// The actual value.
    pub value: Snapshot,
    /// The declared type from the schema (may be a union).
    pub declared_type: Ty,
}

impl TypedSnapshot {
    /// Create a new TypedSnapshot.
    pub fn new(value: Snapshot, declared_type: Ty) -> Self {
        Self {
            value,
            declared_type,
        }
    }
}

/// A deep-copied value tree with no heap references.
///
/// Use `BexEngine::call_function` to get a `TypedSnapshot` which contains
/// both the value and its declared type.
///
/// # When to use Snapshot vs ExternalValue
///
/// - **ExternalValue**: When you want to keep data in the heap and access lazily.
///   Good for passing handles across FFI without copying.
///
/// - **Snapshot**: When you need to convert the entire value to another format
///   (Python objects, JSON, etc.). Since you're traversing anyway, might as
///   well have owned data.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum Snapshot {
    /// Null value.
    #[default]
    Null,

    /// 64-bit signed integer.
    Int(i64),

    /// 64-bit floating point.
    Float(f64),

    /// Boolean value.
    Bool(bool),

    /// Owned string.
    String(String),

    /// Owned array of typed snapshots.
    Array(Vec<TypedSnapshot>),

    /// Owned map with string keys and typed snapshot values.
    Map(IndexMap<String, TypedSnapshot>),

    /// Class instance with class name and typed field values.
    Instance {
        class_name: String,
        fields: IndexMap<String, TypedSnapshot>,
    },

    /// Enum variant with enum name and variant name.
    Variant {
        enum_name: String,
        variant_name: String,
    },
}

impl Snapshot {
    /// Get the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            Snapshot::Null => "null",
            Snapshot::Int(_) => "int",
            Snapshot::Float(_) => "float",
            Snapshot::Bool(_) => "bool",
            Snapshot::String(_) => "string",
            Snapshot::Array(_) => "array",
            Snapshot::Map(_) => "map",
            Snapshot::Instance { .. } => "instance",
            Snapshot::Variant { .. } => "variant",
        }
    }
}
