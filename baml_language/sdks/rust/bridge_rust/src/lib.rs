// reexports
pub use indexmap::IndexMap;
// pub use num_bigint::BitInt;

mod baml_value;
mod runtime;

pub fn get_version() -> &'static str {
    baml_version::CANONICAL_VERSION
}
