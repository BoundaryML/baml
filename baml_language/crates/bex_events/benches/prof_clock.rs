//! Clock micro-bench (plan §4.2 / PR1 gate): ns per timestamp read.
//!
//! The profiling design budgets ~25 ns per call pair, dominated by two clock
//! reads — i.e. it assumes ≤10 ns per read of `now_ticks` (the producer hot
//! path; raw `rdtsc` / `CNTVCT_EL0`, no conversion). The converter row is
//! the consumer-side tick→ns cost, off every producer path. Run with:
//! `cargo bench -p bex_events --bench prof_clock`
#![expect(
    clippy::print_stdout,
    reason = "harness-less bench reports its results on stdout"
)]

use std::{hint::black_box, time::Instant};

fn measure(name: &str, mut f: impl FnMut() -> u64) {
    const WARMUP: u64 = 1_000_000;
    const ITERS: u64 = 20_000_000;
    for _ in 0..WARMUP {
        black_box(f());
    }
    let start = Instant::now();
    for _ in 0..ITERS {
        black_box(f());
    }
    let elapsed = start.elapsed();
    #[expect(clippy::cast_precision_loss, reason = "display only")]
    let ns_per_op = elapsed.as_nanos() as f64 / ITERS as f64;
    println!("{name:32} {ns_per_op:6.2} ns/read");
}

fn main() {
    // Pins the clock anchor (source detection; no calibration runs here).
    bex_events::prof::clock::init();
    println!("prof_clock bench ({} iterations per row)", 20_000_000);
    measure("prof::clock::now_ticks", bex_events::prof::clock::now_ticks);
    let conv = bex_events::prof::clock::TickConverter::from_clock();
    // End-to-end read+convert; the delta vs the now_ticks row is the
    // consumer-side conversion cost in isolation.
    measure("now_ticks + TickConverter::to_ns", || {
        conv.to_ns(bex_events::prof::clock::now_ticks())
    });
    measure("std::time::Instant::now", || {
        black_box(Instant::now());
        0
    });
}
