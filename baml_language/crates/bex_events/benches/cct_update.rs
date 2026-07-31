//! CCT node-update microbench (observability design §10.3): the measured
//! cost of the per-call CCT primitive — intern `(parent_node, function_id)`
//! through one FxHash map into structure-of-arrays counters, bump enters,
//! then close with status/duration/histogram bumps.
//!
//! This pins the "~22 ns/call intern+bumps" row of the §5.11 target table.
//! It measures the *primitive*, not the integrated engine — the P2
//! integrated bench (`cct_engine.rs`, two-ring migration fixture, real
//! record decode) is the ≤50 ns/call exit gate; this file exists first so
//! the primitive's cost is tracked from day one and regressions in the map
//! or SoA layout are visible in isolation.
//!
//! Variants (design §10.3): 1 / 16 / 1024 / 4096 distinct functions called
//! flat under one parent (map pressure sweep), plus a depth-14 nested shape
//! (the corpus-max stack pattern: one distinct context per level).
//!
//! Output: one human-readable line per variant. The NDJSON row emission for
//! `obs-bench report` rides the P2 integrated bench.
//!
//! Run: `cargo bench -p bex_events --bench cct_update`

#![expect(
    clippy::print_stdout,
    reason = "harness-less bench reports its results on stdout"
)]

use std::hint::black_box;
use std::time::Instant;

use rustc_hash::FxHashMap;

/// The §5.1 node storage shape: SoA identity + counters, one intern map.
/// Kept in the bench (not the crate) until the P2 engine lands, then the
/// bench re-points at `bex_events::prof::cct::nodes`.
struct Nodes {
    intern: FxHashMap<(u32, u32), u32>,
    parent: Vec<u32>,
    function: Vec<u32>,
    depth: Vec<u16>,
    enters: Vec<u64>,
    ends_ok: Vec<u64>,
    total_ns: Vec<u64>,
    self_ns: Vec<u64>,
    hist: Vec<[u32; 16]>,
    dirty_epoch: Vec<u32>,
}

impl Nodes {
    fn new() -> Nodes {
        let mut nodes = Nodes {
            intern: FxHashMap::default(),
            parent: Vec::new(),
            function: Vec::new(),
            depth: Vec::new(),
            enters: Vec::new(),
            ends_ok: Vec::new(),
            total_ns: Vec::new(),
            self_ns: Vec::new(),
            hist: Vec::new(),
            dirty_epoch: Vec::new(),
        };
        // Node 0: the partition root pseudo-node.
        nodes.push_node(0, 0, 0);
        nodes
    }

    fn push_node(&mut self, parent: u32, function: u32, depth: u16) -> u32 {
        let id = u32::try_from(self.parent.len()).expect("node table < u32::MAX");
        self.parent.push(parent);
        self.function.push(function);
        self.depth.push(depth);
        self.enters.push(0);
        self.ends_ok.push(0);
        self.total_ns.push(0);
        self.self_ns.push(0);
        self.hist.push([0; 16]);
        self.dirty_epoch.push(0);
        id
    }

    /// The per-CallFunction primitive: intern + enters bump + dirty mark.
    #[inline]
    fn enter(&mut self, parent: u32, function_id: u32, epoch: u32) -> u32 {
        let node = match self.intern.entry((parent, function_id)) {
            std::collections::hash_map::Entry::Occupied(o) => *o.get(),
            std::collections::hash_map::Entry::Vacant(v) => {
                let depth = 0; // filled by the real engine from the stack
                let id = u32::try_from(self.parent.len()).expect("node table < u32::MAX");
                v.insert(id);
                self.parent.push(parent);
                self.function.push(function_id);
                self.depth.push(depth);
                self.enters.push(0);
                self.ends_ok.push(0);
                self.total_ns.push(0);
                self.self_ns.push(0);
                self.hist.push([0; 16]);
                self.dirty_epoch.push(0);
                id
            }
        };
        let i = node as usize;
        self.enters[i] += 1;
        self.dirty_epoch[i] = epoch;
        node
    }

    /// The per-EndFunction primitive: status + total + self + hist bumps.
    #[inline]
    fn close(&mut self, node: u32, duration_ns: u64, self_ns: u64, epoch: u32) {
        let i = node as usize;
        self.ends_ok[i] += 1;
        self.total_ns[i] += duration_ns;
        self.self_ns[i] += self_ns;
        // ×4 stride from 1 µs (design §6.3 kind 9): bucket = log4(µs), 16 buckets.
        let us = duration_ns / 1_000;
        let bucket = if us == 0 {
            0
        } else {
            (us.ilog2() / 2).min(15) as usize
        };
        self.hist[i][bucket] += 1;
        self.dirty_epoch[i] = epoch;
    }
}

fn bench_flat(functions: u32, pairs: u64) -> (f64, usize) {
    let mut nodes = Nodes::new();
    let start = Instant::now();
    let mut fid = 0u32;
    for n in 0..pairs {
        let node = nodes.enter(0, fid, n as u32);
        nodes.close(node, 1_500 + (n & 0xFFF), 700, n as u32);
        fid += 1;
        if fid == functions {
            fid = 0;
        }
    }
    let elapsed = start.elapsed();
    black_box(&nodes);
    // ns per call-pair (one enter + one close).
    (elapsed.as_nanos() as f64 / pairs as f64, nodes.parent.len())
}

fn bench_depth14(pairs: u64) -> (f64, usize) {
    // The corpus-max stack shape: a 14-deep chain of distinct functions,
    // entered and closed leaf-first each iteration (2 ops per level).
    const DEPTH: usize = 14;
    let mut nodes = Nodes::new();
    let mut stack = [0u32; DEPTH];
    let iters = pairs / DEPTH as u64;
    let start = Instant::now();
    for n in 0..iters {
        let epoch = n as u32;
        let mut parent = 0u32;
        for (level, slot) in stack.iter_mut().enumerate() {
            let node = nodes.enter(parent, 100 + level as u32, epoch);
            *slot = node;
            parent = node;
        }
        for slot in stack.iter().rev() {
            nodes.close(*slot, 2_000, 140, epoch);
        }
    }
    let elapsed = start.elapsed();
    black_box(&nodes);
    (
        elapsed.as_nanos() as f64 / (iters * DEPTH as u64) as f64,
        nodes.parent.len(),
    )
}

fn main() {
    // Big enough that map + SoA don't fit in L1 for the 4096 shape; small
    // enough for CI. Override with CCT_BENCH_PAIRS.
    let pairs: u64 = std::env::var("CCT_BENCH_PAIRS")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(4_000_000);

    println!("cct_update: {pairs} call-pairs per variant (ns/pair = enter+close)");
    for functions in [1u32, 16, 1024, 4096] {
        // Warm-up pass then measured pass, best-of-3 (§10.4: best-of-N).
        let mut best = f64::MAX;
        let mut node_count = 0;
        for _ in 0..3 {
            let (ns, nodes) = bench_flat(functions, pairs);
            best = best.min(ns);
            node_count = nodes;
        }
        println!("  flat  functions={functions:<5} nodes={node_count:<6} ns_per_pair={best:.1}");
    }
    let mut best = f64::MAX;
    let mut node_count = 0;
    for _ in 0..3 {
        let (ns, nodes) = bench_depth14(pairs);
        best = best.min(ns);
        node_count = nodes;
    }
    println!("  depth14 chain             nodes={node_count:<6} ns_per_pair={best:.1}");
}
