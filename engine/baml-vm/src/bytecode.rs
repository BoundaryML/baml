//! Instruction set and bytecode representation.

use crate::vm::Value;

/// Individual bytecode instruction.
///
/// For faster iteration we'll start with an in-memory data structure that
/// represents the bytecode instead of real binary instructions since getting
/// those to work correctly is much harder (unsafe Rust, pointer arithmetic).
///
/// We do need to respect some sort of "instruction format" however. In
/// stack-based VMs some instructions don't take any arguments (for example,
/// the `ADD` instruction would grab its operands from the evaluation stack),
/// but some others such as `LOAD_CONST` need to know which constant to load,
/// so they take an unsigned integer as an argument (the index of the constant
/// in the constant pool). Same goes for jump instructions, we need to know the
/// offset.
///
/// We are not limited to one single argument, we can have variable-length
/// instructions in the VM, but we do have to keep the arguments limited to
/// "bytes" (unsigned integers, signed integers, etc). Use the arguments to
/// index into runtime structures such as constant pools, object pools, etc.
/// Don't embed complex data structures in an instruction. Avoid this:
///
/// ```ignore
/// enum Instruction {
///     MySuperDuperInstruction(HashMap<String, Vec<Function>>)
/// }
/// ```
///
/// Instead store the state or complex structure in the [`crate::Vm`] struct and
/// find a way to reference it with very simple instructions.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instruction {
    /// Loads a constant from the bytecode's constant pool.
    ///
    /// Format: `LOAD_CONST i` where `i` is the index of the constant in the
    /// [`Bytecode::constants`] pool.
    LoadConst(usize),

    /// Loads a variable from the frame's local variable slots.
    ///
    /// Format: `LOAD_VAR i` where `i` is the index of the variable in the
    /// [`crate::Frame::locals`] array.
    LoadVar(usize),

    /// Stores a value in the frame's local variable slots.
    ///
    /// Format: `STORE_VAR i` where `i` is the index of the variable in the
    /// [`crate::Frame::locals`] array.
    StoreVar(usize),

    /// Load a global variable from the [`crate::Vm::globals`] array.
    ///
    /// Format: `LOAD_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
    ///
    /// Note that functions are also globals and can be passed around and stored
    /// in local variables, so we need to load their name in the stack before we
    /// call the function.
    LoadGlobal(usize),

    /// Store a value in a global variable.
    ///
    /// Format: `STORE_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
    StoreGlobal(usize),

    /// Load a field of an object.
    ///
    /// Format: `LOAD_FIELD i` where `i` is the index of the field in the
    /// object's fields array.
    LoadField(usize),

    /// Store the value on top of the stack in the field of an object.
    ///
    /// Format: `STORE_FIELD i` where `i` is the index of the field in the
    /// object's fields array.
    StoreField(usize),

    /// Pop the top of [`crate::Vm::stack`] (the evaluation stack).
    Pop,

    /// End a nested block.
    ///
    /// Format: `END_BLOCK n` where `n` is the number of locals in the block's
    /// scope.
    ///
    /// This is instruction is necessary to support "blocks as expressions".
    /// Example:
    ///
    /// ```ignore
    /// fn main() {
    ///     let a = {
    ///         let b = 1;
    ///         b
    ///     };
    /// }
    /// ```
    ///
    /// Technicaly we could emit [`Instruction::StoreVar`] to store the block in
    /// `a` and then emit one [`Instruction::Pop`] for each scoped local. But
    /// if we have many locals we would need a specialized `POP_N` instruction
    /// that pops more than one local in once VM cycle, so since we need a new
    /// instruction anyway we'll just use this one that is similar to
    /// [`Instruction::Return`] but for scoped blocks.
    EndBlock(usize),

    /// Jump to another instruction.
    ///
    /// Format: `JUMP o` where `o` is the offset from the current instruction
    /// to the target instruction (can be negative to jump backwards).
    Jump(isize),

    /// Jump to another instruction if the top of [`crate::Vm::stack`] is false.
    ///
    /// Format: `JUMP_IF_FALSE o` where `o` is the offset from the current
    /// instruction to the target instruction (can be negative to jump
    /// backwards).
    JumpIfFalse(isize),

    /// Builds an array and allocates it on the heap.
    ///
    /// Format: `ALLOC_ARRAY n` where `n` is the number of elements in the
    /// array. All elements must be on the stack by the time this instruction is
    /// executed.
    AllocArray(usize),

    /// Builds an instance of a class and allocates it on the heap.
    ///
    /// Format: `ALLOC_INSTANCE i` where `i` is the index of the class in the
    /// [`crate::Vm::objects`] array.
    AllocInstance(usize),

    /// Create an iterator from an array.
    ///
    /// Format: `CREATE_ITERATOR` - pops an array from the stack and pushes an iterator.
    /// Stack before: [array]
    /// Stack after: [iterator]
    CreateIterator,

    /// Get the next element from an iterator.
    ///
    /// Format: `ITER_NEXT` - pops an iterator from the stack and pushes the next element and a boolean.
    /// Stack before: [iterator]
    /// Stack after: [iterator, element, has_next]
    /// TODO(Rahul): Check with Antonio, if this insn is complex than needed.
    IterNext,

    /// Creates a pending future, pushes it on the stack and notifies embedder.
    ///
    /// Format: `DISPATCH_FUTURE n` where `n` is the number of arguments passed
    /// to the _callable_ future.
    ///
    /// [`Instruction::DispatchFuture`] behaves like a function call
    /// ([`Instruction::Call`]). That is due to the fact that as of right now
    /// the only "futures" we can really run are LLM calls, and the VM doesn't
    /// even run those, that's up to the embedder. So, just like a function
    /// call, the stack should contain the future followed by the arguments, and
    /// this instruction takes care of emmiting a notification to the embedder
    /// so that it can schedule the future.
    DispatchFuture(usize),

    /// Awaits the future on top of the stack.
    ///
    /// VM yields execution back to the embedder because it is blocked awaiting
    /// a future. But obviously, the VM will not "block", it just returns
    /// control flow to the embedder and doesn't care about anything else.
    Await,

    /// Call a function.
    ///
    /// Format: `CALL n` where `n` is the number of arguments passed to the
    /// function.
    ///
    /// Arguments are pushed onto the eval stack and the name of the function
    /// is right below them.
    Call(usize),

    /// Return from a function.
    ///
    /// No arguments needed, result is stored in the eval stack and the VM
    /// simply has to clean up the call stack and continue execution.
    Return,
}

impl std::fmt::Display for Instruction {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            Instruction::LoadConst(i) => write!(f, "LOAD_CONST {i}"),
            Instruction::LoadVar(i) => write!(f, "LOAD_VAR {i}"),
            Instruction::StoreVar(i) => write!(f, "STORE_VAR {i}"),
            Instruction::LoadGlobal(i) => write!(f, "LOAD_GLOBAL {i}"),
            Instruction::StoreGlobal(i) => write!(f, "STORE_GLOBAL {i}"),
            Instruction::LoadField(i) => write!(f, "LOAD_FIELD {i}"),
            Instruction::StoreField(i) => write!(f, "STORE_FIELD {i}"),
            Instruction::Pop => f.write_str("POP"),
            Instruction::EndBlock(n) => write!(f, "END_BLOCK {n}"),
            Instruction::Jump(o) => write!(f, "JUMP {o}"),
            Instruction::JumpIfFalse(o) => write!(f, "JUMP_IF_FALSE {o}"),
            Instruction::AllocArray(n) => write!(f, "ALLOC_ARRAY {n}"),
            Instruction::AllocInstance(i) => write!(f, "ALLOC_INSTANCE {i}"),
            Instruction::CreateIterator => f.write_str("CREATE_ITERATOR"),
            Instruction::IterNext => f.write_str("ITER_NEXT"),
            Instruction::DispatchFuture(i) => write!(f, "DISPATCH_FUTURE {i}"),
            Instruction::Await => f.write_str("AWAIT"),
            Instruction::Call(n) => write!(f, "CALL {n}"),
            Instruction::Return => f.write_str("RETURN"),
        }
    }
}

/// Executable bytecode.
///
/// Contains the instructions to run and all the associated constants.
#[derive(Clone, Debug)]
pub struct Bytecode {
    /// Sequence of instructions.
    pub instructions: Vec<Instruction>,

    /// Constant pool.
    pub constants: Vec<Value>,

    /// Source line mapping.
    ///
    /// Maps instruction indices to their source line numbers.
    /// Each element corresponds to an instruction at the same index.
    pub source_lines: Vec<usize>,
}

impl Bytecode {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            source_lines: Vec::new(),
        }
    }
}

impl std::fmt::Display for Bytecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for instruction in &self.instructions {
            writeln!(f, "{instruction}")?;
        }

        Ok(())
    }
}
