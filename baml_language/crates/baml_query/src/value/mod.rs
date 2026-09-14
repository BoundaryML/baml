//! The neutral value model and its SQL-facing semantics.

pub mod lowering;
pub mod model;
pub mod resolver;
pub mod semantics;

pub use model::{MediaContent, Presence, Value};
pub use resolver::{DecodeCaps, HydrationContext, Resolved, ValueResolver};
