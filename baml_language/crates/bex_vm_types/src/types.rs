//! Heap object types and runtime values.
//!
//! `Future` uses `AtomicU8` + `UnsafeCell<MaybeUninit<Value>>` to allow
//! the engine's spawned task and the VM to read/write the heap object
//! concurrently without a data race. See the `Future` doc for the safety
//! argument; the unsafe code here is intentional and necessary.

#![allow(unsafe_code)]
mod class;
mod const_value;
mod containers;
mod declaration_name;
mod enums;
mod function;
mod future;
mod interface;
mod object;
mod package;
mod type_alias;
mod type_value;
mod value;

use std::collections::HashMap;

use crate::RuntimeTy;
use borsh::{BorshDeserialize, BorshSerialize};
pub use class::*;
pub use const_value::*;
pub use containers::*;
pub use declaration_name::*;
pub use enums::*;
pub use function::*;
pub use future::*;
use indexmap::IndexMap;
pub use interface::*;
pub use object::*;
pub use package::*;
pub use tokio_util::sync::CancellationToken;
pub use type_alias::*;
pub use type_value::*;
pub use value::*;

use crate::{heap_ptr::HeapPtr, indexable::ObjectPool};

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
#[derive(Clone, Debug, Default, BorshSerialize, BorshDeserialize)]
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

    /// Client build metadata for constructing full client trees at runtime.
    /// Keyed by client name.
    pub client_metadata: HashMap<String, ClientBuildMeta>,

    /// Compiled test cases.
    pub test_cases: Vec<TestCase>,

    /// Ordered list of `$init` function names to run at load time.
    /// E.g., `["baml.$init", "$init"]` — builtins before user package.
    /// Empty when there are no top-level let bindings in any package.
    pub package_init_order: Vec<String>,

    /// Per-package program structure (global-index-keyed), sorted by package name
    /// for deterministic output. Holds each package's classes, enums, interfaces,
    /// impl rules, and recursive type aliases. The loader allocates the heap
    /// `Object::Package` / `Object::Interface` / `Object::ImplRule` objects and the
    /// `vm.packages` index from this, resolving each `ObjectIndex` to a
    /// compile-time `HeapPtr` (every slot is pre-allocated, so cross-package
    /// references are order-independent). The single source of truth for interface
    /// dispatch, named-item lookup, and recursive-alias rendering.
    pub packages: IndexMap<baml_type::Name, ProgramPackage>,
}

/// Metadata for building a client tree at runtime.
///
/// Stored on `Program` during compilation, transferred to `SysOpContext` during engine construction.
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
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
#[derive(Debug, Clone, Default, BorshSerialize, BorshDeserialize)]
pub enum ClientBuildType {
    #[default]
    Primitive,
    Fallback,
    RoundRobin,
}

/// Retry policy metadata stored at compile time.
#[derive(Debug, Clone, BorshSerialize, BorshDeserialize)]
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

    /// Flatten every package's recursive type aliases into one
    /// `TypeName → RealizedTy` map (only recursive aliases survive; non-recursive
    /// ones are expanded inline), reconstructing each qualified name from its
    /// package + `LocalName`. The shape output-format rendering consumes.
    ///
    /// Aliases are `Object::TypeAlias` declarations, so this dereferences each
    /// through the object pool rather than reading a side map.
    pub fn recursive_type_aliases(&self) -> IndexMap<baml_type::TypeName, crate::RealizedTy> {
        let mut out = IndexMap::new();
        for (pkg_name, package) in &self.packages {
            for (local, idx) in &package.type_aliases {
                let Some(Object::TypeAlias(alias)) = self.objects.get(idx.raw()) else {
                    // An index that does not resolve to an alias means the pool
                    // and the package map disagree — skip rather than guess.
                    continue;
                };
                let qtn = baml_type::TypeName::new(
                    pkg_name.clone(),
                    local.namespace.clone(),
                    local.name.clone(),
                );
                out.insert(qtn, alias.definition.clone());
            }
        }
        out
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
/// reference. Each [`VmBamlError`](crate::errors::VmBamlError) variant maps
/// to exactly one category via `VmBamlError::category()`. Rich detail stays
/// in `VmBamlError`; this enum is purely for contract enforcement and
/// compiler analysis.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
pub enum SysOpErrorCategory {
    Io,
    Timeout,
    InvalidArgument,
    /// A parser surfaced a structurally-bad input (e.g. UTF-8 decode, JSON
    /// parse, base64 decode). Distinct from `InvalidArgument` so callers
    /// can distinguish "your argument shape was wrong" from "the bytes/
    /// stream we tried to parse are malformed".
    ParseError,
    Unsupported,
    NotImplemented,
    AccessError,
    RenderPrompt,
    LlmClient,
    /// Runtime source compilation was rejected with compiler diagnostics.
    CompilationError,
    /// A live Session already has an evaluation in flight.
    SessionBusy,
    /// Wildcard for development convenience. Must be explicitly declared in
    /// `#[throws(DevOther)]` and should be migrated to named categories.
    DevOther,
    /// A host-language callable raised an exception or invalid-argument error.
    HostCallable,
}

impl std::fmt::Display for SysOpErrorCategory {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Self::Io => write!(f, "Io"),
            Self::Timeout => write!(f, "Timeout"),
            Self::InvalidArgument => write!(f, "InvalidArgument"),
            Self::ParseError => write!(f, "ParseError"),
            Self::Unsupported => write!(f, "Unsupported"),
            Self::NotImplemented => write!(f, "NotImplemented"),
            Self::AccessError => write!(f, "AccessError"),
            Self::RenderPrompt => write!(f, "RenderPrompt"),
            Self::LlmClient => write!(f, "LlmClient"),
            Self::CompilationError => write!(f, "CompilationError"),
            Self::SessionBusy => write!(f, "SessionBusy"),
            Self::DevOther => write!(f, "DevOther"),
            Self::HostCallable => write!(f, "HostCallable"),
        }
    }
}

/// Contract-level panic categories for `sys_op` panic contracts.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, BorshSerialize, BorshDeserialize)]
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

/// Box every unique `ConstValue::Float` reachable from `compile_time_objects`
/// (in `Object::Function` constants) and `globals` into a fresh
/// `Object::Float` entry appended to `compile_time_objects`, and rewrite the
/// function `ConstValue::Float` entries to `ConstValue::Object(idx)`. Returns
/// the bit-pattern → object-index map so callers can rewrite their globals.
///
/// Required because the tagged-pointer `Value` encoding can no longer hold a
/// float inline.
///
/// Globals are *not* rewritten in place — callers typically need to consume
/// them by value (to convert `ConstValue` → `Value`) and rewrite during that
/// pass.
pub fn box_compile_time_floats(
    compile_time_objects: &mut Vec<Object>,
    globals: &[crate::ConstValue],
) -> HashMap<u64, usize> {
    let mut float_indices: HashMap<u64, usize> = HashMap::new();
    // Pre-scan to discover unique floats.
    for obj in &*compile_time_objects {
        if let Object::Function(func) = obj {
            for cv in &func.bytecode.constants {
                if let ConstValue::Float(f) = cv {
                    let next_idx = compile_time_objects.len() + float_indices.len();
                    float_indices.entry(f.to_bits()).or_insert(next_idx);
                }
            }
        }
    }
    for cv in globals {
        if let ConstValue::Float(f) = cv {
            let next_idx = compile_time_objects.len() + float_indices.len();
            float_indices.entry(f.to_bits()).or_insert(next_idx);
        }
    }
    // Append boxes in index order.
    let mut float_entries: Vec<(u64, usize)> =
        float_indices.iter().map(|(k, v)| (*k, *v)).collect();
    float_entries.sort_by_key(|(_, idx)| *idx);
    for (bits, _) in float_entries {
        compile_time_objects.push(Object::Float(f64::from_bits(bits)));
    }
    // Rewrite each function-constant ConstValue::Float -> ConstValue::Object(idx).
    for obj in compile_time_objects.iter_mut() {
        if let Object::Function(func) = obj {
            for cv in &mut func.bytecode.constants {
                if let ConstValue::Float(f) = cv {
                    let idx = float_indices[&f.to_bits()];
                    *cv = ConstValue::Object(crate::indexable::ObjectIndex::from_raw(idx));
                }
            }
        }
    }
    float_indices
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
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub enum TestArgValue {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Array {
        element_type: RuntimeTy,
        items: Vec<TestArgValue>,
    },
    Map {
        key_type: RuntimeTy,
        value_type: RuntimeTy,
        entries: IndexMap<String, TestArgValue>,
    },
}

/// A compiled test case, ready for execution.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct TestCase {
    /// Test name (e.g., "`TestAddOne`").
    pub name: String,
    /// Function names this test targets.
    pub function_names: Vec<String>,
    /// Test arguments, keyed by parameter name.
    pub args: IndexMap<String, TestArgValue>,
    /// Project-root-relative path of the file that *defines* this test block.
    ///
    /// Recorded so `baml test --list` reports the test-defining file
    /// identically whether the program was freshly compiled or served from the
    /// bytecode cache. Empty only for programs compiled before this field
    /// existed.
    pub source_file: String,
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

/// A mutable cell wrapping a single captured value.
///
/// Variables that are closed over are heap-allocated as `Cell` objects so that
/// both the enclosing scope and any closures share the same storage.
#[derive(Clone, Debug, BorshSerialize, BorshDeserialize)]
pub struct Cell {
    pub value: AtomicValueSlot,
}

impl Cell {
    pub fn new(value: Value) -> Self {
        Self {
            value: AtomicValueSlot::new(value),
        }
    }

    #[inline]
    pub fn load(&self) -> Value {
        self.value.load()
    }

    #[inline]
    pub fn store(&self, value: Value) {
        self.value.store(value);
    }
}

/// Types of values.
///
/// Used for checking type errors at runtime. We can probably use some lib
/// that creates this automatically based on the [`Value`] enum.
#[derive(Debug, Clone, Copy, PartialEq, Eq, BorshSerialize, BorshDeserialize)]
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
    ///
    /// Heap-boxed floats are normalised back to the top-level `Type::Float`
    /// so callers comparing `expected` against `got` see the same variant
    /// regardless of which side originated as an inline label vs. a
    /// runtime heap deref.
    pub fn of(value: &Value, when_object: impl FnOnce(HeapPtr) -> ObjectType) -> Self {
        match value.kind() {
            ValueKind::OmittedArg => Type::OmittedArg,
            ValueKind::Int(_) => Type::Int,
            ValueKind::Bool(_) => Type::Bool,
            ValueKind::Object(ptr) => match when_object(ptr) {
                ObjectType::Float => Type::Float,
                other => Type::Object(other),
            },
            // TODO: Actually?
            ValueKind::Null => Type::Object(ObjectType::Any),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{ConstValue, HeapPtr, Instance, Type, Value, format_float};

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

        assert_eq!(value, Value::OMITTED_ARG);
        assert_eq!(
            Type::of(&value, |_| unreachable!("omitted arg is not an object")),
            Type::OmittedArg
        );
        assert_eq!(value.to_string(), "<omitted>");
    }

    #[test]
    fn instance_field_helpers_load_and_store_checked_slots() {
        let instance = Instance::new(
            HeapPtr::null(),
            Box::new([]),
            vec![Value::int(10), Value::int(20)],
        );

        assert_eq!(instance.field_len(), 2);
        assert_eq!(instance.try_load_field(0), Some(Value::int(10)));
        assert_eq!(instance.try_load_field(2), None);
        assert_eq!(instance.load_field(1), Value::int(20));

        assert_eq!(instance.try_store_field(1, Value::int(99)), Ok(()));
        assert_eq!(instance.try_load_field(1), Some(Value::int(99)));
        assert_eq!(instance.try_store_field(2, Value::int(123)), Err(2));
    }

    #[test]
    #[should_panic(expected = "index out of bounds")]
    fn instance_load_field_panics_for_invalid_slot() {
        let instance = Instance::new(HeapPtr::null(), Box::new([]), vec![Value::int(10)]);

        let _ = instance.load_field(1);
    }
}
