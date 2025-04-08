use criterion::{criterion_group, criterion_main};

mod literals;
mod lists;
mod classes;
mod unions;
mod partials;

use literals::bench_literals;
use lists::bench_lists;
use classes::bench_complex_classes;
use unions::bench_unions;
use partials::bench_partials;

criterion_group!(
    benches,
    bench_literals,
    bench_lists,
    bench_complex_classes,
    bench_unions,
    bench_partials
);
criterion_main!(benches);
