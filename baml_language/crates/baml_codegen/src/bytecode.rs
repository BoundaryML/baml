//! Bytecode representation for BAML.

/// A bytecode module (compiled file).
#[derive(Debug, Clone, PartialEq)]
pub struct BytecodeModule {
    pub instructions: Vec<Instruction>,
    pub constants: Vec<Constant>,
}

impl Eq for BytecodeModule {}

/// Bytecode instruction.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Instruction {
    /// Load a constant onto the stack
    LoadConst(u32),

    /// Call a function
    Call { func_id: u32, arg_count: u8 },

    /// Return from function
    Return,

    /// Placeholder for more instructions
    Nop,
}

/// Constant pool entry.
#[derive(Debug, Clone, PartialEq)]
pub enum Constant {
    String(String),
    Int(i64),
    Float(f64),
    Bool(bool),
    Null,
}

impl Eq for Constant {}
