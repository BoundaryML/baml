use bex_str::BexStr;
use divan::{Bencher, black_box};

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
