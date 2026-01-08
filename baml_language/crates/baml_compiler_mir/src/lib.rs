//! Mid-level Intermediate Representation (MIR) for BAML.
//!
//! MIR is a Control Flow Graph (CFG) based representation that sits between
//! VIR (validated IR) and bytecode generation. It simplifies the compilation
//! of complex control flow constructs like match statements and error handling.
//!
//! # Architecture
//!
//! MIR represents functions as a graph of basic blocks:
//!
//! - **Basic Blocks**: Sequences of straight-line statements ending with a terminator
//! - **Terminators**: Instructions that transfer control (goto, branch, switch, return, call)
//! - **Statements**: Non-control-flow operations (assign, drop)
//! - **Places**: Memory locations (locals, fields, indices)
//! - **Rvalues**: Value computations (operations, aggregates, constants)
//!
//! # Example
//!
//! ```text
//! fn example(x: int) -> string {
//!     let _0: string;
//!     let _1: int = x;
//!     let _2: bool;
//!
//!     bb0: {
//!         _2 = _1 > const 0;
//!         branch _2 -> bb1, bb2;
//!     }
//!
//!     bb1: {
//!         _0 = const "positive";
//!         goto -> bb3;
//!     }
//!
//!     bb2: {
//!         _0 = const "non-positive";
//!         goto -> bb3;
//!     }
//!
//!     bb3: {
//!         return;
//!     }
//! }
//! ```

mod builder;
mod ir;
mod lower;
pub mod pretty;

pub use builder::MirBuilder;
pub use ir::*;
pub use lower::lower;

// ============================================================================
// Database Trait
// ============================================================================

/// Database trait for MIR queries. Extends `baml_compiler_tir::Db`.
#[salsa::db]
pub trait Db: baml_compiler_tir::Db {}
