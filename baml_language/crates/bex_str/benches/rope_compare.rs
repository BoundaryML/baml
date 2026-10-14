//! Result-consuming binary/n-ary comparisons for deferred append trees.
use std::{
    hint::black_box,
    ops::Deref,
    time::{Duration, Instant},
};

use bex_str::BexStr;

struct BuiltRope {
    value: BexStr,
    // Keep prior roots alive through reads and first flatten, not just append.
    _snapshots: Vec<BexStr>,
}
impl Deref for BuiltRope {
    type Target = BexStr;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
fn build(part: &BexStr, n: usize, shared: bool) -> BuiltRope {
    let mut s = BexStr::empty();
    let mut snapshots = Vec::new();
    for _ in 0..n {
        if shared {
            snapshots.push(s.clone());
        }
        s = BexStr::concat(s, part.clone());
    }
    BuiltRope {
        value: s,
        _snapshots: snapshots,
    }
}
fn bench(name: &str, mut f: impl FnMut() -> u64, verify: bool) {
    let expected = f();
    if verify {
        println!("{{\"bench\":\"{name}\",\"result\":{expected}}}");
        return;
    }
    let mut iters = 1;
    loop {
        let start = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        if start.elapsed() >= Duration::from_millis(5) {
            break;
        }
        iters *= 2;
    }
    let start = Instant::now();
    while start.elapsed() < Duration::from_millis(200) {
        for _ in 0..iters {
            black_box(f());
        }
    }
    let mut samples = Vec::new();
    for _ in 0..15 {
        let start = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        samples.push(start.elapsed().as_nanos() as f64 / iters as f64);
    }
    samples.sort_by(f64::total_cmp);
    println!(
        "{{\"bench\":\"{name}\",\"median_ns\":{},\"min_ns\":{},\"result\":{expected}}}",
        samples[7], samples[0]
    );
}
// Every input has a cold flatten cache. Construction and destruction happen
// outside the timed interval; cloning one root would accidentally warm it.
fn bench_first_flatten(name: &str, part: &BexStr, n: usize, shared: bool, verify: bool) {
    let consume = |s: &BexStr| {
        let text = s.as_str();
        (text.len() + usize::from(text.ends_with('j'))) as u64
    };
    let expected = consume(&build(part, n, shared));
    if verify {
        println!("{{\"bench\":\"{name}\",\"result\":{expected}}}");
        return;
    }
    const BATCH: usize = 32;
    let sample = || {
        let inputs: Vec<_> = (0..BATCH).map(|_| build(part, n, shared)).collect();
        let start = Instant::now();
        for s in &inputs {
            black_box(consume(black_box(s)));
        }
        let elapsed = start.elapsed().as_nanos() as f64 / BATCH as f64;
        drop(inputs);
        elapsed
    };
    let warmup = Instant::now();
    while warmup.elapsed() < Duration::from_millis(200) {
        black_box(sample());
    }
    let mut samples: Vec<_> = (0..15).map(|_| sample()).collect();
    samples.sort_by(f64::total_cmp);
    println!(
        "{{\"bench\":\"{name}\",\"median_ns\":{},\"min_ns\":{},\"result\":{expected}}}",
        samples[7], samples[0]
    );
}
fn main() {
    let verify = std::env::args().any(|arg| arg == "--verify");
    if cfg!(debug_assertions) && !verify {
        return;
    }
    for (kind, text) in [("ascii", "abcdefghij"), ("unicode", "abcd é😀ij")] {
        let short = BexStr::from(text);
        let long = BexStr::from(text.repeat(16));
        for n in [2_000, 10_000] {
            for (label, part, shared) in [
                ("shared_inline", &short, true),
                ("shared_flat", &long, true),
                ("owned_flat", &long, false),
            ] {
                bench(
                    &format!("build_{label}_{kind}_{n}"),
                    || build(part, n, shared).char_len() as u64,
                    verify,
                );
                bench(
                    &format!("build_and_flatten_{label}_{kind}_{n}"),
                    || {
                        let s = build(part, n, shared);
                        let len = s.as_str().len();
                        (len + usize::from(s.as_str().ends_with('j'))) as u64
                    },
                    verify,
                );
                bench_first_flatten(
                    &format!("first_flatten_{label}_{kind}_{n}"),
                    part,
                    n,
                    shared,
                    verify,
                );
            }
            bench(
                &format!("tail_cloned_left_{kind}_{n}"),
                || {
                    let mut s = BexStr::empty();
                    let mut hits = 0;
                    for _ in 0..n {
                        s = BexStr::concat(s.clone(), short.clone());
                        hits += u64::from(s.char_at(s.char_len() - 1) == Some('j'));
                    }
                    hits + s.char_len() as u64
                },
                verify,
            );
            for slice in [false, true] {
                bench(
                    &format!(
                        "tail_four_{}_cloned_left_{kind}_{n}",
                        if slice { "slice" } else { "char" }
                    ),
                    || {
                        let mut s = BexStr::empty();
                        let mut sum = 0;
                        for _ in 0..n {
                            s = BexStr::concat(s.clone(), short.clone());
                            let count = s.char_len();
                            for i in count - 4..count {
                                let point = if slice {
                                    s.char_slice(i..i + 1).as_str().chars().next().unwrap()
                                } else {
                                    s.char_at(i).unwrap()
                                };
                                sum += point as u64;
                            }
                        }
                        sum + s.char_len() as u64
                    },
                    verify,
                );
            }
            bench(
                &format!("snapshots_{kind}_{n}"),
                || {
                    let mut s = BexStr::empty();
                    let mut snapshots = Vec::new();
                    for i in 0..n {
                        s = BexStr::concat(s.clone(), short.clone());
                        if i % 128 == 0 {
                            snapshots.push(s.clone());
                        }
                    }
                    s.char_len() as u64 + snapshots.iter().map(|s| s.char_len() as u64).sum::<u64>()
                },
                verify,
            );
        }
    }
}
