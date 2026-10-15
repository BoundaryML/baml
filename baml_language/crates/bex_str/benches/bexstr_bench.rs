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
            let new_len = s.byte_len() - 1;
            s = s.byte_slice(0..new_len);
        }
        black_box(s.byte_len());
    });
}

// ── Codepoint offset scan strategies ──────────────────────────────────────────
//
// These benchmark finding the byte offset of the Nth codepoint in a UTF-8
// string. This is the hot path for `char_slice` and `char_at`.

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

// ── Index-by-index scans (B-1918) ──────────────────────────────────────────────
//
// A parser that walks a string with `at(i)` / `slice(i, i + 1)` makes `n`
// codepoint-indexed lookups. Each argument doubles `n`: a linear scan roughly
// doubles the time, a quadratic one (a lookup that rescans from byte 0)
// roughly quadruples it. The non-ASCII fixtures carry one 2-byte char per 10
// codepoints, typical of LLM output with an occasional accent or emoji.

const SCAN_SIZES: [usize; 3] = [5_000, 10_000, 20_000];

fn scan_fixture(kind: &str, n: usize) -> BexStr {
    let unit = match kind {
        "ascii" => "abcdefghij",
        _ => "abcdéfghij",
    };
    let flat = BexStr::from(unit.repeat(n / 10 + 1));
    match kind {
        // A view that starts on a non-ASCII char, off any aligned boundary.
        "slice" => {
            let start = flat.as_str().find('é').unwrap();
            let end = start + flat.as_str()[start..].char_indices().nth(n).unwrap().0;
            flat.byte_slice(start..end)
        }
        // Built by appends, as `s = s + unit` in BAML.
        "concat" => {
            let mut s = BexStr::empty();
            for _ in 0..n / 10 + 1 {
                s = BexStr::concat(s, BexStr::from(unit));
            }
            s
        }
        _ => flat,
    }
}

/// BAML `s.at(i)` for an in-range `i`, as `bex_vm`'s `string.rs` implements it.
fn at(s: &BexStr, i: usize) -> Option<BexStr> {
    (i < s.char_len()).then(|| s.char_slice(i..i + 1))
}

/// Each sample gets a fresh fixture, built outside the timed region, so a
/// concat is still deferred when its scan starts rather than flattened by an
/// earlier sample.
fn bench_at_scan(bencher: Bencher, kind: &str, n: usize) {
    bencher
        .with_inputs(|| scan_fixture(kind, n))
        .bench_local_refs(|s| {
            for i in 0..n {
                black_box(at(s, black_box(i)));
            }
        });
}

#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn at_scan_ascii_flat(bencher: Bencher, n: usize) {
    bench_at_scan(bencher, "ascii", n);
}

#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn at_scan_non_ascii_flat(bencher: Bencher, n: usize) {
    bench_at_scan(bencher, "non_ascii", n);
}

#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn at_scan_non_ascii_slice(bencher: Bencher, n: usize) {
    bench_at_scan(bencher, "slice", n);
}

#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn at_scan_non_ascii_concat(bencher: Bencher, n: usize) {
    bench_at_scan(bencher, "concat", n);
}

// ── Deep ropes (B-1820) ────────────────────────────────────────────────────────
//
// `s = s + "x"` builds a left-leaning rope as deep as the append count. Every
// operation on it must stay iterative (no native recursion proportional to
// depth) and `char_len` must not walk it.

#[divan::bench(args = [100_000, 1_000_000], sample_count = 5, sample_size = 1)]
fn deep_rope_build_count_flatten(bencher: Bencher, n: usize) {
    bencher.bench(|| {
        let mut s = BexStr::from("x".repeat(60));
        for _ in 0..n {
            s = BexStr::concat(s, BexStr::from("é"));
        }
        black_box(s.char_len());
        black_box(s.as_str().len());
    });
}

// ── Workloads (00-design.md §2) ─────────────────────────────────────────────────
//
// End-to-end shapes of BAML string code, built from the same `BexStr` calls the
// VM makes (`+` → `concat`, `replace_all` → `BexStr::from(str::replace)`, `at` →
// `char_slice(i..i + 1)`). Each doubles `n` (codepoints); compare growth factors,
// and compare `w3_append_and_read_*` against `w3_append_only` to see what
// reading costs on top of writing.

/// `n` codepoints of prose-like text; one `é` per 10 codepoints when non-ASCII.
fn workload_unit(ascii: bool) -> &'static str {
    if ascii { "abcd fghij" } else { "abcd éghij" }
}

fn replace_all(s: &BexStr, from: &str, to: &str) -> BexStr {
    BexStr::from(s.as_str().replace(from, to))
}

/// W1 (common): a chain of `replace_all` rewrites, then index every codepoint.
#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn w1_replace_all_then_read_non_ascii(n: usize) -> usize {
    let mut s = BexStr::from(workload_unit(false).repeat(n / 10));
    for (from, to) in [("a", "A"), ("b", "B"), ("A", "a"), ("B", "b")] {
        s = replace_all(&s, from, to);
    }
    let mut hits = 0;
    for i in 0..s.char_len() {
        if at(&s, i).is_some_and(|c| c.as_str() == "a") {
            hits += 1;
        }
    }
    hits
}

/// W3 (mixed): append one unit, then read the last codepoint, `n / 10` times —
/// a builder that inspects what it has built so far. Every read after an
/// append must see the new tail.
#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn w3_append_and_read_ascii(n: usize) -> usize {
    let unit = BexStr::from(workload_unit(true));
    let mut s = BexStr::empty();
    let mut hits = 0;
    for _ in 0..n / 10 {
        s = BexStr::concat(s, unit.clone());
        if at(&s, s.char_len() - 1).is_some_and(|c| c.as_str() == "j") {
            hits += 1;
        }
    }
    hits + s.char_len()
}

/// W3 on non-ASCII text.
#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn w3_append_and_read_non_ascii(n: usize) -> usize {
    let unit = BexStr::from(workload_unit(false));
    let mut s = BexStr::empty();
    let mut hits = 0;
    for _ in 0..n / 10 {
        s = BexStr::concat(s, unit.clone());
        if at(&s, s.char_len() - 1).is_some_and(|c| c.as_str() == "j") {
            hits += 1;
        }
    }
    hits + s.char_len()
}

/// W3's appends without the reads, on non-ASCII text.
#[divan::bench(args = SCAN_SIZES, sample_count = 10, sample_size = 1)]
fn w3_append_only(n: usize) -> usize {
    let unit = BexStr::from(workload_unit(false));
    let mut s = BexStr::empty();
    for _ in 0..n / 10 {
        s = BexStr::concat(s, unit.clone());
    }
    s.char_len()
}
