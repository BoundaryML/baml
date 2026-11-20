//! Native cloud publisher and bounded delivery for Btel recordings.
//!
//! The prepare protocol and the upload envelope are `btel-core`'s (`btel_core::upload`), the
//! contract BCS defines and serves.

#![cfg(not(target_arch = "wasm32"))]

mod body;
pub mod delivery;
pub mod liveness;
mod metadata;
mod plan;
pub mod publisher;
pub use publisher::{CloudPublisher, CloudPublisherConfig};
#[cfg(feature = "diagnostics")]
pub use publisher::{PublisherDiagnostics, PublisherGauges};
