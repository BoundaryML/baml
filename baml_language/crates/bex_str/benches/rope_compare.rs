//! Result-consuming binary/n-ary comparisons for deferred append trees.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use bex_str::BexStr;

fn build(part: &BexStr, n: usize, shared: bool) -> BexStr {
    let mut s = BexStr::empty();
    for _ in 0..n {
        s = if shared {
            BexStr::concat(s.clone(), part.clone())
        } else {
            BexStr::concat(s, part.clone())
        };
    }
    s
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
                    || build(part, n, shared).codepoint_len() as u64,
                    verify,
                );
                bench(
                    &format!("flatten_{label}_{kind}_{n}"),
                    || {
                        let s = build(part, n, shared);
                        let len = s.as_str().len();
                        (len + usize::from(s.as_str().ends_with('j'))) as u64
                    },
                    verify,
                );
            }
            bench(
                &format!("tail_shared_{kind}_{n}"),
                || {
                    let mut s = BexStr::empty();
                    let mut hits = 0;
                    for _ in 0..n {
                        s = BexStr::concat(s.clone(), short.clone());
                        hits += u64::from(s.char_at(s.codepoint_len() - 1) == Some('j'));
                    }
                    hits + s.codepoint_len() as u64
                },
                verify,
            );
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
                    s.codepoint_len() as u64
                        + snapshots
                            .iter()
                            .map(|s| s.codepoint_len() as u64)
                            .sum::<u64>()
                },
                verify,
            );
        }
    }
}
