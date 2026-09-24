use std::{sync::Arc, time::Instant};

use btel_processor::{AggregateDelta, Publisher};
use btel_recorder::{RecordingBuilder, SealedFile};
use btel_records::SpanRecord;
use btel_snapshot::Snapshot;
use btel_types::TelemetryId;

use crate::{LocalDelivery, LocalDeliveryHandle};

/// Local encoding and delivery endpoint. Keep a clone of the delivery worker and
/// call `LocalDelivery::finish` only after the processor finishes this publisher.
pub struct LocalPublisher {
    builder: RecordingBuilder,
    delivery: Option<Delivery>,
}

struct Delivery {
    handle: LocalDeliveryHandle,
    worker: Arc<LocalDelivery>,
}

impl LocalPublisher {
    pub fn new(builder: RecordingBuilder, mut delivery: LocalDelivery) -> Self {
        Self {
            builder,
            delivery: Some(Delivery {
                handle: delivery.take_handle(),
                worker: Arc::new(delivery),
            }),
        }
    }

    /// Used only when startup has already disabled the recording control.
    pub fn disabled(builder: RecordingBuilder) -> Self {
        Self {
            builder,
            delivery: None,
        }
    }

    pub fn delivery(&self) -> Option<&Arc<LocalDelivery>> {
        self.delivery.as_ref().map(|delivery| &delivery.worker)
    }

    fn snapshots(&mut self) {
        for snapshot in self.builder.take_snapshots() {
            if let Some(delivery) = &self.delivery {
                terminal(delivery.handle.send_snapshot(snapshot));
            }
        }
    }

    fn deliver(&self, file: Option<SealedFile>) {
        if let (Some(delivery), Some(file)) = (&self.delivery, file) {
            terminal(delivery.handle.send(file));
        }
    }
}

fn terminal<T, E: Send + 'static>(result: Result<T, E>) -> T {
    result.unwrap_or_else(|error| std::panic::panic_any(error))
}

impl Publisher<Snapshot, Snapshot> for LocalPublisher {
    fn aggregate(&mut self, delta: AggregateDelta) {
        self.builder.aggregate(delta);
    }

    fn span(&mut self, thread: TelemetryId, record: &mut SpanRecord<Snapshot, Snapshot>) {
        self.builder.span(thread, record);
    }

    fn before_batch(&mut self, records: usize) {
        self.builder.before_batch(records);
    }

    fn before_span_chunk(&mut self, records: usize) {
        self.builder.before_span_chunk(records);
    }

    fn after_batch(&mut self, records: usize) {
        self.snapshots();
        let file = terminal(self.builder.after_batch(records));
        self.deliver(file);
    }

    fn max_chunks_per_batch(&self) -> usize {
        btel_settings::publisher::MAX_BATCH_CHUNKS
    }

    fn manages_flush_deadline(&self) -> bool {
        true
    }

    fn deadline(&self) -> Option<Instant> {
        self.builder.deadline()
    }

    fn flush(&mut self) {
        self.snapshots();
        let file = terminal(self.builder.finish_recording());
        self.deliver(file);
    }

    fn finish(&mut self) {
        self.flush();
    }
}
