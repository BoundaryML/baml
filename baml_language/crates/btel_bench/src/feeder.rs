//! Producer policy is selected once; replay monomorphizes each loop.
use btel_core::marker::Marker;
use clap::ValueEnum;
use serde::Serialize;

#[derive(Clone, Copy, Debug, ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ProducerMode {
    Prepared,
    EncodeClock,
    /// Traverse the live-encoding inputs without clock, encoding or transport work.
    FeederOnly,
}

/// Change only timestamp fields; the fixture still defines exact counts/bytes.
#[inline]
pub fn stamp(marker: &mut Marker<'_>, ticks: u64) {
    let (Marker::FunctionEnter { ts_ticks, .. }
    | Marker::FunctionExit { ts_ticks, .. }
    | Marker::FunctionExitAwaited { ts_ticks, .. }
    | Marker::BexThreadStart { ts_ticks, .. }
    | Marker::BexThreadStartSpawned { ts_ticks, .. }
    | Marker::BexThreadEnd { ts_ticks, .. }
    | Marker::SetBoundaryLocalId { ts_ticks, .. }) = marker;
    *ts_ticks = ticks;
}

/// Keep the successful path to one attempt. A rejected attempt has committed
/// no bytes, so retry the same marker without reading another timestamp.
#[inline]
pub(crate) fn write_or_wait(
    factory: &btel_transport::SourceFactory,
    mut write: impl FnMut() -> bool,
) {
    if !write() {
        wait_for_capacity(factory, write);
    }
}

#[cold]
#[inline(never)]
fn wait_for_capacity(factory: &btel_transport::SourceFactory, mut write: impl FnMut() -> bool) {
    loop {
        factory.wake_consumer();
        for _ in 0..16 {
            std::hint::spin_loop();
            if write() {
                return;
            }
        }
        // The drainer needs CPU time to reclaim capacity. Never spin forever
        // without yielding, especially when source threads outnumber cores.
        std::thread::yield_now();
    }
}

/// Share exactly the same producer loop across consumer configurations.
/// This single call is outside the per-marker loop; preventing inlining avoids
/// consumer-type specialization changing producer code/register allocation.
#[inline(never)]
pub(crate) fn replay_source<const ENCODE: bool, const FEEDER_ONLY: bool>(
    producer: &mut btel_transport::Producer,
    factory: &btel_transport::SourceFactory,
    fixture: &crate::workload::Fixture,
    templates: &[Marker<'static>],
    load: crate::replay::SourceLoad,
    start: std::time::Instant,
) -> Result<(), String> {
    use std::{
        thread,
        time::{Duration, Instant},
    };

    use btel_core::clock;
    let mut offset = 0usize;
    let delay = Duration::from_millis(load.start_delay_ms);
    for (batch_index, batch) in fixture.lengths.chunks(load.batch_markers).enumerate() {
        let paced_ns = if load.bytes_per_second == 0 {
            0
        } else {
            offset as u128 * 1_000_000_000 / u128::from(load.bytes_per_second)
        };
        let pauses = u128::from(load.burst_pause_ms) * batch_index as u128 * 1_000_000;
        let nanos = u64::try_from(paced_ns + pauses).map_err(|_| "pacing duration overflow")?;
        let deadline = start
            .checked_add(delay)
            .and_then(|t| t.checked_add(Duration::from_nanos(nanos)))
            .ok_or("pacing deadline overflow")?;
        // Pacing clocks are per batch; EncodeClock also stamps each marker.
        if load.bytes_per_second != 0 || load.start_delay_ms != 0 || load.burst_pause_ms != 0 {
            if let Some(wait) = deadline.checked_duration_since(Instant::now()) {
                thread::sleep(wait);
            }
        }
        for (within_batch, &len) in batch.iter().enumerate() {
            let end = offset + usize::from(len);
            if ENCODE {
                let mut marker = templates[batch_index * load.batch_markers + within_batch];
                if FEEDER_ONLY {
                    // Expose the local copy's contents through a
                    // reference; passing the value caused an extra
                    // stack copy. Keep offset work observable too.
                    // Barriers still make subtraction an estimate.
                    std::hint::black_box(&marker);
                    std::hint::black_box(end);
                } else {
                    stamp(&mut marker, clock::now_ticks());
                    write_or_wait(factory, || producer.write_marker(&marker));
                }
            } else {
                write_or_wait(factory, || producer.write(&fixture.bytes[offset..end]));
            }
            offset = end;
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::{cell::RefCell, rc::Rc};

    use btel_core::{
        clock, marker,
        stage::{MarkerBatch, MarkerRange},
    };
    use btel_transport::{Pipeline, RangeHandler, TransportConfig, transport};

    use super::stamp;
    use crate::{
        noop::{NoopAggregator, NoopEventBuilder, NoopPublisher},
        workload::{Fixture, for_each_template},
    };

    #[test]
    fn clocked_direct_encoding_matches_the_wire_codec_through_recycling() {
        struct Capture(Rc<RefCell<Vec<u8>>>);
        impl RangeHandler for Capture {
            fn accept(&mut self, range: MarkerRange<'_>, _: impl FnMut(MarkerBatch)) {
                self.0.borrow_mut().extend_from_slice(range.bytes);
            }
        }
        let fixture = Fixture::prepare(7, 65, 8, None).unwrap();
        let (factory, mut drainer) = transport(TransportConfig {
            segment_bytes: 64,
            ..TransportConfig::default()
        })
        .unwrap();
        let mut producer = factory.producer(7).unwrap();
        let captured = Rc::new(RefCell::new(Vec::new()));
        let mut pipeline = Pipeline::new(
            Capture(captured.clone()),
            NoopEventBuilder,
            NoopAggregator,
            NoopPublisher,
        );
        let mut expected = Vec::new();
        clock::init();
        let before = clock::now_ticks();
        for_each_template(&fixture.manifest, |mut record| {
            let ticks = clock::now_ticks();
            assert!(ticks >= before);
            stamp(&mut record, ticks);
            assert!(producer.write_marker(&record));
            let mut scratch = [0; marker::MAX_RECORD_LEN];
            let len = record.encode(&mut scratch);
            expected.extend_from_slice(&scratch[..len]);
            drainer.drain(&mut pipeline, 1);
        });
        drop(producer);
        drainer.finish(pipeline).unwrap();
        assert_eq!(*captured.borrow(), expected);
        assert_eq!(expected.len() as u64, fixture.manifest.bytes);
        assert_eq!(
            marker::iter(&expected).count() as u64,
            fixture.manifest.markers
        );
    }

    #[test]
    fn rejected_write_waits_for_draining_then_preserves_every_record() {
        struct Sequences(Rc<RefCell<Vec<u64>>>);
        impl RangeHandler for Sequences {
            fn accept(&mut self, range: MarkerRange<'_>, _: impl FnMut(MarkerBatch)) {
                let (records, remainder) = range.bytes.as_chunks::<4096>();
                assert!(remainder.is_empty());
                for record in records {
                    self.0
                        .borrow_mut()
                        .push(u64::from_le_bytes(record[..8].try_into().unwrap()));
                }
            }
        }
        let config = TransportConfig {
            segment_bytes: 4096,
            memory_bytes: 16 * 1024 * 1024,
            ..TransportConfig::default()
        };
        config.check_retry_capacity(1).unwrap();
        let (factory, mut drainer) = transport(config).unwrap();
        drainer.register_worker();
        let (rejected_tx, rejected_rx) = std::sync::mpsc::channel();
        let writer = std::thread::spawn(move || {
            let mut producer = factory.producer(1).unwrap();
            let mut reported_rejection = false;
            let mut record = [0; 4096];
            for seq in 0u64..5000 {
                record[..8].copy_from_slice(&seq.to_le_bytes());
                super::write_or_wait(&factory, || {
                    let accepted = producer.write(&record);
                    if !accepted && !reported_rejection {
                        reported_rejection = true;
                        rejected_tx.send(()).unwrap();
                    }
                    accepted
                });
            }
        });
        // Deliberately withhold drainage until the real memory limit is hit.
        rejected_rx
            .recv_timeout(std::time::Duration::from_secs(5))
            .unwrap();
        let observed = Rc::new(RefCell::new(Vec::new()));
        let mut pipeline = Pipeline::new(
            Sequences(observed.clone()),
            NoopEventBuilder,
            NoopAggregator,
            NoopPublisher,
        );
        while !writer.is_finished() {
            drainer.drain(&mut pipeline, 1);
            std::thread::yield_now();
        }
        writer.join().unwrap();
        drainer.finish(pipeline).unwrap();
        assert_eq!(*observed.borrow(), (0..5000).collect::<Vec<_>>());
    }

    #[test]
    fn retry_preflight_rejects_a_budget_that_rings_can_permanently_retain() {
        let config = TransportConfig {
            memory_bytes: 16 * 1024 * 1024,
            ..TransportConfig::default()
        };
        config.check_retry_capacity(4).unwrap();
        assert_eq!(
            config.check_retry_capacity(16),
            Err(btel_transport::TransportError::InsufficientRetryCapacity)
        );
    }
}
