//! Compiles a program's MIR to Rust functions that the VM runs in place of
//! their bytecode. A function whose body has no lowering stays bytecode.

mod admit;
mod analysis;
mod codegen;
mod reach;
mod render;
mod structure;

pub use admit::{CompiledFunction, NativeModule, Rejection, compile_program};
