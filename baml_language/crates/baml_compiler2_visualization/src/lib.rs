//! Visualization support for the BAML compiler2 pipeline.
//!
//! Provides control flow graph building directly from the compiler2 AST,
//! bypassing type inference and VIR lowering. This means the CFG survives
//! parse and type errors — `Expr::Missing` / `Stmt::Missing` sentinels
//! produce a valid (if incomplete) graph rather than blocking the entire
//! visualization.

pub mod control_flow;
