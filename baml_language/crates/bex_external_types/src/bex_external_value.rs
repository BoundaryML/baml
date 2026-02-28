//! BexExternalValue - owned value tree for FFI.
//!
//! `BexExternalValue` is a fully-owned value tree with no heap references.
//! Use when you need to traverse and convert the entire object graph,
//! such as for FFI conversion to Python/JS objects.
//!
//! # Union Types
//!
//! When a value comes from a union type (e.g., `int | string` or `Success | Failure`),
//! it's wrapped in the `Union` variant with metadata about the union:
//!
//! ```ignore
//! // Function returns Success | Failure
//! let result: BexExternalValue = engine.call_function("GetStatus", &[]).await?;
//!
//! match result {
//!     BexExternalValue::Union { value, metadata } => {
//!         println!("Selected: {}", metadata.selected_option);
//!         println!("Could have been: {:?}", metadata.union_type);
//!     }
//!     _ => {}
//! }
//! ```

// Re-export Ty and TypeName from baml_type for convenience
pub use baml_type::{Ty, TyAttr, TypeName};
use bex_resource_types::ResourceHandle;
use indexmap::IndexMap;

/// Metadata about a union type, embedded with values from union-typed contexts.
///
/// This mirrors `CFFIValueUnionVariant` from the CFFI protocol, enabling
/// easy serialization for FFI consumers.
#[derive(Clone, Debug, PartialEq)]
pub struct UnionMetadata {
    /// Name of the union type (for named type aliases like `type Result = Success | Failure`).
    pub name: Option<String>,

    /// Whether this union is optional (T?).
    /// An optional type `T?` is equivalent to `T | null`.
    pub is_optional: bool,

    /// Whether there's only one non-null option in the union.
    /// This simplifies FFI handling - languages can unwrap directly.
    pub is_single_pattern: bool,

    /// The full union type for serialization.
    pub union_type: Ty,

    /// Which option of the union was selected (e.g., `Ty::Int`, `Ty::String`, `Ty::Class("Success")`).
    pub selected_option: Ty,
}

impl UnionMetadata {
    /// Create metadata for a union type.
    pub fn new(union_type: Ty, selected_option: Ty) -> Self {
        let (is_optional, is_single_pattern) = match &union_type {
            Ty::Union(members, _) => {
                let has_null = members.iter().any(|m| matches!(m, Ty::Null { .. }));
                let non_null_count = members
                    .iter()
                    .filter(|m| !matches!(m, Ty::Null { .. }))
                    .count();
                (has_null, non_null_count == 1)
            }
            Ty::Optional(..) => (true, true),
            _ => (false, false),
        };

        Self {
            name: None,
            is_optional,
            is_single_pattern,
            union_type,
            selected_option,
        }
    }

    /// Set the name for this union (for named type aliases).
    pub fn with_name(mut self, name: impl Into<String>) -> Self {
        self.name = Some(name.into());
        self
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BexExternalAdt {
    Media(bex_vm_types::MediaValue),
    PromptAst(bex_vm_types::PromptAst),
    Collector(bex_vm_types::CollectorRef),
    Type(baml_type::Ty),
}

/// A deep-copied value tree with no heap references.
///
/// Use `BexEngine::call_function` to get the result. When the return type
/// is a union, the value will be wrapped in the `Union` variant with metadata.
///
/// # When to use BexValue vs BexExternalValue
///
/// - **BexValue**: When you want to keep data in the heap and access lazily.
///   Good for passing handles across FFI without copying.
///
/// - **BexExternalValue**: When you need to convert the entire value to another format
///   (Python objects, JSON, etc.). Since you're traversing anyway, might as
///   well have owned data.
#[derive(Clone, Debug, PartialEq, Default)]
pub enum BexExternalValue {
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

    /// Owned array of values with element type.
    Array {
        /// The declared element type (e.g., `int | string` for `(int | string)[]`).
        element_type: Ty,
        /// The array items.
        items: Vec<BexExternalValue>,
    },

    /// Owned map with string keys and type information.
    Map {
        /// The declared key type (usually `Ty::String`).
        key_type: Ty,
        /// The declared value type (e.g., `int | string` for `map<string, int | string>`).
        value_type: Ty,
        /// The map entries.
        entries: IndexMap<String, BexExternalValue>,
    },

    /// Class instance with class name and field values.
    Instance {
        class_name: String,
        fields: IndexMap<String, BexExternalValue>,
    },

    /// Enum variant with enum name and variant name.
    Variant {
        enum_name: String,
        variant_name: String,
    },

    /// Value from a union type with metadata.
    ///
    /// When the declared type is a union (e.g., `int | string`), the actual
    /// value is wrapped with metadata about the union for FFI serialization.
    Union {
        /// The actual value (one of the union options).
        value: Box<BexExternalValue>,
        /// Metadata about the union type.
        metadata: UnionMetadata,
    },

    /// Resource handle (file, socket, etc.) for sys operations.
    Resource(ResourceHandle),

    /// Reference to a function by its global index.
    ///
    /// Used to return callable function references from SysOps.
    /// The global_index corresponds to the function's position in the VM's globals array.
    FunctionRef {
        /// Global index of the function.
        global_index: usize,
    },

    Handle(crate::Handle),

    // This is a tagged union.
    // Once BAML has support for ADTs, we can remove this
    // and use instances of ADT variants directly similar to how we handle
    // builtin classes and enums.
    Adt(BexExternalAdt),
}

impl BexExternalAdt {
    pub fn type_name(&self) -> &'static str {
        match self {
            BexExternalAdt::Media(media) => match media.kind {
                baml_type::MediaKind::Image => "image",
                baml_type::MediaKind::Audio => "audio",
                baml_type::MediaKind::Video => "video",
                baml_type::MediaKind::Pdf => "pdf",
                baml_type::MediaKind::Generic => "media",
            },
            BexExternalAdt::PromptAst(_) => "prompt_ast",
            BexExternalAdt::Collector(_) => "collector",
            BexExternalAdt::Type(_) => "type",
        }
    }
}

impl BexExternalValue {
    /// Get the type name for error messages.
    pub fn type_name(&self) -> &'static str {
        match self {
            BexExternalValue::Null => "null",
            BexExternalValue::Int(_) => "int",
            BexExternalValue::Float(_) => "float",
            BexExternalValue::Bool(_) => "bool",
            BexExternalValue::String(_) => "string",
            BexExternalValue::Array { .. } => "array",
            BexExternalValue::Map { .. } => "map",
            BexExternalValue::Instance { .. } => "instance",
            BexExternalValue::Variant { .. } => "variant",
            BexExternalValue::Union { .. } => "union",
            BexExternalValue::Resource(handle) => match handle.kind() {
                bex_resource_types::ResourceType::File => "file",
                bex_resource_types::ResourceType::Socket => "socket",
                bex_resource_types::ResourceType::Response => "http-response",
            },
            BexExternalValue::Adt(adt) => adt.type_name(),
            BexExternalValue::FunctionRef { .. } => "function",
            BexExternalValue::Handle(_) => "handle",
        }
    }
}

impl From<i64> for BexExternalValue {
    fn from(value: i64) -> Self {
        BexExternalValue::Int(value)
    }
}

impl From<f64> for BexExternalValue {
    fn from(value: f64) -> Self {
        BexExternalValue::Float(value)
    }
}

impl From<bool> for BexExternalValue {
    fn from(value: bool) -> Self {
        BexExternalValue::Bool(value)
    }
}

impl From<crate::Handle> for BexExternalValue {
    fn from(value: crate::Handle) -> Self {
        BexExternalValue::Handle(value)
    }
}

impl From<String> for BexExternalValue {
    fn from(value: String) -> Self {
        BexExternalValue::String(value)
    }
}

impl From<&str> for BexExternalValue {
    fn from(value: &str) -> Self {
        BexExternalValue::String(value.to_string())
    }
}

/// Trait for types that can be converted to a [`BexExternalValue`].
///
/// Implemented by owned builtin types (`FsFile`, `HttpResponse`, etc.)
/// and simple types (`String`, `bool`, `()`).
///
/// Used by `SysOpOutput<T>::into_result()` to convert typed results
/// back to the common `BexExternalValue` representation.
pub trait AsBexExternalValue {
    fn into_bex_external_value(self) -> BexExternalValue;
}

impl AsBexExternalValue for BexExternalValue {
    fn into_bex_external_value(self) -> BexExternalValue {
        self
    }
}

impl AsBexExternalValue for () {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Null
    }
}

impl AsBexExternalValue for i64 {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Int(self)
    }
}

impl AsBexExternalValue for f64 {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Float(self)
    }
}

impl AsBexExternalValue for String {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::String(self)
    }
}

impl AsBexExternalValue for bool {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Bool(self)
    }
}

impl AsBexExternalValue for bex_vm_types::PromptAst {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::PromptAst(self))
    }
}

impl AsBexExternalValue for baml_type::Ty {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Type(self))
    }
}

impl AsBexExternalValue for bex_vm_types::MediaValue {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Media(self))
    }
}

impl<T: AsBexExternalValue> AsBexExternalValue for Option<T> {
    fn into_bex_external_value(self) -> BexExternalValue {
        match self {
            Some(v) => v.into_bex_external_value(),
            None => BexExternalValue::Null,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_variant_construction() {
        // Test that we can construct a Resource variant with an opaque handle
        let handle = bex_resource_types::ResourceHandle::new_without_cleanup(
            1,
            bex_resource_types::ResourceType::File,
            "test.txt".to_string(),
        );

        let resource = BexExternalValue::Resource(handle);

        // Test type_name returns "file"
        assert_eq!(resource.type_name(), "file");
    }

    #[test]
    fn test_resource_socket_type_name() {
        let handle = bex_resource_types::ResourceHandle::new_without_cleanup(
            2,
            bex_resource_types::ResourceType::Socket,
            "localhost:8080".to_string(),
        );

        let resource = BexExternalValue::Resource(handle);

        // Test type_name returns "socket"
        assert_eq!(resource.type_name(), "socket");
    }
}
