mod builder;
mod inference;
mod ir;
mod lower;
mod optimize;
pub mod pretty;

pub use baml_type::ResolvedAliases;
pub use ir::*;
pub use lower::{
    def_to_item_ref, function_is_interface_body, lower_function, lower_let_body, native_key_for,
    resolved_aliases_for_package, tir2_to_template,
};

/// Database trait for compiler2 MIR queries.
#[salsa::db]
pub trait Db: baml_compiler2_ppir::Db {}
