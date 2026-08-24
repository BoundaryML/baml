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

// Re-export RuntimeTy and TypeName from baml_type for convenience
pub use baml_type::{RuntimeTy, TyAttr, TypeName};
use indexmap::IndexMap;

/// Metadata about a union type, embedded with values from union-typed contexts.
///
/// This mirrors `CFFIValueUnionVariant` from the CFFI protocol, enabling
/// easy serialization for FFI consumers.
#[derive(Clone, Debug, PartialEq)]
pub struct UnionMetadata {
    /// Whether this is the transient engine carrier for a sparse inbound
    /// `InboundValue.value_type` annotation, rather than a value produced from
    /// a declared union. The annotation carries only `selected_option`; the
    /// contextual declared type supplies any enclosing union.
    pub is_inbound_type_annotation: bool,

    /// Name of the union type (for named type aliases like `type Result = Success | Failure`).
    pub name: Option<String>,

    /// Whether this union is optional (T?).
    /// An optional type `T?` is equivalent to `T | null`.
    pub is_optional: bool,

    /// Whether there's only one non-null option in the union.
    /// This simplifies FFI handling - languages can unwrap directly.
    pub is_single_pattern: bool,

    /// The full union type for serialization.
    pub union_type: RuntimeTy,

    /// Which option of the union was selected (e.g., `RuntimeTy::Int`, `RuntimeTy::String`, `RuntimeTy::Class("Success")`).
    pub selected_option: RuntimeTy,
}

impl UnionMetadata {
    /// Create metadata for a union type.
    pub fn new(union_type: RuntimeTy, selected_option: RuntimeTy) -> Self {
        let (is_optional, is_single_pattern) = match &union_type {
            RuntimeTy::Union(members, _) => {
                let is_optional = members.iter().any(RuntimeTy::is_null);
                let non_null_count = members.iter().filter(|member| !member.is_null()).count();
                (is_optional, non_null_count == 1)
            }
            _ => (false, false),
        };

        Self {
            is_inbound_type_annotation: false,
            name: None,
            is_optional,
            is_single_pattern,
            union_type,
            selected_option,
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub enum BexExternalAdt {
    Collector(bex_vm_types::CollectorRef),
    /// A reflected type, carried at the sys-op lane's head so a definition
    /// table can be keyed by declaration identity rather than by name.
    ///
    /// Deliberately pointer-free: the collector can run while a sys-op is
    /// awaiting, so anything a sys-op holds across that boundary must survive
    /// objects moving. A tag does; an address does not.
    Type(baml_type::RuntimeTy<baml_type::TaggedTypeName>),
    /// A reflected type carrying runtime definitions.
    ///
    /// The two arms are the two things a `type` value can be at a boundary,
    /// made explicit rather than inferred:
    ///
    /// * [`Live`](TypeDefRef::Live) — a rooted reference back to the very
    ///   `Object::Type` that produced it. Valid only in the engine that issued
    ///   the handle, which is why it also carries the portable definitions:
    ///   anywhere else (another engine, another process) it degrades to them.
    /// * [`Portable`](TypeDefRef::Portable) — definitions alone, reconstructed
    ///   into fresh heap objects on arrival.
    ///
    /// Wire encoders serialize the portable form in both cases: a handle is a
    /// live capability, not data, so it cannot cross a process (BEP-066 H-4).
    TypeDef(TypeDefRef),
    /// The Rust-backed payload inside a rendered `ai.Prompt`.
    PromptAst(std::sync::Arc<baml_builtins2::PromptAst>),
    /// A media value (image, audio, etc.) passed as a function argument.
    Media(std::sync::Arc<baml_builtins2::MediaValue>),
    /// GC-rooted reference to a heap instance, paired with the full
    /// type identity of the instance (class FQN + concrete generic args).
    ///
    /// `ty` is canonically a `RuntimeTy::Class { name, args }` — the same shape
    /// the wire encoder projects to `BamlTyName`. The `heap_handle` keeps
    /// the instance alive on the heap so the engine can re-enter it for
    /// instance-method calls (`Stream.next`, `Stream.final`, …).
    ///
    /// Currently used by `ai.stream.Stream`; any future stdlib generic
    /// class that wants typed-handle round-trip treatment uses this same
    /// variant.
    TaggedHeapHandle {
        ty: baml_type::RuntimeTy,
        heap_handle: crate::Handle,
    },
}

/// How a `type` value crosses a boundary: as a live reference into the issuing
/// engine, or as portable definitions.
///
/// Both arms carry `def`, so every consumer can read the type's shape without
/// caring which arm it got; only identity differs.
#[derive(Clone, Debug, PartialEq)]
pub enum TypeDefRef {
    /// Same-engine: `handle` resolves to the originating `Object::Type`, so a
    /// value that leaves and returns is the *same* object — its identity,
    /// definitions and provenance are untouched. The handle also keeps that
    /// object rooted for as long as this value lives.
    Live {
        handle: crate::Handle,
        def: bex_vm_types::types::PortableTypeDef,
    },
    /// Definitions only: the receiving engine reconstructs heap objects and
    /// assigns fresh identity. The form every cross-process payload takes.
    Portable(bex_vm_types::types::PortableTypeDef),
}

impl TypeDefRef {
    /// The portable definitions, whichever arm this is.
    #[must_use]
    pub fn def(&self) -> &bex_vm_types::types::PortableTypeDef {
        match self {
            Self::Live { def, .. } | Self::Portable(def) => def,
        }
    }

    /// Consume into the portable definitions, dropping any live reference.
    /// This is what a wire encoder does.
    #[must_use]
    pub fn into_def(self) -> bex_vm_types::types::PortableTypeDef {
        match self {
            Self::Live { def, .. } | Self::Portable(def) => def,
        }
    }
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

    /// Arbitrary-precision signed integer.
    Bigint(num_bigint::BigInt),

    /// 64-bit floating point.
    Float(f64),

    /// Boolean value.
    Bool(bool),

    /// Owned string.
    String(bex_str::BexStr),

    /// Owned array of values with element type.
    Array {
        /// The declared element type (e.g., `int | string` for `(int | string)[]`).
        element_type: RuntimeTy,
        /// The array items.
        items: Vec<BexExternalValue>,
    },

    /// Owned map with string keys and type information.
    Map {
        /// The declared key type (usually `RuntimeTy::String`).
        key_type: RuntimeTy,
        /// The declared value type (e.g., `int | string` for `map<string, int | string>`).
        value_type: RuntimeTy,
        /// The map entries.
        entries: IndexMap<String, BexExternalValue>,
    },

    /// Class instance with class name and field values.
    Instance {
        class_name: String,
        /// Concrete class type arguments for a generic class instance, in De
        /// Bruijn (declaration) order; empty for non-generic classes. Carries a
        /// `GenericBox<int>` instance's `[int]` across the FFI boundary (the
        /// value-level type channel — distinct from a call's
        /// `CallFunctionArgs.type_args`). Populated inbound from
        /// the sparse `InboundValue.value_type`; landed into the VM
        /// `Object::Instance::class_type_args` during contextual materialization.
        type_args: Vec<RuntimeTy>,
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

    /// Reference to a value owned by the host language.
    ///
    /// `Drop` of the last clone fires the registered `HostReleaseFn`.
    /// See [`bex_resource_types::HostValueArc`].
    HostValue(std::sync::Arc<bex_resource_types::HostValueArc>),
}

impl std::fmt::Debug for BexExternalValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Null => write!(f, "Null"),
            Self::Int(v) => f.debug_tuple("Int").field(v).finish(),
            Self::Bigint(v) => f.debug_tuple("Bigint").field(v).finish(),
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
            Self::Instance {
                class_name,
                type_args,
                fields,
            } => f
                .debug_struct("Instance")
                .field("class_name", class_name)
                .field("type_args", type_args)
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
            Self::HostValue(v) => f
                .debug_struct("HostValue")
                .field("key", &v.key)
                .field("kind", &v.kind)
                .finish(),
        }
    }
}

impl PartialEq for BexExternalValue {
    fn eq(&self, other: &Self) -> bool {
        match (self, other) {
            (Self::Null, Self::Null) => true,
            (Self::Int(a), Self::Int(b)) => a == b,
            (Self::Bigint(a), Self::Bigint(b)) => a == b,
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
                    type_args: t1,
                    fields: f1,
                },
                Self::Instance {
                    class_name: c2,
                    type_args: t2,
                    fields: f2,
                },
            ) => c1 == c2 && t1 == t2 && f1 == f2,
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
            (Self::HostValue(a), Self::HostValue(b)) => a.key == b.key && a.kind == b.kind,
            _ => false,
        }
    }
}

impl BexExternalAdt {
    pub fn type_name(&self) -> &'static str {
        match self {
            BexExternalAdt::Collector(_) => "collector",
            BexExternalAdt::Type(_) | BexExternalAdt::TypeDef(_) => "type",
            BexExternalAdt::PromptAst(_) => "prompt_ast",
            BexExternalAdt::Media(_) => "media",
            BexExternalAdt::TaggedHeapHandle { .. } => "tagged_heap_handle",
        }
    }
}

/// Field of the stdlib media wrapper classes (`baml.media.*`) that holds the
/// structural media payload. The wrapper layout is private to the stdlib;
/// host code must reach the payload through
/// [`BexExternalValue::media_wrapper_inner`], never by naming this field
/// elsewhere.
pub const MEDIA_WRAPPER_DATA_FIELD: &str = "_data";

impl BexExternalValue {
    /// If this is an instance of a stdlib media wrapper class
    /// (`baml.media.{Image,Audio,Video,Pdf}`), the media kind it wraps.
    pub fn media_wrapper_kind(&self) -> Option<baml_type::MediaKind> {
        match self {
            BexExternalValue::Instance { class_name, .. } => {
                baml_type::MediaKind::from_wrapper_class_name(class_name)
            }
            _ => None,
        }
    }

    /// The structural media payload of a media wrapper instance. `None` when
    /// this is not a media wrapper, or when the wrapper is missing its
    /// payload field (malformed; callers decide whether that is an error).
    pub fn media_wrapper_inner(&self) -> Option<&BexExternalValue> {
        self.media_wrapper_kind()?;
        match self {
            BexExternalValue::Instance { fields, .. } => fields.get(MEDIA_WRAPPER_DATA_FIELD),
            _ => None,
        }
    }

    /// Construct the transient carrier for an inbound value paired with its
    /// exact host-known type. This reuses the external union representation so
    /// existing type-directed VM materialization can honor `value_type`, while
    /// explicitly distinguishing it from an actual declared union.
    pub fn typed(value: BexExternalValue, value_type: RuntimeTy) -> Self {
        let mut metadata = UnionMetadata::new(
            RuntimeTy::Union(vec![value_type.clone()], TyAttr::default()),
            value_type,
        );
        metadata.is_inbound_type_annotation = true;
        BexExternalValue::Union {
            value: Box::new(value),
            metadata,
        }
    }

    /// Construct a union value (`A | B | ...`) with metadata.
    ///
    /// ```ignore
    /// BexExternalValue::union(BexExternalValue::Int(42), [RuntimeTy::int(), RuntimeTy::string()], RuntimeTy::int())
    /// ```
    pub fn union(
        value: BexExternalValue,
        members: impl IntoIterator<Item = RuntimeTy>,
        selected: RuntimeTy,
    ) -> Self {
        let union_type = RuntimeTy::Union(members.into_iter().collect(), TyAttr::default());
        BexExternalValue::Union {
            value: Box::new(value),
            metadata: UnionMetadata::new(union_type, selected),
        }
    }

    /// Construct an enum variant value.
    pub fn variant(enum_name: impl Into<String>, variant_name: impl Into<String>) -> Self {
        BexExternalValue::Variant {
            enum_name: enum_name.into(),
            variant_name: variant_name.into(),
        }
    }

    /// Construct a non-generic class instance value (empty `type_args`).
    pub fn instance(
        class_name: impl Into<String>,
        fields: IndexMap<&str, BexExternalValue>,
    ) -> Self {
        Self::instance_generic(class_name, vec![], fields)
    }

    /// Construct a class instance value carrying concrete class type arguments
    /// (De Bruijn order). Use for generic class instances; `instance` is the
    /// terse non-generic shorthand.
    pub fn instance_generic(
        class_name: impl Into<String>,
        type_args: Vec<RuntimeTy>,
        fields: IndexMap<&str, BexExternalValue>,
    ) -> Self {
        BexExternalValue::Instance {
            class_name: class_name.into(),
            type_args,
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
            BexExternalValue::Bigint(_) => "bigint",
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
            BexExternalValue::HostValue(_) => "host_value",
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
    pub fn as_string(&self) -> Option<bex_str::BexStr> {
        match self {
            BexExternalValue::String(value) => Some(value.clone()), // O(1) now
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

    /// Human-readable, structural rendering of this value — the form `baml run`
    /// prints in debug mode and the form the CLI surfaces for an uncaught
    /// `throw`.
    ///
    /// This is deliberately distinct from the [`Debug`](std::fmt::Debug) impl,
    /// which leaks Rust-internal shapes: a thrown
    /// `baml.errors.Io { message: "boom" }` renders here as
    /// `baml.errors.Io { message: "boom" }` rather than
    /// `Instance { class_name: "baml.errors.Io", type_args: [], fields: {..} }`,
    /// and a generic instance's `type_args` are omitted entirely instead of
    /// dumping `Class(QualifiedTypeName { .. }, [], TyAttr { .. })`.
    ///
    /// It is a pure structural pretty-printer, not the VM's `baml.ToString`
    /// dispatch: it runs without a live VM (e.g. after the VM has unwound on an
    /// uncaught throw), so it cannot honor user `to_string` overrides.
    pub fn render_readable(&self) -> String {
        match self {
            BexExternalValue::Null => "null".to_string(),
            BexExternalValue::Int(i) => i.to_string(),
            BexExternalValue::Bigint(i) => i.to_string(),
            BexExternalValue::Float(f) => {
                let s = f.to_string();
                if s.contains('.') || !f.is_finite() {
                    s
                } else {
                    format!("{s}.0")
                }
            }
            BexExternalValue::Bool(b) => b.to_string(),
            BexExternalValue::String(s) => format!("{s:?}"),
            BexExternalValue::Array { items, .. } => {
                let inner: Vec<String> = items.iter().map(Self::render_readable).collect();
                format!("[{}]", inner.join(", "))
            }
            BexExternalValue::Map { entries, .. } => {
                let inner: Vec<String> = entries
                    .iter()
                    .map(|(k, v)| format!("{k:?}: {}", v.render_readable()))
                    .collect();
                format!("{{{}}}", inner.join(", "))
            }
            BexExternalValue::Instance {
                class_name, fields, ..
            } => {
                let inner: Vec<String> = fields
                    .iter()
                    .map(|(k, v)| format!("{k}: {}", v.render_readable()))
                    .collect();
                if class_name.is_empty() {
                    format!("{{{}}}", inner.join(", "))
                } else {
                    format!("{class_name} {{{}}}", inner.join(", "))
                }
            }
            BexExternalValue::Variant { variant_name, .. } => variant_name.clone(),
            BexExternalValue::Union { value, .. } => value.render_readable(),
            BexExternalValue::Uint8Array(bytes) => format!("<bytes:{}>", bytes.len()),
            // A rendered prompt handle: render its readable text instead of the
            // `Adt(PromptAst(Message { .. }))` Rust `Debug` dump (B-627). Nested
            // inside a `ai.Prompt { _data: .. }` instance, this makes the
            // CLI's value print readable.
            BexExternalValue::Adt(BexExternalAdt::PromptAst(ast)) => ast.render_text(),
            _ => format!("{self:?}"),
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
        BexExternalValue::String(bex_str::BexStr::from(value))
    }
}

impl From<&str> for BexExternalValue {
    fn from(value: &str) -> Self {
        BexExternalValue::String(bex_str::BexStr::from(value))
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
        BexExternalValue::String(bex_str::BexStr::from(self))
    }
}

impl AsBexExternalValue for bool {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Bool(self)
    }
}

impl AsBexExternalValue for std::sync::Arc<num_bigint::BigInt> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Bigint(std::sync::Arc::unwrap_or_clone(self))
    }
}

impl AsBexExternalValue for baml_type::RuntimeTy<baml_type::TaggedTypeName> {
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
            key_type: baml_type::RuntimeTy::string(),
            value_type: baml_type::RuntimeTy::string(),
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
            key_type: baml_type::RuntimeTy::string(),
            value_type: baml_type::RuntimeTy::unknown(),
            entries: self,
        }
        .into_bex_external_value()
    }
}

impl AsBexExternalValue for Vec<String> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: baml_type::RuntimeTy::string(),
            items: self
                .into_iter()
                .map(|s| BexExternalValue::String(bex_str::BexStr::from(s)))
                .collect(),
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

/// A structural inbound [`BexExternalValue`] that BAML cannot type — stashed
/// verbatim so it rides through the VM as an opaque `Object::RustData`
/// (`RuntimeTy::RustType`) and is re-emitted **unchanged** on the way out.
///
/// This is the round-trip carrier for *structural* host-only values: the ones
/// that arrive with their content inline (an unbound generic `Instance`, a
/// host-only `Map`/`Array`) rather than as a `HostValue` key-handle. Without it
/// such a value, landed in a `RustType` slot, would either be lost or
/// materialized into an introspectable VM object — breaking the opaque-leaf
/// contract (e.g. an unbound `GenericBox(value=5)` must stay distinct from a
/// bound `GenericBox[int]`). See `03c-impl-guide` "Host-only roundtripping".
pub struct OpaqueExternalValue(pub BexExternalValue);

impl ToBexExternalValue for OpaqueExternalValue {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue {
        // Re-emit the stashed value verbatim. Take ownership when this is the
        // sole reference (the common case), else clone the inner value.
        match std::sync::Arc::try_unwrap(self) {
            Ok(inner) => inner.0,
            Err(arc) => arc.0.clone(),
        }
    }
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
    // A structural host-only value stashed verbatim on the way in (an unbound
    // generic instance, a host-only map/array): re-emit it exactly as it
    // arrived so the host decoder reconstructs the same value. See
    // [`OpaqueExternalValue`].
    if let Ok(typed) = arc.clone().downcast::<OpaqueExternalValue>() {
        return Some(typed.to_bex_external_value());
    }
    // A `HostValueArc` wrapped into `Object::RustData` (e.g. the `_handle`
    // slot of a `baml.errors.HostCallable` inbound from the host bridge):
    // convert back to a `BexExternalValue::HostValue` so the outbound
    // encoder emits a `Handle(HOST_VALUE_{CALLABLE,ERROR})` carrying the
    // same `(key, kind)` — letting the originating bridge resolve the
    // handle back to the original native object on round-trip.
    if let Ok(typed) = arc.clone().downcast::<bex_resource_types::HostValueArc>() {
        return Some(BexExternalValue::HostValue(typed));
    }
    None
}

#[cfg(test)]
mod render_readable_tests {
    use super::*;

    /// A thrown error instance renders as `Class { field: value }`, not the
    /// Rust `Debug` shape `Instance { class_name: .., type_args: [], fields: .. }`.
    /// This is the exact B-623 repro.
    #[test]
    fn instance_renders_class_and_fields_not_debug() {
        let value = BexExternalValue::instance(
            "baml.errors.Io",
            IndexMap::from([("message", BexExternalValue::from("boom"))]),
        );
        assert_eq!(
            value.render_readable(),
            r#"baml.errors.Io {message: "boom"}"#
        );
    }

    /// A generic error instance carrying a `Class(..)` in its `type_args` — the
    /// shape that used to dump `Class(QualifiedTypeName { .. }, [], TyAttr { .. })`
    /// under `Debug` — renders readably with the `type_args` omitted and no Rust
    /// internals leaked.
    #[test]
    fn generic_instance_omits_type_args_and_never_leaks_debug() {
        let inner = BexExternalValue::instance(
            "baml.errors.Io",
            IndexMap::from([("message", BexExternalValue::from("boom"))]),
        );
        let value = BexExternalValue::instance_generic(
            "baml.future.AllFailed",
            vec![RuntimeTy::class("baml.errors.Io")],
            IndexMap::from([(
                "errors",
                BexExternalValue::Array {
                    element_type: RuntimeTy::class("baml.errors.Io"),
                    items: vec![inner],
                },
            )]),
        );

        let rendered = value.render_readable();
        assert!(
            rendered.starts_with("baml.future.AllFailed {"),
            "unexpected render: {rendered}"
        );
        assert!(
            rendered.contains(r#"errors: [baml.errors.Io {message: "boom"}]"#),
            "unexpected render: {rendered}"
        );
        // The bug: `Debug` leaks Rust-internal shapes. The readable form must not.
        for leak in ["Instance {", "QualifiedTypeName", "TyAttr", "Class("] {
            assert!(!rendered.contains(leak), "leaked `{leak}` in: {rendered}");
        }
    }

    /// A rendered-prompt handle (`ai.Prompt`'s `_data`) renders as its
    /// readable prompt text, not the `Adt(PromptAst(Message { .. }))` Rust
    /// `Debug` dump. This is the B-627 repro for the CLI value print.
    #[test]
    #[allow(clippy::default_trait_access)]
    fn prompt_ast_renders_readable_text_not_debug() {
        use std::sync::Arc;

        use baml_builtins2::{PromptAst, PromptAstSimple};

        // `metadata` is `serde_json::Value` (not a direct dep of this crate); its
        // `Default` is `Value::Null`, so use `Default::default()` to avoid naming it.
        let message = |role: &str, text: &str| {
            Arc::new(PromptAst::Message {
                role: role.to_string(),
                content: Arc::new(PromptAstSimple::String(text.to_string())),
                metadata: Default::default(),
            })
        };
        let ast = Arc::new(PromptAst::Vec(vec![
            message("system", "You are helpful."),
            message("user", "Hi!"),
        ]));
        let value = BexExternalValue::Adt(BexExternalAdt::PromptAst(ast));

        let rendered = value.render_readable();
        assert_eq!(rendered, "[system]\nYou are helpful.\n\n[user]\nHi!");
        // The bug: the opaque handle used to dump Rust `Debug`. Must not leak.
        for leak in ["Adt(", "PromptAst(", "Message {", "String(", "Null"] {
            assert!(!rendered.contains(leak), "leaked `{leak}` in: {rendered}");
        }
    }
}
