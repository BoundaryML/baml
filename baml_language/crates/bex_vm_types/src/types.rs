use std::collections::HashMap;

use baml_type::Ty;
use bex_resource_types::ResourceHandle;
use indexmap::IndexMap;

use crate::{bytecode::Bytecode, heap_ptr::HeapPtr, indexable::ObjectPool};

// ============================================================================
// Type Tags for Jump Table Dispatch
// ============================================================================

/// Global type tag constants for runtime type identification.
///
/// Re-exported from `baml_typetags` crate to maintain backwards compatibility.
/// These are used by the `TypeTag` instruction to extract a type identifier
/// from any value for jump table dispatch on union types.
pub mod type_tags {
    pub use baml_type::typetag::*;
}

/// Compiled program ready for execution.
///
/// This is what `baml_compiler_emit` produces. It contains all the objects and globals
/// needed to run a BAML program.
///
/// Note: At compile time, globals use `ConstValue` (with `ObjectIndex` for object refs).
/// At load time (`BexEngine::new`), these are converted to `Value` (with `HeapPtr`).
#[derive(Clone, Debug, Default)]
pub struct Program {
    /// Object pool containing functions, classes, strings, etc.
    pub objects: ObjectPool,

    /// Global variables (converted from `ConstValue` to Value at load time).
    pub globals: Vec<ConstValue>,

    /// Maps function names to their object indices.
    pub function_indices: HashMap<String, usize>,

    /// Maps function names to their global indices.
    /// Used for dynamic function lookup at runtime.
    pub function_global_indices: HashMap<String, usize>,

    /// Pre-formatted Jinja `{% macro %}` definitions for all `template_strings`.
    /// Prepended to function prompt templates by `get_jinja_template`.
    pub template_strings_macros: String,

    /// Client build metadata for constructing full client trees at runtime.
    /// Keyed by client name.
    pub client_metadata: HashMap<String, ClientBuildMeta>,

    /// Compiled test cases.
    pub test_cases: Vec<TestCase>,
}

/// Metadata for building a client tree at runtime.
///
/// Stored on `Program` during compilation, transferred to `SysOpContext` during engine construction.
#[derive(Debug, Clone, Default)]
pub struct ClientBuildMeta {
    /// Provider type mapped to client type enum.
    pub client_type: ClientBuildType,
    /// Sub-client names (for composite clients).
    pub sub_client_names: Vec<String>,
    /// Retry policy metadata, if specified.
    pub retry_policy: Option<RetryPolicyMeta>,
    /// Optional round-robin start index (`options { start ... }`).
    pub round_robin_start: Option<i32>,
}

/// Client type for build metadata (mirrors runtime `LlmClientType`).
#[derive(Debug, Clone, Default)]
pub enum ClientBuildType {
    #[default]
    Primitive,
    Fallback,
    RoundRobin,
}

/// Retry policy metadata stored at compile time.
#[derive(Debug, Clone)]
pub struct RetryPolicyMeta {
    pub max_retries: i64,
    pub initial_delay_ms: i64,
    pub multiplier: f64,
    pub max_delay_ms: i64,
}

impl Program {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an object to the pool and return its index.
    pub fn add_object(&mut self, object: Object) -> usize {
        let idx = self.objects.len();
        self.objects.push(object);
        idx
    }

    /// Add a global value (`ConstValue`, converted to Value at load time).
    pub fn add_global(&mut self, value: ConstValue) {
        self.globals.push(value);
    }

    /// Look up a function's object index by name.
    pub fn function_index(&self, name: &str) -> Option<usize> {
        self.function_indices.get(name).copied()
    }
}

// ============================================================================
// SysOp Error/Panic Contract Categories
// ============================================================================

/// Contract-level error categories for `sys_op` throw contracts.
///
/// These are the finite set of categories that `#[throws(...)]` annotations
/// reference. Each `OpErrorKind` variant maps to exactly one category via
/// `OpErrorKind::category()`. Rich detail stays in `OpErrorKind`; this enum
/// is purely for contract enforcement and compiler analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SysOpErrorCategory {
    Io,
    Timeout,
    InvalidArgument,
    Unsupported,
    NotImplemented,
    AccessError,
    RenderPrompt,
    LlmClient,
    /// Wildcard for development convenience. Must be explicitly declared in
    /// `#[throws(DevOther)]` and should be migrated to named categories.
    DevOther,
}

impl std::fmt::Display for SysOpErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io => write!(f, "Io"),
            Self::Timeout => write!(f, "Timeout"),
            Self::InvalidArgument => write!(f, "InvalidArgument"),
            Self::Unsupported => write!(f, "Unsupported"),
            Self::NotImplemented => write!(f, "NotImplemented"),
            Self::AccessError => write!(f, "AccessError"),
            Self::RenderPrompt => write!(f, "RenderPrompt"),
            Self::LlmClient => write!(f, "LlmClient"),
            Self::DevOther => write!(f, "DevOther"),
        }
    }
}

/// Contract-level panic categories for `sys_op` panic contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum SysOpPanicCategory {
    HostPanic,
}

impl std::fmt::Display for SysOpPanicCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::HostPanic => write!(f, "HostPanic"),
        }
    }
}

// ============================================================================
// External Operations
// ============================================================================

/// System operations that run outside the VM.
///
/// Generated from `#[sys_op]` definitions in `baml_builtins::with_builtins!`.
/// Adding a new `#[sys_op]` in the DSL automatically adds an enum variant here.
///
/// The `for_all_sys_ops!` macro carries the definitive list of variants, paths,
/// and `snake_case` names. This enum, `path()`, `sys_op_for_path()`, and `Display`
/// are all generated from it — no manual maintenance needed.
macro_rules! define_sys_op_enum {
    ($({ $Variant:ident, $path:expr, $snake:ident, $uses_ctx:expr, [$($throw_cat:ident),*], [$($panic_cat:ident),*] })*) => {
        #[derive(Clone, Copy, Debug, PartialEq, Eq)]
        pub enum SysOp {
            $( $Variant, )*
        }

        impl SysOp {
            /// Get the DSL path for this `sys_op` (e.g., `"baml.fs.open"`).
            pub const fn path(&self) -> &'static str {
                match self {
                    $( SysOp::$Variant => $path, )*
                }
            }

            /// Error categories this `sys_op` is allowed to throw per its contract.
            pub fn allowed_error_categories(&self) -> &'static [SysOpErrorCategory] {
                match self {
                    $( SysOp::$Variant => &[$(SysOpErrorCategory::$throw_cat),*], )*
                }
            }

            /// Panic categories this `sys_op` is allowed to surface per its contract.
            pub fn allowed_panic_categories(&self) -> &'static [SysOpPanicCategory] {
                match self {
                    $( SysOp::$Variant => &[$(SysOpPanicCategory::$panic_cat),*], )*
                }
            }
        }

        /// Look up a `SysOp` by its DSL path string.
        pub fn sys_op_for_path(path: &str) -> Option<SysOp> {
            match path {
                $( $path => Some(SysOp::$Variant), )*
                _ => None,
            }
        }

        impl std::fmt::Display for SysOp {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{}", self.path())
            }
        }
    };
}

baml_builtins::for_all_sys_ops!(define_sys_op_enum);

// ============================================================================
// Function Types
// ============================================================================

/// Function type.
///
/// # Native Function Pointers
///
/// Native functions are stored as type-erased `*const ()` pointers to avoid
/// a circular dependency between crates:
///
/// - `baml_vm` defines `NativeFunction = fn(&mut Vm, &[Value]) -> Result<...>`
/// - This type references `Vm`, which is defined in `baml_vm`
/// - `baml_vm_types` cannot depend on `baml_vm` (that would be circular)
///
/// The type erasure allows different stages:
///
/// - **Compile time**: The compiler emits `NativeUnresolved` for built-in functions
/// - **Runtime**: The VM resolves these to `Native(ptr)` at load time
///
/// The resolution happens in `baml_vm::native::attach_builtins()`, which looks up
/// native function names and casts the real function pointers to `*const ()`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FunctionKind {
    /// Regular executable function.
    ///
    /// The VM pushes a call frame onto the call stack and runs the bytecode.
    Bytecode,

    /// System operation (LLM calls, HTTP requests, file I/O, etc.).
    ///
    /// The VM yields control to the engine which executes the operation
    /// asynchronously via static dispatch on the `SysOp` enum.
    SysOp(SysOp),

    /// Unresolved native function (placeholder).
    ///
    /// The compiler emits this for built-in functions. The VM resolves these
    /// to `Native(ptr)` at load time. Panics if executed without resolution.
    NativeUnresolved,

    /// Rust native function (type-erased pointer).
    ///
    /// Contains a type-erased function pointer that the VM casts back to
    /// the real `NativeFunction` type when calling.
    ///
    /// # Safety
    ///
    /// The pointer must be cast from a valid `NativeFunction` and only
    /// cast back to that same type when calling.
    Native(*const ()),
}

// SAFETY: FunctionKind contains a raw pointer (*const ()) that points to
// immutable code (function pointers). Code doesn't change at runtime,
// so sharing the pointer between threads is safe.
#[allow(unsafe_code)]
unsafe impl Send for FunctionKind {}
#[allow(unsafe_code)]
unsafe impl Sync for FunctionKind {}

/// LLM-specific metadata for a function.
#[derive(Clone, Debug)]
pub enum FunctionMeta {
    Llm {
        prompt_template: String,
        client: String,
    },
}

/// Represents any Baml function.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Number of additional local slots (beyond callee + params) needed by the frame.
    ///
    /// The VM allocates these slots when creating a bytecode frame, instead of
    /// relying on a dedicated bytecode instruction.
    pub real_local_count: usize,

    /// Bytecode to execute.
    ///
    /// Only relevant if [`Self::kind`] is [`FunctionKind::Bytecode`].
    pub bytecode: Bytecode,

    /// Type of function.
    pub kind: FunctionKind,

    /// Local variable names indexed by slot number.
    ///
    /// Debug info: maps eval-stack slot indices to variable names.
    /// Slot 0 is the function reference, slots 1..arity are parameters.
    pub local_names: Vec<String>,

    /// Lexical scope metadata for named locals.
    ///
    /// Used by debugger UIs to determine which variables are visible at a
    /// given source location.
    pub debug_locals: Vec<crate::bytecode::DebugLocalScope>,

    /// Span of the function as computed by the parser.
    pub span: baml_base::Span,

    /// Block notifications for this function.
    ///
    /// Stores metadata about annotated blocks (//# annotations) in this function.
    /// Instructions reference these by index.
    pub block_notifications: Vec<crate::bytecode::BlockNotification>,

    /// Control-flow visualization metadata indexed by VizEnter/VizExit instructions.
    ///
    /// Stores metadata about control flow structure (branches, loops, scopes).
    pub viz_nodes: Vec<crate::bytecode::VizNodeMeta>,

    /// Return type of the function.
    pub return_type: Ty,

    /// Parameter names in declaration order.
    pub param_names: Vec<String>,

    /// Parameter types in declaration order.
    pub param_types: Vec<Ty>,

    /// LLM-specific metadata (prompt template, client name). `None` for non-LLM functions.
    pub body_meta: Option<FunctionMeta>,

    /// Whether this function should be traced (emit span notifications on call/return).
    /// Set to `true` for LLM functions by the compiler.
    pub trace: bool,
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}

impl Function {
    /// Get the source span associated with a bytecode PC.
    pub fn source_span_for_pc(&self, pc: usize) -> Option<baml_base::Span> {
        self.bytecode.line_entry_for_pc(pc).map(|entry| entry.span)
    }

    /// Get named locals whose lexical scope contains the source span at `pc`.
    pub fn debug_locals_in_scope(&self, pc: usize) -> Vec<&crate::bytecode::DebugLocalScope> {
        let Some(span) = self.source_span_for_pc(pc) else {
            return Vec::new();
        };

        self.debug_locals
            .iter()
            .filter(|local| {
                local.scope_span.file_id == span.file_id
                    && local.scope_span.range.start() <= span.range.start()
                    && local.scope_span.range.end() >= span.range.end()
            })
            .collect()
    }
}

/// A field within a runtime class, carrying type and schema metadata.
#[derive(Clone, Debug)]
pub struct ClassField {
    pub name: String,
    pub field_type: Ty,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub field_attr: baml_type::FieldAttr,
}

/// Runtime class representation.
#[derive(Clone, Debug)]
pub struct Class {
    /// Class name.
    pub name: String,

    /// Class fields with type and schema metadata.
    pub fields: Vec<ClassField>,

    /// Class-level description for LLM prompt schema rendering.
    pub description: Option<String>,

    /// Class-level serialization alias.
    pub alias: Option<String>,

    /// Type tag for this class, used by `TypeTag` instruction for jump table dispatch.
    /// Assigned during codegen as `CLASS_BASE + class_index`.
    pub type_tag: i64,
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Runtime instance representation.
#[derive(Clone, Debug)]
pub struct Instance {
    /// Pointer to the class object in the heap.
    pub class: HeapPtr,

    /// Fields are accessed by index. No string lookups.
    pub fields: Vec<Value>,
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {:p}>", self.class.as_ptr())
    }
}

/// A variant within a runtime enum, carrying schema metadata.
#[derive(Clone, Debug)]
pub struct EnumVariant {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// Runtime enum representation.
#[derive(Clone, Debug)]
pub struct Enum {
    /// Enum name.
    pub name: String,

    /// Enum variants with schema metadata.
    pub variants: Vec<EnumVariant>,

    /// Enum-level description.
    pub description: Option<String>,

    /// Enum-level serialization alias.
    pub alias: Option<String>,
}

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name)
    }
}

/// Same as [`Instance`] but for enums.
#[derive(Clone, Debug)]
pub struct Variant {
    /// Pointer to the enum object in the heap.
    pub enm: HeapPtr,

    /// Index of the variant in the ordered list of variants.
    pub index: usize,
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<variant of {:p}>", self.enm.as_ptr())
    }
}

#[cfg(feature = "heap_debug")]
#[derive(Clone, Debug)]
pub enum SentinelKind {
    Uninit,
    FromSpacePoison {
        epoch: u32,
    },
    TlabCanary {
        chunk_start: usize,
        chunk_end: usize,
    },
}

/// Runtime values.
///
/// This struct should not contain allocated objects and should be [`Copy`].
/// Read the documentation of `Vm::objects` (in `bex_vm` crate) to understand how allocated
/// objects work in the virtual machine.
///
/// # On `Hash`
/// `Value` does not yet implement `Hash`, and should not implement `Eq`. Besides floating point which can be addressed,
/// strings do not yet have referential equality, i.e "hello" can be represented with two different
/// object indices. This makes comparisons nontrivial since they have to fetch the string. Same
/// would happen with any other object type that we don't want to have referential equality for.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),

    /// Pointer to a heap-allocated object.
    ///
    /// This is a raw pointer (`HeapPtr`) that points directly into the heap.
    /// Strings are also objects, don't add `Value::String`.
    Object(HeapPtr),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Float(float) => write!(f, "{float}"),
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::Object(ptr) => write!(f, "{ptr}"),
        }
    }
}

// ============================================================================
// Test Cases
// ============================================================================

/// A constant value for test arguments.
///
/// Self-contained type with no dependency on HIR or external types.
/// Converted from HIR's `TestArgValue` during emission, and converted
/// to `BexExternalValue` in the engine for function calls.
#[derive(Clone, Debug)]
pub enum TestArgValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array {
        element_type: Ty,
        items: Vec<TestArgValue>,
    },
    Map {
        key_type: Ty,
        value_type: Ty,
        entries: IndexMap<String, TestArgValue>,
    },
}

/// A compiled test case, ready for execution.
#[derive(Clone, Debug)]
pub struct TestCase {
    /// Test name (e.g., "`TestAddOne`").
    pub name: String,
    /// Function names this test targets.
    pub function_names: Vec<String>,
    /// Test arguments, keyed by parameter name.
    pub args: IndexMap<String, TestArgValue>,
}

/// Compile-time constant values.
///
/// Similar to `Value` but uses `ObjectIndex` for object references instead of `HeapPtr`.
/// Used in bytecode constants which are converted to `Value` when loading into the engine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ConstValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Index into the object pool (converted to `HeapPtr` at load time).
    Object(crate::ObjectIndex),
}

impl ConstValue {
    /// Convert to a runtime `Value` using a function to resolve object indices to heap pointers.
    pub fn to_value<F>(&self, resolve: F) -> Value
    where
        F: Fn(crate::ObjectIndex) -> HeapPtr,
    {
        match self {
            ConstValue::Null => Value::Null,
            ConstValue::Int(v) => Value::Int(*v),
            ConstValue::Float(v) => Value::Float(*v),
            ConstValue::Bool(v) => Value::Bool(*v),
            ConstValue::Object(idx) => Value::Object(resolve(*idx)),
        }
    }
}

/// Media value.
pub type MediaValue = std::sync::Arc<baml_builtins::MediaValue>;

/// Prompt AST tree node.
pub type PromptAst = std::sync::Arc<baml_builtins::PromptAst>;

/// Opaque handle to a `Collector` object from `bex_events`.
///
/// Uses `Arc<dyn Any + Send + Sync>` to avoid a dependency from `bex_vm_types` on `bex_events`.
/// Downcast to `bex_events::Collector` at the `bex_engine` layer.
#[derive(Clone, Debug)]
pub struct CollectorRef(pub std::sync::Arc<dyn std::any::Any + Send + Sync>);

impl PartialEq for CollectorRef {
    fn eq(&self, other: &Self) -> bool {
        std::sync::Arc::ptr_eq(&self.0, &other.0)
    }
}

/// Any data that the Baml program can reference and is allocated on heap.
///
/// `Vm` (in `bex_vm` crate) should own objects and give references to them to the running Baml
/// program. Internally, in the `Vm` code, note that by reference I don't mean
/// a Rust reference (& or &mut), but rather a [`usize`] that is used to index
/// into the `Vm::objects` pool.
///
/// Read `Vm::objects` for more information.
#[derive(Clone, Debug)]
pub enum Object {
    /// Function object.
    Function(Box<Function>),

    /// Class object.
    Class(Class),

    /// Class instance object.
    Instance(Instance),

    /// Enum object.
    Enum(Enum),

    /// Enum value object.
    Variant(Variant),

    /// Heap allocated string.
    ///
    /// TODO: Add a `Vm::strings` interner to avoid allocating duplicates.
    /// In Rust it's not easy to implement because `Vm::objects`
    /// owns the strings allocated on heap, but the interner would be something
    /// like `HashSet`<&str> and it would store pointers to the strings. That
    /// reference will cause some lifetime issues because the VM would have
    /// pointers to itself, so we'd have to figure how to implement it
    /// otherwise.
    String(String),

    /// List of values.
    Array(Vec<Value>),

    /// Map of values.
    Map(IndexMap<String, Value>),

    Future(Future),

    /// Images, audio, pdf, video.
    Media(MediaValue),

    /// Prompt AST tree node.
    PromptAst(PromptAst),

    /// External resource (file handle, socket, etc.).
    Resource(ResourceHandle),

    /// Collector object (opaque handle to `bex_events::Collector`).
    Collector(CollectorRef),

    /// A type descriptor value — wraps a `baml_type::Ty`.
    Type(baml_type::Ty),

    #[cfg(feature = "heap_debug")]
    Sentinel(SentinelKind),
    // TODO: Figure out how to handle this here.
    // /// Used for `baml.fetch_as` function.
    // BamlType(TypeIR),
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Function(function) => function.fmt(f),
            Object::Class(class) => class.fmt(f),
            Object::Instance(instance) => instance.fmt(f),
            Object::Enum(enm) => enm.fmt(f),
            Object::Variant(value) => value.fmt(f),
            Object::String(string) => string.fmt(f),
            Object::Array(array) => write!(f, "<array len={}>", array.len()),
            Object::Map(map) => write!(f, "<map len={}>", map.len()),
            Object::Media(media) => media.fmt(f),
            Object::Resource(r) => write!(f, "<{r}>"),
            Object::Collector(_) => write!(f, "<collector>"),
            Object::Type(ty) => write!(f, "<type: {ty}>"),
            Object::PromptAst(prompt) => write!(f, "<prompt_ast {prompt:?}>"),
            Object::Future(future) => match future {
                Future::Pending(future) => {
                    write!(f, "<pending: {}>", future.operation)
                }
                Future::Ready(value) => write!(f, "<ready: {value}>"),
            },
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(kind) => write!(f, "<sentinel {kind:?}>"),
            // Object::BamlType(type_ir) => write!(f, "<baml type: {type_ir}>"),
        }
    }
}

#[derive(Clone, Debug)]
pub enum Future {
    /// Pending future.
    ///
    /// Only LLM calls for now.
    Pending(PendingFuture),

    /// Ready value for the future.
    Ready(Value),
}

/// A pending external operation.
///
/// External operations are async functions that run outside the VM, such as
/// LLM calls, HTTP requests, file I/O, or shell commands.
#[derive(Clone, Debug)]
pub struct PendingFuture {
    /// The system operation to execute.
    pub operation: SysOp,
    /// Arguments to the operation.
    pub args: Vec<Value>,
}

/// Types of values.
///
/// Used for checking type errors at runtime. We can probably use some lib
/// that creates this automatically based on the [`Value`] enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Type {
    Int,
    Float,
    Bool,
    Object(ObjectType),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::Int => write!(f, "int"),
            Type::Float => write!(f, "float"),
            Type::Bool => write!(f, "bool"),
            Type::Object(object_type) => write!(f, "{object_type}"),
        }
    }
}

impl<O: Into<ObjectType>> From<O> for Type {
    fn from(obj: O) -> Self {
        Type::Object(obj.into())
    }
}

impl Type {
    /// Get the type of a value.
    pub fn of(value: &Value, when_object: impl FnOnce(HeapPtr) -> ObjectType) -> Self {
        match value {
            Value::Int(_) => Type::Int,
            Value::Float(_) => Type::Float,
            Value::Bool(_) => Type::Bool,
            Value::Object(ptr) => Type::Object(when_object(*ptr)),
            // TODO: Actually?
            Value::Null => Type::Object(ObjectType::Any),
        }
    }
}

/// Object type lattice.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ObjectType {
    /// Top type of the lattice. It is castable to any of the other
    /// types.
    Any,
    Instance,
    Array,
    Map,
    Function(FunctionType),
    Class,
    String,
    Enum,
    Variant,
    Media(baml_base::MediaKind),
    Future(FutureType),
    Resource,
    PromptAst,
    Collector,
    Type,
}

impl ObjectType {
    pub fn of(ob: &Object) -> Self {
        match ob {
            Object::Function(func) => Self::Function(FunctionType::from(&func.kind)),
            Object::Class(_) => Self::Class,
            Object::Instance(_) => Self::Instance,
            Object::Enum(_) => Self::Enum,
            Object::Variant(_) => Self::Enum,
            Object::String(_) => Self::String,
            Object::Array(_) => Self::Array,
            Object::Map(_) => Self::Map,
            Object::Media(media) => Self::Media(media.kind),
            Object::Resource(_) => Self::Resource,
            Object::PromptAst(_) => Self::PromptAst,
            Object::Collector(_) => Self::Collector,
            Object::Type(_) => Self::Type,
            Object::Future(fut) => Self::Future(fut.into()),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(_) => Self::Any,
            // Object::BamlType(_) => Self::Any, // TODO
        }
    }
}

impl From<FutureType> for ObjectType {
    fn from(value: FutureType) -> Self {
        ObjectType::Future(value)
    }
}

impl From<FunctionType> for ObjectType {
    fn from(value: FunctionType) -> Self {
        ObjectType::Function(value)
    }
}

impl std::fmt::Display for ObjectType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ObjectType::Any => write!(f, "any"),
            ObjectType::Instance => write!(f, "instance"),
            ObjectType::Array => write!(f, "array"),
            ObjectType::Map => write!(f, "map"),
            ObjectType::Function(function_type) => write!(f, "{function_type}"),
            ObjectType::Class => write!(f, "class"),
            ObjectType::Enum => write!(f, "enum"),
            ObjectType::Variant => write!(f, "variant"),
            ObjectType::Future(future_type) => write!(f, "{future_type}"),
            ObjectType::String => write!(f, "string"),
            ObjectType::Media(media_kind) => write!(f, "{media_kind}"),
            ObjectType::Resource => write!(f, "resource"),
            ObjectType::PromptAst => write!(f, "prompt_ast"),
            ObjectType::Collector => write!(f, "collector"),
            ObjectType::Type => write!(f, "type"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Top of function type lattice: represents all function types.
    Any,
    Callable,
    SysOp,
}

impl std::fmt::Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionType::Any => write!(f, "any"),
            FunctionType::Callable => write!(f, "callable"),
            FunctionType::SysOp => write!(f, "sys_op"),
        }
    }
}

impl From<&FunctionKind> for FunctionType {
    fn from(value: &FunctionKind) -> Self {
        if matches!(value, FunctionKind::SysOp(_)) {
            FunctionType::SysOp
        } else {
            FunctionType::Callable
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FutureType {
    /// Top of future type lattice: represents all future types.
    Any,
    Pending,
    Ready,
}

impl std::fmt::Display for FutureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FutureType::Any => write!(f, "any"),
            FutureType::Pending => write!(f, "pending"),
            FutureType::Ready => write!(f, "ready"),
        }
    }
}

impl From<&Future> for FutureType {
    fn from(value: &Future) -> Self {
        match value {
            Future::Pending(_) => Self::Pending,
            Future::Ready(_) => Self::Ready,
        }
    }
}
