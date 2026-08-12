//! GC integration tests at the BAML language level.
//!
//! These tests exercise generational GC correctness through realistic BAML
//! programs: heavy allocation loops that generate many short-lived objects,
//! and programs that mix allocation with data access.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

/// Test that a program with heavy allocation pressure runs correctly
/// under generational GC.
///
/// Allocates ~1000 short-lived 3-element array objects across 1000 iterations
/// to exercise Gen0 collection and ensure that the accumulator `sum`
/// is correctly preserved through every GC cycle.
///
/// Expected: sum = 0*3 + 1*3 + 2*3 + ... + 999*3 = 3 * (999 * 1000 / 2) = 1498500
#[tokio::test]
async fn test_heavy_allocation_loop() {
    let output = baml_test!(
        r#"
        function main() -> int {
            let sum = 0;
            let i = 0;
            while (i < 1000) {
                let arr = [i, i * 2, i * 3];
                sum = sum + arr[2];
                i = i + 1;
            }
            sum
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(1498500)));
}

/// Test that a map heap object survives GC cycles and its data remains intact.
///
/// Builds a map, triggers allocation pressure with many short-lived arrays so
/// the GC has chances to run and move objects, then reads the map's length to
/// confirm no stale pointer corruption occurred in the map heap object.
///
/// Note: map element access via `m["key"]` is not yet supported by compiler2
/// (see existing ignored tests in maps.rs). We verify GC correctness via the
/// map's length, which requires the map heap object pointer to remain valid.
#[tokio::test]
async fn test_map_survives_gc_during_operations() {
    let output = baml_test!(
        r#"
        function build_map() -> map<string, int> {
            {
                "a": 1,
                "b": 4,
                "c": 9,
                "d": 16,
                "e": 25
            }
        }

        function main() -> int {
            let m = build_map();

            // Trigger allocation pressure with many short-lived arrays so
            // the GC has chances to run and move objects, including the map.
            let i = 0;
            while (i < 500) {
                let tmp = [i, i + 1, i + 2];
                i = i + 1;
            }

            m.length()
        }
        "#
    );
    assert_eq!(output.result, Ok(BexExternalValue::Int(5)));
}
