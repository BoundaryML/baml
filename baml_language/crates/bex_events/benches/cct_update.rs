//! Integrated P2 raw-decode + CCT aggregation benchmark.
//!
//! This remains a harness-less NDJSON producer so `tools_obs_bench` can run
//! it under pinned/rusage-controlled jobs. It includes an equal-cost flight
//! recorder stub (one raw-range memcpy); the real recorder re-affirms the
//! gate when P6 lands. The depth-14 and eight-producer shapes retain separate
//! raw ranges per logical producer, matching the consumer's multiring drain
//! contract rather than concatenating them into an unrealistically single
//! stream.

use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use bex_events::{
    ids::{BexCallId, BexThreadId, FunctionId},
    prof::{
        cct::EngineCct,
        clock::TickConverter,
        record::{FunctionEndStatus, MAX_RECORD_LEN, RawRecord},
    },
};

const PAIRS: u64 = 2_000_000;
const SAMPLES: usize = 5;
const DEPTH_14: u32 = 14;
const PRODUCER_RINGS: u32 = 8;
const BATCH_PAIRS: u64 = 4096;

#[cfg(target_os = "linux")]
#[allow(unsafe_code, reason = "Linux affinity requires the libc scheduler API")]
fn pin_to_one_allowed_cpu() -> Option<usize> {
    // Absolute nanosecond gates are meaningless if Linux migrates the process
    // between heterogeneous or differently contended virtual CPUs. Preserve
    // the enclosing job's cpuset and select its highest-numbered CPU; children
    // and all benchmark work remain inside the allocation.
    //
    // SAFETY: both cpu_set_t values are initialized before the libc calls,
    // their exact sizes are supplied, and CPU_SET/CPU_ISSET indices are
    // bounded by CPU_SETSIZE.
    unsafe {
        let mut allowed: libc::cpu_set_t = std::mem::zeroed();
        if libc::sched_getaffinity(0, std::mem::size_of::<libc::cpu_set_t>(), &raw mut allowed) != 0
        {
            return None;
        }
        let cpu = (0..libc::CPU_SETSIZE as usize)
            .rev()
            .find(|cpu| libc::CPU_ISSET(*cpu, &allowed))?;
        let mut selected: libc::cpu_set_t = std::mem::zeroed();
        libc::CPU_ZERO(&mut selected);
        libc::CPU_SET(cpu, &mut selected);
        (libc::sched_setaffinity(
            0,
            std::mem::size_of::<libc::cpu_set_t>(),
            &raw const selected,
        ) == 0)
            .then_some(cpu)
    }
}

#[cfg(not(target_os = "linux"))]
fn pin_to_one_allowed_cpu() -> Option<usize> {
    None
}

#[derive(Clone, Copy)]
struct Workload {
    bench_id: &'static str,
    functions: u32,
    depth: u32,
    rings: u32,
}

fn encode(rec: RawRecord<'_>, out: &mut Vec<u8>) {
    let mut buf = [0u8; MAX_RECORD_LEN];
    let len = rec.encode(&mut buf);
    out.extend_from_slice(&buf[..len]);
}

fn raw_batch(functions: u32, depth: u32, thread_id: u64, first_group: u64, groups: u64) -> Vec<u8> {
    let pairs = groups.saturating_mul(u64::from(depth));
    let mut bytes = Vec::with_capacity(pairs as usize * 80);
    for group_offset in 0..groups {
        let group = first_group + group_offset;
        let first_call_id = group.saturating_mul(u64::from(depth)).saturating_add(1);
        let first_tick = group
            .saturating_mul(u64::from(depth).saturating_mul(2).saturating_add(1))
            .saturating_add(1);
        for level in 0..depth {
            let call_id = first_call_id.saturating_add(u64::from(level));
            let parent_call_id = if level == 0 { 0 } else { call_id - 1 };
            // A stable path keeps the depth workload focused on stack/context
            // cost; flat rows vary by call to sweep function cardinality.
            let function_ordinal = if depth == 1 {
                u32::try_from(group % u64::from(functions)).unwrap()
            } else {
                level % functions
            };
            let function_id = 16 + function_ordinal;
            encode(
                RawRecord::CallFunction {
                    flags: 0,
                    thread_id: BexThreadId(thread_id),
                    call_id: BexCallId(call_id),
                    parent_call_id: BexCallId(parent_call_id),
                    function_id: FunctionId(function_id),
                    call_site: None,
                    ts_ticks: first_tick.saturating_add(u64::from(level)),
                },
                &mut bytes,
            );
        }
        for level in (0..depth).rev() {
            let call_id = first_call_id.saturating_add(u64::from(level));
            encode(
                RawRecord::EndFunction {
                    status: FunctionEndStatus::Ok,
                    thread_id: BexThreadId(thread_id),
                    call_id: BexCallId(call_id),
                    ts_ticks: first_tick
                        .saturating_add(u64::from(depth))
                        .saturating_add(u64::from(depth - 1 - level)),
                },
                &mut bytes,
            );
        }
    }
    bytes
}

fn feed(cct: &mut EngineCct, conv: &TickConverter, bytes: &[u8], recorder: &mut Vec<u8>) {
    recorder.clear();
    recorder.extend_from_slice(bytes);
    for raw in bex_events::prof::record::iter(bytes) {
        cct.ingest_raw(&raw.expect("synthetic batch is valid"), conv);
    }
    cct.finish_sweep();
    black_box(recorder.as_slice());
}

fn feed_ranges(
    cct: &mut EngineCct,
    conv: &TickConverter,
    ranges_by_ring: &[Vec<Vec<u8>>],
    recorder: &mut Vec<u8>,
) {
    let max_ranges = ranges_by_ring.iter().map(Vec::len).max().unwrap_or(0);
    for range_index in 0..max_ranges {
        for ranges in ranges_by_ring {
            if let Some(bytes) = ranges.get(range_index) {
                feed(cct, conv, bytes, recorder);
            }
        }
    }
}

fn raw_ranges(workload: Workload, target_pairs: u64) -> (Vec<Vec<Vec<u8>>>, u64) {
    let total_groups = target_pairs / u64::from(workload.depth);
    let groups_per_batch = (BATCH_PAIRS / u64::from(workload.depth)).max(1);
    let groups_per_ring = total_groups / u64::from(workload.rings);
    let extra_rings = total_groups % u64::from(workload.rings);
    let mut ranges_by_ring = Vec::with_capacity(workload.rings as usize);
    for ring in 0..workload.rings {
        let ring_groups = groups_per_ring + u64::from(u64::from(ring) < extra_rings);
        let mut ranges = Vec::new();
        let mut prepared = 0;
        while prepared < ring_groups {
            let groups = groups_per_batch.min(ring_groups - prepared);
            ranges.push(raw_batch(
                workload.functions,
                workload.depth,
                u64::from(ring) + 1,
                prepared,
                groups,
            ));
            prepared += groups;
        }
        ranges_by_ring.push(ranges);
    }
    (ranges_by_ring, total_groups * u64::from(workload.depth))
}

fn fresh_cct(conv: &TickConverter, rings: u32, name: &[u8]) -> EngineCct {
    let mut cct = EngineCct::default();
    for ring in 0..rings {
        cct.ingest_raw(
            &RawRecord::StartThread {
                flags: 0,
                thread_id: BexThreadId(u64::from(ring) + 1),
                parent_thread_id: BexThreadId(0),
                parent_call_id: BexCallId(0),
                ts_ticks: 0,
                name,
            },
            conv,
        );
    }
    cct
}

fn main() {
    let pinned_cpu = pin_to_one_allowed_cpu();
    let workloads = [
        Workload {
            bench_id: "cct_engine_integrated_f1",
            functions: 1,
            depth: 1,
            rings: 1,
        },
        Workload {
            bench_id: "cct_engine_integrated_f16",
            functions: 16,
            depth: 1,
            rings: 1,
        },
        Workload {
            bench_id: "cct_engine_integrated_f1024",
            functions: 1024,
            depth: 1,
            rings: 1,
        },
        Workload {
            bench_id: "cct_engine_integrated_f4096",
            functions: 4096,
            depth: 1,
            rings: 1,
        },
        Workload {
            bench_id: "cct_engine_integrated_depth14",
            functions: 16,
            depth: DEPTH_14,
            rings: 1,
        },
        Workload {
            bench_id: "cct_engine_integrated_depth14_rings8",
            functions: 16,
            depth: DEPTH_14,
            rings: PRODUCER_RINGS,
        },
    ];
    for workload in workloads {
        let conv = TickConverter::identity();
        let (warm_ranges, _) = raw_ranges(workload, BATCH_PAIRS * u64::from(workload.rings));
        let (measured_ranges, measured_pairs) = raw_ranges(workload, PAIRS);
        let max_range_bytes = measured_ranges
            .iter()
            .flat_map(|ranges| ranges.iter())
            .map(Vec::len)
            .max()
            .unwrap_or(0);
        let mut recorder = Vec::with_capacity(max_range_bytes);

        let warmup = Instant::now();
        while warmup.elapsed() < Duration::from_millis(100) {
            // A fresh engine avoids duplicate call-key semantics during warmup.
            let mut warm = fresh_cct(&conv, workload.rings, b"warm");
            feed_ranges(&mut warm, &conv, &warm_ranges, &mut recorder);
            black_box(warm.snapshot());
        }

        let mut elapsed_samples = Vec::with_capacity(SAMPLES);
        let mut nodes = 0usize;
        for _ in 0..SAMPLES {
            let mut cct = fresh_cct(&conv, workload.rings, b"bench");
            let start = Instant::now();
            feed_ranges(&mut cct, &conv, &measured_ranges, &mut recorder);
            elapsed_samples.push(start.elapsed().as_nanos());
            let snapshot = cct.snapshot();
            nodes = snapshot.nodes.len();
            black_box(snapshot);
        }
        elapsed_samples.sort_unstable();
        let median_elapsed_ns = elapsed_samples[SAMPLES / 2];
        let median_ns_per_pair = median_elapsed_ns as f64 / measured_pairs as f64;
        let min_ns_per_pair = elapsed_samples[0] as f64 / measured_pairs as f64;
        let max_ns_per_pair = elapsed_samples[SAMPLES - 1] as f64 / measured_pairs as f64;
        let sample_elapsed_ns = elapsed_samples
            .iter()
            .map(u128::to_string)
            .collect::<Vec<_>>()
            .join(",");
        println!(
            "{{\"schema_version\":1,\"bench_id\":\"{}\",\
             \"evidence\":\"measured\",\"pairs\":{measured_pairs},\"functions\":{},\
             \"depth\":{},\"producer_rings\":{},\"raw_decode\":true,\
             \"recorder_memcpy_stub\":true,\"pinned_cpu\":{},\
             \"samples\":{SAMPLES},\"nodes\":{nodes},\
             \"sample_elapsed_ns\":[{sample_elapsed_ns}],\
             \"median_elapsed_ns\":{median_elapsed_ns},\
             \"min_ns_per_call_pair\":{min_ns_per_pair:.3},\
             \"median_ns_per_call_pair\":{median_ns_per_pair:.3},\
             \"max_ns_per_call_pair\":{max_ns_per_pair:.3}}}",
            workload.bench_id,
            workload.functions,
            workload.depth,
            workload.rings,
            pinned_cpu.map_or_else(|| "null".to_owned(), |cpu| cpu.to_string()),
        );
    }
}
