mod builder;
mod cleanup;
mod ir;
mod lower;
pub mod pretty;

pub use ir::*;
pub use lower::lower_function;

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_tir::Db {}
