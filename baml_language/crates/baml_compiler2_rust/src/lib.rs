//! Compiles a program's MIR to Rust functions that the VM runs in place of
//! their bytecode. A function whose body has no lowering stays bytecode.

mod admit;
mod analysis;
mod codegen;
mod coords;
mod index;
mod init;
mod operands;
mod reach;
mod render;
mod structure;

pub use admit::{CompiledFunction, Rejection, RustModule, compile_program};
