use std::collections::HashMap;

use indexmap::IndexMap;

use crate::{
    bytecode::Bytecode,
    errors::{InternalError, VmError},
    indexable::{GlobalPool, ObjectIndex, ObjectPool},
};

// ============================================================================
// Type Tags for Jump Table Dispatch
// ============================================================================

/// Global type tag constants for runtime type identification.
///
/// These are used by the `TypeTag` instruction to extract a type identifier
/// from any value for jump table dispatch on union types.
///
/// Primitives have fixed tags (0-9 reserved), classes start at 100.
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
    /// Base value for class type tags (classes start at 100).
    pub const CLASS_BASE: i64 = 100;
    /// Unknown/invalid type tag.
    pub const UNKNOWN: i64 = -1;
}

/// Compiled program ready for execution.
///
/// This is what `baml_codegen` produces. It contains all the objects and globals
/// needed to run a BAML program.
#[derive(Clone, Debug)]
pub struct Program {
    /// Object pool containing functions, classes, strings, etc.
    pub objects: ObjectPool,

    /// Global variables (typically function references).
    pub globals: GlobalPool,

    /// Maps function names to their object indices.
    pub function_indices: HashMap<String, usize>,

    /// Global type tag mapping for classes (assigned during codegen).
    ///
    /// Maps class name -> type tag (`CLASS_BASE` + index).
    /// Used by the `TypeTag` instruction for jump table dispatch on unions.
    pub class_type_tags: HashMap<String, i64>,
}

impl Default for Program {
    fn default() -> Self {
        Self {
            objects: ObjectPool::new(),
            globals: GlobalPool::new(),
            function_indices: HashMap::new(),
            class_type_tags: HashMap::new(),
        }
    }
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

    /// Add a global value.
    pub fn add_global(&mut self, value: Value) {
        self.globals.push(value);
    }

    /// Look up a function's object index by name.
    pub fn function_index(&self, name: &str) -> Option<crate::ObjectIndex> {
        self.function_indices
            .get(name)
            .map(|&idx| crate::ObjectIndex::from_raw(idx))
    }
}

/// Function type.
#[derive(Clone, Copy, Debug)]
pub enum FunctionKind {
    /// Regular executable function.
    ///
    /// The VM pushes a call frame onto the call stack and runs the bytecode.
    Exec,

    /// LLM function.
    ///
    /// The VM will handle control flow to the Baml runtime to produce the
    /// result and then push it on top of the eval stack.
    Llm,

    /// Built-in `baml.fetch_as` function.
    Future,

    /// Builtin functions.
    ///
    /// Contains a Rust function pointer that implements the actual logic.
    Native(crate::native::NativeFunction),
}

/// Represents any Baml function.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Bytecode to execute.
    ///
    /// Only relevant if [`Self::kind`] is [`FunctionKind::Exec`].
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
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Runtime instance representation.
#[derive(Clone, Debug)]
pub struct Instance {
    /// Class index in the [`crate::Vm::objects`] pool.
    pub class: ObjectIndex,

    /// Fields are accessed by index. No string lookups.
    pub fields: Vec<Value>,
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {}>", self.class)
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
    /// Locate the enum.
    pub enm: ObjectIndex,

    /// Index of the variant in the ordered list of variants.
    pub index: usize,
}

impl std::fmt::Display for Variant {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<variant of {}>", self.enm)
    }
}

/// Runtime values.
///
/// This struct should not contain allocated objects and should be [`Copy`].
/// Read the documentation of [`crate::vm::Vm::objects`] to understand how allocated
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

    /// Index into the [`crate::vm::Vm::objects`] vec.
    ///
    /// Strings are also objects, don't add `Value::String`.
    Object(ObjectIndex),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Float(float) => write!(f, "{float}"),
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::Object(object) => write!(f, "{object}"),
        }
    }
}

/// Any data that the Baml program can reference and is allocated on heap.
///
/// [`crate::vm::Vm`] should own objects and give references to them to the running Baml
/// program. Internally, in the [`crate::vm::Vm`] code, note that by reference I don't mean
/// a Rust reference (& or &mut), but rather a [`usize`] that is used to index
/// into the [`crate::vm::Vm::objects`] pool.
///
/// Read [`crate::vm::Vm::objects`] for more information.
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
    /// In Rust it's not easy to implement because [`crate::vm::Vm::objects`]
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
    // Media(BamlMedia),

    // TODO: Figure out how to handle this here.
    // /// Used for `baml.fetch_as` function.
    // BamlType(TypeIR),
}

impl Object {
    /// Helper to unwrap an [`Object::Function`].
    ///
    /// Used to deal with some borrow checker issues in the [`crate::vm::Vm::exec`]
    /// function.
    #[inline]
    pub fn as_function(&self) -> Result<&Function, VmError> {
        match self {
            Object::Function(function) => Ok(function),
            _ => Err(InternalError::TypeError {
                expected: FunctionType::Any.into(),
                got: ObjectType::of(self).into(),
            }
            .into()),
        }
    }

    pub fn as_string(&self) -> Result<&String, InternalError> {
        let Self::String(str) = self else {
            return Err(InternalError::TypeError {
                expected: ObjectType::String.into(),
                got: ObjectType::of(self).into(),
            });
        };

        Ok(str)
    }
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
            Object::Array(array) => write!(f, "{array:?}"),
            Object::Map(map) => write!(f, "{map:?}"),
            // Object::Media(_media) => write!(f, "<media>"),
            Object::Future(future) => match future {
                Future::Pending(future) => {
                    write!(f, "<pending: {}>", future.function)
                }
                Future::Ready(value) => write!(f, "<ready: {value}>"),
            },
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

#[derive(Clone, Debug)]
pub enum FutureKind {
    Llm,
    Net,
}

#[derive(Clone, Debug)]
pub struct PendingFuture {
    pub function: String,
    pub kind: FutureKind,
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
    pub fn of(value: &Value, when_object: impl FnOnce(ObjectIndex) -> ObjectType) -> Self {
        match value {
            Value::Int(_) => Type::Int,
            Value::Float(_) => Type::Float,
            Value::Bool(_) => Type::Bool,
            Value::Object(index) => Type::Object(when_object(*index)),
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
    Media,
    Future(FutureType),
}

impl ObjectType {
    pub fn of(ob: &Object) -> Self {
        match ob {
            Object::Function(func) => Self::Function(func.kind.into()),
            Object::Class(_) => Self::Class,
            Object::Instance(_) => Self::Instance,
            Object::Enum(_) => Self::Enum,
            Object::Variant(_) => Self::Enum,
            Object::String(_) => Self::String,
            Object::Array(_) => Self::Array,
            Object::Map(_) => Self::Map,
            // Object::Media(_) => Self::Media,
            Object::Future(fut) => Self::Future(fut.into()),
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
            ObjectType::Media => write!(f, "media"),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum FunctionType {
    /// Top of function type lattice: represents all function types.
    Any,
    Callable,
    Llm,
}

impl std::fmt::Display for FunctionType {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            FunctionType::Any => write!(f, "any"),
            FunctionType::Callable => write!(f, "callable"),
            FunctionType::Llm => write!(f, "llm"),
        }
    }
}

impl From<FunctionKind> for FunctionType {
    fn from(value: FunctionKind) -> Self {
        if matches!(value, FunctionKind::Llm) {
            FunctionType::Llm
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

#[cfg(test)]
mod tests {
    use super::*;

    // ========================================================================
    // Type Tags Tests
    // ========================================================================

    #[test]
    fn type_tags_are_distinct() {
        // Ensure all primitive type tags are unique
        let tags = [
            type_tags::INT,
            type_tags::STRING,
            type_tags::BOOL,
            type_tags::NULL,
            type_tags::FLOAT,
            type_tags::ENUM,
            type_tags::LIST,
            type_tags::MAP,
            type_tags::FUNCTION,
            type_tags::FUTURE,
        ];

        // Check no duplicates
        for i in 0..tags.len() {
            for j in (i + 1)..tags.len() {
                assert_ne!(
                    tags[i], tags[j],
                    "Type tags at indices {i} and {j} should be different"
                );
            }
        }
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_tags_primitive_range() {
        // Primitives should be in 0-9 range
        assert!(type_tags::INT >= 0 && type_tags::INT < 10);
        assert!(type_tags::STRING >= 0 && type_tags::STRING < 10);
        assert!(type_tags::BOOL >= 0 && type_tags::BOOL < 10);
        assert!(type_tags::NULL >= 0 && type_tags::NULL < 10);
        assert!(type_tags::FLOAT >= 0 && type_tags::FLOAT < 10);
        assert!(type_tags::ENUM >= 0 && type_tags::ENUM < 10);
        assert!(type_tags::LIST >= 0 && type_tags::LIST < 10);
        assert!(type_tags::MAP >= 0 && type_tags::MAP < 10);
        assert!(type_tags::FUNCTION >= 0 && type_tags::FUNCTION < 10);
        assert!(type_tags::FUTURE >= 0 && type_tags::FUTURE < 10);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_tags_class_base_above_primitives() {
        // Class base should be above all primitive tags
        assert!(type_tags::CLASS_BASE > type_tags::INT);
        assert!(type_tags::CLASS_BASE > type_tags::STRING);
        assert!(type_tags::CLASS_BASE > type_tags::BOOL);
        assert!(type_tags::CLASS_BASE > type_tags::NULL);
        assert!(type_tags::CLASS_BASE > type_tags::FLOAT);
        assert!(type_tags::CLASS_BASE > type_tags::ENUM);
        assert!(type_tags::CLASS_BASE > type_tags::LIST);
        assert!(type_tags::CLASS_BASE > type_tags::MAP);
        assert!(type_tags::CLASS_BASE > type_tags::FUNCTION);
        assert!(type_tags::CLASS_BASE > type_tags::FUTURE);

        // Class base should be 100
        assert_eq!(type_tags::CLASS_BASE, 100);
    }

    #[test]
    #[allow(clippy::assertions_on_constants)]
    fn type_tags_unknown_is_negative() {
        // Unknown should be negative to distinguish from valid tags
        assert!(type_tags::UNKNOWN < 0);
        assert_eq!(type_tags::UNKNOWN, -1);
    }

    #[test]
    fn type_tags_expected_values() {
        // Verify the exact values match the documentation
        assert_eq!(type_tags::INT, 0);
        assert_eq!(type_tags::STRING, 1);
        assert_eq!(type_tags::BOOL, 2);
        assert_eq!(type_tags::NULL, 3);
        assert_eq!(type_tags::FLOAT, 4);
        assert_eq!(type_tags::ENUM, 5);
        assert_eq!(type_tags::LIST, 6);
        assert_eq!(type_tags::MAP, 7);
        assert_eq!(type_tags::FUNCTION, 8);
        assert_eq!(type_tags::FUTURE, 9);
    }
}
