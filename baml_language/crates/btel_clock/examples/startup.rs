//! Run in release mode, once per process, to compare clock startup and reads.
use std::{hint::black_box, time::Instant};

use btel_clock::{ClockMode, ClockRuntime};
#[allow(
    clippy::print_stdout,
    clippy::cast_precision_loss,
    reason = "benchmark JSON and diagnostic loop averages"
)]
fn main() {
    let begin = Instant::now();
    let runtime = ClockRuntime::new(ClockMode::Auto);
    let constructed = Instant::now();
    let epoch = runtime.start_run();
    epoch.attach_thread();
    black_box(epoch.read());
    let ready = Instant::now();
    let reads = 10_000_000_u64;
    let read_start = Instant::now();
    for _ in 0..reads {
        black_box(black_box(&*epoch).read());
    }
    let read_elapsed = read_start.elapsed();
    let finish_start = Instant::now();
    epoch.finish_thread();
    let finish = finish_start.elapsed();
    println!(
        "{{\"source\":\"{:?}\",\"constructor_ns\":{},\"first_read_ready_ns\":{},\"raw_loop_ns_per_read\":{},\"finish_ns\":{}}}",
        epoch.metadata().source,
        constructed.duration_since(begin).as_nanos(),
        ready.duration_since(begin).as_nanos(),
        read_elapsed.as_nanos() as f64 / reads as f64,
        finish.as_nanos()
    );
}
