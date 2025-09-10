use std::collections::HashMap;

pub(super) mod indexable;

use baml_types::{BamlMap, BamlMedia};
use indexable::{EvalStack, GlobalPool, ObjectIndex, ObjectPool, StackIndex};

use crate::{
    bytecode::{BinOp, Bytecode, CmpOp, Instruction},
    UnaryOp,
};

/// Max call stack size.
const MAX_FRAMES: usize = 256;

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

    /// Builtin or OS interfacing function.
    ///
    /// For OS interfacing, VM will handle control flow to a Rust wrapper that
    /// calls into the OS and returns a result. Needed for features like
    /// `fetch`.
    ///
    /// Builtin functions like `len` work the same way.
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

impl std::fmt::Display for Enum {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<enum {}>", self.name)
    }
}

/// Any data that the Baml program can reference and is allocated on heap.
///
/// [`Vm`] should own objects and give references to them to the running Baml
/// program. Internally, in the [`Vm`] code, note that by reference I don't mean
/// a Rust reference (& or &mut), but rather a [`usize`] that is used to index
/// into the [`Vm::objects`] pool.
///
/// Read [`Vm::objects`] for more information.
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
    /// In Rust it's not easy to implement because [`Vm::objects`] owns the
    /// strings allocated on heap, but the interner would be something like
    /// HashSet<&str> and it would store pointers to the strings. That reference
    /// will cause some lifetime issues because the VM would have pointers to
    /// itself, so we'd have to figure how to implement it otherwise.
    String(String),

    /// List of values.
    Array(Vec<Value>),

    /// Map of values.
    Map(BamlMap<String, Value>),

    Future(Future),

    /// Images, audio, pdf, video.
    Media(BamlMedia),
}

#[derive(Clone, Debug)]
pub struct LlmFuture {
    pub llm_function: String,
    pub args: Vec<Value>,
}

#[derive(Clone, Debug)]
pub enum Future {
    /// Pending future.
    ///
    /// Only LLM calls for now.
    Pending(LlmFuture),

    /// Ready value for the future.
    Ready(Value),
}

impl Object {
    /// Helper to unwrap an [`Object::Function`].
    ///
    /// Used to deal with some borrow checker issues in the [`Vm::exec`]
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
                Future::Pending(llm_future) => write!(f, "<pending: {}>", llm_future.llm_function),
                Future::Ready(value) => write!(f, "<ready: {value}>"),
            },
        }
    }
}

/// Runtime values.
///
/// This struct should not contain allocated objects and should be [`Copy`].
/// Read the documentation of [`Vm::objects`] to understand how allocated
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

    /// Index into the [`Vm::objects`] vec.
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

impl From<FutureType> for ObjectType {
    fn from(value: FutureType) -> Self {
        ObjectType::Future(value)
    }
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

impl From<FunctionType> for ObjectType {
    fn from(value: FunctionType) -> Self {
        ObjectType::Function(value)
    }
}

impl<Ob> From<Ob> for Type
where
    Ob: Into<ObjectType>,
{
    fn from(value: Ob) -> Self {
        Type::Object(value.into())
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

/// Bug in the VM or somehow invalid source code got compiled and executed.
///
/// If the VM throws this it's either a bug in the compiler or in the VM itself.
#[derive(Debug, PartialEq)]
pub enum InternalError {
    /// The number of arguments passed to a function doesn't match the function
    /// arity.
    InvalidArgumentCount { expected: usize, got: usize },

    /// Attempt to access the top of the stack but it's empty.
    UnexpectedEmptyStack,

    /// Attempt to access a stack slot from the top of the stack,
    /// and stack doesn't have enough items.
    /// Argument is the amount of slots from the top of the stack (inclusive - 0 is top itself)
    /// that were queried.
    NotEnoughItemsOnStack(usize),

    /// Reference an object that does not exist in the object pool.
    /// Argument is the reference index.
    InvalidObjectRef(usize),

    /// Attempt to use a value but it's not the expected type.
    TypeError { expected: Type, got: Type },

    /// Attempt to apply a binary operation to two values of different types.
    CannotApplyBinOp { left: Type, right: Type, op: BinOp },

    /// Attempt to apply a comparison operation to two values of different types.
    CannotApplyCmpOp { left: Type, right: Type, op: CmpOp },

    /// Attempt to apply a unary operation to a value of the wrong type.
    CannotApplyUnaryOp { op: UnaryOp, value: Type },

    /// Array index out of bounds.
    ArrayIndexOutOfBounds { index: usize, length: usize },

    /// Array index is negative.
    ArrayIndexIsNegative(i64),

    /// Instruction pointer is negative.
    NegativeInstructionPtr(isize),
}

/// Errors that can happen at runtime.
///
/// Either logic errors in the user's source code or bugs in our compiler/VM
/// stack.
#[derive(Debug, PartialEq)]
pub enum RuntimeError {
    /// Ah yes, classic stack overflow.
    StackOverflow,

    /// User code triggered an assertion failure via the [`Instruction::Assert`] opcode.
    AssertionError,

    /// VM internal error.
    InternalError(InternalError),

    /// Map does not contain the requested key.
    NoSuchKeyInMap,

    /// Right hand side of division operation is zero.
    DivisionByZero { left: Value, right: Value },

    /// Any error, provide a custom message for this one.
    Other(String),
}

/// Any kind of virtual machine error.
#[derive(Debug, PartialEq)]
pub enum VmError {
    RuntimeError(RuntimeError),
}

impl From<InternalError> for VmError {
    fn from(error: InternalError) -> Self {
        VmError::RuntimeError(RuntimeError::InternalError(error))
    }
}

impl From<RuntimeError> for VmError {
    fn from(error: RuntimeError) -> Self {
        VmError::RuntimeError(error)
    }
}

impl std::fmt::Display for VmError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        // Figure out something nicer here.
        match self {
            VmError::RuntimeError(error) => write!(f, "{error:?}"),
        }
    }
}

impl std::fmt::Display for InternalError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "Internal VM Erorr: ")?;

        match self {
            InternalError::InvalidArgumentCount { expected, got } => {
                write!(
                    f,
                    "invalid argument count: expected {expected} arguments, got {got}"
                )
            }
            InternalError::UnexpectedEmptyStack => write!(f, "unexpected empty eval stack"),
            InternalError::NotEnoughItemsOnStack(count) => {
                write!(f, "not enough items on stack: {count}")
            }
            InternalError::InvalidObjectRef(index) => {
                write!(f, "invalid object reference: {index}")
            }
            InternalError::TypeError { expected, got } => {
                write!(f, "type error: expected {expected}, got {got}")
            }
            InternalError::CannotApplyBinOp { left, right, op } => {
                write!(f, "cannot apply binary operation: {left} {op} {right}")
            }
            InternalError::CannotApplyCmpOp { left, right, op } => {
                write!(f, "cannot apply comparison operation: {left} {op} {right}")
            }
            InternalError::CannotApplyUnaryOp { op, value } => {
                write!(f, "cannot apply unary operation: {op} {value}")
            }
            InternalError::ArrayIndexOutOfBounds { index, length } => {
                write!(f, "array index out of bounds: {index} of {length}")
            }
            InternalError::ArrayIndexIsNegative(index) => {
                write!(f, "array index is negative: {index}")
            }
            InternalError::NegativeInstructionPtr(ptr) => {
                write!(f, "negative instruction pointer: {ptr}")
            }
        }
    }
}

impl std::error::Error for VmError {}

impl std::error::Error for InternalError {}

/// Call frame.
///
/// This is what gets pushed onto the call stack every time we call a function.
///
/// As with [`Value`], this struct should not own allocated objects (like
/// functions) but instead use references to index into [`Vm::objects`]. Should
/// be [`Copy`].
#[derive(Clone, Copy, Debug)]
pub struct Frame {
    /// The running function.
    pub function: ObjectIndex,

    /// Instruction pointer (IP) or program counter (PC).
    ///
    /// Points to the next instruction that the VM will execute. It is of type
    /// [`isize`] because some jumps can create negative offsets (for loops)
    /// and it's easier to operate on an [`isize`] and cast it to [`usize`]
    /// only once (when we index into [`Bytecode::instructions`]). However,
    /// this number should never be negative, otherwise indexing into the
    /// instruction vec will throw [`InternalError::NegativeInstructionPtr`].
    pub instruction_ptr: isize,

    /// Local variables offset in the eval stack.
    pub locals_offset: StackIndex,
}

/// The beast.
///
/// This is a stack based virtual machine. Stack based machines work by pushing
/// and popping values from an "evaluation stack". Picture this example from
/// [Crafting Interpreters](https://craftinginterpreters.com/a-virtual-machine.html):
///
/// ```ignore
/// fn echo(n) {
///     print(n)
///     return n
/// }
///
/// print(echo(echo(1) + echo(2)) + echo(echo(4) + echo(5)))
/// ```
///
/// Output should be:
///
/// ```text
/// 1
/// 2
/// 3
/// 4
/// 5
/// 9
/// 12
/// ```
///
/// The code above would create an AST similar to this:
///
/// ```text
///                 +-------+
///                 | print |
///                 +-------+
///                     |
///                   +---+
///          +--------| + |--------+
///          |        +---+        |
///      +------+               +------+
///      | echo |               | echo |
///      +------+               +------+
///          |                     |
///        +---+                 +---+
///        | + |                 | + |
///        +---+                 +---+
///          |                     |
///     +---------+           +----------+
///     |         |           |          |
/// +------+   +------+   +------+   +------+
/// | echo |   | echo |   | echo |   | echo |
/// +------+   +------+   +------+   +------+
///     |         |           |          |
///   +---+     +---+       +---+      +---+
///   | 1 |     | 2 |       | 4 |      | 5 |
///   +---+     +---+       +---+      +---+
/// ```
///
/// If we "flatten" the AST considering the "lifetime" of each value, we get
/// this structure:
///
/// ```text
///                   +---+
/// constant 1 ...... | 1 |
/// echo(1) ......... |   |---+
/// constant 2 ...... |   | 2 |
/// echo(2) ......... |   |   |
///                   +---+---+
/// add 1+2 ......... | 3 |
/// echo(3) ......... |   |---+
/// constant 4 ...... |   | 4 |
/// echo(4) ......... |   |   |---+
/// constant 5 ...... |   |   | 5 |
/// echo(5) ......... |   |   |   |
///                   |   |---+---+
/// add 4+5 ......... |   | 9 |
/// echo(9) ......... |   |   |
///                   +---+---+
/// add 3+9 ......... |12 |
/// print(12) ....... |   |
///                   +---+
/// ```
///
/// Looks like a stack doesn't it? That's the evaluation stack. All values in
/// the program flow through that stack, eliminating the need for instructions
/// with registers. Instead of `ADD r2, r0, r1` we just have `ADD`, which pops
/// two values from the stack, produces the result and pushes it back on top.
/// Simple, right? The drawback is that we need to execute more instructions to
/// achieve the same result as a register based VM. If we want to add two
/// variables, a register VM would run a single instruction:
///
/// ```text
/// ADD r2, r0, r1  // Add the contents of r0 and r1 and store the result in r2
///                 // r2 = r0 + r1
/// ```
///
/// Meanwhile a stack VM would run 4 instructions:
///
/// ```text
/// LOAD_VAR 0   // Push the contents of variable 0 on top of the stack
/// LOAD_VAR 1   // Push the contents of variable 1 on top of the stack
/// ADD          // Pop two values, add and push the result on top of the stack
/// STORE_VAR 2  // Store the top of the stack in variable 2
/// ```
///
/// Basically it's slower because it needs more cycles to do the same thing.
/// Other than that, pretty much everything is better in a stack VM, especially
/// simplicity (we don't even need to figure out which registers to use and when
/// to use them).
pub struct Vm {
    /// Call stack.
    ///
    /// On each function call we create a new [`Frame`] and push it on this
    /// stack. On each return, we destroy the frame and pop it from the stack
    /// to resume the execution of the previous frame.
    pub frames: Vec<Frame>,

    /// Evaluation stack.
    ///
    /// This stack only stores values.
    pub stack: EvalStack,

    /// Object pool.
    ///
    /// For now, since we don't have a garbage collector yet, this is basically
    /// an arena of objects. **Every object** is allocated here and will be
    /// destroyed when the lifetime of the [`Vm`] ends. Do not allocate objects
    /// elsewhere since that will make adding a garbage collector harder.
    /// Only allocate objects here and use indices to reference them, don't
    /// bother with Rust references because they will introduce lifetime issues.
    pub objects: ObjectPool,

    /// Global variables.
    ///
    /// This stores the functions and globally declared variables.
    pub globals: GlobalPool,

    /// Offset of the first runtime allocated object.
    ///
    /// This is used to track the index of the first runtime allocated object.
    /// When the embedder calls [`Vm::collect_garbage`] it will drop all values
    /// after this offset.
    pub runtime_allocs_offset: ObjectIndex,
}

/// VM execution state.
///
/// The virtual machine cannot deal with futures, so when when it stumbles upon
/// future creation instructions, it returns control flow to the embedder,
/// expecting the embedder to schedule the future and yield back the control
/// flow to the VM.
///
/// Similarly, when the VM encounters an await point, it returns control flow to
/// the embedder, expecting the embedder to await the future and fulfil it with
/// the final result before yielding back control flow to the VM.
#[derive(Debug, PartialEq)]
pub enum VmExecState {
    /// VM cannot proceed. It is awaiting a pending future to complete.
    Await(ObjectIndex),

    /// VM notifies caller about a future that needs to be scheduled.
    ///
    /// Bytecode execution continues when control flow is handled back to the
    /// VM.
    ScheduleFuture(ObjectIndex),

    /// VM has completed the execution of all available bytecode.
    Complete(Value),
}

#[derive(Clone, Debug)]
pub struct BamlVmProgram {
    pub objects: ObjectPool,
    pub globals: GlobalPool,
    pub resolved_function_names: HashMap<String, (ObjectIndex, FunctionKind)>,
    pub resolved_class_names: HashMap<String, ObjectIndex>,
    pub resolved_enums_names: HashMap<String, ObjectIndex>,
}

impl Vm {
    pub fn new(
        BamlVmProgram {
            objects, globals, ..
        }: BamlVmProgram,
    ) -> Self {
        Self {
            frames: Vec::new(),
            stack: EvalStack(Vec::new()),
            runtime_allocs_offset: ObjectIndex(objects.len()),
            objects,
            globals,
        }
    }

    /// Bootstraps the VM preparing the given function to run.
    pub fn set_entry_point(&mut self, function: ObjectIndex, args: &[Value]) {
        debug_assert!(
            matches!(self.objects[function], Object::Function(_)),
            "expect function as entry point, got {:?}",
            self.objects[function]
        );

        // TODO: Run collect_garbage in codegen after each function call.
        if self.objects.len() != self.runtime_allocs_offset.0 {
            eprintln!("WARNING: garbage collection did not run before setting a new entry point");
        }

        self.stack.push(Value::Object(function));
        self.stack.extend(args.iter().copied());

        self.frames.push(Frame {
            function,
            instruction_ptr: 0,
            locals_offset: StackIndex(0),
        });
    }

    /// Restores the VM state and prepares it for the next execution.
    ///
    /// This is used to clear the stack and frames after execution.
    pub fn finalize(&mut self) {
        // If the VM returns correctly with VmExecState::Complete, the eval
        // stack and call stack should be empty.
        self.stack.clear();
        self.frames.clear();
        self.collect_garbage();
    }

    /// Returns a reference to the pending future.
    ///
    /// Returns [`InternalError::TypeError`] if the future is not pending, or not a future.
    pub fn pending_future(&self, future: ObjectIndex) -> Result<&LlmFuture, InternalError> {
        match &self.objects[future] {
            Object::Future(Future::Pending(llm_future)) => Ok(llm_future),
            other => Err(InternalError::TypeError {
                expected: FutureType::Pending.into(),
                got: ObjectType::of(other).into(),
            }),
        }
    }

    pub fn fulfil_future(
        &mut self,
        future_index: ObjectIndex,
        value: Value,
    ) -> Result<(), InternalError> {
        let Object::Future(future) = &mut self.objects[future_index] else {
            return Err(InternalError::TypeError {
                expected: FutureType::Any.into(),
                got: ObjectType::of(&self.objects[future_index]).into(),
            });
        };

        *future = Future::Ready(value);

        // At any given moment, the VM can only await a single future, because
        // we can only call the AWAIT instruction on a future on top of the
        // stack. If that future being await is fulfilled, we need to replace
        // the future on the stack with the ready value so that the next
        // instruction that the VM runs can use the value, not the future
        // object.
        if let Some(Value::Object(index)) = self.stack.last() {
            if *index == future_index {
                self.stack.pop();
                self.stack.push(value);
            }
        }

        Ok(())
    }

    /// Keeps only compile time necessary objects.
    ///
    /// Everything allocated while the program run is dropped.
    pub fn collect_garbage(&mut self) {
        self.objects.drain(self.runtime_allocs_offset..);
    }

    /// Allocates an array on the heap and returns it to the caller.
    pub fn alloc_array(&mut self, values: Vec<Value>) -> Value {
        Value::Object(self.objects.insert(Object::Array(values)))
    }

    pub fn alloc_map(&mut self, values: BamlMap<String, Value>) -> Value {
        Value::Object(self.objects.insert(Object::Map(values)))
    }

    pub fn alloc_string(&mut self, s: String) -> Value {
        Value::Object(self.objects.insert(Object::String(s)))
    }

    /// TODO: Seems to low level for an embedder, provide an API that takes
    /// class name and mapping of field name => value instead.
    pub fn alloc_instance(&mut self, class: ObjectIndex, fields: Vec<Value>) -> Value {
        Value::Object(
            self.objects
                .insert(Object::Instance(Instance { class, fields })),
        )
    }

    // TODO: Same problem as above. Ideally takes (&str, &str) instead.
    pub fn alloc_variant(&mut self, enm: ObjectIndex, index: usize) -> Value {
        Value::Object(self.objects.insert(Object::Variant(Variant { enm, index })))
    }

    pub fn alloc_media(&mut self, media: BamlMedia) -> Value {
        Value::Object(self.objects.insert(Object::Media(media)))
    }

    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    pub fn exec(&mut self) -> Result<VmExecState, VmError> {
        // Grab the last frame from the call stack.
        //
        // Note that [`Frame`] is [`Copy`], so in case the borrow checker
        // complains too much and you can't circumvent it then you can make a
        // local copy of the frame, modify it as needed, and then when we're
        // done with this frame store it back in the vector to persist changes.
        // It's a similar trick to what we've implemented in the cycle detection
        // algorithm. Take a look at the `strong_connect` function in the
        // `tarjan.rs` file.
        let Some(mut frame) = self.frames.last_mut() else {
            // This should actually return "Void" or () like Rust.
            return Ok(VmExecState::Complete(Value::Null));
        };

        // Grab a reference to the function object. We do this before the loop
        // because there's no need to run this on every single iteration. Read
        // the implementations of `Instruction::Call` and `Instruction::Return`
        // below.
        //
        // We do run into some issues/boilerplate, take a look at the impl of
        // `Instruction::AllocArray`. We can write a macro or something.
        let mut function = self.objects[frame.function].as_function()?;

        loop {
            // Current instruction pointer.
            let instruction_ptr = frame.instruction_ptr;

            // NOTE: `core::intrinsics::unlikely` is only available on nightly.
            // This branch is a big annoyance for small functions (like pushing the frame)
            // and gets smaller the bigger the function due to branch (mis)prediction.
            if instruction_ptr < 0 {
                return Err(InternalError::NegativeInstructionPtr(instruction_ptr).into());
            }

            // Move the frame's IP to the next instruction. We'll deal with
            // jump offsets later.
            frame.instruction_ptr += 1;

            // Runtime debugging information.
            // #[cfg(debug_assertions)]
            // {
            //     let stack = self
            //         .stack
            //         .iter()
            //         .map(|v| crate::debug::display_value(v, &self.objects))
            //         .collect::<Vec<_>>()
            //         .join(", ");
            //
            //     eprintln!("[{stack}]");
            //
            //     let (instruction, metadata) = crate::debug::display_instruction(
            //         instruction_ptr,
            //         function,
            //         &self.stack,
            //         &self.objects,
            //         &self.globals,
            //     );
            //
            //     eprintln!("{instruction} {metadata}");
            // }

            match function.bytecode.instructions[instruction_ptr as usize] {
                Instruction::LoadConst(index) => {
                    let value = &function.bytecode.constants[index];
                    self.stack.push(*value);
                }

                Instruction::LoadVar(index) => {
                    let value = self.stack[frame.locals_offset + index];
                    self.stack.push(value);
                }

                Instruction::StoreVar(index) => {
                    // Consume the value. There are some intricacies when it
                    // comes to consuming the value or not, mainly, should this
                    // work?
                    //
                    // let a = 1;
                    // let b = (a = 2);
                    //
                    // If yes, then we should not consume the value and emit
                    // a pop instruction after each semicolon.
                    let value = self.stack.ensure_pop()?;

                    self.stack[frame.locals_offset + index] = value;
                }

                Instruction::LoadGlobal(index) => {
                    let value = &self.globals[index];
                    self.stack.push(*value);
                }

                Instruction::StoreGlobal(index) => {
                    // Consume the value. Read impl of Instruction::StoreVar.
                    let value = self.stack.ensure_pop()?;

                    self.globals[index] = value;
                }

                Instruction::LoadField(index) => {
                    let top = self.stack.ensure_pop()?;

                    let reference = self.objects.as_object(&top, ObjectType::Instance)?;

                    let Object::Instance(instance) = &self.objects[reference] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(&self.objects[reference]).into(),
                        }
                        .into());
                    };

                    // Push the value on top of the stack.
                    self.stack.push(instance.fields[index]);
                }

                Instruction::StoreField(index) => {
                    let reference = self.objects.as_object(
                        &self.stack[self.stack.ensure_slot_from_top(1)?],
                        ObjectType::Instance,
                    )?;

                    let Object::Instance(instance) = &mut self.objects[reference] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Instance.into(),
                            got: ObjectType::of(&self.objects[reference]).into(),
                        }
                        .into());
                    };

                    // Consume and set the value.
                    instance.fields[index] = self.stack.ensure_pop()?;

                    // Consume the intance.
                    self.stack.ensure_pop()?;

                    // TODO: Borrow checker stuff.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::Pop(n) => {
                    let drain_range = StackIndex(self.stack.len() - n)..;
                    self.stack.drain(drain_range);
                }

                Instruction::Copy(offset) => {
                    let index = self.stack.ensure_slot_from_top(offset)?;
                    let value = self.stack[index];
                    self.stack.push(value);
                }

                Instruction::PopReplace(n) => {
                    let value = self.stack.ensure_pop()?;

                    // Pop the last `n` locals from the stack.
                    let drain_range = StackIndex(self.stack.len() - n)..;
                    self.stack.drain(drain_range);

                    // Push the value back on top of the stack.
                    self.stack.push(value);
                }

                Instruction::Jump(offset) => {
                    // Reassign the frame's IP to the new instruction.
                    // Remember that offset can be negative here, so even though
                    // we're adding it can still jump backwards.
                    frame.instruction_ptr = instruction_ptr + offset;
                }

                Instruction::JumpIfFalse(offset) => {
                    match &self.stack[self.stack.ensure_stack_top()?] {
                        // Reassign only if the top of the stack is false.
                        Value::Bool(value) => {
                            if !value {
                                frame.instruction_ptr = instruction_ptr + offset;
                            }
                        }

                        // Type error, we don't have "falsey" values in the language
                        // so we should always check booleans.
                        other => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Bool,
                                got: self.objects.type_of(other),
                            }))
                        }
                    }
                }

                Instruction::BinOp(op) => {
                    let right = self.stack.ensure_pop()?;
                    let left = self.stack.ensure_pop()?;

                    let result = match (left, right) {
                        (Value::Int(left), Value::Int(right)) => Value::Int(match op {
                            BinOp::Div if right == 0 => {
                                return Err(RuntimeError::DivisionByZero {
                                    left: Value::Int(left),
                                    right: Value::Int(right),
                                }
                                .into());
                            }

                            BinOp::Add => left + right,
                            BinOp::Sub => left - right,
                            BinOp::Mul => left * right,
                            BinOp::Div => left / right,
                            BinOp::Mod => left % right,

                            BinOp::BitAnd => left & right,
                            BinOp::BitOr => left | right,
                            BinOp::BitXor => left ^ right,
                            BinOp::Shl => left << right,
                            BinOp::Shr => left >> right,
                        }),

                        (Value::Float(left), Value::Float(right)) => {
                            Value::Float(match op {
                                BinOp::Div if right == 0.0 => {
                                    return Err(RuntimeError::DivisionByZero {
                                        left: Value::Float(left),
                                        right: Value::Float(right),
                                    }
                                    .into());
                                }

                                BinOp::Add => left + right,
                                BinOp::Sub => left - right,
                                BinOp::Mul => left * right,
                                BinOp::Div => left / right,
                                BinOp::Mod => left % right,

                                // Bitwise ops not applicable to floats.
                                BinOp::BitAnd
                                | BinOp::BitOr
                                | BinOp::BitXor
                                | BinOp::Shl
                                | BinOp::Shr => {
                                    return Err(VmError::from(InternalError::CannotApplyBinOp {
                                        left: Type::Float,
                                        right: Type::Float,
                                        op,
                                    }));
                                }
                            })
                        }

                        (Value::Object(_), Value::Object(_)) if op == BinOp::Add => {
                            let left = self.objects.as_string(&left)?;
                            let right = self.objects.as_string(&right)?;

                            let mut concat = left.clone();
                            concat.push_str(right);

                            let concat_str_object =
                                Value::Object(self.objects.insert(Object::String(concat)));

                            // Borrow check.
                            function = self.objects[frame.function].as_function()?;

                            concat_str_object
                        }

                        _ => {
                            return Err(VmError::from(InternalError::CannotApplyBinOp {
                                left: self.objects.type_of(&left),
                                right: self.objects.type_of(&right),
                                op,
                            }));
                        }
                    };

                    self.stack.push(result);
                }

                Instruction::CmpOp(op) => {
                    let right = self.stack.ensure_pop()?;
                    let left = self.stack.ensure_pop()?;

                    let result = match (left, right) {
                        (Value::Int(left), Value::Int(right)) => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,

                            CmpOp::InstanceOf => {
                                return Err(InternalError::CannotApplyCmpOp {
                                    left: Type::Int,
                                    right: Type::Int,
                                    op,
                                }
                                .into())
                            }
                        }),

                        (Value::Float(left), Value::Float(right)) => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,
                            CmpOp::Lt => left < right,
                            CmpOp::LtEq => left <= right,
                            CmpOp::Gt => left > right,
                            CmpOp::GtEq => left >= right,

                            CmpOp::InstanceOf => {
                                return Err(InternalError::CannotApplyCmpOp {
                                    left: Type::Float,
                                    right: Type::Float,
                                    op,
                                }
                                .into())
                            }
                        }),

                        _ => Value::Bool(match op {
                            CmpOp::Eq => left == right,
                            CmpOp::NotEq => left != right,

                            CmpOp::InstanceOf => {
                                let left = self.objects.as_object(&left, ObjectType::Instance)?;

                                let Object::Instance(instance) = &self.objects[left] else {
                                    return Err(InternalError::TypeError {
                                        expected: ObjectType::Instance.into(),
                                        got: ObjectType::of(&self.objects[left]).into(),
                                    }
                                    .into());
                                };

                                let right = self.objects.as_object(&right, ObjectType::Class)?;

                                instance.class == right
                            }

                            _ => {
                                return Err(VmError::from(InternalError::CannotApplyCmpOp {
                                    left: self.objects.type_of(&left),
                                    right: self.objects.type_of(&right),
                                    op,
                                }))
                            }
                        }),
                    };

                    self.stack.push(result);
                }

                Instruction::UnaryOp(op) => {
                    let value = self.stack.ensure_pop()?;

                    let result = match (op, value) {
                        (UnaryOp::Not, Value::Bool(value)) => Value::Bool(!value),
                        (UnaryOp::Neg, Value::Int(value)) => Value::Int(-value),
                        (UnaryOp::Neg, Value::Float(value)) => Value::Float(-value),
                        _ => {
                            return Err(VmError::from(InternalError::CannotApplyUnaryOp {
                                op,
                                value: self.objects.type_of(&value),
                            }));
                        }
                    };

                    self.stack.push(result);
                }

                Instruction::AllocArray(size) => {
                    // Pop all the elements from the stack and create an array.
                    let drain_range = StackIndex(self.stack.len() - size)..;
                    let array = self.stack.drain(drain_range).collect();

                    // Allocate it on the heap.
                    self.objects.push(Object::Array(array));

                    // Push the array object on top of the stack.
                    self.stack
                        .push(Value::Object(ObjectIndex(self.objects.len() - 1)));

                    // objects.push() above might've reallocated the vector so
                    // borrow checker complains. Restore the reference.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::LoadArrayElement => {
                    // Stack should contain [array, index]
                    // Pop the index first, then the array
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;

                    let array_ob_index = self.objects.as_object(&array_value, ObjectType::Array)?;

                    let Object::Array(array) = &self.objects[array_ob_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(&self.objects[array_ob_index]).into(),
                        }));
                    };

                    // Get the index
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        _ => {
                            return Err(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.objects.type_of(&index_value),
                            }
                            .into());
                        }
                    };

                    // Check bounds
                    if index >= array.len() {
                        return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                            index,
                            length: array.len(),
                        }));
                    }

                    // Push the element onto the stack
                    self.stack.push(array[index]);
                }

                Instruction::LoadMapElement => {
                    // LoadMapElement Instruction
                    //
                    // Stack before: [map, key]
                    // Stack after: [value]
                    //
                    // Interpretation steps:
                    // 1. Pop key from stack (top element)
                    // 2. Pop map reference from stack (bottom element)
                    // 3. Validate that the popped map reference is indeed a map object
                    // 4. Get the key as a string from the objects pool (maps use string keys)
                    //    - Validate key_value is an object reference to a String
                    //    - Get the string reference from the objects pool
                    // 5. Look up the value at map[key]
                    // 6. Handle the case where key doesn't exist in the map
                    //    - Return a runtime error NoSuchKeyInMap if key not found
                    // 7. Push the found value onto the stack

                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;

                    let map_index = self.objects.as_object(&map_value, ObjectType::Map)?;

                    let Object::Map(map) = &self.objects[map_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(&self.objects[map_index]).into(),
                        }));
                    };

                    // Get the string key from the objects pool
                    let key_index = self.objects.as_object(&key_value, ObjectType::String)?;
                    let key = self.objects[key_index].as_string()?;

                    // Look up the value in the map
                    let value = map.get(key).copied().ok_or(RuntimeError::NoSuchKeyInMap)?;

                    // Push the value onto the stack
                    self.stack.push(value);
                }

                Instruction::StoreArrayElement => {
                    // StoreArrayElement Instruction
                    //
                    // Stack before: [array, index, value]
                    // Stack after: []
                    //
                    // Interpretation steps:
                    // 1. Pop value from stack (top element)
                    // 2. Pop index from stack (next element)
                    // 3. Pop array reference from stack (bottom element)
                    // 4. Validate that the popped array reference is indeed an array object
                    // 5. Validate that index is an integer
                    // 6. Check if index is non-negative
                    // 7. Check if index is within array bounds
                    // 8. Store the value at array[index]
                    // 9. No value is pushed back to stack (mutation in place)

                    let value = self.stack.ensure_pop()?;
                    let index_value = self.stack.ensure_pop()?;
                    let array_value = self.stack.ensure_pop()?;

                    let array_ob_index = self.objects.as_object(&array_value, ObjectType::Array)?;

                    let Object::Array(array) = &mut self.objects[array_ob_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Array.into(),
                            got: ObjectType::of(&self.objects[array_ob_index]).into(),
                        }));
                    };

                    // Get the index
                    let index = match index_value {
                        Value::Int(i) => {
                            if i < 0 {
                                return Err(InternalError::ArrayIndexIsNegative(i).into());
                            }
                            i as usize
                        }
                        _ => {
                            return Err(InternalError::TypeError {
                                expected: Type::Int,
                                got: self.objects.type_of(&index_value),
                            }
                            .into());
                        }
                    };

                    // Check bounds
                    if index >= array.len() {
                        return Err(VmError::from(InternalError::ArrayIndexOutOfBounds {
                            index,
                            length: array.len(),
                        }));
                    }

                    // Store the value at the index
                    array[index] = value;

                    // Restore function reference after mutable borrow of self.objects
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::StoreMapElement => {
                    // StoreMapElement Instruction
                    //
                    // Stack before: [map, key, value]
                    // Stack after: []
                    //
                    // Interpretation steps:
                    // 1. Pop value from stack (top element)
                    // 2. Pop key from stack (next element)
                    // 3. Pop map reference from stack (bottom element)
                    // 4. Validate that the popped map reference is indeed a map object
                    // 5. Get the key as a string from the objects pool (maps use string keys)
                    //    - Validate key_value is an object reference to a String
                    //    - Clone the string from the objects pool
                    // 6. Store/update the value at map[key]
                    // 7. No value is pushed back to stack (mutation in place)

                    let value = self.stack.ensure_pop()?;
                    let key_value = self.stack.ensure_pop()?;
                    let map_value = self.stack.ensure_pop()?;

                    // Get the string key from the objects pool.
                    let key_index = self.objects.as_object(&key_value, ObjectType::String)?;
                    let key = self.objects[key_index].as_string()?.clone();

                    let map_index = self.objects.as_object(&map_value, ObjectType::Map)?;

                    let Object::Map(map) = &mut self.objects[map_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: ObjectType::Map.into(),
                            got: ObjectType::of(&self.objects[map_index]).into(),
                        }));
                    };

                    // Store the value at the key
                    map.insert(key, value);

                    // borrow check
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::AllocInstance(index) => {
                    let Object::Class(class) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Class.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Allocate the fields.
                    let mut fields = Vec::with_capacity(class.field_names.len());
                    fields.resize(class.field_names.len(), Value::Null);

                    // Allocate an instance of the class.
                    self.objects.push(Object::Instance(Instance {
                        class: index,
                        fields,
                    }));

                    // Push the instance object on top of the stack.
                    self.stack
                        .push(Value::Object(ObjectIndex(self.objects.len() - 1)));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }

                // TODO: Contains a lot of typechecking, we know at compile time
                // that all this stuff is right. Should do something about it.
                Instruction::AllocVariant(enum_index) => {
                    let Object::Enum(enm) = &self.objects[enum_index] else {
                        return Err(InternalError::TypeError {
                            expected: ObjectType::Enum.into(),
                            got: ObjectType::of(&self.objects[enum_index]).into(),
                        }
                        .into());
                    };

                    let variant = self.stack.ensure_pop()?;

                    let Value::Int(variant_index) = variant else {
                        return Err(InternalError::TypeError {
                            expected: Type::Int,
                            got: self.objects.type_of(&variant),
                        }
                        .into());
                    };

                    if variant_index < 0 {
                        return Err(InternalError::ArrayIndexIsNegative(variant_index).into());
                    }

                    if variant_index as usize >= enm.variant_names.len() {
                        return Err(InternalError::ArrayIndexOutOfBounds {
                            index: variant_index as usize,
                            length: enm.variant_names.len(),
                        }
                        .into());
                    }

                    let object_index = self.objects.insert(Object::Variant(Variant {
                        enm: enum_index,
                        index: variant_index as usize,
                    }));

                    // Push the variant object on top of the stack.
                    self.stack.push(Value::Object(object_index));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::DispatchFuture(arg_count) => {
                    let args_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    let expected_type = FunctionType::Llm;

                    let index = self
                        .objects
                        .as_object(&self.stack[args_offset], expected_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(llm_function) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: expected_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != llm_function.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: llm_function.arity,
                            got: arg_count,
                        }));
                    }

                    // Not a future.
                    if !matches!(llm_function.kind, FunctionKind::Llm) {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: FunctionType::Llm.into(),
                            got: FunctionType::from(llm_function.kind).into(),
                        }));
                    }

                    // Collect the LLM function call args and cleanup the LLM
                    // call.
                    let llm_args = self.stack.drain(args_offset..).skip(1).collect();

                    // Create the pending future.
                    let llm_future = LlmFuture {
                        llm_function: llm_function.name.clone(),
                        args: llm_args,
                    };

                    // Allocate the future.
                    let object_index = self
                        .objects
                        .insert(Object::Future(Future::Pending(llm_future)));

                    // Now leave the future on top of the stack.
                    self.stack.push(Value::Object(object_index));

                    // Yield control flow back to the embedder.
                    return Ok(VmExecState::ScheduleFuture(object_index));
                }

                Instruction::Await => {
                    let value = self.stack.ensure_stack_top()?;

                    let wanted_type = FutureType::Any;

                    let index = self
                        .objects
                        .as_object(&self.stack[value], wanted_type.into())?;

                    let Object::Future(awaiting) = &self.objects[index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: wanted_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }));
                    };

                    match awaiting {
                        // Can't do nothing, handle control flow back to embedder.
                        Future::Pending(_) => {
                            return Ok(VmExecState::Await(index));
                        }

                        // Replace the future on the eval stack with the ready
                        // value.
                        Future::Ready(value) => {
                            self.stack.pop();
                            self.stack.push(*value);
                        }
                    }
                }

                Instruction::Call(arg_count) => {
                    // Function calls are pushed onto the stack like this:
                    //
                    // [callee, arg1, arg2, ..., argN]
                    //
                    // The callee is a ref to the function object, and after
                    // that we have all the function arguments. `arg_count` is
                    // the number of arguments pushed after the callee.
                    //
                    // That's how we compute the relative offset of the callee
                    // and it's local args in the stack.
                    let locals_offset = self.stack.ensure_slot_from_top(arg_count)?;

                    // Get the function object from the stack.
                    let local = &self.stack[locals_offset];

                    let function_type = FunctionType::Callable;

                    let index = self.objects.as_object(local, function_type.into())?;

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callee) = &self.objects[index] else {
                        return Err(InternalError::TypeError {
                            expected: function_type.into(),
                            got: ObjectType::of(&self.objects[index]).into(),
                        }
                        .into());
                    };

                    // Compiler should have already checked this so we could
                    // skip it but it's an easy and fast check.
                    if arg_count != callee.arity {
                        return Err(VmError::from(InternalError::InvalidArgumentCount {
                            expected: callee.arity,
                            got: arg_count,
                        }));
                    }

                    // Check if we've reached the max call stack size.
                    if self.frames.len() >= MAX_FRAMES {
                        return Err(VmError::RuntimeError(RuntimeError::StackOverflow));
                    }

                    match callee.kind {
                        FunctionKind::Native(func) => {
                            // NOTE: (perf) could use drain(..) instead, or even maintain the arguments
                            // reference in the stack, using `swap` to insert the result.
                            let args = self.stack[StackIndex(locals_offset.0 + 1)..].to_owned();

                            // Run Rust native function.
                            let result = func(self, &args)?;

                            // Drop function call and place result on top.
                            self.stack.drain(locals_offset..);
                            self.stack.push(result);

                            // Rust borrow check workaround because we're passing VM as
                            // mut and technically the frame pointer could be
                            // invalidated. Frame is Copy so we can maintain a
                            // local owned copy to avoid this but then we'd need
                            // to presist changes when moving to a new frame.
                            //
                            // We use `ObjectIndex` constructor directly because we know it's a
                            // valid reference (we are executing instructions inside of it).
                            frame = self.frames.last_mut().expect("last_mut() was pushed above");
                            function = self.objects[frame.function].as_function()?;
                        }

                        FunctionKind::Exec => {
                            // Otherwise push the new frame.
                            self.frames.push(Frame {
                                function: index,
                                instruction_ptr: 0,
                                locals_offset,
                            });

                            // Point to next frame.
                            frame = self.frames.last_mut().expect("last_mut() was pushed above");

                            // Grab function ref. We do this to avoid running this
                            // code at the beginning of each iteration since it's
                            // totaly unnecessary. The function only changes when the
                            // frame changes.
                            function = self.objects[frame.function].as_function()?;
                        }

                        FunctionKind::Llm => {
                            return Err(InternalError::TypeError {
                                expected: FunctionType::Callable.into(),
                                got: FunctionType::from(callee.kind).into(),
                            }
                            .into());
                        }
                    }
                }

                Instruction::Return => {
                    // Pop the result from the eval stack.
                    let result = self.stack.ensure_pop()?;

                    // Restore the eval stack to the state before the function
                    // was called and leave the result on top.
                    self.stack.drain(frame.locals_offset..);
                    self.stack.push(result);

                    // Pop from the call stack.
                    self.frames.pop();

                    // If there are no more frames, we're done.
                    let Some(previous_frame) = self.frames.last_mut() else {
                        return self
                            .stack
                            .ensure_pop()
                            .map(VmExecState::Complete)
                            .map_err(Into::into);
                    };

                    // Resume previous frame execution.
                    frame = previous_frame;

                    // Point to the previous frame's function. Read the
                    // implementation of `Instruction::Call` above this one for
                    // more information about this piece.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::Assert => {
                    let value = self.stack.pop().ok_or(RuntimeError::AssertionError)?;

                    let Value::Bool(condition_result) = value else {
                        return Err(InternalError::TypeError {
                            expected: Type::Bool,
                            got: self.objects.type_of(&value),
                        }
                        .into());
                    };

                    if !condition_result {
                        return Err(RuntimeError::AssertionError.into());
                    }
                }

                Instruction::AllocMap(n) => {
                    let map = if n > 0 {
                        let end_of_values = self.stack.ensure_slot_from_top(2 * n - 1)?;
                        let end_of_keys = self.stack.ensure_slot_from_top(n - 1)?;
                        let idx_of_last_key = self.stack.ensure_slot_from_top(n - 1)?;

                        // We can safely copy the objects that act as values so there's no problem
                        // with not draining them.
                        let values = self.stack[end_of_values..end_of_keys].iter().copied();

                        // We cannot copy key references since we aren't interning yet, so we
                        // must clone the strings.
                        // Here we'll also double-check that the keys are strings. This adds `n`
                        // branches which is not ideal for performance. Might want to consider this
                        // in map accesses.
                        let keys = self.stack[idx_of_last_key..].iter().map(|k| {
                            let ob_index = self.objects.as_object(k, ObjectType::String)?;

                            self.objects[ob_index].as_string().cloned()
                        });

                        let pairs = values
                            .zip(keys)
                            .map(|(val, key_res)| key_res.map(|k| (k, val)));

                        let map = pairs.collect::<Result<BamlMap<_, _>, _>>()?;

                        // drain & drop the drain so that vec is empty.
                        self.stack.drain(end_of_values..);

                        map
                    } else {
                        // nothing to pop.
                        BamlMap::new()
                    };

                    let ob_index = self.objects.insert(Object::Map(map));

                    self.stack.push(Value::Object(ob_index));

                    // borrow check.
                    function = self.objects[frame.function].as_function()?;
                }
            }
        }
    }
}
