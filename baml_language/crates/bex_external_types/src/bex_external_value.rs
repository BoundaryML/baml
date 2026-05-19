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
    Collector(bex_vm_types::CollectorRef),
    Type(baml_type::Ty),
    /// A rendered prompt AST (from `baml.llm.render_prompt`).
    PromptAst(std::sync::Arc<baml_builtins2::PromptAst>),
    /// A media value (image, audio, etc.) passed as a function argument.
    Media(std::sync::Arc<baml_builtins2::MediaValue>),
    /// GC-rooted reference to a heap instance, paired with the full
    /// type identity of the instance (class FQN + concrete generic args).
    ///
    /// `ty` is canonically a `Ty::Class { name, args }` — the same shape
    /// the wire encoder projects to `BamlTyName`. The `heap_handle` keeps
    /// the instance alive on the heap so the engine can re-enter it for
    /// instance-method calls (`Stream.next`, `Stream.final`, …).
    ///
    /// Currently used by `baml.llm.Stream`; any future stdlib generic
    /// class that wants typed-handle round-trip treatment uses this same
    /// variant.
    TaggedHeapHandle {
        ty: baml_type::Ty,
        heap_handle: crate::Handle,
    },
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
#[derive(Clone, Default)]
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

    /// Binary data (byte array).
    Uint8Array(Vec<u8>),

    /// Opaque Rust data for `$rust_type` fields.
    /// Engine converts to `Object::RustData` on the VM heap.
    RustData(std::sync::Arc<dyn std::any::Any + Send + Sync>),

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

impl std::fmt::Debug for BexExternalValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Int(v) => f.debug_tuple("Int").field(v).finish(),
            Self::Float(v) => f.debug_tuple("Float").field(v).finish(),
            Self::Bool(v) => f.debug_tuple("Bool").field(v).finish(),
            Self::String(v) => f.debug_tuple("String").field(v).finish(),
            Self::Array {
                element_type,
                items,
            } => f
                .debug_struct("Array")
                .field("element_type", element_type)
                .field("items", items)
                .finish(),
            Self::Map {
                key_type,
                value_type,
                entries,
            } => f
                .debug_struct("Map")
                .field("key_type", key_type)
                .field("value_type", value_type)
                .field("entries", entries)
                .finish(),
            Self::Instance { class_name, fields } => f
                .debug_struct("Instance")
                .field("class_name", class_name)
                .field("fields", fields)
                .finish(),
            Self::Variant {
                enum_name,
                variant_name,
            } => f
                .debug_struct("Variant")
                .field("enum_name", enum_name)
                .field("variant_name", variant_name)
                .finish(),
            Self::Union { value, metadata } => f
                .debug_struct("Union")
                .field("value", value)
                .field("metadata", metadata)
                .finish(),
            Self::Uint8Array(v) => f.debug_tuple("Uint8Array").field(v).finish(),
            Self::RustData(_) => write!(f, "RustData(...)"),
            Self::FunctionRef { global_index } => f
                .debug_struct("FunctionRef")
                .field("global_index", global_index)
                .finish(),
            Self::Handle(v) => f.debug_tuple("Handle").field(v).finish(),
            Self::Adt(v) => f.debug_tuple("Adt").field(v).finish(),
        }
    }
}

impl PartialEq for BexExternalValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Float(a), Self::Float(b)) => a == b,
            (Self::Bool(a), Self::Bool(b)) => a == b,
            (Self::String(a), Self::String(b)) => a == b,
            (
                Self::Array {
                    element_type: et1,
                    items: i1,
                },
                Self::Array {
                    element_type: et2,
                    items: i2,
                },
            ) => et1 == et2 && i1 == i2,
            (
                Self::Map {
                    key_type: k1,
                    value_type: v1,
                    entries: e1,
                },
                Self::Map {
                    key_type: k2,
                    value_type: v2,
                    entries: e2,
                },
            ) => k1 == k2 && v1 == v2 && e1 == e2,
            (
                Self::Instance {
                    class_name: c1,
                    fields: f1,
                },
                Self::Instance {
                    class_name: c2,
                    fields: f2,
                },
            ) => c1 == c2 && f1 == f2,
            (
                Self::Variant {
                    enum_name: e1,
                    variant_name: v1,
                },
                Self::Variant {
                    enum_name: e2,
                    variant_name: v2,
                },
            ) => e1 == e2 && v1 == v2,
            (
                Self::Union {
                    value: v1,
                    metadata: m1,
                },
                Self::Union {
                    value: v2,
                    metadata: m2,
                },
            ) => v1 == v2 && m1 == m2,
            (Self::Uint8Array(a), Self::Uint8Array(b)) => a == b,
            (Self::RustData(a), Self::RustData(b)) => std::sync::Arc::ptr_eq(a, b),
            (Self::FunctionRef { global_index: a }, Self::FunctionRef { global_index: b }) => {
                a == b
            }
            (Self::Handle(a), Self::Handle(b)) => a == b,
            (Self::Adt(a), Self::Adt(b)) => a == b,
            _ => false,
        }
    }
}

impl BexExternalAdt {
    pub fn type_name(&self) -> &'static str {
        match self {
            BexExternalAdt::Collector(_) => "collector",
            BexExternalAdt::Type(_) => "type",
            BexExternalAdt::PromptAst(_) => "prompt_ast",
            BexExternalAdt::Media(_) => "media",
            BexExternalAdt::TaggedHeapHandle { .. } => "tagged_heap_handle",
        }
    }
}

impl BexExternalValue {
    /// Construct a union value (`A | B | ...`) with metadata.
    ///
    /// ```ignore
    /// BexExternalValue::union(BexExternalValue::Int(42), [Ty::int(), Ty::string()], Ty::int())
    /// ```
    pub fn union(
        value: BexExternalValue,
        members: impl IntoIterator<Item = Ty>,
        selected: Ty,
    ) -> Self {
        let union_type = Ty::Union(members.into_iter().collect(), TyAttr::default());
        BexExternalValue::Union {
            value: Box::new(value),
            metadata: UnionMetadata::new(union_type, selected),
        }
    }

    /// Construct an optional value (`T?`) with metadata.
    ///
    /// Selected type is auto-detected: `inner` when non-null, `Ty::null()` when null.
    pub fn optional(value: BexExternalValue, inner: Ty) -> Self {
        let selected = if matches!(value, BexExternalValue::Null) {
            Ty::null()
        } else {
            inner.clone()
        };
        let optional_type = Ty::Optional(Box::new(inner), TyAttr::default());
        BexExternalValue::Union {
            value: Box::new(value),
            metadata: UnionMetadata::new(optional_type, selected),
        }
    }

    /// Construct an enum variant value.
    pub fn variant(enum_name: impl Into<String>, variant_name: impl Into<String>) -> Self {
        BexExternalValue::Variant {
            enum_name: enum_name.into(),
            variant_name: variant_name.into(),
        }
    }

    /// Construct a class instance value.
    pub fn instance(
        class_name: impl Into<String>,
        fields: IndexMap<&str, BexExternalValue>,
    ) -> Self {
        BexExternalValue::Instance {
            class_name: class_name.into(),
            fields: fields
                .into_iter()
                .map(|(k, v)| (k.to_string(), v))
                .collect(),
        }
    }

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
            BexExternalValue::Uint8Array(_) => "uint8array",
            BexExternalValue::RustData(_) => "rust_data",
            BexExternalValue::Adt(adt) => adt.type_name(),
            BexExternalValue::FunctionRef { .. } => "function",
            BexExternalValue::Handle(_) => "handle",
        }
    }

    /// Return the inner string if this is a `String` value, peeling off any
    /// surrounding `Union` wrapper.
    ///
    /// Useful when reading optional or union-typed fields from an `Instance`
    /// (e.g. `ScanOptions { cwd: string? }`): the runtime stores the field as
    /// `Union { value: String(...), .. }` for static-typed inputs and as
    /// `String(...)` for ad-hoc literals, and consumers don't usually care
    /// about that distinction.
    pub fn as_string(&self) -> Option<String> {
        match self {
            BexExternalValue::String(value) => Some(value.clone()),
            BexExternalValue::Union { value, .. } => value.as_string(),
            _ => None,
        }
    }

    /// Return the inner bool if this is a `Bool` value, peeling off any
    /// surrounding `Union` wrapper. See [`Self::as_string`].
    pub fn as_bool(&self) -> Option<bool> {
        match self {
            BexExternalValue::Bool(value) => Some(*value),
            BexExternalValue::Union { value, .. } => value.as_bool(),
            _ => None,
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

impl AsBexExternalValue for baml_type::Ty {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Type(self))
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

impl AsBexExternalValue for indexmap::IndexMap<String, String> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Map {
            key_type: baml_type::Ty::string(),
            value_type: baml_type::Ty::string(),
            entries: self
                .into_iter()
                .map(|(k, v)| (k, v.into_bex_external_value()))
                .collect(),
        }
        .into_bex_external_value()
    }
}

impl AsBexExternalValue for indexmap::IndexMap<String, BexExternalValue> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Map {
            key_type: baml_type::Ty::string(),
            value_type: baml_type::Ty::unknown(),
            entries: self,
        }
        .into_bex_external_value()
    }
}

impl AsBexExternalValue for Vec<String> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: baml_type::Ty::string(),
            items: self.into_iter().map(BexExternalValue::String).collect(),
        }
        .into_bex_external_value()
    }
}

impl AsBexExternalValue for Vec<u8> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Uint8Array(self)
    }
}

/// Trait for opaque Rust data stored in `Object::RustData` that knows how to
/// convert itself to a [`BexExternalValue`].
///
/// Implement this for any type that is stored as `RustData` on the VM heap and
/// needs to survive the VM-to-external conversion boundary (e.g. `PromptAst`,
/// `MediaValue`).
pub trait ToBexExternalValue: std::any::Any + Send + Sync {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue;
}

impl ToBexExternalValue for baml_builtins2::PromptAst {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::PromptAst(self))
    }
}

impl ToBexExternalValue for baml_builtins2::MediaValue {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Media(self))
    }
}

/// Try to convert an `Arc<dyn Any + Send + Sync>` from `Object::RustData` to a
/// [`BexExternalValue`] by attempting downcast to known [`ToBexExternalValue`]
/// implementors.
///
/// Returns `None` if the concrete type is not recognised.
pub fn try_convert_rust_data(
    arc: &std::sync::Arc<dyn std::any::Any + Send + Sync>,
) -> Option<BexExternalValue> {
    if let Ok(typed) = arc.clone().downcast::<baml_builtins2::PromptAst>() {
        return Some(typed.to_bex_external_value());
    }
    if let Ok(typed) = arc.clone().downcast::<baml_builtins2::MediaValue>() {
        return Some(typed.to_bex_external_value());
    }
    None
}
