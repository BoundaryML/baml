//! Tests for bytecode generation.
//!
//! These tests verify that the compiler generates correct bytecode
//! for various BAML constructs by compiling BAML source code through
//! the full pipeline.
//!
//! Tests are organized by category:
//! - `arrays` - Array construction
//! - `classes` - Class construction and field operations
//! - `for_loops` - For-in loops
//! - `functions` - Function calls, parameters, and returns
//! - `if_else` - If/else expressions and statements
//! - `operators` - Arithmetic and logical operators
//! - `while_loops` - While loops, break, continue

mod common;

mod arrays;
mod classes;
mod for_loops;
mod functions;
mod if_else;
mod operators;
mod while_loops;
