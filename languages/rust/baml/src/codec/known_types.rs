//! Re-export `KnownTypes` from crate root.
//!
//! The actual trait definition is in `crate::known_types` to avoid
//! circular dependencies between error.rs and codec/.

pub use crate::known_types::KnownTypes;
