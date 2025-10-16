#[cfg(not(target_arch = "wasm32"))]
use criterion::{criterion_group, criterion_main};

#[cfg(not(target_arch = "wasm32"))]
mod classes;
#[cfg(not(target_arch = "wasm32"))]
mod lists;
#[cfg(not(target_arch = "wasm32"))]
mod literals;
#[cfg(not(target_arch = "wasm32"))]
mod partials;
#[cfg(not(target_arch = "wasm32"))]
mod unions;

#[cfg(not(target_arch = "wasm32"))]
use classes::bench_complex_classes;
#[cfg(not(target_arch = "wasm32"))]
use lists::bench_lists;
#[cfg(not(target_arch = "wasm32"))]
use literals::bench_literals;
#[cfg(not(target_arch = "wasm32"))]
use partials::bench_partials;
#[cfg(not(target_arch = "wasm32"))]
use unions::bench_unions;

#[cfg(not(target_arch = "wasm32"))]
criterion_group!(
    benches,
    bench_literals,
    bench_lists,
    bench_complex_classes,
    bench_unions,
    bench_partials
);
#[cfg(not(target_arch = "wasm32"))]
criterion_main!(benches);

#[cfg(target_arch = "wasm32")]
fn main() {
    // No-op for WASM builds
}
