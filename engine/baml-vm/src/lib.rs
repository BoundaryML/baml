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

pub mod bytecode;
pub mod debug;
pub mod errors;
pub mod indexable;
pub mod native;
pub mod test;
pub mod types;
pub mod vm;
pub mod watch;

pub use bytecode::{BinOp, Bytecode, CmpOp, Instruction, UnaryOp};
pub use errors::{InternalError, RuntimeError, StackTrace};
pub use indexable::{EvalStack, GlobalIndex, GlobalPool, ObjectIndex, ObjectPool, StackIndex};
pub use types::{
    Class, Enum, Function, FunctionKind, Future, FutureKind, Object, ObjectType, Value, Variant,
    VizNodeMeta,
};
pub use vm::{BamlVmProgram, Frame, Vm, VmExecState};
