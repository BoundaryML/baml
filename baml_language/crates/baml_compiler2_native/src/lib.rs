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
//! A function is *admitted* when nothing in its body needs the VM mid-body:
//! no suspension (await, sys op, spawn), no call whose target is not a
//! statically known admitted function or a non-yielding builtin, no type
//! argument known only at runtime, no captured local. Everything else stays bytecode in the same
//! program, with identical semantics. Admission is a fixpoint over the call
//! graph: a function calling a rejected function is rejected.

mod emit;
mod structure;

pub use emit::{CompiledFunction, NativeModule, Rejection, compile_program};
