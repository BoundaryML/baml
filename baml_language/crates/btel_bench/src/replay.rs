//! Concurrent replay. Source setup precedes the start gate; shutdown drains to completion.
use std::{
    sync::{
        Arc, Condvar, Mutex,
        atomic::{AtomicBool, Ordering},
        mpsc,
    },
    thread,
    time::{Duration, Instant},
};

use btel_core::{clock, marker::Marker};
use btel_transport::{
    DrainTarget, TransportConfig,
    batch::{BatchConfig, BatchReceiver, CopyHandoff, CopyLocal},
    transport,
};
use serde::Serialize;

use crate::{
    feeder::ProducerMode,
    noop,
    workload::{Fixture, Manifest},
};

#[derive(Clone, Copy, Debug, clap::ValueEnum, Serialize)]
#[serde(rename_all = "kebab-case")]
pub enum ConsumerMode {
    Discard,
    CopyLocal,
    CopyHandoff,
}

#[derive(Clone, Copy, Debug, Serialize)]
pub struct SourceLoad {
    pub bytes_per_second: u64,
    pub start_delay_ms: u64,
    pub batch_markers: usize,
    pub burst_pause_ms: u64,
}

#[derive(Clone, Copy)]
pub struct ReplayConfig {
    pub consumer_mode: ConsumerMode,
    pub batch: BatchConfig,
    pub producer_mode: ProducerMode,
    pub transport: TransportConfig,
    pub source_budget: usize,
    pub idle_rings: usize,
    pub idle_timeout: Duration,
}

#[derive(Serialize)]
pub struct ReplayResult {
    pub consumer_mode: ConsumerMode,
    pub downstream_threads: usize,
    pub batch_payload_bytes: usize,
    pub batch_source_ranges: usize,
    pub batch_queue_capacity: usize,
    pub batch_retained_capacity: usize,
    pub producer_mode: ProducerMode,
    pub clock_kind: Option<String>,
    pub clock_reads: u64,
    pub preset: &'static str,
    pub full_ring_policy: &'static str,
    pub pipeline: &'static str,
    pub sources: Vec<Manifest>,
    pub loads: Vec<SourceLoad>,
    pub source_threads: usize,
    pub consumer_threads: usize,
    pub idle_rings: usize,
    pub input_bytes: u64,
    pub markers: u64,
    /// Actual ring traffic; zero for the feeder-only baseline. Input totals
    /// above describe the workload even when it never reaches the ring.
    pub ring_bytes: u64,
    pub ring_markers: u64,
    pub replay_seconds: f64,
    pub producer_seconds: f64,
    pub final_drain_seconds: f64,
    pub bytes_per_second: Option<f64>,
    pub replay_cpu_seconds: Option<f64>,
    pub peak_rss_bytes_including_preparation: Option<u64>,
    pub segment_bytes: usize,
    pub freelist_segments: usize,
    pub memory_budget_bytes: u64,
    pub source_budget: usize,
    pub idle_timeout_micros: u128,
}

#[derive(Clone, Copy)]
enum Start {
    Pending,
    Run(Instant),
    Abort,
}
struct Gate {
    state: Mutex<Start>,
    wake: Condvar,
}
impl Gate {
    fn new() -> Self {
        Self {
            state: Mutex::new(Start::Pending),
            wake: Condvar::new(),
        }
    }
    fn release(&self, start: Start) {
        *self.state.lock().unwrap() = start;
        self.wake.notify_all();
    }
    fn wait(&self) -> Option<Instant> {
        let mut state = self.state.lock().unwrap();
        loop {
            match *state {
                Start::Run(at) => return Some(at),
                Start::Abort => return None,
                Start::Pending => state = self.wake.wait(state).unwrap(),
            }
        }
    }
}

pub fn run(
    config: ReplayConfig,
    fixtures: Vec<(Fixture, SourceLoad)>,
) -> Result<ReplayResult, String> {
    if matches!(config.producer_mode, ProducerMode::FeederOnly) {
        return run_target(config, fixtures, noop::DiscardRanges, None);
    }
    match config.consumer_mode {
        ConsumerMode::Discard => run_target(config, fixtures, noop::DiscardRanges, None),
        ConsumerMode::CopyLocal => run_target(
            config,
            fixtures,
            CopyLocal::new(config.batch, noop::ReturnBatches)?,
            None,
        ),
        ConsumerMode::CopyHandoff => {
            let (target, receiver) = CopyHandoff::new(config.batch)?;
            run_target(config, fixtures, target, Some(receiver))
        }
    }
}

fn run_target<T: DrainTarget<Output = ()> + Send + 'static>(
    config: ReplayConfig,
    fixtures: Vec<(Fixture, SourceLoad)>,
    target: T,
    receiver: Option<BatchReceiver>,
) -> Result<ReplayResult, String> {
    match config.producer_mode {
        ProducerMode::Prepared => run_with::<false, false, T>(config, fixtures, target, receiver),
        ProducerMode::EncodeClock => run_with::<true, false, T>(config, fixtures, target, receiver),
        ProducerMode::FeederOnly => run_with::<true, true, T>(config, fixtures, target, receiver),
    }
}

fn run_with<
    const ENCODE: bool,
    const FEEDER_ONLY: bool,
    T: DrainTarget<Output = ()> + Send + 'static,
>(
    config: ReplayConfig,
    fixtures: Vec<(Fixture, SourceLoad)>,
    target: T,
    batch_receiver: Option<BatchReceiver>,
) -> Result<ReplayResult, String> {
    if ENCODE && !FEEDER_ONLY {
        clock::init();
    }
    if fixtures.is_empty() || config.source_budget == 0 {
        return Err("sources and source budget must be positive".into());
    }
    if fixtures.iter().any(|(_, load)| load.batch_markers == 0) {
        return Err("batch markers must be positive".into());
    }
    if fixtures.iter().any(|(f, _)| {
        f.lengths
            .iter()
            .any(|len| usize::from(*len) > config.transport.segment_bytes)
    }) {
        return Err("segment cannot hold the largest fixture marker".into());
    }
    config
        .transport
        .check_retry_capacity(
            fixtures
                .len()
                .checked_add(config.idle_rings)
                .ok_or("source count overflow")?,
        )
        .map_err(|e| {
            format!("retry configuration: {e}; increase memory or reduce sources/freelist")
        })?;
    let sources: Vec<_> = fixtures.iter().map(|(f, _)| f.manifest.clone()).collect();
    let loads: Vec<_> = fixtures.iter().map(|(_, load)| *load).collect();
    let input_bytes = sources
        .iter()
        .try_fold(0u64, |n, s| n.checked_add(s.bytes))
        .ok_or("total bytes overflow")?;
    let markers = sources
        .iter()
        .try_fold(0u64, |n, s| n.checked_add(s.markers))
        .ok_or("total markers overflow")?;
    let (factory, mut drainer) = transport(config.transport).map_err(|e| e.to_string())?;
    // Keep idle sources registered and Active during replay, on their owner (main) thread.
    let mut idle = Vec::new();
    for i in 0..config.idle_rings {
        idle.push(
            factory
                .producer(u64::MAX - i as u64)
                .map_err(|e| e.to_string())?,
        );
    }
    let gate = Arc::new(Gate::new());
    let stopped = Arc::new(AtomicBool::new(false));
    let (ready_tx, ready_rx) = mpsc::channel();
    let downstream = batch_receiver.map(|receiver| {
        let gate = gate.clone();
        let ready = ready_tx.clone();
        thread::spawn(move || {
            ready.send(Ok::<_, String>(())).unwrap();
            if gate.wait().is_some() {
                receiver.run(noop::ReturnBatches);
            }
            Instant::now()
        })
    });
    // A feeder baseline has the same producer setup, but no drainer competing
    // for CPU. Const specialization removes this choice from the marker loop.
    let receiver = if FEEDER_ONLY {
        None
    } else {
        let gate = gate.clone();
        let stopped = stopped.clone();
        let ready = ready_tx.clone();
        Some(thread::spawn(move || {
            drainer.register_worker();
            ready.send(Ok::<_, String>(())).unwrap();
            if gate.wait().is_none() {
                return Ok(Instant::now());
            }
            let mut pipeline = target;
            while !stopped.load(Ordering::Acquire) {
                if !drainer.drain(&mut pipeline, config.source_budget) {
                    drainer.idle(&mut pipeline, config.source_budget, config.idle_timeout);
                }
            }
            drainer.finish(pipeline).map_err(|e| e.to_string())?;
            Ok::<_, String>(Instant::now())
        }))
    };
    let mut writers = Vec::new();
    for (fixture, load) in fixtures {
        // Prepare structural values before timing; the timed loop reads the
        // clock and performs the real encoding directly into the ring slot.
        let mut templates: Vec<Marker<'static>> = Vec::new();
        if ENCODE {
            crate::workload::for_each_template(&fixture.manifest, |m| templates.push(m));
        }
        let factory = factory.clone();
        let gate = gate.clone();
        let ready = ready_tx.clone();
        writers.push(thread::spawn(move || {
            let producer = factory
                .producer(fixture.manifest.source_id)
                .map_err(|e| e.to_string());
            ready
                .send(producer.as_ref().map(|_| ()).map_err(Clone::clone))
                .unwrap();
            let mut producer = producer?;
            let Some(start) = gate.wait() else {
                return Err("replay aborted during setup".into());
            };
            crate::feeder::replay_source::<ENCODE, FEEDER_ONLY>(
                &mut producer,
                &factory,
                &fixture,
                &templates,
                load,
                start,
            )?;
            // Keep prepared input alive until the thread exits, after producer
            // drop publishes its last write. No per-marker measurement counter.
            Ok::<_, String>((fixture, templates))
        }));
    }
    drop(ready_tx);
    let mut setup_error = None;
    for _ in 0..writers.len() + usize::from(receiver.is_some()) + usize::from(downstream.is_some())
    {
        match ready_rx.recv().map_err(|e| e.to_string())? {
            Ok(()) => {}
            Err(e) => setup_error = Some(e),
        }
    }
    let cpu_start = usage().map(|u| u.0);
    let start = Instant::now();
    gate.release(if setup_error.is_some() {
        Start::Abort
    } else {
        Start::Run(start)
    });
    let mut error = setup_error;
    let mut retained_fixtures = Vec::new();
    for writer in writers {
        match writer.join() {
            Ok(Ok(fixture)) => retained_fixtures.push(fixture),
            Ok(Err(e)) => {
                error.get_or_insert(e);
            }
            Err(_) => {
                error.get_or_insert("producer panicked".into());
            }
        }
    }
    let producers_done = Instant::now();
    drop(idle);
    stopped.store(true, Ordering::Release);
    factory.wake_consumer();
    let finish = match receiver {
        Some(receiver) => receiver.join().map_err(|_| "drainer panicked")??,
        None => producers_done,
    };
    let finish = match downstream {
        Some(worker) => finish.max(worker.join().map_err(|_| "batch worker panicked")?),
        None => finish,
    };
    let usage_end = usage();
    if let Some(error) = error {
        return Err(error);
    }
    let elapsed = finish.duration_since(start).as_secs_f64();
    #[expect(
        clippy::cast_precision_loss,
        reason = "throughput is approximate; exact byte total is reported separately"
    )]
    let bytes_per_second = (!FEEDER_ONLY).then_some(input_bytes as f64 / elapsed);
    Ok(ReplayResult {
        consumer_mode: config.consumer_mode,
        downstream_threads: usize::from(
            !FEEDER_ONLY && matches!(config.consumer_mode, ConsumerMode::CopyHandoff),
        ),
        batch_payload_bytes: config.batch.payload_bytes,
        batch_source_ranges: config.batch.source_ranges,
        batch_queue_capacity: config.batch.queue_batches,
        batch_retained_capacity: config.batch.retained_batches,
        producer_mode: config.producer_mode,
        clock_kind: (ENCODE && !FEEDER_ONLY).then(|| format!("{:?}", clock::meta().kind)),
        clock_reads: if ENCODE && !FEEDER_ONLY { markers } else { 0 },
        preset: if FEEDER_ONLY {
            "feeder-only"
        } else {
            match config.consumer_mode {
                ConsumerMode::Discard => "drain-only",
                ConsumerMode::CopyLocal => "copy-local",
                ConsumerMode::CopyHandoff => "copy-handoff",
            }
        },
        full_ring_policy: if FEEDER_ONLY {
            "not-applicable"
        } else {
            "retry-spin-then-yield"
        },
        pipeline: if FEEDER_ONLY {
            "none"
        } else {
            std::any::type_name::<T>()
        },
        source_threads: sources.len(),
        consumer_threads: usize::from(!FEEDER_ONLY),
        idle_rings: config.idle_rings,
        input_bytes,
        markers,
        ring_bytes: if FEEDER_ONLY { 0 } else { input_bytes },
        ring_markers: if FEEDER_ONLY { 0 } else { markers },
        sources,
        loads,
        replay_seconds: elapsed,
        producer_seconds: producers_done.duration_since(start).as_secs_f64(),
        final_drain_seconds: finish.duration_since(producers_done).as_secs_f64(),
        bytes_per_second,
        replay_cpu_seconds: cpu_start.zip(usage_end).map(|(a, b)| b.0 - a),
        peak_rss_bytes_including_preparation: usage_end.map(|u| u.1),
        segment_bytes: config.transport.segment_bytes,
        freelist_segments: config.transport.freelist_segments,
        memory_budget_bytes: config.transport.memory_bytes,
        source_budget: config.source_budget,
        idle_timeout_micros: config.idle_timeout.as_micros(),
    })
}

/// Two process-level observations around replay, never measurements inside stages.
#[cfg(unix)]
#[allow(unsafe_code)]
fn usage() -> Option<(f64, u64)> {
    let mut usage = std::mem::MaybeUninit::<libc::rusage>::uninit();
    // SAFETY: getrusage initializes the supplied object on success.
    if unsafe { libc::getrusage(libc::RUSAGE_SELF, usage.as_mut_ptr()) } != 0 {
        return None;
    }
    let usage = unsafe { usage.assume_init() };
    let seconds = |time: libc::timeval| -> Option<f64> {
        Duration::from_secs(u64::try_from(time.tv_sec).ok()?)
            .checked_add(Duration::from_micros(u64::try_from(time.tv_usec).ok()?))
            .map(|time| time.as_secs_f64())
    };
    let cpu = seconds(usage.ru_utime)? + seconds(usage.ru_stime)?;
    let rss = u64::try_from(usage.ru_maxrss).ok()?;
    #[cfg(target_os = "macos")]
    let bytes = rss;
    #[cfg(not(target_os = "macos"))]
    let bytes = rss.checked_mul(1024)?;
    Some((cpu, bytes))
}
#[cfg(not(unix))]
fn usage() -> Option<(f64, u64)> {
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn baseline_preserves_workload_but_reports_no_telemetry_work() {
        let run_mode = |producer_mode, consumer_mode| {
            let fixtures = [(1, 25), (2, 41)]
                .into_iter()
                .map(|(source, calls)| {
                    (
                        Fixture::prepare(source, calls, 8, None).unwrap(),
                        SourceLoad {
                            bytes_per_second: 0,
                            start_delay_ms: 1,
                            batch_markers: 7,
                            burst_pause_ms: 0,
                        },
                    )
                })
                .collect();
            run(
                ReplayConfig {
                    consumer_mode,
                    batch: BatchConfig::default(),
                    producer_mode,
                    transport: TransportConfig::default(),
                    source_budget: 1,
                    idle_rings: 1,
                    idle_timeout: Duration::from_micros(200),
                },
                fixtures,
            )
            .unwrap()
        };
        for consumer_mode in [
            ConsumerMode::Discard,
            ConsumerMode::CopyLocal,
            ConsumerMode::CopyHandoff,
        ] {
            let baseline = run_mode(ProducerMode::FeederOnly, consumer_mode);
            let full = run_mode(ProducerMode::EncodeClock, consumer_mode);
            assert_eq!(baseline.input_bytes, full.input_bytes);
            assert_eq!(baseline.markers, full.markers);
            assert_eq!(baseline.source_threads, full.source_threads);
            assert_eq!(
                serde_json::to_value(&baseline.sources).unwrap(),
                serde_json::to_value(&full.sources).unwrap()
            );
            assert_eq!(baseline.clock_reads, 0);
            assert!(baseline.clock_kind.is_none());
            assert_eq!(baseline.consumer_threads, 0);
            assert_eq!(baseline.ring_bytes, 0);
            assert_eq!(baseline.ring_markers, 0);
            assert!(baseline.bytes_per_second.is_none());
            assert!(baseline.final_drain_seconds.abs() < f64::EPSILON);
            assert!(baseline.replay_seconds >= 0.001);
            assert_eq!(full.clock_reads, full.markers);
            assert_eq!(full.ring_bytes, full.input_bytes);
            assert_eq!(full.ring_markers, full.markers);
            assert_eq!(full.consumer_threads, 1);
            assert!(full.bytes_per_second.unwrap() > 0.0);
            assert_eq!(baseline.downstream_threads, 0);
            assert_eq!(
                full.downstream_threads,
                usize::from(matches!(consumer_mode, ConsumerMode::CopyHandoff))
            );
        }
    }
}
