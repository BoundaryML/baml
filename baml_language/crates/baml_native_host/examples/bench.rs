//! Time calls through the bindings of `tests/type_shapes`, for the
//! native bridges report.
//!
//! ```text
//! bench <libtype_shapes.dylib> <case> <n>
//! ```
//!
//! Each case calls one function `n` times, one call after the other, on a
//! tokio runtime with one thread, and prints the time per call. The cases
//! with a size do one call with that many elements. Instructions per call
//! come from running a case at two sizes under `/usr/bin/time -l`.
#![allow(
    clippy::print_stdout,
    clippy::print_stderr,
    reason = "a benchmark that prints its numbers"
)]

#[path = "../tests/type_shapes/bindings.rs"]
mod bindings;
#[path = "../tests/type_shapes/profile.rs"]
mod profile;

use std::time::Instant;

use bindings::{bridge_test, class_refs, complex_models, lists, primitives};

fn main() {
    let arguments: Vec<String> = std::env::args().collect();
    let [_, path, case, n] = arguments.as_slice() else {
        eprintln!("usage: bench <library> <case> <n>");
        std::process::exit(2);
    };
    let n: i64 = n.parse().expect("a count");
    bindings::_runtime::init(path).expect("the library loads");
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_time()
        .build()
        .expect("a runtime");
    let started = Instant::now();
    let calls = runtime.block_on(async {
        match case.as_str() {
            "add" => {
                for i in 0..n {
                    assert_eq!(bridge_test::add(i, 1).await.expect("add"), i + 1);
                }
                n
            }
            "int" => {
                for i in 0..n {
                    assert_eq!(primitives::round_trip_int(i).await.expect("int"), i);
                }
                n
            }
            "inner" => {
                for i in 0..n {
                    let inner = class_refs::Inner { value: i };
                    assert_eq!(
                        class_refs::round_trip_inner(&inner).await.expect("inner"),
                        inner
                    );
                }
                n
            }
            "profile" => {
                let profile = profile::complex_profile();
                for _ in 0..n {
                    let back = complex_models::round_trip_complex_profile(&profile)
                        .await
                        .expect("profile");
                    assert_eq!(back.id, profile.id);
                }
                n
            }
            // One call each, with `n` elements.
            "ints_out" => {
                let xs = bridge_test::make_ints(n).await.expect("ints");
                assert_eq!(i64::try_from(xs.len()).expect("small"), n);
                1
            }
            "ints_in" => {
                let xs: Vec<i64> = (0..n).collect();
                assert_eq!(bridge_test::count_ints(xs).await.expect("count"), n);
                1
            }
            "ints_round_trip" => {
                let xs: Vec<i64> = (0..n).collect();
                assert_eq!(
                    lists::round_trip_ints(xs).await.expect("ints").len(),
                    xs_len(n)
                );
                1
            }
            "points_out" => {
                let points = bridge_test::make_points(n).await.expect("points");
                assert_eq!(i64::try_from(points.len()).expect("small"), n);
                1
            }
            "points_in" => {
                let points: Vec<bridge_test::Point> = (0..n)
                    .map(|i| bridge_test::Point { x: i, y: n - i })
                    .collect();
                assert_eq!(bridge_test::count_points(points).await.expect("count"), n);
                1
            }
            other => panic!("no case {other}"),
        }
    });
    let elapsed = started.elapsed();
    #[expect(clippy::cast_precision_loss, reason = "a rate")]
    let each = elapsed.as_nanos() as f64 / calls as f64;
    println!("{case} n={n}: {elapsed:?}, {each:.1} ns per call");
}

fn xs_len(n: i64) -> usize {
    usize::try_from(n).expect("small")
}
