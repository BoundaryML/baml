//! Test utilities for bytecode verification.
//!
//! This module provides test-friendly representations of bytecode that use
//! human-readable names instead of indices, making tests more readable.

use crate::{BinOp, CmpOp, UnaryOp};

/// Test-friendly representation of VM values.
#[derive(Debug, Clone, PartialEq)]
pub enum Value {
    Null,
    Int(i64),
    Float(f64),
    Bool(bool),
    String(String),
    Function(String),
    Class(String),
    Enum(String),
}

impl Value {
    /// Shorthand for creating a string value.
    pub fn string(s: &str) -> Self {
        Value::String(s.to_string())
    }

    /// Shorthand for creating a function reference.
    pub fn function(name: &str) -> Self {
        Value::Function(name.to_string())
    }

    /// Shorthand for creating a class reference.
    pub fn class(name: &str) -> Self {
        Value::Class(name.to_string())
    }

    /// Shorthand for creating an enum reference.
    pub fn enm(name: &str) -> Self {
        Value::Enum(name.to_string())
    }
}

/// Test-friendly bytecode instruction that embeds values/names instead of indices.
#[derive(Clone, Debug, PartialEq)]
pub enum Instruction {
    /// Load a constant value.
    LoadConst(Value),
    /// Load a local variable by name.
    LoadVar(String),
    /// Store to a local variable by name.
    StoreVar(String),
    /// Load a global by value (function/class/enum name).
    LoadGlobal(Value),
    /// Store to a global by value.
    StoreGlobal(Value),
    /// Load a field by index.
    LoadField(usize),
    /// Store a field by index.
    StoreField(usize),
    /// Pop N values from stack.
    Pop(usize),
    /// Copy the i-th value from top of stack.
    Copy(usize),
    /// Pop N values but keep top value.
    PopReplace(usize),
    /// Unconditional jump.
    Jump(isize),
    /// Jump if top of stack is false.
    JumpIfFalse(isize),
    /// Binary operation.
    BinOp(BinOp),
    /// Comparison operation.
    CmpOp(CmpOp),
    /// Unary operation.
    UnaryOp(UnaryOp),
    /// Allocate array with N elements from stack.
    AllocArray(usize),
    /// Allocate map with N entries from stack.
    AllocMap(usize),
    /// Load element from array.
    LoadArrayElement,
    /// Load element from map.
    LoadMapElement,
    /// Store element to array.
    StoreArrayElement,
    /// Store element to map.
    StoreMapElement,
    /// Allocate class instance.
    AllocInstance(Value),
    /// Allocate enum variant.
    AllocVariant(Value),
    /// Dispatch a future.
    DispatchFuture(usize),
    /// Await a future.
    Await,
    /// Watch a variable.
    Watch(usize),
    /// Notify about a variable change.
    Notify(usize),
    /// Call function with N arguments.
    Call(usize),
    /// Return from function.
    Return,
    /// Assert top of stack is true.
    Assert,
    /// Notify about block enter/exit.
    NotifyBlock(usize),
}

impl Instruction {
    /// Helper to create `LoadVar` instruction.
    pub fn load_var(name: &str) -> Self {
        Instruction::LoadVar(name.to_string())
    }

    /// Helper to create `StoreVar` instruction.
    pub fn store_var(name: &str) -> Self {
        Instruction::StoreVar(name.to_string())
    }

    /// Helper to create `LoadGlobal` with function name.
    pub fn load_global_fn(name: &str) -> Self {
        Instruction::LoadGlobal(Value::function(name))
    }
}
