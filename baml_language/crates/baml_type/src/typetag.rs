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
//!
//! # Limitations: Class Type Tags and Jump Tables
//!
//! Class type tags are **globally assigned** in declaration order. When a
//! `match` operates on a union of a few classes out of many, the tags may be
//! sparse across the range. The emitter's jump table strategy requires ≥50%
//! density, so in projects with many classes it will typically fall back to
//! sequential `instanceof` chains — making the jump table path for class
//! matching effectively dead code.
//!
//! This matches how other languages handle it: Rust/Haskell use dense
//! per-enum discriminants for ADTs (always jump-table friendly), while
//! Java/Kotlin/C# always use `instanceof` chains for class type matching.
//!
//! A proper fix would be **tagged union wrapping**: treat `Cat | Dog | Bird`
//! as a runtime type that wraps its payload with a union-local discriminant
//! `0..N-1`, assigned at the point a value enters the union. This would make
//! class matching O(1) via dense jump tables, but requires the runtime to
//! distinguish union types from their constituents.

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
