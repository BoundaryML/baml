//! Semantic stages are generic over event/output types, not tied to a future schema.

/// A committed source range, borrowed only for the duration of a stage call.
#[derive(Clone, Copy, Debug)]
pub struct MarkerRange<'a> {
    pub source_id: u64,
    pub bytes: &'a [u8],
}

/// Owned handoff shape. Phase A's discard drainer never creates a batch.
/// The later transport driver can recycle this allocation after borrowing it.
#[derive(Debug)]
pub struct MarkerBatch {
    pub source_id: u64,
    pub bytes: Vec<u8>,
}

pub trait EventBuilder {
    type Event;

    fn build(&mut self, input: MarkerRange<'_>, emit: impl FnMut(Self::Event));

    fn finish(&mut self, _emit: impl FnMut(Self::Event)) {}
}

pub trait Aggregator<E> {
    type Aggregate;

    fn observe(&mut self, event: &E, emit: impl FnMut(Self::Aggregate));

    fn finish(&mut self, _emit: impl FnMut(Self::Aggregate)) {}
}

/// Events and aggregates are separate branches: either may be published.
pub trait Publisher<E, A> {
    fn event(&mut self, event: E);
    fn aggregate(&mut self, aggregate: A);
    fn finish(&mut self) {}
}
