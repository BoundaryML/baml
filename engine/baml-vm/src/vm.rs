use crate::bytecode::{Bytecode, Instruction};

/// Max call stack size.
const MAX_FRAMES: usize = 256;

/// Function type.
#[derive(Clone, Debug)]
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

    /// OS interfacing function.
    ///
    /// VM will handle control flow to a Rust wrapper that calls into the OS
    /// and returns a result. Needed for features like `fetch`.
    Native,
}

/// Represents any Baml function.
#[derive(Clone, Debug)]
pub struct Function {
    /// Function name.
    pub name: String,

    /// Number of arguments the function accepts.
    pub arity: usize,

    /// Bytecode to execute.
    pub bytecode: Bytecode,

    /// Type of function.
    pub kind: FunctionKind,

    /// Local variable names.
    ///
    /// This is basically debug info, VM doesn't need it all to run.
    pub local_var_names: Vec<String>,
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
    pub class: usize,

    /// Fields are accessed by index. No string lookups.
    pub fields: Vec<Value>,
}

impl std::fmt::Display for Instance {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "<instance of {}>", self.class)
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

    /// Iterator over an array (array for now).
    Iterator { iterable: usize, index: usize },
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
            _ => Err(InternalError::InvalidFunctionRef.into()),
        }
    }
}

impl std::fmt::Display for Object {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Object::Function(function) => function.fmt(f),
            Object::Class(class) => class.fmt(f),
            Object::Instance(instance) => instance.fmt(f),
            Object::String(string) => string.fmt(f),
            Object::Array(array) => std::fmt::Debug::fmt(array, f),
            Object::Iterator { iterable, index } => {
                write!(f, "<iterator iterable={iterable} index={index}>")
            }
        }
    }
}

/// Runtime values.
///
/// This struct should not contain allocated objects and should be [`Copy`].
/// Read the documentation of [`Vm::objects`] to understand how allocated
/// objects work in the virtual machine.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),

    /// Index into the [`Vm::objects`] vec.
    ///
    /// Strings are also objects, don't add `Value::String`.
    Object(usize),
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
#[derive(Debug)]
pub enum Type {
    Int,
    Float,
    Bool,
    Object,
}

impl Type {
    /// Get the type of a value.
    pub fn of(value: &Value) -> Self {
        match value {
            Value::Int(_) => Type::Int,
            Value::Float(_) => Type::Float,
            Value::Bool(_) => Type::Bool,
            Value::Object(_) => Type::Object,
            // TODO: Actually?
            Value::Null => Type::Object,
        }
    }
}

/// Bug in the VM or somehow invalid source code got compiled and executed.
///
/// If the VM throws this it's either a bug in the compiler or in the VM itself.
#[derive(Debug)]
pub enum InternalError {
    /// The number of arguments passed to a function doesn't match the function
    /// arity.
    InvalidArgumentCount { expected: usize, got: usize },

    /// Attempt to access a function but object is not of type [`Object::Function`].
    ///
    /// TODO: Probably can be turned into [`InternalError::TypeError`] (expected
    /// function, got something else).
    InvalidFunctionRef,

    /// Attempt to access the top of the stack but it's empty.
    UnexpectedEmptyStack,

    /// Attempt to access the top of the stack but it's not the expected type.
    TypeError { expected: Type, got: Type },
}

/// Errors that can happen at runtime.
///
/// Either logic errors in the user's source code or bugs in our compiler/VM
/// stack.
#[derive(Debug)]
pub enum RuntimeError {
    /// Ah yes, classic stack overflow.
    StackOverflow,

    /// VM internal error.
    InternalError(InternalError),
}

/// Any kind of virtual machine error.
#[derive(Debug)]
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

impl std::error::Error for VmError {}

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
    pub function: usize,

    /// Instruction pointer (IP) or program counter (PC).
    ///
    /// Points to the next instruction that the VM will execute. It is of type
    /// [`isize`] because some jumps can create negative offsets (for loops)
    /// and it's easier to operate on an [`isize`] and cast it to [`usize`]
    /// only once (when we index into [`Bytecode::instructions`]). However,
    /// this number should never be negative, otherwise indexing into the
    /// instruction vec will panic.
    pub instruction_ptr: isize,

    /// Local variables offset in the eval stack.
    pub locals_offset: usize,
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
    pub stack: Vec<Value>,

    /// Object pool.
    ///
    /// For now, since we don't have a garbage collector yet, this is basically
    /// an arena of objects. **Every object** is allocated here and will be
    /// destroyed when the lifetime of the [`Vm`] ends. Do not allocate objects
    /// elsewhere since that will make adding a garbage collector harder.
    /// Only allocate objects here and use indices to reference them, don't
    /// bother with Rust references because they will introduce lifetime issues.
    pub objects: Vec<Object>,

    /// Global variables.
    ///
    /// This stores the functions and globally declared variables.
    pub globals: Vec<Value>,
}

impl Vm {
    /// Main VM execution loop.
    ///
    /// Each "cycle" (loop iteration) executes a single instruction.
    pub fn exec(&mut self) -> Result<Value, VmError> {
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
            return Ok(Value::Null);
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

            // Move the frame's IP to the next instruction. We'll deal with
            // jump offsets later.
            frame.instruction_ptr += 1;

            // Runtime debugging information.
            #[cfg(debug_assertions)]
            {
                let stack = self
                    .stack
                    .iter()
                    .map(|v| crate::debug::display_value(v, &self.objects))
                    .collect::<Vec<_>>()
                    .join(", ");

                eprintln!("[{stack}]");

                let (instruction, metadata) = crate::debug::display_instruction(
                    instruction_ptr,
                    function,
                    &self.stack,
                    &self.objects,
                    &self.globals,
                );

                eprintln!("{instruction} {metadata}");
            }

            match function.bytecode.instructions[instruction_ptr as usize] {
                Instruction::LoadConst(index) => {
                    let value = &function.bytecode.constants[index];
                    self.stack.push(*value);
                }

                Instruction::LoadVar(index) => {
                    let value = &self.stack[frame.locals_offset + index];
                    self.stack.push(*value);
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
                    let Some(value) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    self.stack[frame.locals_offset + index] = value;
                }

                Instruction::LoadGlobal(index) => {
                    let value = &self.globals[index];
                    self.stack.push(*value);
                }

                Instruction::StoreGlobal(index) => {
                    // Consume the value. Read impl of Instruction::StoreVar.
                    let Some(value) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    self.globals[index] = value;
                }

                Instruction::LoadField(index) => {
                    let Some(Value::Object(reference)) = self.stack.pop() else {
                        panic!("expect object, got {:?}", self.stack[self.stack.len() - 1]);
                    };

                    let Object::Instance(instance) = &self.objects[reference] else {
                        panic!("expect instance, got {:?}", self.objects[reference]);
                    };

                    // Push the value on top of the stack.
                    self.stack.push(instance.fields[index]);
                }

                Instruction::StoreField(index) => {
                    let Some(value) = self.stack.last() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    let Value::Object(reference) = self.stack[self.stack.len() - 2] else {
                        panic!("expect object, got {:?}", self.stack[self.stack.len() - 2]);
                    };

                    let Object::Instance(instance) = &mut self.objects[reference] else {
                        panic!("expect instance, got {:?}", self.objects[reference]);
                    };

                    // Set the value.
                    instance.fields[index] = *value;

                    // Consume the value.
                    self.stack.pop();

                    // TODO: Borrow checker stuff.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::Pop => {
                    self.stack.pop();
                }

                Instruction::Jump(offset) => {
                    // Reassign the frame's IP to the new instruction.
                    // Remember that offset can be negative here, so even though
                    // we're adding it can still jump backwards.
                    frame.instruction_ptr = instruction_ptr + offset;
                }

                Instruction::JumpIfFalse(offset) => match self.stack.last() {
                    // Reassign only if the top of the stack is false.
                    Some(Value::Bool(value)) => {
                        if !value {
                            frame.instruction_ptr = instruction_ptr + offset;
                        }
                    }

                    // Type error, we don't have "falsey" values in the language
                    // so we should always check booleans.
                    Some(other) => {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: Type::Bool,
                            got: Type::of(other),
                        }))
                    }

                    // Empty stack, can't execute instruction.
                    None => return Err(InternalError::UnexpectedEmptyStack.into()),
                },

                Instruction::AllocArray(size) => {
                    // Pop all the elements from the stack and create an array.
                    let array = self.stack.drain(self.stack.len() - size..).collect();

                    // Allocate it on the heap.
                    self.objects.push(Object::Array(array));

                    // Push the array object on top of the stack.
                    self.stack.push(Value::Object(self.objects.len() - 1));

                    // objects.push() above might've reallocated the vector so
                    // borrow checker complains. Restore the reference.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::AllocInstance(index) => {
                    let Object::Class(class) = &self.objects[index] else {
                        panic!("expect class, got {:?}", self.objects[index]);
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
                    self.stack.push(Value::Object(self.objects.len() - 1));

                    // Same as in the instruction above.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::EndBlock(n) => {
                    let Some(value) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    // Pop the last `n` locals from the stack.
                    self.stack.drain(self.stack.len() - n..).for_each(drop);

                    // Push the value back on top of the stack.
                    self.stack.push(value);
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
                    let locals_offset = if self.stack.is_empty() {
                        0
                    } else {
                        self.stack.len() - arg_count - 1
                    };

                    // Get the function object from the stack.
                    let Value::Object(index) = &self.stack[locals_offset] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: Type::Object,
                            got: Type::of(&self.stack[locals_offset]),
                        }));
                    };

                    // Can't call a function if it's not a function ¯\_(ツ)_/¯
                    let Object::Function(callee) = &self.objects[*index] else {
                        return Err(InternalError::InvalidFunctionRef.into());
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

                    // Otherwise push the new frame.
                    self.frames.push(Frame {
                        function: *index,
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

                Instruction::Return => {
                    // Pop the result from the eval stack.
                    let Some(result) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

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
                            .pop()
                            .ok_or(InternalError::UnexpectedEmptyStack.into());
                    };

                    // Resume previous frame execution.
                    frame = previous_frame;

                    // Point to the previous frame's function. Read the
                    // implementation of `Instruction::Call` above this one for
                    // more information about this piece.
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::CreateIterator => {
                    // Pop the array from the stack
                    let Some(Value::Object(array_index)) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    // Verify it's an array
                    let Object::Array(_) = &self.objects[array_index] else {
                        return Err(VmError::from(InternalError::TypeError {
                            expected: Type::Object,
                            got: Type::Object,
                        }));
                    };

                    // Create the iterator
                    self.objects.push(Object::Iterator {
                        iterable: array_index,
                        index: 0,
                    });

                    // Push the iterator on the stack
                    self.stack.push(Value::Object(self.objects.len() - 1));

                    // Restore function reference
                    function = self.objects[frame.function].as_function()?;
                }

                Instruction::IterNext => {
                    // Pop the iterator from the stack
                    let Some(Value::Object(iterator_index)) = self.stack.pop() else {
                        return Err(InternalError::UnexpectedEmptyStack.into());
                    };

                    let (array_index, current_index) = match &self.objects[iterator_index] {
                        Object::Iterator {
                            iterable: array,
                            index,
                        } => (*array, *index),
                        _ => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Object,
                                got: Type::Object,
                            }))
                        }
                    };

                    // Get the element from the array
                    let (element, has_next) = match &self.objects[array_index] {
                        Object::Array(array) => {
                            if current_index < array.len() {
                                let element = array[current_index];
                                let has_next = (current_index + 1) < array.len();
                                (element, has_next)
                            } else {
                                (Value::Null, false)
                            }
                        }
                        _ => {
                            return Err(VmError::from(InternalError::TypeError {
                                expected: Type::Object,
                                got: Type::Object,
                            }))
                        }
                    };

                    // Now update the iterator index
                    match &mut self.objects[iterator_index] {
                        Object::Iterator { index, .. } => {
                            *index += 1;
                        }
                        _ => unreachable!(), // We already checked this above
                    };

                    // Push iterator back, then element, then has_next
                    self.stack.push(Value::Object(iterator_index));
                    self.stack.push(element);
                    self.stack.push(Value::Bool(has_next));

                    // Restore function reference
                    function = self.objects[frame.function].as_function()?;
                }
            }
        }
    }
}
