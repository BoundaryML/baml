//! Type signatures for BAML built-in functions.
//!
//! This crate provides compile-time type information for built-in functions,
//! used by the type checker (`baml_compiler_tir`). It does NOT include
//! runtime implementations - those live in `bex_vm`.
//!
//! This separation allows the type checker to avoid depending on the VM.
//!
//! # Adding a new builtin
//!
//! Add a new entry in the `define_builtins!` macro invocation below.
//! This generates both the path constant and the signature in one place.

mod adt;

pub use adt::*;
