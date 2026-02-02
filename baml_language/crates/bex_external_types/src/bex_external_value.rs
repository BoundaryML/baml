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

// Re-export Ty from baml_type for convenience
pub use baml_type::Ty;
use indexmap::IndexMap;
use sys_resource_types::ResourceHandle;

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
            Ty::Union(members) => {
                let has_null = members.iter().any(|m| matches!(m, Ty::Null));
                let non_null_count = members.iter().filter(|m| !matches!(m, Ty::Null)).count();
                (has_null, non_null_count == 1)
            }
            Ty::Optional(_) => (true, true),
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

/// A deep-copied value tree with no heap references.
///
/// Use `BexEngine::call_function` to get the result. When the return type
/// is a union, the value will be wrapped in the `Union` variant with metadata.
///
/// # When to use BexExternalValue vs BexValue
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

    Media {
        handle: crate::Handle,
        kind: baml_base::MediaKind,
    },

    /// Resource handle (file, socket, etc.) for sys operations.
    Resource(ResourceHandle),

    /// Prompt AST - a structured prompt for LLM calls.
    PromptAst(PromptAst),

    /// Primitive LLM client.
    PrimitiveClient(PrimitiveClientValue),

    /// Reference to a function by its global index.
    ///
    /// Used to return callable function references from SysOps.
    /// The global_index corresponds to the function's position in the VM's globals array.
    FunctionRef {
        /// Global index of the function.
        global_index: usize,
    },
}

/// Extracted PrimitiveClient data (no HeapPtr, fully owned).
#[derive(Clone, Debug, PartialEq)]
pub struct PrimitiveClientValue {
    /// Client name (e.g., "GPT4").
    pub name: String,
    /// Provider type (e.g., "openai", "anthropic").
    pub provider: String,
    /// Default role for chat messages (e.g., "user").
    pub default_role: String,
    /// Allowed roles for chat messages.
    pub allowed_roles: Vec<String>,
    /// Options extracted as a map (was HeapPtr in VM).
    pub options: indexmap::IndexMap<String, BexExternalValue>,
}

/// Prompt AST - a structured prompt for LLM calls.
/// This is a copy of bex_vm_types::PromptAst but with no HeapPtr references.
#[derive(Clone, Debug, PartialEq)]
pub enum PromptAst {
    /// A plain string.
    String(String),

    /// A media value - serializable opaque handle.
    Media(usize),

    /// A message with a role, content, and optional metadata.
    Message {
        role: String,
        content: Box<PromptAst>,
        /// Metadata stored as extracted value.
        metadata: Box<BexExternalValue>,
    },

    /// A sequence of prompt nodes.
    Vec(Vec<PromptAst>),
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
            BexExternalValue::Media { kind, .. } => match kind {
                baml_base::MediaKind::Image => "image",
                baml_base::MediaKind::Audio => "audio",
                baml_base::MediaKind::Video => "video",
                baml_base::MediaKind::Pdf => "pdf",
                baml_base::MediaKind::Generic => "media",
            },
            BexExternalValue::Resource(handle) => match handle.kind() {
                sys_resource_types::ResourceType::File => "file",
                sys_resource_types::ResourceType::Socket => "socket",
                sys_resource_types::ResourceType::Response => "http-response",
            },
            BexExternalValue::PromptAst(_) => "prompt_ast",
            BexExternalValue::PrimitiveClient(_) => "primitive_client",
            BexExternalValue::FunctionRef { .. } => "function",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_resource_variant_construction() {
        // Test that we can construct a Resource variant with an opaque handle
        let handle = sys_resource_types::ResourceHandle::new_without_cleanup(
            1,
            sys_resource_types::ResourceType::File,
            "test.txt".to_string(),
        );

        let resource = BexExternalValue::Resource(handle);

        // Test type_name returns "file"
        assert_eq!(resource.type_name(), "file");
    }

    #[test]
    fn test_resource_socket_type_name() {
        let handle = sys_resource_types::ResourceHandle::new_without_cleanup(
            2,
            sys_resource_types::ResourceType::Socket,
            "localhost:8080".to_string(),
        );

        let resource = BexExternalValue::Resource(handle);

        // Test type_name returns "socket"
        assert_eq!(resource.type_name(), "socket");
    }
}
