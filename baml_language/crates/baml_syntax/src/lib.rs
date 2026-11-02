//! The BAML syntax tree.
//!
//! A parsed file is three flat allocations: its text, its tokens (every
//! byte of the file belongs to exactly one token, trivia included), and typed
//! node structs in per-type arenas. A node holds the *positions* of its parts
//! — `Token<K>` for a token it owns, `Id<T>` for a child node, `List<T>` for a
//! sequence — never text and never trivia, so the tree is small, `Send +
//! Sync` plain data, and lossless by construction: the text is always there.
//!
//! Every grammar position is spelled out in a struct's type:
//!
//! - `Token<kw::Function>`, `Id<Name>` — present, or the node would not have
//!   this kind;
//! - `Result<Id<ParamList>, Missing>` — required by the grammar, but the
//!   parser may have had to leave it empty (always with a diagnostic);
//!   [`tree::Missing`] records where it should have been;
//! - `Option<Id<ThrowsClause>>` — grammatically optional;
//! - every `Any*` union has a `Bogus` variant holding a token range, so
//!   recovered garbage keeps its place in the tree without loss.
//!
//! There is no untyped node layer. The struct definitions are the grammar;
//! [`baml_syntax_macros`] derive the walking, the kinds and the arenas from
//! them, and a struct that is not registered — or a registered type that is
//! not a node — fails to compile.
//!
//! The node definitions live in [`nodes`]; operator precedence, shared by
//! the parser and the formatter, in [`ops`]. This crate has no dependency on a
//! parser or on Salsa.

// Lets the derive macros emit `::baml_syntax::…` paths that resolve both from
// other crates and from inside this one.
extern crate self as baml_syntax;

// First: the `Token!` macro it defines is used by the modules below.
#[macro_use]
pub mod token;

pub mod debug;
pub mod nodes;
pub mod ops;
pub mod tree;

#[cfg(test)]
mod tests;

pub use baml_syntax_macros::{AnyNode, Node, Walk, define_tree};
