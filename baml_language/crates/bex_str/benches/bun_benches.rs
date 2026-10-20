//! Ports of Bun's string microbenchmarks to `BexStr`.
//!
//! Source (paths relative to the Bun repo root):
//! `bench/snippets/string-{search,includes,equality-16bit}.mjs` and
//! `bench/snippets/text-{encoder,decoder}.mjs`.
//!
//! Each benchmark is one call of the original benchmark's run function, with
//! the same inputs. JS string methods map to the BAML `String` method of the
//! same meaning, implemented in `common/mod.rs` (`mod baml`) exactly as
//! `bex_vm/src/package_baml/string.rs` implements it, so the timings are what
//! the BAML runtime would pay for the string work (minus VM dispatch).
//!
//! JS indexes by UTF-16 code unit; BAML by `char`. The two agree on every
//! input here except where noted at the benchmark.

use bex_str::BexStr;
use divan::{Bencher, black_box};

mod common;
use common::{baml, s};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping bun_benches in debug/test profile.");
        return;
    }
    divan::main();
}

/// JS `s += ch` repeated `len` times: an append-built rope, as in the originals.
fn make_string(len: usize, ch: &str) -> BexStr {
    let ch = s(ch);
    let mut out = BexStr::empty();
    for _ in 0..len {
        out = baml::add(&out, &ch);
    }
    out
}

// ═══ Bun: bench/snippets/string-search.mjs ═══════════════════════════════════
//
// Each benchmark runs one operation over 64 generated strings. The two regexp
// `match` benchmarks are not ported (BexStr has no regex engine).

const SEARCH_N: usize = 64;

struct SearchFixtures {
    urls: Vec<BexStr>,
    file_paths: Vec<BexStr>,
    csv_lines: Vec<BexStr>,
    sentences: Vec<BexStr>,
}

fn search_fixtures() -> SearchFixtures {
    let mut f = SearchFixtures {
        urls: vec![],
        file_paths: vec![],
        csv_lines: vec![],
        sentences: vec![],
    };
    for i in 0..SEARCH_N {
        let scheme = if i % 4 == 0 { "http" } else { "https" };
        f.urls.push(s(&format!(
            "{scheme}://example{i}.com/some/longer/path/to/a/resource{i}?query=string&value={i}"
        )));
        let ext = match i % 3 {
            0 => ".test.tsx",
            1 => ".tsx",
            _ => ".css",
        };
        f.file_paths.push(s(&format!(
            "/Users/someone/projects/my-app-{i}/src/components/Button{i}/index{ext}"
        )));
        f.csv_lines.push(s(&format!(
            "alpha{i},bravo,charlie,delta,echo,foxtrot,golf,hotel,india,juliett,kilo,lima{i}"
        )));
        f.sentences.push(s(&format!(
            "The quick brown fox {i} jumps over the lazy dog and then runs away into the quick forest {i}"
        )));
    }
    f
}

#[divan::bench]
fn bun_search_starts_with_urls(bencher: Bencher) {
    let f = search_fixtures();
    let p = s("https://");
    bencher.bench(|| {
        black_box(&f.urls)
            .iter()
            .filter(|u| baml::starts_with(u, &p))
            .count()
    });
}

#[divan::bench]
fn bun_search_ends_with_paths(bencher: Bencher) {
    let f = search_fixtures();
    let p = s(".test.tsx");
    bencher.bench(|| {
        black_box(&f.file_paths)
            .iter()
            .filter(|u| baml::ends_with(u, &p))
            .count()
    });
}

#[divan::bench]
fn bun_search_last_index_of_slash_paths(bencher: Bencher) {
    let f = search_fixtures();
    let p = s("/");
    bencher.bench(|| {
        black_box(&f.file_paths)
            .iter()
            .map(|u| baml::last_index_of(u, &p).unwrap_or(-1))
            .sum::<i64>()
    });
}

#[divan::bench]
fn bun_search_last_index_of_quick_sentences(bencher: Bencher) {
    let f = search_fixtures();
    let p = s("quick");
    bencher.bench(|| {
        black_box(&f.sentences)
            .iter()
            .map(|u| baml::last_index_of(u, &p).unwrap_or(-1))
            .sum::<i64>()
    });
}

#[divan::bench]
fn bun_search_split_comma_csv(bencher: Bencher) {
    let f = search_fixtures();
    let p = s(",");
    bencher.bench(|| {
        black_box(&f.csv_lines)
            .iter()
            .map(|u| baml::split(u, &p).len())
            .sum::<usize>()
    });
}

#[divan::bench]
fn bun_search_split_space_sentences(bencher: Bencher) {
    let f = search_fixtures();
    let p = s(" ");
    bencher.bench(|| {
        black_box(&f.sentences)
            .iter()
            .map(|u| baml::split(u, &p).len())
            .sum::<usize>()
    });
}

#[divan::bench]
fn bun_search_substring_urls(bencher: Bencher) {
    let f = search_fixtures();
    bencher.bench(|| {
        black_box(&f.urls)
            .iter()
            .map(|u| baml::length(&baml::slice(u, 8, 24)))
            .sum::<i64>()
    });
}

// ═══ Bun: bench/snippets/string-includes.mjs ═════════════════════════════════

const INCLUDES_SHORT: &str = "The quick brown fox jumps over the lazy dog";

fn includes_bench(bencher: Bencher, long: bool, needle: &str) {
    let hay = if long {
        s(&INCLUDES_SHORT.repeat(100))
    } else {
        s(INCLUDES_SHORT)
    };
    let needle = s(needle);
    bencher.bench(|| baml::includes(black_box(&hay), &needle));
}

#[divan::bench]
fn bun_includes_short_hit_middle(bencher: Bencher) {
    includes_bench(bencher, false, "jumps");
}

#[divan::bench]
fn bun_includes_short_hit_start(bencher: Bencher) {
    includes_bench(bencher, false, "The");
}

#[divan::bench]
fn bun_includes_short_hit_end(bencher: Bencher) {
    includes_bench(bencher, false, "dog");
}

#[divan::bench]
fn bun_includes_short_miss(bencher: Bencher) {
    includes_bench(bencher, false, "cat");
}

#[divan::bench]
fn bun_includes_long_hit_middle(bencher: Bencher) {
    includes_bench(bencher, true, "jumps");
}

#[divan::bench]
fn bun_includes_long_miss(bencher: Bencher) {
    includes_bench(bencher, true, "cat");
}

/// `includes("fox", 10)`: BAML's `includes` takes no position, so this uses
/// the positional search behind `index_of` with a start.
#[divan::bench]
fn bun_includes_with_position(bencher: Bencher) {
    let hay = s(INCLUDES_SHORT);
    let needle = s("fox");
    bencher.bench(|| baml::index_of_from(black_box(&hay), &needle, 10).is_some());
}

// ═══ Bun: bench/snippets/string-equality-16bit.mjs ═══════════════════════════
//
// The original times 1e6 iterations of { a === b; a === c } on two distinct,
// append-built strings with equal content and one that differs in the last
// char. One benchmark call here is one iteration.

fn equality_bench(bencher: Bencher, len: usize) {
    let a = baml::add(&make_string(len, "α"), &s("β"));
    let b = baml::add(&make_string(len, "α"), &s("β"));
    let c = baml::add(&make_string(len, "α"), &s("γ"));
    bencher.bench(|| {
        let (a, b, c) = black_box((&a, &b, &c));
        i32::from(a == b) + i32::from(a == c)
    });
}

#[divan::bench]
fn bun_equality_65_char_16bit(bencher: Bencher) {
    equality_bench(bencher, 64);
}

#[divan::bench]
fn bun_equality_256_char_16bit(bencher: Bencher) {
    equality_bench(bencher, 255);
}

// ═══ Bun: bench/snippets/text-encoder.mjs and text-decoder.mjs ═══════════════
//
// `TextEncoder.encode(s)` → `s.to_utf8()`; `TextDecoder.decode(b)` →
// `string.from_utf8(b)`. The Node `Buffer#toString` variants are not ported.

const TEXT_SHORT: &str = "Hello World!";
const TEXT_SHORT_UTF16: &str = "Hello World 💕💕💕";

fn encode_bench(bencher: Bencher, text: BexStr) {
    bencher.bench(|| baml::to_utf8(black_box(&text)));
}

#[divan::bench]
fn bun_encode_4_ascii(bencher: Bencher) {
    encode_bench(bencher, s("heyo"));
}

#[divan::bench]
fn bun_encode_4_utf8(bencher: Bencher) {
    encode_bench(bencher, s("💕💕"));
}

#[divan::bench]
fn bun_encode_short_ascii(bencher: Bencher) {
    encode_bench(bencher, s(TEXT_SHORT));
}

#[divan::bench]
fn bun_encode_short_utf8(bencher: Bencher) {
    encode_bench(bencher, s(TEXT_SHORT_UTF16));
}

#[divan::bench]
fn bun_encode_long_ascii(bencher: Bencher) {
    encode_bench(bencher, s(&TEXT_SHORT.repeat(1024)));
}

#[divan::bench]
fn bun_encode_long_utf8(bencher: Bencher) {
    encode_bench(bencher, s(&TEXT_SHORT_UTF16.repeat(1024)));
}

fn decode_bench(bencher: Bencher, text: &str) {
    let bytes = text.as_bytes().to_vec();
    bencher.bench(|| baml::from_utf8(black_box(&bytes)));
}

#[divan::bench]
fn bun_decode_short_ascii(bencher: Bencher) {
    decode_bench(bencher, TEXT_SHORT);
}

#[divan::bench]
fn bun_decode_short_utf8(bencher: Bencher) {
    decode_bench(bencher, TEXT_SHORT_UTF16);
}

#[divan::bench]
fn bun_decode_long_ascii(bencher: Bencher) {
    decode_bench(bencher, &TEXT_SHORT.repeat(1024));
}

#[divan::bench]
fn bun_decode_long_utf8(bencher: Bencher) {
    decode_bench(bencher, &TEXT_SHORT_UTF16.repeat(1024));
}
