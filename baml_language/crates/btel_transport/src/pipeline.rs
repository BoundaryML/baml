//! The stage assembly. No runtime mode enum or trait objects enter this path.
use btel_core::stage::{Aggregator, EventBuilder, MarkerBatch, MarkerRange, Publisher};

use crate::RangeHandler;

pub struct Pipeline<H, B, A, P> {
    range_handler: H,
    builder: B,
    aggregator: A,
    publisher: P,
}

impl<H, B, A, P> Pipeline<H, B, A, P>
where
    H: RangeHandler,
    B: EventBuilder,
    A: Aggregator<B::Event>,
    P: Publisher<B::Event, A::Aggregate>,
{
    pub const fn new(range_handler: H, builder: B, aggregator: A, publisher: P) -> Self {
        Self {
            range_handler,
            builder,
            aggregator,
            publisher,
        }
    }

    /// Only the round-robin drainer may submit committed source ranges.
    pub(crate) fn consume(&mut self, input: MarkerRange<'_>) {
        let Self {
            range_handler,
            builder,
            aggregator,
            publisher,
        } = self;
        range_handler.accept(input, |batch| {
            build_batch(builder, aggregator, publisher, batch);
        });
    }

    /// Called only after the transport has consumed all final source bytes.
    /// Flush stages in dependency order; consume self to prevent double finish.
    pub(crate) fn finish(mut self) -> P {
        let Self {
            range_handler,
            builder,
            aggregator,
            publisher,
        } = &mut self;
        range_handler.finish(|batch| build_batch(builder, aggregator, publisher, batch));
        builder.finish(|event| {
            aggregator.observe(&event, |aggregate| publisher.aggregate(aggregate));
            publisher.event(event);
        });
        aggregator.finish(|aggregate| publisher.aggregate(aggregate));
        publisher.finish();
        self.publisher
    }
}

fn build_batch<B, A, P>(builder: &mut B, aggregator: &mut A, publisher: &mut P, batch: MarkerBatch)
where
    B: EventBuilder,
    A: Aggregator<B::Event>,
    P: Publisher<B::Event, A::Aggregate>,
{
    builder.build(
        MarkerRange {
            source_id: batch.source_id,
            bytes: &batch.bytes,
        },
        |event| {
            aggregator.observe(&event, |aggregate| publisher.aggregate(aggregate));
            publisher.event(event);
        },
    );
    // The driver, not the semantic builder, owns the allocation. Phase A has
    // no shared pool; a future copy/return driver returns this memory there.
    drop(batch);
}
