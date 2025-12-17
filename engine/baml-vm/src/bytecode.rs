//! Instruction set and bytecode representation.

use crate::{types::Value, GlobalIndex, ObjectIndex};

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
#[allow(clippy::large_enum_variant)]
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instruction {
    /// Loads a constant from the bytecode's constant pool.
    ///
    /// Format: `LOAD_CONST i` where `i` is the index of the constant in the
    /// [`Bytecode::constants`] pool.
    LoadConst(usize),

    /// Loads a variable from the frame's local variable slots.
    ///
    /// Format: `LOAD_VAR i` where `i` is the relative index of the variable in
    /// [`crate::Vm::stack`] array.
    LoadVar(usize),

    /// Stores a value in the frame's local variable slots.
    ///
    /// Format: `STORE_VAR i` where `i` is the relative index of the variable in
    /// [`crate::Vm::stack`] array.
    StoreVar(usize),

    /// Load a global variable from the [`crate::Vm::globals`] array.
    ///
    /// Format: `LOAD_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
    ///
    /// Note that functions are also globals and can be passed around and stored
    /// in local variables, so we need to load their name in the stack before we
    /// call the function.
    LoadGlobal(GlobalIndex),

    /// Store a value in a global variable.
    ///
    /// Format: `STORE_GLOBAL i` where `i` is the index of the global variable
    /// in the [`crate::Vm::globals`] array.
    StoreGlobal(GlobalIndex),

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

    /// Pop N values from the top of [`crate::Vm::stack`] (the evaluation stack).
    ///
    /// Format: `POP n` where `n` is the number of values to pop.
    Pop(usize),

    /// Copy the i-th value from the top of the stack to the top.
    ///
    /// Format: `COPY i` where `i` is the offset from the top of the stack.
    /// `COPY 0` copies the top element (duplicates it).
    /// `COPY 1` copies the second element from the top.
    Copy(usize),

    /// End a nested block and put the result value on top of the stack.
    ///
    /// Format: `POP_REPLACE n` where `n` is the number of locals in the block's
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
    PopReplace(usize),

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

    /// Performs an arithmetic binary operation.
    ///
    /// Format: `BIN_OP op` where `op` is the binary operation to perform.
    BinOp(BinOp),

    /// Performs a comparison binary operation.
    ///
    /// Format: `CMP_OP op` where `op` is the comparison operation to perform.
    CmpOp(CmpOp),

    /// Performs a unary operation.
    ///
    /// Format: `UNARY_OP op` where `op` is the unary operation to perform.
    UnaryOp(UnaryOp),

    /// Builds an array and allocates it on the heap.
    ///
    /// Format: `ALLOC_ARRAY n` where `n` is the number of elements in the
    /// array. All elements must be on the stack by the time this instruction is
    /// executed.
    AllocArray(usize),

    /// Builds a map and allocates it on the heap.
    ///
    /// Format `ALLOC_MAP n` where `n` is the number of entries in the map.
    /// `n` keys are popped first and then `n` values are popped after that.
    /// In total that's 2n stack required before the instruction is executed.
    AllocMap(usize),

    /// Loads an element from an array at a given index.
    ///
    /// Format: `LOAD_ARRAY_ELEMENT` where the stack contains [array, index] and
    /// the result is the element at that index.
    LoadArrayElement,

    /// Loads a value from a map at a given key.
    ///
    /// Format: `LOAD_MAP_ELEMENT` where the stack contains [map, key] and
    /// the result is the value at that key.
    LoadMapElement,

    /// Stores a value into an array at a given index.
    ///
    /// Format: `STORE_ARRAY_ELEMENT` where the stack contains [array, index, value]
    /// and stores the value at array[index].
    StoreArrayElement,

    /// Stores a value into a map at a given key.
    ///
    /// Format: `STORE_MAP_ELEMENT` where the stack contains [map, key, value]
    /// and stores the value at map[key].
    StoreMapElement,

    /// Builds an instance of a class and allocates it on the heap.
    ///
    /// Format: `ALLOC_INSTANCE i` where `i` is the index of the class in the
    /// [`crate::Vm::objects`] array.
    AllocInstance(ObjectIndex),

    /// Builds a variant of an enum and allocates it on the heap.
    ///
    /// Format: `ALLOC_VARIANT i` where `i` is the index of the enum in the
    /// [`crate::Vm::objects`] array.
    AllocVariant(ObjectIndex),

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

    /// Creates a watched var and tracks its state.
    ///
    /// Format: `WATCH i` where `i` is the relative index of the variable in the
    /// [`crate::Vm::stack`] array.
    Watch(usize),

    /// Manually triggers notifications for a watched variable.
    Notify(usize),

    /// Enter a visualization node.
    ///
    /// Format: `VIZ_ENTER i` where `i` is the index into the current
    /// function's `viz_nodes` array.
    VizEnter(usize),

    /// Exit a visualization node.
    ///
    /// Format: `VIZ_EXIT i` where `i` is the index into the current
    /// function's `viz_nodes` array.
    VizExit(usize),

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

    /// Pops a `Bool` value from the stack. If the value is `false`, raises
    /// an assertion error.
    ///
    /// Format: `ASSERT`
    Assert,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum BinOp {
    Add,
    Sub,
    Mul,
    Div,
    Mod,
    BitAnd,
    BitOr,
    BitXor,
    Shl,
    Shr,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    InstanceOf,
}

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum UnaryOp {
    Not,
    Neg,
}

impl std::fmt::Display for BinOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            BinOp::Add => "+",
            BinOp::Sub => "-",
            BinOp::Mul => "*",
            BinOp::Div => "/",
            BinOp::Mod => "%",
            BinOp::BitAnd => "&",
            BinOp::BitOr => "|",
            BinOp::BitXor => "^",
            BinOp::Shl => "<<",
            BinOp::Shr => ">>",
        })
    }
}

impl std::fmt::Display for CmpOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            CmpOp::Eq => "==",
            CmpOp::NotEq => "!=",
            CmpOp::Lt => "<",
            CmpOp::LtEq => "<=",
            CmpOp::Gt => ">",
            CmpOp::GtEq => ">=",
            CmpOp::InstanceOf => "instanceof",
        })
    }
}

impl std::fmt::Display for UnaryOp {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            UnaryOp::Not => "!",
            UnaryOp::Neg => "-",
        })
    }
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
            Instruction::Pop(n) => write!(f, "POP {n}"),
            Instruction::Copy(i) => write!(f, "COPY {i}"),
            Instruction::PopReplace(n) => write!(f, "POP_REPLACE {n}"),
            Instruction::Jump(o) => write!(f, "JUMP {o:+}"),
            Instruction::JumpIfFalse(o) => write!(f, "JUMP_IF_FALSE {o:+}"),
            Instruction::BinOp(op) => write!(f, "BIN_OP {op}"),
            Instruction::CmpOp(op) => write!(f, "CMP_OP {op}"),
            Instruction::UnaryOp(op) => write!(f, "UNARY_OP {op}"),
            Instruction::AllocArray(n) => write!(f, "ALLOC_ARRAY {n}"),
            Instruction::LoadArrayElement => f.write_str("LOAD_ARRAY_ELEMENT"),
            Instruction::LoadMapElement => f.write_str("LOAD_MAP_ELEMENT"),
            Instruction::StoreArrayElement => f.write_str("STORE_ARRAY_ELEMENT"),
            Instruction::StoreMapElement => f.write_str("STORE_MAP_ELEMENT"),
            Instruction::AllocInstance(i) => write!(f, "ALLOC_INSTANCE {i}"),
            Instruction::AllocVariant(i) => write!(f, "ALLOC_VARIANT {i}"),
            Instruction::DispatchFuture(i) => write!(f, "DISPATCH_FUTURE {i}"),
            Instruction::Await => f.write_str("AWAIT"),
            Instruction::Call(n) => write!(f, "CALL {n}"),
            Instruction::Return => f.write_str("RETURN"),
            Instruction::Assert => f.write_str("ASSERT"),
            Instruction::AllocMap(n) => write!(f, "ALLOC_MAP {n}"),
            Instruction::Watch(i) => write!(f, "WATCH {i}"),
            Instruction::Notify(i) => write!(f, "NOTIFY {i}"),
            Instruction::VizEnter(i) => write!(f, "VIZ_ENTER {i}"),
            Instruction::VizExit(i) => write!(f, "VIZ_EXIT {i}"),
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

    pub scopes: Vec<usize>,
}

impl Bytecode {
    pub fn new() -> Self {
        Self {
            instructions: Vec::new(),
            constants: Vec::new(),
            source_lines: Vec::new(),
            scopes: Vec::new(),
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
