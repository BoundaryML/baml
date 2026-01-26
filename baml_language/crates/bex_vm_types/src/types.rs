use std::collections::HashMap;

use indexmap::IndexMap;
use sys_resource_types::ResourceHandle;

use crate::{bytecode::Bytecode, heap_ptr::HeapPtr, indexable::ObjectPool};

// ============================================================================
// Type Tags for Jump Table Dispatch
// ============================================================================

/// Global type tag constants for runtime type identification.
///
/// These are used by the `TypeTag` instruction to extract a type identifier
/// from any value for jump table dispatch on union types.
///
/// Primitives have fixed tags (0-10 reserved), classes start at 100.
pub mod type_tags {
    /// Integer type tag.
    pub const INT: i64 = 0;
    /// String type tag.
    pub const STRING: i64 = 1;
    /// Boolean type tag.
    pub const BOOL: i64 = 2;
    /// Null type tag.
    pub const NULL: i64 = 3;
    /// Float type tag.
    pub const FLOAT: i64 = 4;
    /// Enum variant type tag (all variants share this).
    pub const ENUM: i64 = 5;
    /// List/array type tag.
    pub const LIST: i64 = 6;
    /// Map type tag.
    pub const MAP: i64 = 7;
    /// Function type tag.
    pub const FUNCTION: i64 = 8;
    /// Future type tag.
    pub const FUTURE: i64 = 9;
    /// Media type tag.
    pub const MEDIA: i64 = 10;
    /// Resource type tag (file handle, socket, etc.).
    pub const RESOURCE: i64 = 11;
    /// `PromptAst` type tag.
    pub const PROMPT_AST: i64 = 12;
    /// Base value for class type tags (classes start at 100).
    pub const CLASS_BASE: i64 = 100;
    /// Unknown/invalid type tag.
    pub const UNKNOWN: i64 = -1;
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
// External Operations
// ============================================================================

/// External operation to be executed by the engine.
///
/// This enum enables static dispatch instead of dynamic dispatch via traits.
/// The engine matches on this enum to execute the appropriate async operation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExternalOp {
    /// LLM function call (user-defined functions with LLM body).
    Llm,
    /// System operation (file I/O, shell, HTTP, etc.).
    Sys(SysOp),
}

/// System operations that run outside the VM.
///
/// These are built-in async operations provided by the engine.
/// Add new variants here as new system capabilities are added.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SysOp {
    /// Open a file: `baml.fs.open(path: String) -> File`
    FsOpen,
    /// Read file contents: `File.read() -> String`
    FsRead,
    /// Close a file: `File.close()`
    FsClose,
    /// Execute a shell command: `baml.sys.shell(cmd: String) -> String`
    Shell,
    /// Connect to a TCP socket: `baml.net.connect(addr: String) -> Socket`
    NetConnect,
    /// Read from a socket: `Socket.read() -> String`
    NetRead,
    /// Close a socket: `Socket.close()`
    NetClose,
    /// HTTP fetch: `baml.http.fetch(url: String) -> Response`
    HttpFetch,
    /// Get response body as text: `Response.text() -> String`
    HttpResponseText,
    /// Get response status code: `Response.status() -> i64`
    HttpResponseStatus,
    /// Check if response is OK (2xx): `Response.ok() -> bool`
    HttpResponseOk,
    /// Get request URL: `Response.url() -> String`
    HttpResponseUrl,
    /// Get response headers: `Response.headers() -> Map<String, String>`
    HttpResponseHeaders,
}

impl std::fmt::Display for ExternalOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            ExternalOp::Llm => write!(f, "llm"),
            ExternalOp::Sys(sys_op) => write!(f, "{sys_op}"),
        }
    }
}

impl std::fmt::Display for SysOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            SysOp::FsOpen => write!(f, "fs.open"),
            SysOp::FsRead => write!(f, "fs.read"),
            SysOp::FsClose => write!(f, "fs.close"),
            SysOp::Shell => write!(f, "sys.shell"),
            SysOp::NetConnect => write!(f, "net.connect"),
            SysOp::NetRead => write!(f, "net.read"),
            SysOp::NetClose => write!(f, "net.close"),
            SysOp::HttpFetch => write!(f, "http.fetch"),
            SysOp::HttpResponseText => write!(f, "http.Response.text"),
            SysOp::HttpResponseStatus => write!(f, "http.Response.status"),
            SysOp::HttpResponseOk => write!(f, "http.Response.ok"),
            SysOp::HttpResponseUrl => write!(f, "http.Response.url"),
            SysOp::HttpResponseHeaders => write!(f, "http.Response.headers"),
        }
    }
}

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

    /// External operation (LLM calls, HTTP requests, file I/O, etc.).
    ///
    /// The VM yields control to the engine which executes the operation
    /// asynchronously via static dispatch on the `ExternalOp` enum.
    External(ExternalOp),

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

/// Represents any Baml function.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Bytecode to execute.
    ///
    /// Only relevant if [`Self::kind`] is [`FunctionKind::Bytecode`].
    pub bytecode: Bytecode,

    /// Type of function.
    pub kind: FunctionKind,

    /// Local variable names.
    ///
    /// This is basically debug info, VM doesn't need it all to run.
    pub locals_in_scope: Vec<Vec<String>>,

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
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}

/// Runtime class representation.
#[derive(Clone, Debug)]
pub struct Class {
    /// Class name.
    pub name: String,

    /// Class field names. Debug info, VM doesn't need this.
    pub field_names: Vec<String>,

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

/// Runtime class representation.
#[derive(Clone, Debug)]
pub struct Enum {
    /// Enum name.
    pub name: String,

    /// Enum variant names. Debug info, VM doesn't need this.
    pub variant_names: Vec<String>,
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
    Function(Function),

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
    // TODO: Figure out media.
    // /// Images, audio, pdf, video.
    Media(MediaValue),

    /// External resource (file handle, socket, etc.).
    Resource(ResourceHandle),

    /// Prompt AST tree node.
    PromptAst(PromptAst),

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
    /// The external operation to execute.
    pub operation: ExternalOp,
    /// Arguments to the operation.
    pub args: Vec<Value>,
}

#[derive(Clone, Debug)]
pub struct MediaValue {
    pub kind: baml_base::MediaKind,
    pub content: MediaContent,
    pub mime_type: Option<String>,
}

#[derive(Clone, Debug)]
pub enum MediaContent {
    Url {
        url: String,
        base64_data: Option<String>,
    },
    Base64 {
        base64_data: String,
    },
    File {
        file: String,
        base64_data: Option<String>,
    },
}

impl std::fmt::Display for MediaValue {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}::{}", self.kind, self.content)
    }
}

impl std::fmt::Display for MediaContent {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            MediaContent::Url { url, .. } => write!(f, "url({url})"),
            MediaContent::Base64 { base64_data, .. } => {
                // Show first 5, last 5, and total length for context
                let len = base64_data.len();
                if len <= 10 {
                    write!(f, "base64({base64_data}, len={len})")
                } else {
                    let start = &base64_data[..5];
                    let end = &base64_data[len.saturating_sub(5)..];
                    write!(f, "base64({start}...{end}, len={len})")
                }
            }
            MediaContent::File { file, .. } => write!(f, "file({file})"),
        }
    }
}

// ============================================================================
// PromptAst - represents a structured prompt (recursive tree)
// ============================================================================

/// Options for printing an output format in a prompt.
#[derive(Debug, Clone)]
pub struct PrintOutputFormatOptions {
    /// Separator for union/or types (e.g., " | " or " or ")
    pub or_splitter: String,
}

/// A node in the prompt AST tree.
#[derive(Debug, Clone)]
pub enum PromptAst {
    /// A plain string.
    String(String),

    /// A media value (image, audio, video, etc.) - references heap object.
    Media(HeapPtr),

    /// A message with a role, content, and optional metadata.
    Message {
        role: String,
        content: Box<PromptAst>,
        metadata: Value,
    },

    /// A sequence of prompt nodes.
    Vec(Vec<PromptAst>),

    /// Output format - prints the expected output format for the LLM.
    PrintOutputFormat(PrintOutputFormatOptions),
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
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Top of function type lattice: represents all function types.
    Any,
    Callable,
    External,
}

impl std::fmt::Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionType::Any => write!(f, "any"),
            FunctionType::Callable => write!(f, "callable"),
            FunctionType::External => write!(f, "external"),
        }
    }
}

impl From<&FunctionKind> for FunctionType {
    fn from(value: &FunctionKind) -> Self {
        if matches!(value, FunctionKind::External(_)) {
            FunctionType::External
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
