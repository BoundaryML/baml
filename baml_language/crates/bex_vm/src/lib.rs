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

pub(crate) mod array_index;
pub mod debug;
pub mod errors;
pub mod indexable;
pub mod kperf;
pub mod package_ai;
pub mod package_baml;
pub mod package_boundary;
pub mod package_load;
mod type_context;
mod type_match;
pub mod types;
pub mod vm;

pub use errors::{StackFrame, VmPanic, format_traceback};
pub use indexable::EvalStack;
pub use package_baml::NativeFunction;
pub use vm::{
    BexVm, BytecodeFrame, BytecodeProgram, Frame, VmCallCaptureEvent, VmCallCaptureKind,
    VmCallInputCapture, VmCallInputCaptureHook, VmCaptureMask, VmEventSourceLocation, VmExecState,
    convert_program,
};
