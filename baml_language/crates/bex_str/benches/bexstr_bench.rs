use bex_str::BexStr;
use divan::{Bencher, black_box};
use indexmap::IndexMap;

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping bexstr_bench in debug/test profile.");
        return;
    }
    divan::main();
}

#[divan::bench]
fn bench_clone_inline(bencher: Bencher) {
    let s = BexStr::from("hello");
    bencher.bench(|| black_box(s.clone()));
}

#[divan::bench]
fn bench_clone_flat(bencher: Bencher) {
    let s = BexStr::from("a".repeat(100));
    bencher.bench(|| black_box(s.clone()));
}

#[divan::bench]
fn bench_concat_loop_1k(bencher: Bencher) {
    bencher.bench(|| {
        let mut s = BexStr::from("x");
        for _ in 0..1000 {
            s = BexStr::concat(s, BexStr::from("y"));
        }
        black_box(s.as_str());
    });
}

#[divan::bench]
fn bench_substring_chain_100(bencher: Bencher) {
    let base = BexStr::from("a".repeat(10_000));
    bencher.bench(|| {
        let mut s = base.clone();
        for _ in 0..100 {
            let new_len = s.len() - 1;
            s = s.substring(0, new_len);
        }
        black_box(s.len());
    });
}

#[divan::bench]
#[allow(clippy::mutable_key_type)]
fn bench_map_lookup(bencher: Bencher) {
    let mut map: IndexMap<BexStr, i32> = IndexMap::new();
    for i in 0..100 {
        map.insert(BexStr::from(format!("key_{i}")), i);
    }
    bencher.bench(|| {
        black_box(map.get("key_50"));
    });
}
