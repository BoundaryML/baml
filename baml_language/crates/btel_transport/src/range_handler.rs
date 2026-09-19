//! Pluggable disposition of bytes selected by the concrete round-robin Drainer.
//! Implementations cannot choose rings, move the traversal cursor, or reclaim slots.
use btel_core::stage::{MarkerBatch, MarkerRange};

pub trait RangeHandler {
    /// On return the source range has been accepted; its ring may reclaim it.
    /// A retaining implementation must own its copy before returning.
    fn accept(&mut self, input: MarkerRange<'_>, emit: impl FnMut(MarkerBatch));

    fn finish(&mut self, _emit: impl FnMut(MarkerBatch)) {}
}
