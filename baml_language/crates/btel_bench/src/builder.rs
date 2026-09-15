//! Benchmark sink: observe completed output without aggregation or storage.
use btel_builder::{TelemetryEvent, TelemetryEventBuilder};
use btel_transport::batch::{BatchProcessor, SharedMarkerBatch};
use serde::Serialize;

#[derive(Default, Debug, Serialize)]
pub struct BuilderReport {
    pub closed_spans: u64,
    pub standalone_events: u64,
    pub unmatched_halves: usize,
}

#[derive(Default)]
pub struct BuildSpans {
    builder: TelemetryEventBuilder,
    report: BuilderReport,
    error: Option<String>,
}

impl BatchProcessor for BuildSpans {
    type Output = Result<BuilderReport, String>;

    fn process(&mut self, batch: &SharedMarkerBatch) {
        // Still return all batches after failure so producers/drainer can
        // finish. This run will fail validation rather than hang or report success.
        if self.error.is_some() {
            return;
        }
        for range in batch.ranges() {
            let result = self.builder.try_build(range, |event| {
                match &event {
                    TelemetryEvent::ClosedSpan(_) => self.report.closed_spans += 1,
                    _ => self.report.standalone_events += 1,
                }
                std::hint::black_box(event);
            });
            if let Err(error) = result {
                self.error = Some(format!("span builder failed: {error:?}"));
                return;
            }
        }
    }

    fn finish(mut self) -> Self::Output {
        if let Some(error) = self.error {
            return Err(error);
        }
        self.report.unmatched_halves = self.builder.open_spans().len();
        Ok(self.report)
    }
}

#[cfg(test)]
mod tests {
    use std::{thread, time::Duration};

    use btel_core::stage::MarkerRange;
    use btel_transport::{
        DrainTarget, TransportConfig,
        batch::{BatchConfig, CopyHandoff},
    };

    use super::*;
    use crate::{
        feeder::ProducerMode,
        replay::{self, ConsumerMode, ReplayConfig, SourceLoad},
        workload::Fixture,
    };

    #[test]
    fn malformed_batches_report_failure_after_recycling_and_shutdown() {
        let (mut sender, receiver) = CopyHandoff::new(BatchConfig {
            payload_bytes: 4,
            source_ranges: 1,
            queue_batches: 1,
            retained_batches: 1,
        })
        .unwrap();
        let worker = thread::spawn(move || receiver.run(BuildSpans::default()));
        for _ in 0..1000 {
            sender.accept(MarkerRange {
                source_id: 1,
                bytes: &[255],
            });
            sender.end_step();
        }
        sender.finish();
        assert!(worker.join().unwrap().unwrap_err().contains("UnknownTag"));
    }

    #[test]
    fn tiny_recycled_batches_build_every_nested_call_in_both_producer_modes() {
        for producer_mode in [ProducerMode::Prepared, ProducerMode::EncodeClock] {
            let fixtures = (1..=3)
                .map(|source| {
                    (
                        Fixture::prepare(source, 4097, 64, None).unwrap(),
                        SourceLoad {
                            bytes_per_second: 0,
                            start_delay_ms: 0,
                            batch_markers: 1024,
                            burst_pause_ms: 0,
                        },
                    )
                })
                .collect();
            let result = replay::run(
                ReplayConfig {
                    consumer_mode: ConsumerMode::BuildSpans,
                    batch: BatchConfig {
                        payload_bytes: 64,
                        source_ranges: 1,
                        queue_batches: 2,
                        retained_batches: 2,
                    },
                    producer_mode,
                    transport: TransportConfig {
                        segment_bytes: 64,
                        freelist_segments: 1,
                        memory_bytes: 32 * 1024 * 1024,
                    },
                    source_budget: 1,
                    idle_rings: 2,
                    idle_timeout: Duration::from_micros(20),
                },
                fixtures,
            )
            .unwrap();
            let report = result.builder.unwrap();
            assert_eq!(report.closed_spans, 3 * 4097);
            assert_eq!(report.standalone_events, 6);
            assert_eq!(report.unmatched_halves, 0);
        }
    }
}
