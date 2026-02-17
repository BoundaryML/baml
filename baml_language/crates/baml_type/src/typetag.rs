//! Type tag constants for BAML runtime type identification.
//!
//! This crate defines global type tag constants used by both the compiler
//! (for generating type-discriminated switch statements) and the VM
//! (for runtime type identification).
//!
//! # Type Tag Assignment
//!
//! - **Primitives** (0-99): Fixed tags for built-in types
//! - **Classes** (100+): Dynamically assigned at compile time as `CLASS_BASE + index`
//!
//! # Usage
//!
//! The `TypeTag` instruction extracts a type identifier from any value,
//! enabling efficient jump table dispatch on union types.

/// Integer type tag.
pub const INT: i64 = 0;

/// String type tag.
pub const STRING: i64 = 1;

/// Boolean type tag.
pub const BOOL: i64 = 2;

/// Null type tag.
pub const NULL: i64 = 3;

/// Float type tag.
pub const FLOAT: i64 = 4;

/// Enum variant type tag (all variants share this).
pub const ENUM: i64 = 5;

/// List/array type tag.
pub const LIST: i64 = 6;

/// Map type tag.
pub const MAP: i64 = 7;

/// Function type tag.
pub const FUNCTION: i64 = 8;

/// Future type tag.
pub const FUTURE: i64 = 9;

/// Media type tag.
pub const MEDIA: i64 = 10;

/// Resource type tag (file handle, socket, etc.).
pub const RESOURCE: i64 = 11;

/// `PromptAst` type tag.
pub const PROMPT_AST: i64 = 12;

/// `Collector` type tag.
pub const COLLECTOR: i64 = 13;

/// `Type` meta-type tag.
pub const TYPE: i64 = 14;

/// Base value for class type tags (classes start at 100).
pub const CLASS_BASE: i64 = 100;

/// Unknown/invalid type tag.
pub const UNKNOWN: i64 = -1;
