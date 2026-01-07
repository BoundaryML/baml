//! Validated Intermediate Representation (VIR) for BAML.
//!
//! # What "Validated" Means
//!
//! VIR represents **valid, complete programs** ready for code generation. Unlike
//! earlier IRs (HIR, TIR) which preserve error nodes for LSP error recovery,
//! VIR guarantees:
//!
//! - **No Missing nodes** - All syntax holes have been rejected
//! - **No Unknown types** - All expressions have concrete, resolved types
//! - **No unresolved references** - All names/paths are validated
//!
//! Lowering from HIR to VIR is **fallible**. Programs with errors cannot be
//! represented in VIR - they fail at the lowering boundary.
//!
//! # Comparison with Other IRs
//!
//! | IR  | Error Nodes            | Use Case                          |
//! |-----|------------------------|-----------------------------------|
//! | HIR | Yes (Missing variants) | LSP features, incremental parsing |
//! | TIR | Yes (Unknown types)    | Type inference, diagnostics       |
//! | VIR | **No**                 | Code generation, optimization     |
//!
//! # Key Design Principles
//!
//! 1. **Everything is an expression** - `let`, `if`, `while`, `assign` all return values
//! 2. **Explicit scoping** - `Let(name, ty, value, body)` binds in `body` and returns body's value
//! 3. **Explicit sequencing** - `Seq(first, second)` evaluates both, returns second
//! 4. **Unit type for effects** - `while`, `assign`, `break` return `Unit`
//!
//! # Example Transformation
//!
//! HIR/TIR has:
//! ```text
//! Block { stmts: Vec<StmtId>, tail_expr: Option<ExprId> }
//! Stmt::Let { pattern, initializer }
//! Stmt::While { condition, body }
//! Stmt::Expr(ExprId)
//! Expr::Missing, Stmt::Missing  // For error recovery
//! ```
//!
//! VIR has:
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

/// Database trait for VIR queries. Extends `baml_tir::Db`.
#[salsa::db]
pub trait Db: baml_tir::Db {}
