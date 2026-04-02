mod builder;
mod ir;
mod lower;
mod optimize;
pub mod pretty;

pub use ir::*;
pub use lower::{
    convert_tir2_ty, def_to_item_ref, lower_function, lower_let_body, qtn_to_type_name,
};

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_tir::Db {}
