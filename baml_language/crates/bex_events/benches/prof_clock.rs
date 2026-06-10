//! Clock micro-bench (plan §4.2 / PR1 gate): ns per timestamp read.
//!
//! The profiling design budgets ~25 ns per call pair, dominated by two clock
//! reads — i.e. it assumes ≤10 ns per read. Run with:
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
    // Pays minstant's one-time TSC calibration before measuring.
    bex_events::prof::clock::init();
    println!("prof_clock bench ({} iterations per row)", 20_000_000);
    measure("prof::clock::now_ns", bex_events::prof::clock::now_ns);
    measure("minstant::Instant::now", || {
        black_box(minstant::Instant::now());
        0
    });
    measure("std::time::Instant::now", || {
        black_box(Instant::now());
        0
    });
}
