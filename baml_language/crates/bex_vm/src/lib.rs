//! Baml virtual machine.
//!
//! This crate implements a stack based virtual machine similar to the `CPython`
//! VM or Lox VM from [Crafting Interpreters](https://craftinginterpreters.com/).
//!
//! Main entry point is [`BexVm::exec`] which runs the VM cycle:
//! 1. Decode Instruction.
//! 2. Execute Instruction.
//! 3. Increment instruction pointer and repeat loop.
//!
//! The instructions that the VM runs are defined in [`bex_vm_types::bytecode::Instruction`] enum.

pub mod builtins;
pub mod debug;
pub mod errors;
pub mod indexable;
pub mod native;
pub mod types;
pub mod vm;
pub mod watch;

pub use errors::{InternalError, RuntimeError, StackTrace};
pub use indexable::EvalStack;
pub use vm::{BexVm, BytecodeProgram, VmExecState, convert_program};
