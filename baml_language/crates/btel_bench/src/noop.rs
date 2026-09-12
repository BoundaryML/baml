//! No-op range handling and semantic stages; the concrete Drainer still polls rings.
use btel_core::stage::{Aggregator, EventBuilder, MarkerBatch, MarkerRange, Publisher};
use btel_transport::{Pipeline, RangeHandler};

#[derive(Default)]
pub struct DiscardRanges;
impl RangeHandler for DiscardRanges {
    fn accept(&mut self, _input: MarkerRange<'_>, _emit: impl FnMut(MarkerBatch)) {}
}

#[derive(Default)]
pub struct NoopEventBuilder;
impl EventBuilder for NoopEventBuilder {
    type Event = ();
    fn build(&mut self, _input: MarkerRange<'_>, _emit: impl FnMut(())) {}
}

#[derive(Default)]
pub struct NoopAggregator;
impl Aggregator<()> for NoopAggregator {
    type Aggregate = ();
    fn observe(&mut self, _event: &(), _emit: impl FnMut(())) {}
}

#[derive(Default)]
pub struct NoopPublisher;
impl Publisher<(), ()> for NoopPublisher {
    fn event(&mut self, (): ()) {}
    fn aggregate(&mut self, (): ()) {}
}

pub type NoopPipeline = Pipeline<DiscardRanges, NoopEventBuilder, NoopAggregator, NoopPublisher>;

pub fn pipeline() -> NoopPipeline {
    Pipeline::new(
        DiscardRanges,
        NoopEventBuilder,
        NoopAggregator,
        NoopPublisher,
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn all_noops_accept_input_without_emitting_semantic_work() {
        let range = MarkerRange {
            source_id: 1,
            bytes: &[1, 2, 3],
        };
        DiscardRanges.accept(range, |_| panic!("discard must not manufacture a batch"));
        NoopEventBuilder.build(range, |()| panic!("no-op builder emitted"));
        NoopAggregator.observe(&(), |()| panic!("no-op aggregator emitted"));
        let (factory, mut drainer) =
            btel_transport::transport(btel_transport::TransportConfig::default()).unwrap();
        let mut producer = factory.producer(1).unwrap();
        assert!(producer.write(range.bytes));
        let mut pipeline = pipeline();
        assert!(drainer.drain(&mut pipeline, 1));
        assert!(!drainer.drain(&mut pipeline, 1));
        drop(producer);
        drainer.finish(pipeline).unwrap();
    }
}
