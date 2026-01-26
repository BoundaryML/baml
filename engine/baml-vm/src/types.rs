use baml_types::{BamlMap, BamlMedia, TypeIR};
use baml_viz_events::RuntimeNodeType;

use crate::{
    bytecode::Bytecode,
    errors::{InternalError, VmError},
    indexable::ObjectIndex,
};

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
    pub span: internal_baml_diagnostics::Span,

    /// Control-flow visualization metadata indexed by viz instructions.
    pub viz_nodes: Vec<VizNodeMeta>,
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
    /// Class index in the [`Vm::objects`] pool.
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

/// Static metadata describing a control-flow visualization node.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct VizNodeMeta {
    /// Stable, pre-order node id matching control_flow.rs ordering.
    pub node_id: u32,
    /// Unique log-filter key within a function (formerly lexical id).
    pub log_filter_key: String,

    /// Parent log-filter key, if any.
    pub parent_log_filter_key: Option<String>,

    /// Logical node type, shared with the viz events crate.
    pub node_type: RuntimeNodeType,

    /// Human-readable label used in the visualization.
    pub label: String,

    /// Markdown header level for header nodes.
    pub header_level: Option<u8>,
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
    /// TODO: Add [`Vm::strings`] interner to avoid allocating duplicates.
    /// In Rust it's not easy to implement because [`crate::vm::Vm::objects`]
    /// owns the strings allocated on heap, but the interner would be something
    /// like HashSet<&str> and it would store pointers to the strings. That
    /// reference will cause some lifetime issues because the VM would have
    /// pointers to itself, so we'd have to figure how to implement it
    /// otherwise.
    String(String),

    /// List of values.
    Array(Vec<Value>),

    /// Map of values.
    Map(BamlMap<String, Value>),

    Future(Future),

    /// Images, audio, pdf, video.
    Media(BamlMedia),

    /// Used for `baml.fetch_as` function.
    BamlType(TypeIR),
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
            Object::Media(_media) => write!(f, "<media>"),
            Object::Future(future) => match future {
                Future::Pending(future) => {
                    write!(f, "<pending: {}>", future.function)
                }
                Future::Ready(value) => write!(f, "<ready: {value}>"),
            },

            Object::BamlType(type_ir) => write!(f, "<baml type: {type_ir}>"),
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
            Object::Media(_) => Self::Media,
            Object::Future(fut) => Self::Future(fut.into()),
            Object::BamlType(_) => Self::Any, // TODO
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
