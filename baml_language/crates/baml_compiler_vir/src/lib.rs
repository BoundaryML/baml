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
mod schema;
mod schema_lower;
mod ty;

pub use expr::*;
pub use lower::{LoweringError, lower_from_hir};
pub use pretty::pretty_print;
pub use schema::*;
pub use ty::*;

/// Query the project schema — classes, enums, and functions with resolved types.
///
/// This is a Salsa tracked query that reads HIR items, TIR resolved types,
/// and HIR attributes to produce a complete VIR schema.
#[salsa::tracked]
pub fn project_schema(db: &dyn Db, project: baml_workspace::Project) -> VirSchema {
    let type_aliases = baml_compiler_tir::type_aliases(db, project)
        .aliases(db)
        .clone();
    let recursive_aliases = baml_compiler_tir::find_recursive_aliases(&type_aliases);
    schema_lower::lower_schema(db, project, &type_aliases, &recursive_aliases)
}

/// Database trait for VIR queries. Extends `baml_compiler_tir::Db`.
#[salsa::db]
pub trait Db: baml_compiler_tir::Db {}
