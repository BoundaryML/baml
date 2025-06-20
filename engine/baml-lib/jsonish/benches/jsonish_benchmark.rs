use criterion::{criterion_group, criterion_main};

mod classes;
mod lists;
mod literals;
mod partials;
mod unions;

use classes::bench_complex_classes;
use lists::bench_lists;
use literals::bench_literals;
use partials::bench_partials;
use unions::bench_unions;

criterion_group!(
    benches,
    bench_literals,
    bench_lists,
    bench_complex_classes,
    bench_unions,
    bench_partials
);
criterion_main!(benches);
