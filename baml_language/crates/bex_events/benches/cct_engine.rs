//! The P2 integrated CCT bench (observability design §5.11): full raw
//! record decode + causal dispatch + intern + counter bumps + charge +
//! recent-ring + an equal-cost flight-recorder memcpy stub, measured per
//! call-pair. **Exit gate: ≤50 ns/call-pair** (target 45, never-exceed 60 —
//! one number set for the engine and this bench). C2's nightly absolute
//! runs this; the P6 real recorder re-affirms the number before P9.
//!
//! Variants:
//! - hot loop (3-node shape, one thread, the 36M-call pathology)
//! - 3,537-node shape (corpus p99), depth-14 stacks
//! - two-ring migration (the §5.2 defer/replay path under churn)
//!
//! Run: `cargo bench -p bex_events --bench cct_engine`

#![expect(
    clippy::print_stdout,
    reason = "harness-less bench reports its results on stdout"
)]

use std::hint::black_box;
use std::time::Instant;

use bex_events::ids::{BexCallId, BexThreadId, FunctionId};
use bex_events::prof::cct::CctEngine;
use bex_events::prof::record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord, ThreadEndStatus};

fn encode_into(out: &mut Vec<u8>, rec: &RawRecord<'_>) {
    let mut buf = [0u8; MAX_RECORD_LEN];
    let len = rec.encode(&mut buf);
    out.extend_from_slice(&buf[..len]);
}

/// The §5.9 equal-cost stub: one memcpy of every drained range into a
/// bounded ring, matching the real flight recorder's per-byte cost.
struct RecorderStub {
    buf: Vec<u8>,
    cap: usize,
    write: usize,
}

impl RecorderStub {
    fn new(cap: usize) -> RecorderStub {
        RecorderStub {
            buf: vec![0u8; cap],
            cap,
            write: 0,
        }
    }

    #[inline]
    fn record(&mut self, bytes: &[u8]) {
        let mut src = bytes;
        while !src.is_empty() {
            let room = self.cap - self.write;
            let n = room.min(src.len());
            self.buf[self.write..self.write + n].copy_from_slice(&src[..n]);
            self.write = (self.write + n) % self.cap;
            src = &src[n..];
        }
    }
}

fn bench_range(
    label: &str,
    ranges: &[Vec<u8>],
    call_pairs: u64,
    engine_ctor: impl Fn() -> CctEngine,
) {
    let mut best = f64::MAX;
    let mut nodes = 0;
    for _ in 0..3 {
        let mut engine = engine_ctor();
        let mut recorder = RecorderStub::new(16 * 1024 * 1024);
        let start = Instant::now();
        for range in ranges {
            recorder.record(range);
            engine.consume(range, &mut |t| t);
        }
        engine.sweep_tick(&mut |t| t);
        let elapsed = start.elapsed();
        black_box(&engine);
        black_box(&recorder.buf);
        best = best.min(elapsed.as_nanos() as f64 / call_pairs as f64);
        nodes = engine.nodes().len();
        let diag = engine.diagnostics();
        assert_eq!(
            diag.synthesized_parents, 0,
            "{label}: bench streams must resolve without synthesis"
        );
    }
    println!("  {label:<28} nodes={nodes:<6} ns_per_pair={best:.1}");
}

fn hot_loop_ranges(pairs: u64) -> Vec<Vec<u8>> {
    // main -> step -> add, the workloads/hotloop shape: one thread, two
    // profiled calls per iteration.
    let mut out = Vec::new();
    let mut range = Vec::with_capacity(1 << 20);
    encode_into(
        &mut range,
        &RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 0,
            name: b"",
        },
    );
    encode_into(
        &mut range,
        &RawRecord::CallFunction {
            flags: 0,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            parent_call_id: BexCallId(0),
            function_id: FunctionId(16),
            call_site: None,
            ts_ticks: 1,
        },
    );
    let mut call_id = 2u64;
    let mut ts = 2u64;
    for _ in 0..pairs / 2 {
        for function in [17u32, 18u32] {
            encode_into(
                &mut range,
                &RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(call_id),
                    parent_call_id: BexCallId(if function == 17 { 1 } else { call_id - 1 }),
                    function_id: FunctionId(function),
                    call_site: None,
                    ts_ticks: ts,
                },
            );
            ts += 1;
            call_id += 1;
        }
        for offset in [1u64, 2u64] {
            encode_into(
                &mut range,
                &RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(call_id - offset),
                    ts_ticks: ts,
                },
            );
            ts += 1;
        }
        // Match the consumer's real drained-range granularity (~256 KiB).
        if range.len() > 256 * 1024 {
            out.push(std::mem::take(&mut range));
        }
    }
    encode_into(
        &mut range,
        &RawRecord::EndFunction {
            status: FunctionEndStatus::Ok,
            thread_id: BexThreadId(1),
            call_id: BexCallId(1),
            ts_ticks: ts,
        },
    );
    encode_into(
        &mut range,
        &RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: ts + 1,
        },
    );
    out.push(range);
    out
}

/// The corpus-p99 shape: W distinct depth-14 chains (W×14 ≈ 3,537 nodes),
/// revisited round-robin.
fn p99_ranges(pairs: u64) -> Vec<Vec<u8>> {
    const WIDTH: u64 = 253; // 253 chains × 14 deep ≈ 3,542 contexts
    const DEPTH: u64 = 14;
    let mut out = Vec::new();
    let mut range = Vec::with_capacity(1 << 20);
    encode_into(
        &mut range,
        &RawRecord::StartThread {
            flags: 0,
            thread_id: BexThreadId(1),
            parent_thread_id: BexThreadId(0),
            parent_call_id: BexCallId(0),
            ts_ticks: 0,
            name: b"",
        },
    );
    let mut call_id = 1u64;
    let mut ts = 1u64;
    let traversals = pairs / DEPTH;
    for t in 0..traversals {
        let chain = t % WIDTH;
        let base_fn = 16 + u32::try_from(chain).unwrap() * u32::try_from(DEPTH).unwrap();
        let first_call = call_id;
        for level in 0..DEPTH {
            encode_into(
                &mut range,
                &RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(call_id),
                    parent_call_id: BexCallId(if level == 0 { 0 } else { call_id - 1 }),
                    function_id: FunctionId(base_fn + u32::try_from(level).unwrap()),
                    call_site: None,
                    ts_ticks: ts,
                },
            );
            call_id += 1;
            ts += 1;
        }
        for level in (0..DEPTH).rev() {
            encode_into(
                &mut range,
                &RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(1),
                    call_id: BexCallId(first_call + level),
                    ts_ticks: ts,
                },
            );
            ts += 1;
        }
        if range.len() > 256 * 1024 {
            out.push(std::mem::take(&mut range));
        }
    }
    encode_into(
        &mut range,
        &RawRecord::EndThread {
            status: ThreadEndStatus::Completed,
            thread_id: BexThreadId(1),
            ts_ticks: ts,
        },
    );
    out.push(range);
    out
}

/// Two-ring migration: 8 logical threads, each thread\'s stream chopped at
/// await points into chunk pairs — the pre-await chunk on ring A, the
/// post-await chunk on ring B, with B\'s chunk drained FIRST (the §5.2
/// defer/replay path). Stack depth stays bounded (≤2), like real awaits.
fn migration_ranges(pairs: u64) -> Vec<Vec<u8>> {
    const THREADS: u64 = 8;
    /// Call-pairs per chunk before the stream "migrates" rings again.
    const CHUNK_PAIRS: u64 = 4096;
    let per_thread = pairs / THREADS;
    let mut out = Vec::new();
    for tid in 1..=THREADS {
        let mut ts = tid * 1_000_000_000;
        let mut call_id = 1u64;
        let mut first_chunk = true;
        let mut remaining = per_thread;
        while remaining > 0 {
            let chunk = remaining.min(CHUNK_PAIRS);
            remaining -= chunk;
            let mut pre = Vec::new();
            let mut post = Vec::new();
            if first_chunk {
                encode_into(
                    &mut pre,
                    &RawRecord::StartThread {
                        flags: 0,
                        thread_id: BexThreadId(tid),
                        parent_thread_id: BexThreadId(0),
                        parent_call_id: BexCallId(0),
                        ts_ticks: ts,
                        name: b"",
                    },
                );
                first_chunk = false;
            }
            // Pre-await half: open a root call, then an inner pair per
            // iteration; the matching closes land in the post half.
            for i in 0..chunk {
                let root = call_id;
                encode_into(
                    &mut pre,
                    &RawRecord::CallFunction {
                        flags: 0,
                        thread_id: BexThreadId(tid),
                        call_id: BexCallId(root),
                        parent_call_id: BexCallId(0),
                        function_id: FunctionId(16 + u32::try_from(i % 16).unwrap()),
                        call_site: None,
                        ts_ticks: ts,
                    },
                );
                ts += 1;
                encode_into(
                    &mut post,
                    &RawRecord::CallFunction {
                        flags: 0,
                        thread_id: BexThreadId(tid),
                        call_id: BexCallId(root + 1),
                        parent_call_id: BexCallId(root),
                        function_id: FunctionId(64),
                        call_site: None,
                        ts_ticks: ts,
                    },
                );
                ts += 1;
                encode_into(
                    &mut post,
                    &RawRecord::EndFunction {
                        status: FunctionEndStatus::Ok,
                        thread_id: BexThreadId(tid),
                        call_id: BexCallId(root + 1),
                        ts_ticks: ts,
                    },
                );
                ts += 1;
                encode_into(
                    &mut post,
                    &RawRecord::EndFunction {
                        status: FunctionEndStatus::Ok,
                        thread_id: BexThreadId(tid),
                        call_id: BexCallId(root),
                        ts_ticks: ts,
                    },
                );
                ts += 1;
                call_id = root + 2;
            }
            if remaining == 0 {
                encode_into(
                    &mut post,
                    &RawRecord::EndThread {
                        status: ThreadEndStatus::Completed,
                        thread_id: BexThreadId(tid),
                        ts_ticks: ts,
                    },
                );
            }
            // Post-await ring drains FIRST: worst-case defer volume per
            // chunk pair.
            out.push(post);
            out.push(pre);
        }
    }
    out
}

fn main() {
    let pairs: u64 = std::env::var("CCT_BENCH_PAIRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_000_000);
    println!(
        "cct_engine integrated bench: {pairs} call-pairs/variant (decode + dispatch + intern + \
         charge + recent-ring + 16 MiB recorder stub). Gate: <=50 ns/pair."
    );
    if std::env::var("CCT_BENCH_PROBE").is_ok() {
        // Cost decomposition: decode-only and recorder-only over the
        // hot-loop stream, to localize the per-pair budget.
        let ranges = hot_loop_ranges(pairs);
        let mut best = f64::MAX;
        for _ in 0..3 {
            let start = Instant::now();
            let mut n: u64 = 0;
            for range in &ranges {
                for rec in bex_events::prof::record::iter(range) {
                    if rec.is_ok() {
                        n += 1;
                    }
                }
            }
            black_box(n);
            best = best.min(start.elapsed().as_nanos() as f64 / pairs as f64);
        }
        println!("  probe decode-only            ns_per_pair={best:.1}");
        let mut best = f64::MAX;
        for _ in 0..3 {
            let mut recorder = RecorderStub::new(16 * 1024 * 1024);
            let start = Instant::now();
            for range in &ranges {
                recorder.record(range);
            }
            black_box(&recorder.buf);
            best = best.min(start.elapsed().as_nanos() as f64 / pairs as f64);
        }
        println!("  probe recorder-only          ns_per_pair={best:.1}");
    }
    bench_range("hotloop (3 nodes)", &hot_loop_ranges(pairs), pairs, || {
        CctEngine::new(64)
    });
    bench_range(
        "p99 (3,542 nodes, depth 14)",
        &p99_ranges(pairs),
        pairs,
        || CctEngine::new(4096),
    );
    bench_range(
        "two-ring migration (8 thr)",
        &migration_ranges(pairs),
        pairs,
        || CctEngine::new(64),
    );
}
