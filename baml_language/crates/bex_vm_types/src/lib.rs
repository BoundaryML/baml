//! Baml virtual machine.
//!
//! This crate implements a stack based virtual machine similar to the `CPython`
//! VM or Lox VM from [Crafting Interpreters](https://craftinginterpreters.com/).
//!
//! Main entry point is `Vm::exec` (in `bex_vm` crate) which runs the VM cycle:
//! 1. Decode Instruction.
//! 2. Execute Instruction.
//! 3. Increment instruction pointer and repeat loop.
//!
//! The instructions that the VM runs are defined in [`Instruction`] enum.

pub mod bytecode;
pub mod heap_ptr;
pub mod indexable;
pub mod types;

pub use bytecode::{
    BinOp, Bytecode, CmpOp, Instruction, JumpTableData, UnaryOp, VizExecDelta, VizExecEvent,
    VizNodeMeta, VizNodeType,
};
pub use heap_ptr::HeapPtr;
pub use indexable::{GlobalIndex, GlobalPool, ObjectIndex, ObjectPool, StackIndex};
pub use types::{
    Class, ClassField, ClientBuildMeta, ClientBuildType, CollectorRef, ConstValue, Enum,
    EnumVariant, Function, FunctionKind, FunctionMeta, Future, Instance, MediaValue, Object,
    ObjectType, PendingFuture, Program, PromptAst, RetryPolicyMeta, SysOp, SysOpErrorCategory,
    SysOpPanicCategory, TestArgValue, TestCase, Value, Variant, sys_op_for_path, type_tags,
};
