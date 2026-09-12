//! Experimental producer rings and reusable batch transport.
//! Start with `runtime.rs` and `drain_target.rs`, then `batch.rs`.
//! `pipeline.rs` retains the synchronous semantic-stage adapter.
#[cfg(not(baml_loom))]
pub mod batch;
#[cfg(not(baml_loom))]
mod drain_target;
#[cfg(not(baml_loom))]
pub use drain_target::DrainTarget;
#[allow(
    dead_code,
    reason = "preserve copied governor behavior for later experiments"
)]
mod memory;
#[cfg(not(baml_loom))]
pub mod pipeline;
#[cfg(not(baml_loom))]
pub mod range_handler;
mod registry;
mod ring;
#[cfg(not(baml_loom))]
pub mod runtime;
#[allow(
    dead_code,
    reason = "preserve copied sizing policy for baseline comparisons"
)]
mod sizing;
mod sync;
mod wake;
#[cfg(test)]
use btel_core::ids;
#[cfg(not(baml_loom))]
pub use pipeline::Pipeline;
#[cfg(not(baml_loom))]
pub use range_handler::RangeHandler;
#[cfg(not(baml_loom))]
pub use runtime::{Drainer, Producer, SourceFactory, TransportConfig, TransportError, transport};
// Only the copied tests use this facade; production and benchmarks use Btel APIs.
#[cfg(test)]
mod prof {
    pub(crate) use btel_core::marker as record;

    pub(crate) use crate::{registry, ring, sync};
}
#[cfg(test)]
mod concurrency_tests;
