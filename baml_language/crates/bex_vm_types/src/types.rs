//! Heap object types and runtime values.
//!
//! `Future` uses `AtomicU8` + `UnsafeCell<MaybeUninit<Value>>` to allow
//! the engine's spawned task and the VM to read/write the heap object
//! concurrently without a data race. See the `Future` doc for the safety
//! argument; the unsafe code here is intentional and necessary.

#![allow(unsafe_code)]

use std::{
    any::Any,
    cell::UnsafeCell,
    collections::HashMap,
    mem::MaybeUninit,
    sync::{
        Arc,
        atomic::{AtomicU8, Ordering},
    },
};

use baml_type::Ty;
use indexmap::IndexMap;
use serde::{Deserialize, Serialize};
pub use tokio_util::sync::CancellationToken;

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
#[derive(Clone, Debug, Default, Serialize, Deserialize)]
pub struct Program {
    /// Object pool containing functions, classes, strings, etc.
    pub objects: ObjectPool,

    /// Global variables (converted from `ConstValue` to Value at load time).
    ///
    /// # Init-only invariant
    /// Globals are populated by `$init` (top-level let bindings) at engine
    /// load time and **not** mutated again. The runtime freezes them into a
    /// shared `Arc<[Value]>` after `$init` finishes; only `$init` may emit a
    /// `StoreGlobal` against the still-mutable pool. See `Instruction::StoreGlobal`.
    pub globals: Vec<ConstValue>,

    /// Maps function names to their object indices.
    pub function_indices: HashMap<String, usize>,

    /// Maps function names to their global indices.
    /// Used for dynamic function lookup at runtime.
    pub function_global_indices: HashMap<String, usize>,

    /// Maps let-binding fully-qualified names to their global slot indices.
    /// E.g., `"user.my_const" -> 5`. Populated in Pass 1; slots hold `ConstValue::Null`
    /// until `$init` runs at load time via `StoreGlobal`.
    pub let_global_indices: HashMap<String, usize>,

    /// Pre-formatted Jinja `{% macro %}` definitions for all `template_strings`.
    /// Prepended to function prompt templates by `get_jinja_template`.
    pub template_strings_macros: String,

    /// Client build metadata for constructing full client trees at runtime.
    /// Keyed by client name.
    pub client_metadata: HashMap<String, ClientBuildMeta>,

    /// Compiled test cases.
    pub test_cases: Vec<TestCase>,

    /// Ordered list of `$init` function names to run at load time.
    /// E.g., `["baml.$init", "$init"]` — builtins before user package.
    /// Empty when there are no top-level let bindings in any package.
    pub package_init_order: Vec<String>,

    /// Recursive type alias definitions for output format rendering.
    /// Only recursive aliases are stored (non-recursive ones are expanded inline).
    /// Keyed by [`baml_type::TypeName`] for consistent identity with `Ty::TypeAlias`.
    pub recursive_type_alias_defs: IndexMap<baml_type::TypeName, Ty>,
}

/// Metadata for building a client tree at runtime.
///
/// Stored on `Program` during compilation, transferred to `SysOpContext` during engine construction.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub enum ClientBuildType {
    #[default]
    Primitive,
    Fallback,
    RoundRobin,
}

/// Retry policy metadata stored at compile time.
#[derive(Debug, Clone, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
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

// System operations that run outside the VM.
//
// Generated from `.baml` `$rust_io_function` definitions in `baml_builtins2`.
// The enum, `path()`, `sys_op_for_path()`, and `Display` are all generated
// by `baml_builtins2_codegen` at build time.
// SysOp enum, path(), allowed_error_categories(), allowed_panic_categories(),
// Display, and sys_op_for_path() — generated from .baml $rust_io_function definitions.
include!(concat!(env!("OUT_DIR"), "/sys_op_generated.rs"));

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

impl Serialize for FunctionKind {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Native pointers are runtime-only; serialize as NativeUnresolved.
        match self {
            Self::Native(_) => Self::NativeUnresolved.serialize(serializer),
            _ => {
                #[derive(Serialize)]
                enum FunctionKindRef<'a> {
                    Bytecode,
                    SysOp(&'a SysOp),
                    NativeUnresolved,
                }
                match self {
                    Self::Bytecode => FunctionKindRef::Bytecode.serialize(serializer),
                    Self::SysOp(op) => FunctionKindRef::SysOp(op).serialize(serializer),
                    Self::NativeUnresolved => {
                        FunctionKindRef::NativeUnresolved.serialize(serializer)
                    }
                    Self::Native(_) => unreachable!(),
                }
            }
        }
    }
}

impl<'de> Deserialize<'de> for FunctionKind {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        #[derive(Deserialize)]
        enum FunctionKindDe {
            Bytecode,
            SysOp(SysOp),
            NativeUnresolved,
        }
        match FunctionKindDe::deserialize(deserializer)? {
            FunctionKindDe::Bytecode => Ok(Self::Bytecode),
            FunctionKindDe::SysOp(op) => Ok(Self::SysOp(op)),
            FunctionKindDe::NativeUnresolved => Ok(Self::NativeUnresolved),
        }
    }
}

/// LLM-specific metadata for a function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub enum FunctionMeta {
    Llm {
        prompt_template: String,
        client: String,
    },
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum FunctionOrigin {
    UserDefined,
    Companion,
    Internal,
    Builtin,
    /// Synthesized by the auto-derive pass (e.g. `to_json` / `from_json`
    /// methods generated on every user class). Filterable from bytecode
    /// snapshots via `Function::origin`.
    AutoDerive,
}

impl FunctionOrigin {
    pub const fn is_user_callable(self) -> bool {
        matches!(self, Self::UserDefined | Self::Companion | Self::AutoDerive)
    }

    /// True for methods synthesized by the auto-derive pass; used to filter
    /// them from default bytecode snapshots in tests.
    pub const fn is_auto_derived(self) -> bool {
        matches!(self, Self::AutoDerive)
    }
}

/// Represents any Baml function.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Source file path where this function is defined.
    ///
    /// Set at emit time for bytecode functions. Empty string for builtins and
    /// synthesized functions that have no source file.
    pub source_file: String,

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

    /// Stream-expanded return type (e.g. `null | MyClass$stream` for a function
    /// returning `MyClass`). Only meaningful for LLM functions; set to `Null` for
    /// non-LLM functions. See `PpirExpansionItems::stream_return_types`.
    pub stream_return_type: Ty,

    /// Parameter names in declaration order.
    pub param_names: Vec<String>,

    /// Parameter types in declaration order.
    pub param_types: Vec<Ty>,

    /// Whether each parameter has a BAML default expression.
    pub param_has_default: Vec<bool>,

    /// Inferred throws type — the union of all types this function (and its callees)
    /// may throw. `None` if the function never throws. Used by the engine to convert
    /// uncaught throw values to `BexExternalValue`.
    pub throws_type: Option<Ty>,

    /// Provenance of this function in the compiler/runtime pipeline.
    pub origin: FunctionOrigin,

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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassField {
    pub name: String,
    /// Resolved field type with `TypeVar`s erased to `Ty::Void`.
    ///
    /// Used by paths that don't care about parametric class type-args
    /// (codegen, `sys_ops` walking, output-format rendering).  For typed
    /// runtime walking against an `Instance::class_type_args` binding, use
    /// `field_template` and call `substitute` on it instead.
    pub field_type: Ty,
    /// Field-type template with `TypeArgRef(N)` leaves for class-level
    /// generic params (`N` indexes into `Instance::class_type_args`).
    ///
    /// Populated by emit using the enclosing class's `generic_params`.  For
    /// non-generic classes this is `TyTemplate::Concrete(field_type.clone())`.
    pub field_template: baml_type::TyTemplate,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// Runtime class representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Class {
    /// Type identity: carries short name, module path, and display name.
    /// Use `name.display_name` for the display string (e.g. "baml.llm.OrchestrationStep" or "Person").
    pub name: baml_type::TypeName,

    /// Class fields with type and schema metadata.
    pub fields: Vec<ClassField>,

    /// Class-level description for LLM prompt schema rendering.
    pub description: Option<String>,

    /// Class-level serialization alias.
    pub alias: Option<String>,

    /// Type tag for this class, used by `TypeTag` instruction for jump table dispatch.
    /// Assigned during codegen as `CLASS_BASE + class_index`.
    pub type_tag: i64,

    /// Class-level type attribute (e.g., from @@stream.done).
    pub ty_attr: baml_type::TyAttr,
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Runtime instance representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Instance {
    /// Pointer to the class object in the heap.
    pub class: HeapPtr,

    /// Resolved class-level type args at construction time.  Empty when the
    /// class is non-generic.  De Bruijn-ordered to match
    /// `enclosing_generic_params()`: index 0 = first class param, etc.
    pub class_type_args: Vec<baml_type::Ty>,

    /// Fields are accessed by index. No string lookups.
    pub fields: Vec<Value>,
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {:p}>", self.class.as_ptr())
    }
}

/// A variant within a runtime enum, carrying schema metadata.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct EnumVariant {
    pub name: String,
    pub description: Option<String>,
    pub alias: Option<String>,
    pub skip: bool,
}

/// Runtime enum representation.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Enum {
    /// Type identity: carries short name, module path, and display name.
    /// Use `name.display_name` for the display string.
    pub name: baml_type::TypeName,

    /// Enum variants with schema metadata.
    pub variants: Vec<EnumVariant>,

    /// Enum-level description.
    pub description: Option<String>,

    /// Enum-level serialization alias.
    pub alias: Option<String>,

    /// Enum-level type attribute.
    pub ty_attr: baml_type::TyAttr,
}

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name)
    }
}

/// Same as [`Instance`] but for enums.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub enum Value {
    /// Internal sentinel for an omitted defaulted argument.
    ///
    /// This value is only valid between call binding and a callee-entry default
    /// prologue. It must not be serialized or exposed to host code.
    OmittedArg,
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

impl Value {
    /// Returns the [`HeapPtr`] if this is an [`Object`].
    pub const fn as_object_ptr(&self) -> Option<HeapPtr> {
        match self {
            Value::OmittedArg => None,
            Value::Object(ptr) => Some(*ptr),
            _ => None,
        }
    }
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::OmittedArg => write!(f, "<omitted>"),
            Value::Null => write!(f, "null"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Float(float) => write!(f, "{}", format_float(*float)),
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::Object(ptr) => write!(f, "{ptr}"),
        }
    }
}

/// Format an f64 to string, following JS/TS conventions for special values
/// and preserving `.0` for whole-number floats.
///
/// - `1.0` → `"1.0"` (not `"1"` — preserves float identity)
/// - `3.14` → `"3.14"`
/// - `f64::INFINITY` → `"Infinity"` (JS-style)
/// - `f64::NEG_INFINITY` → `"-Infinity"` (JS-style)
/// - `f64::NAN` → `"NaN"` (JS-style)
pub fn format_float(f: f64) -> String {
    if f.is_nan() {
        return "NaN".to_string();
    }
    if f.is_infinite() {
        return if f.is_sign_positive() {
            "Infinity".to_string()
        } else {
            "-Infinity".to_string()
        };
    }
    let s = f.to_string();
    if s.contains('.') { s } else { format!("{s}.0") }
}

// Error class / instance enums — generated from `errors.baml` class definitions.
// ErrorClass (tag enum), ErrorInstance (with Value fields), associated methods.
include!(concat!(env!("OUT_DIR"), "/errors_generated.rs"));
// Panic class / instance enums — generated from `panics.baml` class definitions.
// PanicClass (tag enum), PanicInstance (with Value fields), associated methods.
include!(concat!(env!("OUT_DIR"), "/panics_generated.rs"));

// ============================================================================
// Test Cases
// ============================================================================

/// A constant value for test arguments.
///
/// Self-contained type with no dependency on HIR or external types.
/// Converted from HIR's `TestArgValue` during emission, and converted
/// to `BexExternalValue` in the engine for function calls.
#[derive(Clone, Debug, Serialize, Deserialize)]
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
#[derive(Clone, Debug, Serialize, Deserialize)]
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
///
/// Note: `ConstValue::Type` is intentionally excluded from the `to_value` conversion — the
/// `LoadType` instruction reads the `TyTemplate` directly from the constant pool at execution
/// time and substitutes type arguments from `frame.type_args` before allocating an
/// `Object::Type` on the heap.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub enum ConstValue {
    OmittedArg,
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Index into the object pool (converted to `HeapPtr` at load time).
    Object(crate::ObjectIndex),
    /// A type template for use by the `LoadType` instruction.
    ///
    /// Unlike the other variants, this constant is **not** pre-resolved at load
    /// time — `LoadType` reads the template here and performs substitution at
    /// runtime (using the current frame's `type_args`).
    Type(baml_type::TyTemplate),
    /// A parametric-class `IsType` check constant.
    ///
    /// Used by `Instruction::IsType` when the expected type is a generic class
    /// instantiation (e.g. `Foo<int>` or `Foo<T>`).  Like `ConstValue::Type`,
    /// this constant is **not** pre-resolved: the `IsType` VM dispatch reads it
    /// directly from the raw constant pool and resolves the `class_obj` index to
    /// a `HeapPtr` at execution time.
    ClassWithTypeArgs {
        /// Compile-time index of the class object in the object pool.
        class_obj: crate::ObjectIndex,
        /// Templates for the class-level type args, in De Bruijn order.
        /// `TypeArgRef(n)` refers to `frame.type_args[n]`.
        type_args_templates: Vec<baml_type::TyTemplate>,
    },
}

impl ConstValue {
    /// Convert to a runtime `Value` using a function to resolve object indices to heap pointers.
    ///
    /// # Panics
    ///
    /// Panics if called on `ConstValue::Type` — type-template constants are
    /// handled at runtime by the `LoadType` instruction, not pre-resolved at
    /// load time.
    pub fn to_value<F>(&self, resolve: F) -> Value
    where
        F: Fn(crate::ObjectIndex) -> HeapPtr,
    {
        match self {
            ConstValue::OmittedArg => Value::OmittedArg,
            ConstValue::Null => Value::Null,
            ConstValue::Int(v) => Value::Int(*v),
            ConstValue::Float(v) => Value::Float(*v),
            ConstValue::Bool(v) => Value::Bool(*v),
            ConstValue::Object(idx) => Value::Object(resolve(*idx)),
            ConstValue::Type(_) => {
                panic!(
                    "ConstValue::Type must not be pre-resolved via to_value — \
                     use the LoadType instruction instead"
                )
            }
            ConstValue::ClassWithTypeArgs { .. } => {
                panic!(
                    "ConstValue::ClassWithTypeArgs must not be pre-resolved via to_value — \
                     use the IsType instruction instead"
                )
            }
        }
    }
}

/// Media value.
///
/// Kept as a type alias for compatibility with downstream crates that still use it.
/// Within `bex_vm`, media is now stored as `Object::Instance` with a `$rust_type` `_data` field.
pub type MediaValue = std::sync::Arc<baml_builtins2::MediaValue>;

/// Prompt AST tree node.
pub type PromptAst = std::sync::Arc<baml_builtins2::PromptAst>;

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
    Class(Box<Class>),

    /// Class instance object.
    Instance(Instance),

    /// Enum object.
    Enum(Box<Enum>),

    /// Enum value object.
    Variant(Variant),

    /// A closure: a function paired with captured variable cells.
    Closure(Closure),

    /// A method bound to a specific receiver instance.
    /// Created by `MakeBoundMethod`. The receiver is inserted as `self`
    /// at call time by `CallIndirect`.
    BoundMethod(BoundMethod),

    /// A mutable cell holding a single captured value.
    Cell(Cell),

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

    /// Byte array (uint8array).
    Uint8Array(Vec<u8>),

    /// List of values.
    Array(Vec<Value>),

    /// Map of values.
    Map(IndexMap<String, Value>),

    Future(Future),
    /// Only used for requesting scheduling of a future, passed from VM to engine.
    UnscheduledFuture(UnscheduledFuture),

    /// Opaque Rust-managed data, accessed via `Arc<dyn Any>` downcast.
    /// Used for `$rust_type` fields in builtin classes (including media classes Pdf, Audio, Video, Image).
    RustData(Arc<dyn Any + Send + Sync>),

    /// Collector object (opaque handle to `bex_events::Collector`).
    Collector(CollectorRef),

    /// A type descriptor value — wraps a `baml_type::Ty`.
    Type(Box<baml_type::Ty>),

    #[cfg(feature = "heap_debug")]
    Sentinel(SentinelKind),
}

const _: () = assert!(
    std::mem::size_of::<Object>() <= 80,
    "Object enum size regression — expected <= 80 bytes"
);

// Custom serde for Object: RustData and Collector contain non-serializable
// trait objects (Arc<dyn Any>). They should never appear in a compiled Program.
#[derive(Serialize, Deserialize)]
enum ObjectSerde {
    Function(Box<Function>),
    Class(Box<Class>),
    Instance(Instance),
    Enum(Box<Enum>),
    Variant(Variant),
    Closure(Closure),
    BoundMethod(BoundMethod),
    Cell(Cell),
    String(String),
    Uint8Array(Vec<u8>),
    Array(Vec<Value>),
    Map(IndexMap<String, Value>),
    Future(Future),
    UnscheduledFuture(UnscheduledFuture),
    Type(Box<baml_type::Ty>),
}

impl Serialize for Object {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        let proxy = match self {
            Self::Function(v) => ObjectSerde::Function(v.clone()),
            Self::Class(v) => ObjectSerde::Class(v.clone()),
            Self::Instance(v) => ObjectSerde::Instance(v.clone()),
            Self::Enum(v) => ObjectSerde::Enum(v.clone()),
            Self::Variant(v) => ObjectSerde::Variant(v.clone()),
            Self::Closure(v) => ObjectSerde::Closure(v.clone()),
            Self::BoundMethod(v) => ObjectSerde::BoundMethod(v.clone()),
            Self::Cell(v) => ObjectSerde::Cell(v.clone()),
            Self::String(v) => ObjectSerde::String(v.clone()),
            Self::Uint8Array(v) => ObjectSerde::Uint8Array(v.clone()),
            Self::Array(v) => ObjectSerde::Array(v.clone()),
            Self::Map(v) => ObjectSerde::Map(v.clone()),
            Self::Future(v) => ObjectSerde::Future(v.clone()),
            Self::UnscheduledFuture(v) => ObjectSerde::UnscheduledFuture(v.clone()),
            Self::Type(v) => ObjectSerde::Type(v.clone()),
            Self::RustData(_) => {
                return Err(serde::ser::Error::custom("RustData cannot be serialized"));
            }
            Self::Collector(_) => {
                return Err(serde::ser::Error::custom("Collector cannot be serialized"));
            }
            #[cfg(feature = "heap_debug")]
            Self::Sentinel(_) => {
                return Err(serde::ser::Error::custom("Sentinel cannot be serialized"));
            }
        };
        proxy.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for Object {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        let proxy = ObjectSerde::deserialize(deserializer)?;
        Ok(match proxy {
            ObjectSerde::Function(v) => Self::Function(v),
            ObjectSerde::Class(v) => Self::Class(v),
            ObjectSerde::Instance(v) => Self::Instance(v),
            ObjectSerde::Enum(v) => Self::Enum(v),
            ObjectSerde::Variant(v) => Self::Variant(v),
            ObjectSerde::Closure(v) => Self::Closure(v),
            ObjectSerde::BoundMethod(v) => Self::BoundMethod(v),
            ObjectSerde::Cell(v) => Self::Cell(v),
            ObjectSerde::String(v) => Self::String(v),
            ObjectSerde::Uint8Array(v) => Self::Uint8Array(v),
            ObjectSerde::Array(v) => Self::Array(v),
            ObjectSerde::Map(v) => Self::Map(v),
            ObjectSerde::Future(v) => Self::Future(v),
            ObjectSerde::UnscheduledFuture(v) => Self::UnscheduledFuture(v),
            ObjectSerde::Type(v) => Self::Type(v),
        })
    }
}

/// A closure: a function object paired with a list of captured variable cells.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Closure {
    /// Pointer to the underlying `Object::Function`.
    pub function: HeapPtr,
    /// Captured cells, one per closed-over variable (each is `Object::Cell`).
    pub captures: Vec<Value>,
    /// Type arguments captured from the enclosing generic context at the time
    /// the closure is created by `MakeClosure`.
    ///
    /// Populated by the `MakeClosure { ntypeargs }` instruction which pops
    /// `ntypeargs` `Object::Type` values from the operand stack immediately
    /// before the cell captures.  These become `frame.type_args` when the
    /// closure is invoked, so that `LoadType(TypeArgRef(N))` inside the
    /// closure body resolves correctly.
    pub captured_type_args: Vec<baml_type::Ty>,
}

/// A method bound to a specific receiver instance.
///
/// Created by `MakeBoundMethod`. The receiver is inserted as `self`
/// at call time by `CallIndirect`.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BoundMethod {
    /// Pointer to the underlying `Object::Function`.
    pub function: HeapPtr,
    /// The receiver value (inserted as `self` at call time).
    pub receiver: Value,
}

/// A mutable cell wrapping a single captured value.
///
/// Variables that are closed over are heap-allocated as `Cell` objects so that
/// both the enclosing scope and any closures share the same storage.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Cell {
    pub value: Value,
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Function(function) => function.fmt(f),
            Object::Class(class) => class.fmt(f),
            Object::Instance(instance) => instance.fmt(f),
            Object::Enum(enm) => enm.fmt(f),
            Object::Variant(value) => value.fmt(f),
            Object::Closure(closure) => {
                let captures_len = closure.captures.len();
                write!(f, "<closure captures={captures_len}>")
            }
            Object::BoundMethod(_) => write!(f, "<bound_method>"),
            Object::Cell(cell) => write!(f, "<cell {}>", cell.value),
            Object::String(string) => string.fmt(f),
            Object::Uint8Array(bytes) => write!(f, "<uint8array len={}>", bytes.len()),
            Object::Array(array) => write!(f, "<array len={}>", array.len()),
            Object::Map(map) => write!(f, "<map len={}>", map.len()),
            Object::RustData(_) => write!(f, "<rust_data>"),
            Object::Collector(_) => write!(f, "<collector>"),
            Object::Type(ty) => write!(f, "<type: {ty}>"),
            Object::Future(future) => match future.read() {
                FutureRead::Pending(id) => {
                    write!(f, "<pending: future #{}>", id.id)
                }
                FutureRead::Ready(value) => write!(f, "<ready: {value}>"),
                FutureRead::Error(value) => write!(f, "<error: {value}>"),
                FutureRead::Cancelled => write!(f, "<cancelled>"),
                FutureRead::InternalError(id) => {
                    write!(f, "<internal error: future #{}>", id.id)
                }
            },
            Object::UnscheduledFuture(_) => write!(f, "<unscheduled: spawn>"),
            #[cfg(feature = "heap_debug")]
            Object::Sentinel(kind) => write!(f, "<sentinel {kind:?}>"),
            // Object::BamlType(type_ir) => write!(f, "<baml type: {type_ir}>"),
        }
    }
}

/// Error payload carried by a future's [`Future::ready`] `SetOnce` when the
/// underlying engine produced an unrecoverable internal error.
///
/// Type-erased so this crate doesn't have to pull in `bex_engine`'s
/// `EngineError` (which would form a cycle). The engine boxes its
/// `EngineError` into this shape when transitioning a future to the
/// `InternalError` terminal state (see `FutureRead::InternalError`);
/// consumers (on the await side) downcast when surfacing the error to
/// the host.
pub type FutureInternalError = Box<dyn std::error::Error + Send + Sync>;

/// A future heap object.
///
/// Holds the cross-thread state-machine for one `spawn { ... }` body:
/// atomic discriminant, optional result value, cancellation token, and a
/// `SetOnce` that wakes any consumer blocked in `await`. All synchronization
/// primitives live on the heap object itself — there is no central
/// `FutureManager` registry — so producer (spawned task) and consumer
/// (awaiter / `f.cancel()` caller) communicate directly through the heap.
///
/// Concretely:
///
/// - `state` is loaded with `Acquire` and stored with `Release`. When a
///   reader observes a terminal-state tag, all preceding payload writes by
///   the writer are visible to it.
/// - `id` is set at construction and never modified. It's purely for
///   debug/tracing; nothing keys lookups off it anymore.
/// - `value` is wrapped in [`UnsafeCell<MaybeUninit<Value>>`] and is written
///   *at most once* (during the unique transition from `Pending` to
///   `Ready` or `Error`). It is only readable when `state` indicates
///   `Ready` or `Error`.
/// - `cancel` is the producer-observable cancel token. Consumers fire it
///   via `f.cancel()`; the producer's next `await` checkpoint throws
///   `baml.panics.Cancelled`. Children spawned by the producer derive
///   their tokens from this one so cancellation cascades.
/// - `ready` is the cross-task wake mechanism. Producers set it after any
///   terminal state transition; the awaiter (via VM `Await` → engine)
///   awaits on a clone of this Arc.
///
/// # Safety
///
/// Writers (the producer thread, plus `f.cancel()` callers) coordinate via
/// the `state` atomic itself: terminal transitions are
/// `compare_exchange(Pending → terminal)`; the first CAS wins and is the
/// sole authority that writes `value`. The Acquire/Release pairing on
/// `state` provides the happens-before for cross-thread reads of `value`.
#[repr(C)]
pub struct Future {
    /// Atomic discriminant (one of [`FutureTag`]). Loaded with `Acquire`,
    /// stored with `Release`. The first thread to successfully transition
    /// `Pending → terminal` via `compare_exchange` is the unique writer.
    state: AtomicU8,
    /// Set at construction; never modified. Purely for debug/tracing.
    id: FutureId,
    /// Written at most once by whichever writer wins the `state` CAS.
    /// Valid only when `state` indicates `Ready` or `Error`. For
    /// `Cancelled` / `InternalError`, this stays uninitialized.
    value: UnsafeCell<MaybeUninit<Value>>,
    /// Cancellation signal observed by the producer. Fired by
    /// `f.cancel()` or by parent-cascade when an ancestor is cancelled.
    pub cancel: CancellationToken,
    /// Cross-task wake: producer (or cancel) sets it on terminal
    /// transition; awaiter clones the Arc and `.wait().await`s.
    /// `Ok(())` is "look at `state` for the actual outcome"; `Err(_)`
    /// carries an unrecoverable engine error for surfacing through the
    /// engine's `Await` resume path.
    pub ready: Arc<tokio::sync::SetOnce<Result<(), FutureInternalError>>>,
}

// SAFETY: All access to `value` is gated by the Acquire/Release handshake
// on `state` and the single-writer invariant enforced by the
// `FutureManager`'s state mutex.
unsafe impl Send for Future {}
unsafe impl Sync for Future {}

// Futures are runtime-only; they never appear in a compiled Program. Reject
// serialization explicitly so a malformed program fails fast.
impl Serialize for Future {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom("Future cannot be serialized"))
    }
}

impl<'de> Deserialize<'de> for Future {
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom("Future cannot be deserialized"))
    }
}

// `UnscheduledFuture` is a runtime spawn-request slot — same lifecycle
// shape as `Future`, never appears in a compiled `Program`. The pack
// envelope (`baml_exec::PackEnvelope`) serializes the bytecode + the
// constant heap; if an `UnscheduledFuture` ever reaches the serializer
// that's a malformed program and we want to fail fast.
impl Serialize for UnscheduledFuture {
    fn serialize<S: serde::Serializer>(&self, _serializer: S) -> Result<S::Ok, S::Error> {
        Err(serde::ser::Error::custom(
            "UnscheduledFuture cannot be serialized",
        ))
    }
}

impl<'de> Deserialize<'de> for UnscheduledFuture {
    fn deserialize<D: serde::Deserializer<'de>>(_deserializer: D) -> Result<Self, D::Error> {
        Err(serde::de::Error::custom(
            "UnscheduledFuture cannot be deserialized",
        ))
    }
}

// `Future::read` calls `MaybeUninit::<Value>::assume_init_read`, which is
// sound only because `Value: Copy`. If `Value` ever gains a non-trivial
// `Drop` (e.g. by holding an `Arc<…>` or `Box<…>`), `assume_init_read`
// becomes UB on the second read. Guard against that at compile time.
const _: () = {
    const fn assert_copy<T: Copy>() {}
    assert_copy::<Value>();
};

/// Discriminant byte for [`Future::state`].
#[repr(u8)]
enum FutureTag {
    Pending = 0,
    Ready = 1,
    Error = 2,
    Cancelled = 3,
    InternalError = 4,
}

/// Snapshot view of a [`Future`] used for pattern matching at read sites.
///
/// Returned by [`Future::read`] after an `Acquire`-load of the discriminant
/// and (for `Ready`/`Error`) a synchronized read of the payload.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum FutureRead {
    /// Pending future.
    ///
    /// In terms of synchronization, this is "pending" from the heap's point of view.
    /// It will remain pending until set otherwise, but yielding back to the engine *could* see an immediate completion.
    Pending(FutureId),

    /// Ready value for the future.
    Ready(Value),

    /// A BAML error or panic occurred while executing the future.
    /// If awaited, the error/panic value will be thrown.
    ///
    /// Note: not currently produced by the engine. Reserved for future
    /// user-callable async functions that throw BAML values; the engine
    /// today routes all sys-op errors through `internal_error_future`.
    Error(Value),

    /// The future was cancelled before completion.
    /// If awaited, this will throw `baml.panics.Cancelled`.
    Cancelled,

    /// An unrecoverable internal error occurred while executing the future.
    /// The originating `FutureId` is preserved so the VM can yield control back
    /// to the engine on `Await`, allowing the engine to surface the underlying
    /// error from the `FutureManager`'s registry. Such entries are leaked from
    /// `FutureManager::active_futures` by design.
    InternalError(FutureId),
}

impl Future {
    /// Construct a new [`Future`] in the `Pending` state.
    ///
    /// `cancel` is the future's own cancel token — fired by `f.cancel()`
    /// and observed by the producer. The caller is responsible for deriving
    /// it from the spawning thread's token so cascade cancellation works.
    pub fn pending(id: FutureId, cancel: CancellationToken) -> Self {
        Self {
            state: AtomicU8::new(FutureTag::Pending as u8),
            id,
            value: UnsafeCell::new(MaybeUninit::uninit()),
            cancel,
            ready: Arc::new(tokio::sync::SetOnce::new()),
        }
    }

    /// `FutureId` for debug/tracing purposes.
    pub fn id(&self) -> FutureId {
        self.id
    }

    /// Read the current state with appropriate atomic ordering.
    ///
    /// `Acquire`-loads the discriminant, then dispatches to the right
    /// payload field. For `Ready`/`Error`, reading `value` is synchronized
    /// against the writer's `Release`-store so the value is fully visible.
    pub fn read(&self) -> FutureRead {
        let tag = self.state.load(Ordering::Acquire);
        match tag {
            t if t == FutureTag::Pending as u8 => FutureRead::Pending(self.id),
            t if t == FutureTag::Ready as u8 => {
                // SAFETY: the Acquire-load above synchronized with the
                // writer's Release-store of `Ready`, so the preceding
                // `value` write is visible. `Value: Copy`, so a read here
                // does not move the underlying data.
                let v = unsafe { (*self.value.get()).assume_init_read() };
                FutureRead::Ready(v)
            }
            t if t == FutureTag::Error as u8 => {
                // SAFETY: as for `Ready`. See above.
                let v = unsafe { (*self.value.get()).assume_init_read() };
                FutureRead::Error(v)
            }
            t if t == FutureTag::Cancelled as u8 => FutureRead::Cancelled,
            t if t == FutureTag::InternalError as u8 => FutureRead::InternalError(self.id),
            other => unreachable!("invalid Future discriminant byte: {other}"),
        }
    }

    /// Mutable access to the embedded `Value` for `Ready`/`Error` states.
    ///
    /// Used by the GC's fixup pass to update heap pointers after a move.
    /// Returns `Some(&mut Value)` only if the current state is `Ready` or
    /// `Error`. The GC runs with all permits parked, so synchronization is
    /// not needed — but a `Relaxed` load is used for clarity.
    ///
    /// # Safety
    ///
    /// The caller must hold exclusive access to the heap (e.g., a parked
    /// `HeapGuard`). Concurrent calls to `set_*` would race.
    pub unsafe fn value_mut_for_fixup(&mut self) -> Option<&mut Value> {
        let tag = *self.state.get_mut();
        if tag == FutureTag::Ready as u8 || tag == FutureTag::Error as u8 {
            // SAFETY: state indicates `Ready`/`Error`; the value is
            // initialized. `&mut self` proves no concurrent reader.
            Some(unsafe { (*self.value.get()).assume_init_mut() })
        } else {
            None
        }
    }

    /// Attempt to transition `Pending → Ready`, writing `value` and firing
    /// the wake signal. Returns `true` if the transition was performed.
    ///
    /// A `false` return means another writer (a concurrent `f.cancel()`,
    /// most likely) already settled the future to a different terminal
    /// state. The producer in that case discards `value` and exits.
    ///
    /// Cross-thread synchronization: the speculative `value` write happens
    /// before the CAS; the CAS uses `AcqRel` so a reader observing `Ready`
    /// also observes the value write. If the CAS fails, the value cell is
    /// reset back to uninitialized to keep GC honest (Ready/Error states
    /// are the only ones for which GC traces the cell, and our state is
    /// not Ready, so the cell shouldn't claim to hold a tracked Value).
    ///
    /// Fires the generational write barrier on `heap` for `self_ptr`
    /// before the value write. This is required because a `Future` can
    /// survive across GCs (rooted by `FutureManagerInner::active_futures`)
    /// and may end up in Gen2; if `value` is a `Value::Object` referring
    /// to a younger-generation heap object, the next Minor GC's
    /// dirty-card scan must find this reference. Without the barrier the
    /// young object would be reclaimed and the `Future`'s `value` left
    /// dangling.
    ///
    /// # Safety
    ///
    /// Caller must hold the heap permit (to keep `value`'s embedded
    /// `HeapPtr`, if any, valid against concurrent GC moves). `self_ptr`
    /// must be the [`HeapPtr`] under which `self` lives, so the write
    /// barrier marks the correct card.
    pub unsafe fn settle_ready(
        &self,
        heap: &impl crate::WriteBarrier,
        self_ptr: HeapPtr,
        value: Value,
    ) -> bool {
        // Fire the generational write barrier BEFORE the speculative
        // value write. If `value` is a young heap pointer and our CAS
        // later wins, the card mark is what tells the next minor GC
        // to find this reference. (If the CAS loses, the rollback
        // below reverts the value cell but the spurious card mark is
        // benign — the GC will simply rescan it.)
        heap.write_barrier(self_ptr, value);
        // SAFETY: speculative write; observed by readers only if our CAS
        // wins (Release synchronizes the write to subsequent Acquire-loads).
        unsafe { (*self.value.get()).write(value) };
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Ready as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let _ = self.ready.set(Ok(()));
                true
            }
            Err(_) => {
                // CAS failed — another writer beat us. Roll back the
                // speculative write so GC's `value_mut_for_fixup` (which
                // gates on state) doesn't trip over stale contents.
                // SAFETY: state isn't Ready/Error, so no reader will look.
                unsafe { *self.value.get() = MaybeUninit::uninit() };
                false
            }
        }
    }

    /// Attempt to transition `Pending → Error`, writing the error value
    /// and firing the wake signal. Mirror of [`Self::settle_ready`].
    ///
    /// Fires the generational write barrier — see [`Self::settle_ready`].
    ///
    /// # Safety
    ///
    /// See [`Self::settle_ready`].
    pub unsafe fn settle_error(
        &self,
        heap: &impl crate::WriteBarrier,
        self_ptr: HeapPtr,
        value: Value,
    ) -> bool {
        heap.write_barrier(self_ptr, value);
        // SAFETY: see settle_ready.
        unsafe { (*self.value.get()).write(value) };
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Error as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let _ = self.ready.set(Ok(()));
                true
            }
            Err(_) => {
                // SAFETY: see settle_ready.
                unsafe { *self.value.get() = MaybeUninit::uninit() };
                false
            }
        }
    }

    /// Attempt to transition `Pending → Cancelled`. Fires the cancel
    /// token (so the producer's next await checkpoint observes it) and
    /// the wake signal (so any current awaiter resumes).
    ///
    /// Returns `true` if the transition was performed. Idempotent in the
    /// sense that repeated calls all return `false` after the first
    /// successful one.
    pub fn settle_cancelled(&self) -> bool {
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::Cancelled as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                self.cancel.cancel();
                let _ = self.ready.set(Ok(()));
                true
            }
            Err(_) => false,
        }
    }

    /// Attempt to transition `Pending → InternalError`, carrying `err`
    /// on the wake signal for the engine to surface to the host on the
    /// awaiter's next `await` re-execution.
    ///
    /// Returns `true` if the transition was performed.
    pub fn settle_internal_error(&self, err: FutureInternalError) -> bool {
        match self.state.compare_exchange(
            FutureTag::Pending as u8,
            FutureTag::InternalError as u8,
            Ordering::AcqRel,
            Ordering::Acquire,
        ) {
            Ok(_) => {
                let _ = self.ready.set(Err(err));
                true
            }
            Err(_) => false,
        }
    }
}

impl Clone for Future {
    fn clone(&self) -> Self {
        // Snapshot the current state and clone the corresponding payload.
        // The only legitimate caller is the GC's heap-relocation copy
        // (`gc.rs` `copy_object_to_inactive`), which clones the heap object
        // into the inactive space.
        //
        // The `cancel` token and `ready` SetOnce are reference-counted
        // (CancellationToken has internal `Arc`, `ready` is wrapped in an
        // explicit `Arc`), so the clone shares the same underlying sync
        // primitives. Producers that hold a clone of `ready` from before
        // the GC move continue to wake the same set of waiters, and the
        // moved heap copy observes the same `ready.set(...)` because both
        // copies' `Arc<SetOnce>` point at the same allocation.
        //
        // Futures are conceptually *handles*, not values: there is no
        // "the same future, but a copy" at the user level. User-side
        // `deep_copy` reflects this by sharing the original `HeapPtr`
        // for any `Future` rather than calling this `Clone` impl. See
        // `crates/bex_vm/src/package_baml/root.rs::deep_copy_value_recursive`.
        let read = self.read();
        let cloned = Self {
            state: AtomicU8::new(0), // placeholder; rewritten below
            id: self.id,
            value: UnsafeCell::new(MaybeUninit::uninit()),
            cancel: self.cancel.clone(),
            ready: Arc::clone(&self.ready),
        };
        let tag: u8 = match read {
            FutureRead::Pending(_) => FutureTag::Pending as u8,
            FutureRead::Ready(v) => {
                // SAFETY: we just constructed `cloned` and have exclusive
                // access; no other observer exists yet.
                unsafe { (*cloned.value.get()).write(v) };
                FutureTag::Ready as u8
            }
            FutureRead::Error(v) => {
                // SAFETY: as above.
                unsafe { (*cloned.value.get()).write(v) };
                FutureTag::Error as u8
            }
            FutureRead::Cancelled => FutureTag::Cancelled as u8,
            FutureRead::InternalError(_) => FutureTag::InternalError as u8,
        };
        cloned.state.store(tag, Ordering::Release);
        cloned
    }
}

impl std::fmt::Debug for Future {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self.read() {
            FutureRead::Pending(id) => f.debug_tuple("Pending").field(&id).finish(),
            FutureRead::Ready(v) => f.debug_tuple("Ready").field(&v).finish(),
            FutureRead::Error(v) => f.debug_tuple("Error").field(&v).finish(),
            FutureRead::Cancelled => f.write_str("Cancelled"),
            FutureRead::InternalError(id) => f.debug_tuple("InternalError").field(&id).finish(),
        }
    }
}

/// A pending user `spawn { body }` request that the engine still has to
/// dispatch on a fresh `BexThread`.
///
/// BEP-034 phase D′: this struct used to also carry sys-op invocations
/// (`kind: SysOp { ... }`), but sys-ops now go through the single-yield
/// `VmExecState::SysOp` path without allocating a heap object. Only the
/// spawn case survives.
#[derive(Clone, Debug)]
pub struct UnscheduledFuture {
    /// Pointer to an `Object::Closure` carrying the spawn body.
    pub closure: HeapPtr,
    /// Optional human-readable name attached at the spawn site. Surfaces in
    /// debug, stack traces, and the playground. Held here as a `HeapPtr` so
    /// the GC keeps the underlying string alive while the unscheduled
    /// future is on the heap.
    pub name: Option<HeapPtr>,
}

/// A unique identifier for a future.
///
/// Unlike `bex_engine::CallId`, these are created for every scheduled future (sys op or function call),
/// not just when there is a new call from the host.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct FutureId {
    id: usize,
}

impl std::fmt::Display for FutureId {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Render as a bare number so error messages read as
        // "Future with ID 42 not found" instead of "FutureId { id: 42 }".
        self.id.fmt(f)
    }
}

impl FutureId {
    /// Construct a [`FutureId`] from a raw `usize`.
    ///
    /// # Contract
    ///
    /// Each `FutureId` constructed for a given engine **must** have a `usize`
    /// value distinct from every other live `FutureId` in that engine. The
    /// engine satisfies this by issuing values from a monotonic
    /// [`AtomicUsize`](::core::sync::atomic::AtomicUsize) counter inside its
    /// `FutureManager`.
    ///
    /// Violating this contract does **not** cause memory unsafety, but it
    /// causes `FutureManager` lookup collisions (two distinct futures sharing
    /// the same map key, with all the silent data corruption that implies).
    /// Outside of the engine and its tests, prefer calls that route through
    /// `FutureManagerGuard::new_future` instead of constructing ids by hand.
    pub fn from_usize(id: usize) -> Self {
        Self { id }
    }

    pub fn as_usize(self) -> usize {
        self.id
    }
}

/// Types of values.
///
/// Used for checking type errors at runtime. We can probably use some lib
/// that creates this automatically based on the [`Value`] enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Type {
    OmittedArg,
    Int,
    Float,
    Bool,
    Object(ObjectType),
}

impl std::fmt::Display for Type {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Type::OmittedArg => write!(f, "omitted argument"),
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
            Value::OmittedArg => Type::OmittedArg,
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
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObjectType {
    /// Top type of the lattice. It is castable to any of the other
    /// types.
    Any,
    Instance,
    Uint8Array,
    Array,
    Map,
    Function(FunctionType),
    Closure,
    Cell,
    Class,
    String,
    Enum,
    Variant,
    Future(FutureType),
    UnscheduledFuture,
    Collector,
    Type,
    RustData,
}

impl ObjectType {
    pub fn of(ob: &Object) -> Self {
        match ob {
            Object::Function(func) => Self::Function(FunctionType::from(&func.kind)),
            Object::Closure(_) => Self::Closure,
            Object::BoundMethod(_) => Self::Closure, // Treat as callable like closures
            Object::Cell(_) => Self::Cell,
            Object::Class(_) => Self::Class,
            Object::Instance(_) => Self::Instance,
            Object::Enum(_) => Self::Enum,
            Object::Variant(_) => Self::Enum,
            Object::String(_) => Self::String,
            Object::Uint8Array(_) => Self::Uint8Array,
            Object::Array(_) => Self::Array,
            Object::Map(_) => Self::Map,
            Object::RustData(_) => Self::RustData,
            Object::Collector(_) => Self::Collector,
            Object::Type(_) => Self::Type,
            Object::Future(fut) => Self::Future(fut.into()),
            Object::UnscheduledFuture(_) => Self::UnscheduledFuture,
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
            ObjectType::Closure => write!(f, "closure"),
            ObjectType::Cell => write!(f, "cell"),
            ObjectType::Class => write!(f, "class"),
            ObjectType::Enum => write!(f, "enum"),
            ObjectType::Variant => write!(f, "variant"),
            ObjectType::Future(future_type) => write!(f, "{future_type}"),
            ObjectType::UnscheduledFuture => write!(f, "unscheduled_future"),
            ObjectType::String => write!(f, "string"),
            ObjectType::Uint8Array => write!(f, "uint8array"),
            ObjectType::Collector => write!(f, "collector"),
            ObjectType::Type => write!(f, "type"),
            ObjectType::RustData => write!(f, "rust_data"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
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

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum FutureType {
    /// Top of future type lattice: represents all future types.
    Any,
    Pending,
    Ready,
    Error,
    Cancelled,
    InternalError,
}

impl FutureType {
    pub fn of(future: &Future) -> Self {
        match future.read() {
            FutureRead::Pending(_) => Self::Pending,
            FutureRead::Ready(_) => Self::Ready,
            FutureRead::Error(_) => Self::Error,
            FutureRead::Cancelled => Self::Cancelled,
            FutureRead::InternalError(_) => Self::InternalError,
        }
    }
}

impl std::fmt::Display for FutureType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FutureType::Any => write!(f, "any"),
            FutureType::Pending => write!(f, "pending"),
            FutureType::Ready => write!(f, "ready"),
            FutureType::Error => write!(f, "error"),
            FutureType::Cancelled => write!(f, "cancelled"),
            FutureType::InternalError => write!(f, "internal_error"),
        }
    }
}

impl From<&Future> for FutureType {
    fn from(value: &Future) -> Self {
        Self::of(value)
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstValue, Type, Value, format_float};

    #[test]
    fn test_format_float() {
        // Whole-number floats must include ".0"
        assert_eq!(format_float(0.0), "0.0");
        assert_eq!(format_float(1.0), "1.0");
        assert_eq!(format_float(-1.0), "-1.0");
        assert_eq!(format_float(100.0), "100.0");
        assert_eq!(format_float(999_999_999_999_999.0), "999999999999999.0");

        // Fractional floats unchanged
        assert_eq!(format_float(2.5), "2.5");
        assert_eq!(format_float(0.1), "0.1");
        assert_eq!(format_float(-0.001), "-0.001");

        // Non-finite values: JS-style names, no ".0"
        assert_eq!(format_float(f64::INFINITY), "Infinity");
        assert_eq!(format_float(f64::NEG_INFINITY), "-Infinity");
        assert_eq!(format_float(f64::NAN), "NaN");
    }

    #[test]
    fn omitted_arg_roundtrip_stays_in_sync() {
        let value = ConstValue::OmittedArg.to_value(|_| unreachable!("no object expected"));

        assert_eq!(value, Value::OmittedArg);
        assert_eq!(
            Type::of(&value, |_| unreachable!("omitted arg is not an object")),
            Type::OmittedArg
        );
        assert_eq!(value.to_string(), "<omitted>");
    }
}
