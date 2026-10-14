//! Semantically matched, result-consuming comparisons with node_compare.mjs.
//! No black boxes inside the indexed loop: both engines may optimize results.
use std::{
    hint::black_box,
    time::{Duration, Instant},
};

use bex_str::BexStr;

const N: usize = 20_000;
fn unit(kind: &str) -> &'static str {
    match kind {
        "ascii" => "abcd fghij",
        "bmp" => "abcd éghij",
        "astral" => "abcd 😀ghij",
        _ => unreachable!(),
    }
}
fn fixture(kind: &str, repr: &str) -> BexStr {
    let text = unit(kind).repeat(N / 10);
    match repr {
        "flat" => BexStr::from(text),
        "slice" => {
            BexStr::from(format!("{}{}tail", "前é🙂xyz".repeat(17), text)).char_slice(102..102 + N)
        }
        "rope" | "rope_shared" => {
            let part = BexStr::from(unit(kind));
            let mut s = BexStr::empty();
            for _ in 0..N / 10 {
                s = if repr == "rope_shared" {
                    BexStr::concat(s.clone(), part.clone())
                } else {
                    BexStr::concat(s, part.clone())
                };
            }
            s
        }
        _ => unreachable!(),
    }
}
fn run(s: &BexStr, op: &str) -> u64 {
    match op {
        "at" | "slice" => {
            let mut hits = 0;
            for i in 0..N {
                hits += u64::from(s.char_slice(i..i + 1).as_str() == "a");
            }
            hits
        }
        "codepoint" => (0..N).map(|i| s.char_at(i).unwrap() as u64).sum(),
        "random" => (0..N)
            .map(|i| s.char_at((i * 7919) % N).unwrap() as u64)
            .sum(),
        "find_from" => {
            let mut pos = 0;
            while let Some(i) = s.char_find_from("a", pos) {
                pos = i + 1;
            }
            pos as u64
        }
        _ => unreachable!(),
    }
}
fn append(read: bool, shared: bool, kind: &str) -> u64 {
    let part = BexStr::from(unit(kind));
    let mut s = BexStr::empty();
    let mut hits = 0;
    for _ in 0..N / 10 {
        s = if shared {
            BexStr::concat(s.clone(), part.clone())
        } else {
            BexStr::concat(s, part.clone())
        };
        if read {
            hits += u64::from(s.char_slice(N.min(s.char_len()) - 1..s.char_len()).as_str() == "j");
        }
    }
    hits + s.char_len() as u64
}
fn replace_reads(kind: &str) -> u64 {
    let mut s = BexStr::from(unit(kind).repeat(N / 10));
    let mut hits = 0;
    for round in 0..50 {
        let (from, to) = if round % 2 == 0 {
            ("gh", "GH")
        } else {
            ("GH", "gh")
        };
        s = BexStr::from(s.as_str().replace(from, to));
        for r in 0..100 {
            hits += u64::from(
                s.char_slice((r * 7919 + round) % N..(r * 7919 + round) % N + 1)
                    .as_str()
                    == "a",
            );
        }
    }
    hits
}
fn bench(name: &str, mut f: impl FnMut() -> u64, verify: bool) {
    let expected = f();
    if verify {
        println!("{{\"bench\":\"{name}\",\"result\":{expected}}}");
        return;
    }
    let mut iters = 1;
    loop {
        let t = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        if t.elapsed() >= Duration::from_millis(5) || iters >= 1 << 20 {
            break;
        }
        iters *= 2;
    }
    let warm = Instant::now();
    while warm.elapsed() < Duration::from_millis(200) {
        for _ in 0..iters {
            black_box(f());
        }
    }
    let mut times = Vec::new();
    for _ in 0..15 {
        let t = Instant::now();
        for _ in 0..iters {
            black_box(f());
        }
        times.push(t.elapsed().as_nanos() as f64 / iters as f64);
    }
    times.sort_by(f64::total_cmp);
    println!(
        "{{\"bench\":\"{name}\",\"n\":{N},\"median_ns\":{},\"min_ns\":{},\"result\":{expected}}}",
        times[7], times[0]
    );
}
fn main() {
    if std::env::args().any(|arg| arg == "--memory") {
        for kind in ["ascii", "bmp", "astral"] {
            for repr in ["flat", "slice", "rope", "rope_shared"] {
                let s = fixture(kind, repr);
                println!(
                    "{{\"kind\":\"{kind}\",\"repr\":\"{repr}\",\"bytes\":{},\"codepoints\":{},\"retained_heap_bytes\":{},\"handle_bytes\":{},\"flat_header_bytes\":{}}}",
                    s.byte_len(),
                    s.char_len(),
                    s.retained_heap_bytes().unwrap(),
                    std::mem::size_of::<BexStr>(),
                    std::mem::size_of::<bex_str::FlatStr>()
                );
            }
        }
        return;
    }
    let verify = std::env::args().any(|arg| arg == "--verify");
    if cfg!(debug_assertions) && !verify {
        return;
    }
    let filter = std::env::args()
        .find_map(|arg| arg.strip_prefix("--filter=").map(str::to_owned))
        .unwrap_or_default();
    for kind in ["ascii", "bmp", "astral"] {
        for repr in ["flat", "slice", "rope", "rope_shared"] {
            let s = fixture(kind, repr);
            // Match the warmed Node fixture and exclude one-time index/flatten.
            s.as_str();
            s.char_at(N - 1);
            s.char_at(N - 1);
            for op in ["at", "slice", "codepoint", "random", "find_from"] {
                let name = format!("{op}_{kind}_{repr}");
                if name.contains(&filter) {
                    bench(&name, || run(black_box(&s), op), verify);
                }
            }
        }
        for (name, read, shared) in [
            ("append_tail", true, false),
            ("append_tail_shared", true, true),
            ("append_only", false, false),
        ] {
            let name = format!("{name}_{kind}");
            if name.contains(&filter) {
                bench(&name, || append(read, shared, kind), verify);
            }
        }
        let name = format!("build_then_read_{kind}");
        if name.contains(&filter) {
            bench(&name, || run(&fixture(kind, "rope"), "at"), verify);
        }
        let name = format!("build_then_read_shared_{kind}");
        if name.contains(&filter) {
            bench(&name, || run(&fixture(kind, "rope_shared"), "at"), verify);
        }
        let name = format!("replace_and_read_{kind}");
        if name.contains(&filter) {
            bench(&name, || replace_reads(kind), verify);
        }
        let name = format!("cold_random_{kind}");
        if name.contains(&filter) {
            bench(
                &name,
                || {
                    let s = BexStr::from(unit(kind).repeat(N / 10));
                    (0..100)
                        .map(|i| s.char_at((i * 7919) % N).unwrap() as u64)
                        .sum()
                },
                verify,
            );
        }
    }
}
