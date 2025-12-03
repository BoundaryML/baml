//! Instruction set and bytecode representation.
//!
//! This module defines the bytecode instructions for the BAML VM.
//! The VM is stack-based, similar to `CPython` or the Lox VM.

use crate::types::Value;

/// Individual bytecode instruction.
///
/// For faster iteration we use an enum-based representation instead of
/// raw binary instructions. Some instructions take arguments (like indices
/// into pools), while others operate purely on the stack.
#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Instruction {
    /// Loads a constant from the bytecode's constant pool.
    ///
    /// Format: `LOAD_CONST i` where `i` is the index of the constant.
    LoadConst(usize),

    /// Loads a variable from the frame's local variable slots.
    ///
    /// Format: `LOAD_VAR i` where `i` is the relative index of the variable.
    LoadVar(usize),

    /// Stores a value in the frame's local variable slots.
    ///
    /// Format: `STORE_VAR i` where `i` is the relative index of the variable.
    StoreVar(usize),

    /// Load a global variable from the globals array.
    ///
    /// Format: `LOAD_GLOBAL i` where `i` is the index of the global.
    LoadGlobal(usize),

    /// Store a value in a global variable.
    ///
    /// Format: `STORE_GLOBAL i` where `i` is the index of the global.
    StoreGlobal(usize),

    /// Load a field of an object.
    ///
    /// Format: `LOAD_FIELD i` where `i` is the index of the field.
    LoadField(usize),

    /// Store the value on top of the stack in the field of an object.
    ///
    /// Format: `STORE_FIELD i` where `i` is the index of the field.
    StoreField(usize),

    /// Pop N values from the top of the evaluation stack.
    ///
    /// Format: `POP n` where `n` is the number of values to pop.
    Pop(usize),

    /// Copy the i-th value from the top of the stack to the top.
    ///
    /// Format: `COPY i` where `i` is the offset from the top of the stack.
    /// `COPY 0` copies the top element (duplicates it).
    Copy(usize),

    /// End a nested block and put the result value on top of the stack.
    ///
    /// Format: `POP_REPLACE n` where `n` is the number of locals in the block's scope.
    PopReplace(usize),

    /// Jump to another instruction.
    ///
    /// Format: `JUMP o` where `o` is the offset from the current instruction
    /// (can be negative to jump backwards).
    Jump(isize),

    /// Jump to another instruction if the top of stack is false.
    ///
    /// Format: `JUMP_IF_FALSE o` where `o` is the offset.
    JumpIfFalse(isize),

    /// Performs an arithmetic binary operation.
    BinOp(BinOp),

    /// Performs a comparison binary operation.
    CmpOp(CmpOp),

    /// Performs a unary operation.
    UnaryOp(UnaryOp),

    /// Builds an array and allocates it on the heap.
    ///
    /// Format: `ALLOC_ARRAY n` where `n` is the number of elements.
    AllocArray(usize),

    /// Builds a map and allocates it on the heap.
    ///
    /// Format `ALLOC_MAP n` where `n` is the number of entries.
    AllocMap(usize),

    /// Loads an element from an array at a given index.
    LoadArrayElement,

    /// Loads a value from a map at a given key.
    LoadMapElement,

    /// Stores a value into an array at a given index.
    StoreArrayElement,

    /// Stores a value into a map at a given key.
    StoreMapElement,

    /// Builds an instance of a class and allocates it on the heap.
    ///
    /// Format: `ALLOC_INSTANCE i` where `i` is the index of the class object.
    AllocInstance(usize),

    /// Builds a variant of an enum and allocates it on the heap.
    ///
    /// Format: `ALLOC_VARIANT i` where `i` is the index of the enum object.
    AllocVariant(usize),

    /// Creates a pending future, pushes it on the stack and notifies embedder.
    ///
    /// Format: `DISPATCH_FUTURE n` where `n` is the number of arguments.
    DispatchFuture(usize),

    /// Awaits the future on top of the stack.
    Await,

    /// Creates a watched var and tracks its state.
    ///
    /// Format: `WATCH i` where `i` is the relative index of the variable.
    Watch(usize),

    /// Manually triggers notifications for a watched variable.
    Notify(usize),

    /// Call a function.
    ///
    /// Format: `CALL n` where `n` is the number of arguments.
    Call(usize),

    /// Return from a function.
    Return,

    /// Pops a `Bool` value from the stack. If false, raises an assertion error.
    Assert,

    /// Notifies about entering or exiting a block.
    ///
    /// Format: `NOTIFY_BLOCK block_index` where `block_index` is the index
    /// into the current function's `block_notifications` array.
    NotifyBlock(usize),
}

/// Binary operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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

/// Comparison operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CmpOp {
    Eq,
    NotEq,
    Lt,
    LtEq,
    Gt,
    GtEq,
    InstanceOf,
}

/// Unary operations.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
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
            Instruction::AllocMap(n) => write!(f, "ALLOC_MAP {n}"),
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
            Instruction::Watch(i) => write!(f, "WATCH {i}"),
            Instruction::NotifyBlock(i) => write!(f, "NOTIFY_BLOCK {i}"),
            Instruction::Notify(i) => write!(f, "NOTIFY {i}"),
        }
    }
}

/// Executable bytecode.
///
/// Contains the instructions to run and all the associated constants.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Bytecode {
    /// Sequence of instructions.
    pub instructions: Vec<Instruction>,

    /// Constant pool.
    pub constants: Vec<Value>,

    /// Source line mapping (instruction index -> source line number).
    pub source_lines: Vec<usize>,

    /// Scope mapping (instruction index -> scope id).
    pub scopes: Vec<usize>,
}

impl Bytecode {
    pub fn new() -> Self {
        Self::default()
    }
}

impl std::fmt::Display for Bytecode {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        for (i, instruction) in self.instructions.iter().enumerate() {
            writeln!(f, "{i:4} {instruction}")?;
        }
        Ok(())
    }
}
