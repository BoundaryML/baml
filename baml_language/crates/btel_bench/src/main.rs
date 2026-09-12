//! One binary, selectable downstream stages. Start with `--help`.
use std::{io, path::PathBuf, time::Duration};

use btel_bench::{
    feeder::ProducerMode,
    replay::{self, ConsumerMode, ReplayConfig, SourceLoad},
    workload::Fixture,
};
use btel_transport::{TransportConfig, batch::BatchConfig};
use clap::Parser;

#[derive(Parser)]
#[command(about = "Measure Btel producers, transport, and incremental span building")]
struct Args {
    /// Discard, copy locally, hand off buffers, or build spans on the worker.
    #[arg(long, value_enum, default_value_t = ConsumerMode::Discard)]
    consumer_mode: ConsumerMode,
    #[arg(long, default_value_t = 262_144)]
    batch_payload_bytes: usize,
    #[arg(long, default_value_t = 256)]
    batch_source_ranges: usize,
    #[arg(long, default_value_t = 64)]
    batch_queue_capacity: usize,
    #[arg(long, default_value_t = 8)]
    batch_retained_capacity: usize,
    /// Live clock/encoding, prepared-byte replay, or a feeder-only baseline.
    #[arg(long, value_enum, default_value_t = ProducerMode::EncodeClock)]
    producer_mode: ProducerMode,
    #[arg(long, default_value_t = 4)]
    threads: usize,
    #[arg(long, default_value_t = 1_000_000)]
    calls_per_source: u64,
    /// Optional per-source call counts; exactly --threads comma-separated entries.
    #[arg(long, value_delimiter = ',')]
    source_calls: Vec<u64>,
    /// Per-source byte rates. Zero is unpaced. Omit for all unpaced.
    #[arg(long, value_delimiter = ',')]
    source_rates: Vec<u64>,
    #[arg(long, value_delimiter = ',')]
    source_delays_ms: Vec<u64>,
    #[arg(long, default_value_t = 1)]
    depth: usize,
    #[arg(long, default_value_t = 0)]
    idle_rings: usize,
    #[arg(long, default_value_t = 262_144)]
    segment_bytes: usize,
    #[arg(long, default_value_t = 2)]
    freelist_segments: usize,
    #[arg(long, default_value_t = 256)]
    memory_mib: u64,
    #[arg(long, default_value_t = 16)]
    source_budget: usize,
    #[arg(long, default_value_t = 1024)]
    batch_markers: usize,
    #[arg(long, default_value_t = 0)]
    burst_pause_ms: u64,
    #[arg(long, default_value_t = 200)]
    idle_timeout_micros: u64,
    /// Generate reusable fixtures here on first use; validate and load on later runs.
    #[arg(long)]
    fixture_dir: Option<PathBuf>,
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let args = Args::parse();
    if args.threads == 0 {
        return Err("--threads must be positive".into());
    }
    for (name, count) in [
        ("source-calls", args.source_calls.len()),
        ("source-rates", args.source_rates.len()),
        ("source-delays-ms", args.source_delays_ms.len()),
    ] {
        if count != 0 && count != args.threads {
            return Err(format!("--{name} needs exactly --threads entries").into());
        }
    }
    let mut fixtures = Vec::new();
    for i in 0..args.threads {
        let fixture = Fixture::prepare(
            i as u64 + 1,
            args.source_calls
                .get(i)
                .copied()
                .unwrap_or(args.calls_per_source),
            args.depth,
            args.fixture_dir.as_deref(),
        )?;
        fixtures.push((
            fixture,
            SourceLoad {
                bytes_per_second: args.source_rates.get(i).copied().unwrap_or(0),
                start_delay_ms: args.source_delays_ms.get(i).copied().unwrap_or(0),
                batch_markers: args.batch_markers,
                burst_pause_ms: args.burst_pause_ms,
            },
        ));
    }
    let result = replay::run(
        ReplayConfig {
            consumer_mode: args.consumer_mode,
            batch: BatchConfig {
                payload_bytes: args.batch_payload_bytes,
                source_ranges: args.batch_source_ranges,
                queue_batches: args.batch_queue_capacity,
                retained_batches: args.batch_retained_capacity,
            },
            producer_mode: args.producer_mode,
            transport: TransportConfig {
                segment_bytes: args.segment_bytes,
                freelist_segments: args.freelist_segments,
                memory_bytes: args
                    .memory_mib
                    .checked_mul(1024 * 1024)
                    .ok_or("memory budget overflow")?,
            },
            source_budget: args.source_budget,
            idle_rings: args.idle_rings,
            idle_timeout: Duration::from_micros(args.idle_timeout_micros),
        },
        fixtures,
    )?;
    serde_json::to_writer_pretty(io::stdout().lock(), &result)?;
    Ok(())
}
