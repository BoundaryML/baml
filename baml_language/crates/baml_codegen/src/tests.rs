//! Tests for bytecode generation.
//!
//! These tests verify that the compiler generates correct bytecode
//! for various BAML constructs by compiling BAML source code through
//! the full pipeline.
//!
//! Tests are organized by category:
//! - `arrays` - Array construction
//! - `assertions` - Assert statements
//! - `builtins` - Built-in method calls
//! - `classes` - Class construction and field operations
//! - `enums` - Enum variants
//! - `for_loops` - For-in loops
//! - `functions` - Function calls, parameters, and returns
//! - `if_else` - If/else expressions and statements
//! - `maps` - Map operations
//! - `operators` - Arithmetic and logical operators
//! - `scopes` - Local variable scoping
//! - `watch` - Watch functionality
//! - `while_loops` - While loops, break, continue

mod common;

mod arrays;
mod assertions;
mod builtins;
mod classes;
mod enums;
mod for_loops;
mod functions;
mod if_else;
mod maps;
mod operators;
mod scopes;
mod watch;
mod while_loops;
