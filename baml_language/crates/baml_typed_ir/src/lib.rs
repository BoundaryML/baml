//! Typed Intermediate Representation for BAML.
//!
//! This crate provides a unified expression-based IR where **everything is an expression**.
//! Unlike the HIR/THIR which distinguish between statements and expressions with awkward
//! "tail expression" handling, this IR treats all constructs uniformly.
//!
//! # Key Design Principles
//!
//! 1. **Everything is an expression** - `let`, `if`, `while`, `assign` all return values
//! 2. **Explicit scoping** - `Let(name, ty, value, body)` binds in `body` and returns body's value
//! 3. **Explicit sequencing** - `Seq(first, second)` evaluates both, returns second
//! 4. **Unit type for effects** - `while`, `assign`, `break` return `Unit`
//!
//! # No Missing Nodes
//!
//! Unlike HIR which has `Missing` variants for LSP error recovery, `TypedIR`
//! represents only **valid, complete programs**. Lowering from HIR is fallible
//! and will return an error if any `Missing` nodes are encountered.
//!
//! # Comparison with HIR/THIR
//!
//! HIR/THIR has:
//! ```text
//! Block { stmts: Vec<StmtId>, tail_expr: Option<ExprId> }
//! Stmt::Let { pattern, initializer }
//! Stmt::While { condition, body }
//! Stmt::Expr(ExprId)
//! Expr::Missing, Stmt::Missing  // For error recovery
//! ```
//!
//! `TypedIR` has:
//! ```text
//! Let { pattern, ty, value, body }  // Returns body's value
//! Seq { first, second }             // Returns second's value
//! While { condition, body }         // Returns Unit
//! // No Missing - lowering fails if source has errors
//! ```
//!
//! This eliminates the need for separate statement handling and makes traversals uniform.

mod expr;
mod lower;
mod pretty;
mod ty;

pub use expr::*;
pub use lower::{LoweringError, lower_from_hir};
pub use pretty::pretty_print;
pub use ty::*;

/// Database trait for `TypedIR` queries.
///
/// Extends THIR's database since we need `InferenceResult` during lowering.
#[salsa::db]
pub trait Db: baml_thir::Db {}
