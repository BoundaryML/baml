//! BAML Virtual Machine bytecode definitions.
//!
//! This crate defines the bytecode instruction set and data structures
//! for the BAML stack-based virtual machine.

mod bytecode;
pub mod debug;
pub mod test;
mod types;

pub use bytecode::*;
pub use types::*;
