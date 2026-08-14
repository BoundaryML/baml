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

// ── Codepoint offset scan strategies ──────────────────────────────────────────
//
// These benchmark finding the byte offset of the Nth codepoint in a UTF-8
// string. This is the hot path for .substring() and .char_at().

/// Current approach: Rust's char_indices iterator.
fn nth_codepoint_char_indices(s: &str, n: usize) -> usize {
    s.char_indices().nth(n).map_or(s.len(), |(i, _)| i)
}

/// Word-level scan: process 8 bytes at a time, count leading bytes via popcount.
fn nth_codepoint_word_scan(bytes: &[u8], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut remaining = n;
    let mut i = 0;

    // Process 8 bytes at a time
    while i + 8 <= bytes.len() {
        let word = u64::from_ne_bytes(bytes[i..i + 8].try_into().unwrap());
        // A continuation byte has bit7=1 AND bit6=0, i.e. (b & 0xC0) == 0x80.
        let hi = word & 0x8080808080808080;
        let lo = word & 0x4040404040404040;
        let cont_mask = hi & !lo;
        let num_leading = 8 - (cont_mask >> 7).count_ones() as usize;

        if remaining <= num_leading {
            // Target is within this word, fall back to scalar
            break;
        }
        remaining -= num_leading;
        i += 8;
    }

    // Scalar scan for remaining bytes
    while i < bytes.len() {
        if (bytes[i] & 0xC0) != 0x80 {
            remaining -= 1;
            if remaining == 0 {
                return i;
            }
        }
        i += 1;
    }
    bytes.len()
}

/// Binary search + SIMD bytecount: O(log n) iterations of SIMD prefix scans.
fn nth_codepoint_binary_search(bytes: &[u8], n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut lo: usize = 0;
    let mut hi: usize = bytes.len();

    while lo < hi {
        let mid = (lo + hi) / 2;
        // Snap to char boundary (scan forward to next leading byte)
        let mut mid_snapped = mid;
        while mid_snapped < bytes.len() && (bytes[mid_snapped] & 0xC0) == 0x80 {
            mid_snapped += 1;
        }
        let count = bytecount::num_chars(&bytes[..mid_snapped]);
        if count < n {
            lo = mid_snapped + 1;
        } else if count > n {
            hi = mid;
        } else {
            return mid_snapped;
        }
    }
    // Final snap to char boundary
    while lo < bytes.len() && (bytes[lo] & 0xC0) == 0x80 {
        lo += 1;
    }
    lo
}

// ASCII string — best case for all approaches
#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_ascii_char_indices(bencher: Bencher, n: usize) {
    let s = "a".repeat(10_001);
    bencher.bench(|| black_box(nth_codepoint_char_indices(&s, n)));
}

#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_ascii_word_scan(bencher: Bencher, n: usize) {
    let s = "a".repeat(10_001);
    bencher.bench(|| black_box(nth_codepoint_word_scan(s.as_bytes(), n)));
}

#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_ascii_binary_search(bencher: Bencher, n: usize) {
    let s = "a".repeat(10_001);
    bencher.bench(|| black_box(nth_codepoint_binary_search(s.as_bytes(), n)));
}

// Mixed UTF-8 — realistic LLM output with occasional multibyte
#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_mixed_char_indices(bencher: Bencher, n: usize) {
    let s = "hello 😀 world héllo café résumé 日本語 ".repeat(300); // ~12K chars
    bencher.bench(|| black_box(nth_codepoint_char_indices(&s, n)));
}

#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_mixed_word_scan(bencher: Bencher, n: usize) {
    let s = "hello 😀 world héllo café résumé 日本語 ".repeat(300);
    bencher.bench(|| black_box(nth_codepoint_word_scan(s.as_bytes(), n)));
}

#[divan::bench(args = [100, 1000, 5000, 10000])]
fn nth_codepoint_mixed_binary_search(bencher: Bencher, n: usize) {
    let s = "hello 😀 world héllo café résumé 日本語 ".repeat(300);
    bencher.bench(|| black_box(nth_codepoint_binary_search(s.as_bytes(), n)));
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
