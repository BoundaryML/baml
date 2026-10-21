//! BexExternalValue - owned value tree for FFI.
//!
//! `BexExternalValue` is a fully-owned value tree with no heap references.
//! Use when you need to traverse and convert the entire object graph,
//! such as for FFI conversion to Python/JS objects.
//!
//! # Identity
//!
//! Every value carries the concrete type it has. A scalar, `null`, enum
//! variant, media value or handle implies its own; a list carries its
//! element type, a map its key and value types, a class instance its class
//! and fully bound type arguments, a host callable its function type. The
//! engine builds a value as what it says it is and then asks whether that
//! type is a member of the slot the value was bound for; it never reads a
//! slot into a value.
//!
//! # Union Types
//!
//! When a value the engine produced comes from a union type (e.g., `int | string`
//! or `Success | Failure`), it's wrapped in the `Union` variant with metadata
//! about the union:
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
pub use baml_type::{RuntimeTy, TypeName, WireHead};
use indexmap::IndexMap;

/// A type as the engine's boundary states it: compiled declarations by name,
/// runtime declarations by a rooted handle to the declaration object.
pub type WireTy = baml_type::WireTy<crate::Handle>;
/// A parameter of a [`WireTy`] function type.
pub type WireFunctionParamTy = baml_type::RealizedFunctionParamTy<WireHead<crate::Handle>>;

/// A type as an annotation beside a value crossing by reference: compiled
/// declarations by name, runtime ones by their label. It references nothing,
/// so it cannot carry a live head at all.
pub type AnnotationTy = baml_type::WireTy<std::convert::Infallible>;

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
    pub union_type: WireTy,

    /// Which option of the union was selected (e.g., `RuntimeTy::Int`, `RuntimeTy::String`, `RuntimeTy::Class("Success")`).
    pub selected_option: WireTy,
}

impl UnionMetadata {
    /// Create metadata for a union type.
    pub fn new(union_type: WireTy, selected_option: WireTy) -> Self {
        let (is_optional, is_single_pattern) = match &union_type {
            WireTy::Union(members) => {
                let is_optional = members.iter().any(WireTy::is_null);
                let non_null_count = members.iter().filter(|member| !member.is_null()).count();
                (is_optional, non_null_count == 1)
            }
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
}

#[derive(Clone, Debug, PartialEq)]
pub enum BexExternalAdt {
    /// A reflected type, carried at the sys-op lane's head so a definition
    /// table can be keyed by declaration identity rather than by name.
    ///
    /// Deliberately pointer-free: the collector can run while a sys-op is
    /// awaiting, so anything a sys-op holds across that boundary must survive
    /// objects moving. A tag does; an address does not.
    Type(baml_type::RuntimeTy<baml_type::TaggedTypeName>),
    /// A reflected type as the host boundary states it: a compiled
    /// declaration by name, a runtime declaration by a rooted handle to the
    /// declaration object. Inbound it is anchored to the declarations it
    /// references and lands as a fresh `type` value over them; outbound every
    /// runtime declaration it names is a row the host holds.
    WireType(WireTy),
    /// The Rust-backed payload inside a rendered `ai.Prompt`.
    PromptAst(std::sync::Arc<bex_vm_types::PromptAst>),
    /// A media value (image, audio, etc.) passed as a function argument.
    Media(std::sync::Arc<bex_vm_types::MediaValue>),
    /// GC-rooted reference to an engine-owned heap value with a trusted
    /// boundary kind and optional host-facing type decoration.
    ///
    /// `kind` is the only boundary discriminator. `ty` is diagnostic metadata
    /// for host annotations and must never select a wrapper or drive generic
    /// substitution: live nominal identity and type arguments come from the
    /// resolved heap object. `heap_handle` keeps that object alive so the
    /// engine can re-enter it for method calls.
    TaggedHeapHandle {
        kind: TaggedHeapHandleKind,
        ty: AnnotationTy,
        heap_handle: crate::Handle,
    },
}

/// Trusted role of a rooted heap capability crossing a host boundary.
///
/// This is deliberately independent of [`RuntimeTy`]. A runtime-created type
/// may share a display name with a compiled type, so a name-shaped wire type
/// can describe a value but cannot establish its identity or behavior.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TaggedHeapHandleKind {
    Callable,
    Stream,
    FunctionSpec,
    RuntimeValue,
}

/// A deep-copied value tree with no heap references.
///
/// Use `BexEngine::call_function` to get the result. When the return type
/// is a union, the value will be wrapped in the `Union` variant with metadata.
///
/// Inbound, every value is the serialization of a concretely typed value: a
/// container names its component types, an instance its class and type
/// arguments, a callable its function type. The engine builds the value as it
/// is and checks the result against its slot; nothing is read from the slot
/// into the value.
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
        /// The element type of the list (`int | string` for a `(int | string)[]`).
        /// Inbound, every item must be a member of it; the list's concrete type
        /// is `element_type[]`.
        element_type: WireTy,
        /// The array items.
        items: Vec<BexExternalValue>,
    },

    /// Owned map with string keys and type information.
    Map {
        /// The key type (usually `RuntimeTy::String`); every key must be a member.
        key_type: WireTy,
        /// The value type (`int | string` for a `map<string, int | string>`);
        /// every value must be a member.
        value_type: WireTy,
        /// The map entries.
        entries: IndexMap<String, BexExternalValue>,
    },

    /// Class instance with class name and field values.
    Instance {
        /// The class, as a host spells it (`user.Reading`, `baml.errors.Io`).
        class_name: String,
        /// The class's type arguments, in declaration order; empty for a
        /// non-generic class. A generic instance arrives fully bound: a
        /// `GenericBox<int>` carries `[int]` (the value-level type channel,
        /// distinct from a call's `CallFunctionArgs.type_args`), and lands in
        /// the VM `Object::Instance::class_type_args`.
        type_args: Vec<WireTy>,
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
    /// value the engine produced is wrapped with metadata about the union for
    /// FFI serialization. Outbound only: a host states the type of a node in
    /// the node itself, so an inbound union envelope is refused.
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
    RustData(std::sync::Arc<dyn bex_vm_types::BexRustData>),

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

    /// Reference to a value owned by the host language, by its key and kind.
    ///
    /// Inbound, an opaque host value: it lands as `$rust_type` data. A host
    /// callable arrives as [`HostCallable`](Self::HostCallable) with the
    /// function type it inhabits; outbound, the engine hands a bound callable
    /// back to its host's dispatch as this variant, since the host identifies
    /// it by key alone.
    ///
    /// `Drop` of the last clone fires the registered `HostReleaseFn`.
    /// See [`bex_resource_types::HostValueArc`].
    HostValue(std::sync::Arc<bex_resource_types::HostValueArc>),

    /// A host-language callable with the function type it inhabits: the type
    /// the host declared for it, or the type of the slot it was passed to. The
    /// engine binds it to an `Object::HostClosure` of exactly this type.
    HostCallable {
        /// The callable in the host's registry
        /// (`HostValueKind::Callable`).
        handle: std::sync::Arc<bex_resource_types::HostValueArc>,
        params: Box<[WireFunctionParamTy]>,
        ret: Box<WireTy>,
        throws: Box<WireTy>,
    },
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
            Self::HostCallable {
                handle,
                params,
                ret,
                throws,
            } => f
                .debug_struct("HostCallable")
                .field("key", &handle.key)
                .field("params", params)
                .field("ret", ret)
                .field("throws", throws)
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
            (
                Self::HostCallable {
                    handle: a,
                    params: a_params,
                    ret: a_ret,
                    throws: a_throws,
                },
                Self::HostCallable {
                    handle: b,
                    params: b_params,
                    ret: b_ret,
                    throws: b_throws,
                },
            ) => {
                a.key == b.key
                    && a.kind == b.kind
                    && a_params == b_params
                    && a_ret == b_ret
                    && a_throws == b_throws
            }
            _ => false,
        }
    }
}

impl BexExternalAdt {
    pub fn type_name(&self) -> &'static str {
        match self {
            BexExternalAdt::Type(_) | BexExternalAdt::WireType(_) => "type",
            BexExternalAdt::PromptAst(_) => "prompt_ast",
            BexExternalAdt::Media(_) => "media",
            BexExternalAdt::TaggedHeapHandle { .. } => "tagged_heap_handle",
        }
    }
}

/// Field of the stdlib media and prompt wrapper classes (`baml.media.*`,
/// `ai.Prompt`) that holds the Rust-backed payload. The wrapper layout is
/// private to the stdlib: the engine flattens a wrapper instance to its
/// payload on the way out and builds one around a payload on the way in, so
/// no host ever names this field.
pub const MEDIA_WRAPPER_DATA_FIELD: &str = "_data";

impl BexExternalValue {
    /// Construct a union value (`A | B | ...`) with metadata.
    ///
    /// ```ignore
    /// BexExternalValue::union(BexExternalValue::Int(42), [RuntimeTy::int(), RuntimeTy::string()], RuntimeTy::int())
    /// ```
    pub fn union(
        value: BexExternalValue,
        members: impl IntoIterator<Item = WireTy>,
        selected: WireTy,
    ) -> Self {
        let union_type = WireTy::Union(members.into_iter().collect());
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
        type_args: Vec<WireTy>,
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
            BexExternalValue::HostCallable { .. } => "host_callable",
        }
    }

    /// This value as a host would send it back: every union envelope opened,
    /// at every depth. The envelope says which member of a declared union a
    /// value the engine handed out came from; a host value is just the value,
    /// which states its own type, so an in-process caller that feeds an
    /// engine result back in (the CLI's JSON output, a playground run) opens
    /// the envelopes first.
    #[must_use]
    pub fn into_host_value(self) -> Self {
        match self {
            BexExternalValue::Union { value, .. } => value.into_host_value(),
            BexExternalValue::Array {
                element_type,
                items,
            } => BexExternalValue::Array {
                element_type,
                items: items.into_iter().map(Self::into_host_value).collect(),
            },
            BexExternalValue::Map {
                key_type,
                value_type,
                entries,
            } => BexExternalValue::Map {
                key_type,
                value_type,
                entries: entries
                    .into_iter()
                    .map(|(key, value)| (key, value.into_host_value()))
                    .collect(),
            },
            BexExternalValue::Instance {
                class_name,
                type_args,
                fields,
            } => BexExternalValue::Instance {
                class_name,
                type_args,
                fields: fields
                    .into_iter()
                    .map(|(key, value)| (key, value.into_host_value()))
                    .collect(),
            },
            BexExternalValue::Null
            | BexExternalValue::Int(_)
            | BexExternalValue::Bigint(_)
            | BexExternalValue::Float(_)
            | BexExternalValue::Bool(_)
            | BexExternalValue::String(_)
            | BexExternalValue::Variant { .. }
            | BexExternalValue::Uint8Array(_)
            | BexExternalValue::RustData(_)
            | BexExternalValue::FunctionRef { .. }
            | BexExternalValue::Handle(_)
            | BexExternalValue::Adt(_)
            | BexExternalValue::HostValue(_)
            | BexExternalValue::HostCallable { .. } => self,
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
    /// dumping `Class(QualifiedTypeName { .. }, [])`.
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
            key_type: WireTy::string(),
            value_type: WireTy::string(),
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
            key_type: WireTy::string(),
            value_type: WireTy::unknown(),
            entries: self,
        }
        .into_bex_external_value()
    }
}

impl AsBexExternalValue for Vec<String> {
    fn into_bex_external_value(self) -> BexExternalValue {
        BexExternalValue::Array {
            element_type: WireTy::string(),
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
pub trait ToBexExternalValue: bex_vm_types::BexRustData {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue;
}

impl ToBexExternalValue for bex_vm_types::PromptAst {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::PromptAst(self))
    }
}

impl ToBexExternalValue for bex_vm_types::MediaValue {
    fn to_bex_external_value(self: std::sync::Arc<Self>) -> BexExternalValue {
        BexExternalValue::Adt(BexExternalAdt::Media(self))
    }
}

/// Try to convert the payload of an `Object::RustData` to a
/// [`BexExternalValue`] by attempting downcast to known [`ToBexExternalValue`]
/// implementors.
///
/// Returns `None` if the concrete type is not recognised.
pub fn try_convert_rust_data(
    arc: &std::sync::Arc<dyn bex_vm_types::BexRustData>,
) -> Option<BexExternalValue> {
    use bex_vm_types::RustDataArc;
    if let Ok(typed) = arc.clone().downcast_payload::<bex_vm_types::PromptAst>() {
        return Some(typed.to_bex_external_value());
    }
    if let Ok(typed) = arc.clone().downcast_payload::<bex_vm_types::MediaValue>() {
        return Some(typed.to_bex_external_value());
    }
    // A `HostValueArc` wrapped into `Object::RustData` (e.g. the `_handle`
    // slot of a `baml.errors.HostCallable` inbound from the host bridge):
    // convert back to a `BexExternalValue::HostValue` so the outbound
    // encoder emits a `Handle(HOST_VALUE_{CALLABLE,ERROR})` carrying the
    // same `(key, kind)` — letting the originating bridge resolve the
    // handle back to the original native object on round-trip.
    if let Ok(typed) = arc
        .clone()
        .downcast_payload::<bex_resource_types::HostValueArc>()
    {
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
    /// shape that used to dump `Class(QualifiedTypeName { .. }, [])`
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
            vec![WireTy::static_class("baml.errors.Io")],
            IndexMap::from([(
                "errors",
                BexExternalValue::Array {
                    element_type: WireTy::static_class("baml.errors.Io"),
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
        for leak in ["Instance {", "QualifiedTypeName", "Class("] {
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

        use bex_vm_types::{PromptAst, PromptAstSimple};

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
