//! BAML to Rust, running inside the BEX VM.
//!
//! This backend compiles the MIR of a program's functions to Rust functions
//! that the VM calls in place of their bytecode (`Function::native_body`).
//! Generated code holds VM values, allocates through the VM's TLAB, raises the
//! VM's exceptions, and calls the one implementation of every operation the
//! interpreter also runs (`bex_vm::compiled_rt`). It therefore never changes
//! what a program computes; it only removes dispatch, eval-stack traffic, and
//! tag checks on statically typed scalars.
//!
//! A function is *admitted* when every construct in its body has a lowering;
//! everything else stays bytecode in the same program, with identical
//! semantics. An admitted function that can suspend (it awaits, spawns, runs a
//! sys op, or calls bytecode, a function value, a callback-running builtin, or
//! another suspending function) compiles to a resumable state machine the VM
//! drives; the rest run to completion inside the call that enters them. See
//! `DESIGN.txt`.

mod admit;
mod analysis;
mod codegen;
mod render;
mod structure;

pub use admit::{CompiledFunction, NativeModule, Rejection, compile_program};
