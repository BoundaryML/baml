//! Byte disposition is independent of both ring traversal and semantic stages.
use btel_core::stage::MarkerRange;

pub trait DrainTarget {
    type Output;

    /// Returning accepts the whole range. The source ring may then reuse it;
    /// a retaining target must finish its copy before returning.
    fn accept(&mut self, range: MarkerRange<'_>);

    /// End of one bounded round-robin step. Publish partial batches here so
    /// sparse sources do not wait for a payload buffer to fill.
    fn end_step(&mut self) {}

    /// All producers are gone and their rings have been drained. Deliver any
    /// remaining bytes before closing the downstream input.
    fn finish(self) -> Self::Output;
}
