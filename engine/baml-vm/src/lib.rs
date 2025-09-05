//! Baml virtual machine.
//!
//! This crate implements a stack based virtual machine similar to the CPython
//! VM or Lox VM from [Crafting Interpreters](https://craftinginterpreters.com/).
//!
//! Main entry point is [`Vm::exec`] which runs the VM cycle:
//! 1. Decode Instruction.
//! 2. Execute Instruction.
//! 3. Increment instruction pointer and repeat loop.
//!
//! The instructions that the VM runs are defined in [`Instruction`] enum.

mod bytecode;
pub mod debug;
pub mod native;
pub(crate) mod vm;

pub use bytecode::{BinOp, Bytecode, CmpOp, Instruction, UnaryOp};
pub use vm::{
    indexable::{EvalStack, GlobalIndex, GlobalPool, ObjectIndex, ObjectPool, StackIndex},
    BamlVmProgram, Class, Frame, Function, FunctionKind, Instance, InternalError, Object,
    RuntimeError, Value, Vm, VmError, VmExecState,
};
