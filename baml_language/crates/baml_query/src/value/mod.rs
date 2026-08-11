//! The virtual BAML value surface (D7): canonical semantics for path
//! navigation, comparison, and rendering, plus the resolver contract and
//! the DataFusion lowering that turns ordinary SQL operators/subscripts
//! into these internal operations.
//!
//! The value model is `bex_events::store::canon::CanonValue` — this crate
//! never invents a second codec or CID space.

pub mod lowering;
pub mod resolver;
pub mod semantics;

pub use resolver::{Resolved, ValueResolver};
pub use semantics::{CmpOp, Nav, PathSeg};
