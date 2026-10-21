//! Performance regression coverage for the native primitive-array sort path.

use baml_tests::baml_test;
use bex_engine::BexExternalValue;

#[tokio::test]
async fn perf_large_int_array_uses_native_fast_path() {
    // 10k pseudo-random ints. On the native fast path this is instant; the
    // comparator path (CPS insertion sort) would make O(n²) ≈ 10⁷–10⁸ yields
    // into BAML and blow the coarse bound below by orders of magnitude — the
    // bound is the assertion that primitives no longer route through
    // `sort_by` + an `a.cmp(b)` comparator.
    let start = std::time::Instant::now();
    let output = baml_test!(
        r#"
        function main() -> int throws never {
            let xs: int[] = []
            let seed = 42
            let i = 0
            while (i < 10000) {
                seed = (seed * 1103515245 + 12345) % 2147483648
                xs.push(seed)
                i += 1
            }
            let result = xs.sort()
            if (result != xs) { return -3 }
            let j = 1
            while (j < 10000) {
                match (xs.at(j - 1)) {
                    null => { return -1 }
                    let a: int => {
                        match (xs.at(j)) {
                            null => { return -1 }
                            let b: int => { if (a > b) { return -2 } }
                        }
                    }
                }
                j += 1
            }
            return xs.length()
        }
        "#
    );
    assert_eq!(output.result.unwrap(), BexExternalValue::Int(10000));
    let elapsed = start.elapsed();
    assert!(
        elapsed < std::time::Duration::from_secs(60),
        "10k-int sort took {elapsed:?}; the native fast path should be far \
         under this bound — did primitives fall back to the comparator path?"
    );
}
