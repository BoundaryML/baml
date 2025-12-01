//! Runtime value types for the BAML VM.

use crate::Bytecode;

/// Runtime values.
///
/// This enum represents the values that can exist on the evaluation stack.
/// Heap-allocated objects are referenced by index into an object pool.
#[derive(Clone, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    /// Index into the object pool.
    Object(usize),
}

impl std::fmt::Display for Value {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Value::Null => write!(f, "null"),
            Value::Int(int) => write!(f, "{int}"),
            Value::Float(float) => write!(f, "{float}"),
            Value::Bool(bool) => write!(f, "{bool}"),
            Value::Object(idx) => write!(f, "<object {idx}>"),
        }
    }
}

/// Function type.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FunctionKind {
    /// Regular executable function.
    Exec,
    /// LLM function.
    Llm,
    /// Future-based function (e.g., `baml.fetch_as`).
    Future,
    /// Native builtin function.
    Native,
}

/// Represents a BAML function.
#[derive(Clone, Debug, PartialEq)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Bytecode to execute (only relevant if kind is Exec).
    pub bytecode: Bytecode,

    /// Type of function.
    pub kind: FunctionKind,

    /// Local variable names per scope (debug info).
    pub locals_in_scope: Vec<Vec<String>>,
}

impl Function {
    pub fn new(name: String, arity: usize) -> Self {
        Self {
            name,
            arity,
            bytecode: Bytecode::new(),
            kind: FunctionKind::Exec,
            locals_in_scope: Vec::new(),
        }
    }
}

impl std::fmt::Display for Function {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<fn {}>", self.name)
    }
}

/// Runtime class representation.
#[derive(Clone, Debug, PartialEq)]
pub struct Class {
    /// Class name.
    pub name: String,
    /// Field names (debug info).
    pub field_names: Vec<String>,
}

impl std::fmt::Display for Class {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<class {}>", self.name)
    }
}

/// Runtime enum representation.
#[derive(Clone, Debug, PartialEq)]
pub struct Enum {
    /// Enum name.
    pub name: String,
    /// Variant names.
    pub variant_names: Vec<String>,
}

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name)
    }
}

/// Any data that a BAML program can reference (allocated on heap).
#[derive(Clone, Debug, PartialEq)]
pub enum Object {
    /// Function object.
    Function(Function),
    /// Class object.
    Class(Class),
    /// Class instance object.
    Instance {
        class_index: usize,
        fields: Vec<Value>,
    },
    /// Enum object.
    Enum(Enum),
    /// Enum variant object.
    Variant {
        enum_index: usize,
        variant_index: usize,
    },
    /// Heap allocated string.
    String(String),
    /// List of values.
    Array(Vec<Value>),
    /// Map of string keys to values.
    Map(Vec<(String, Value)>),
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Function(func) => func.fmt(f),
            Object::Class(class) => class.fmt(f),
            Object::Instance { class_index, .. } => write!(f, "<instance of class {class_index}>"),
            Object::Enum(enm) => enm.fmt(f),
            Object::Variant {
                enum_index,
                variant_index,
            } => write!(f, "<variant {variant_index} of enum {enum_index}>"),
            Object::String(s) => write!(f, "\"{s}\""),
            Object::Array(arr) => write!(f, "{arr:?}"),
            Object::Map(map) => write!(f, "{map:?}"),
        }
    }
}

/// A compiled BAML program ready for execution.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Program {
    /// Object pool (functions, classes, instances, strings, etc.).
    pub objects: Vec<Object>,

    /// Globals pool (references to objects).
    pub globals: Vec<Value>,

    /// Map of function names to their object indices.
    pub function_indices: std::collections::HashMap<String, usize>,

    /// Map of class names to their object indices.
    pub class_indices: std::collections::HashMap<String, usize>,

    /// Map of enum names to their object indices.
    pub enum_indices: std::collections::HashMap<String, usize>,
}

impl Program {
    pub fn new() -> Self {
        Self::default()
    }

    /// Add an object to the pool and return its index.
    pub fn add_object(&mut self, obj: Object) -> usize {
        let index = self.objects.len();
        self.objects.push(obj);
        index
    }

    /// Add a global and return its index.
    pub fn add_global(&mut self, value: Value) -> usize {
        let index = self.globals.len();
        self.globals.push(value);
        index
    }
}
