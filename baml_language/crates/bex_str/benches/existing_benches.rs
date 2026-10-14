//! Ports of the existing V8 and Bun string microbenchmarks to `BexStr`.
//!
//! Sources (read from local checkouts; paths relative to each repo root):
//! - V8, as vendored in Node: `deps/v8/test/js-perf-test/Strings/*.js` and
//!   `deps/v8/test/js-perf-test/StringIterators/string-iterator.js`.
//! - Bun: `bench/snippets/string-{search,includes,equality-16bit}.mjs`,
//!   `bench/snippets/text-{encoder,decoder}.mjs`.
//!
//! Each benchmark is one call of the original benchmark's run function, with
//! the same inputs. JS string methods map to the BAML `String` method of the
//! same meaning, implemented below (`mod baml`) exactly as
//! `bex_vm/src/package_baml/string.rs` implements it, so the timings are what
//! the BAML runtime would pay for the string work (minus VM dispatch).
//!
//! JS indexes by UTF-16 code unit; BAML by `char`. The two agree on every
//! input here except where noted at the benchmark.

use bex_str::BexStr;
use divan::{Bencher, black_box};

fn main() {
    if cfg!(debug_assertions) {
        eprintln!("Skipping existing_benches in debug/test profile.");
        return;
    }
    divan::main();
}

/// BAML `String` methods, mirroring `bex_vm/src/package_baml/string.rs` and
/// `bex_vm/src/array_index.rs` (which `bex_str` cannot depend on).
mod baml {
    use bex_str::BexStr;

    fn resolve_index(index: i64, len: usize) -> Option<usize> {
        let len_i64 = i64::try_from(len).ok()?;
        let i = if index < 0 { index + len_i64 } else { index };
        usize::try_from(i).ok().filter(|&i| i < len)
    }

    fn resolve_slice_bound(bound: i64, len: usize) -> usize {
        let len_i64 = i64::try_from(len).unwrap_or(i64::MAX);
        let resolved = if bound < 0 { bound + len_i64 } else { bound };
        usize::try_from(resolved.clamp(0, len_i64)).unwrap_or(len)
    }

    /// `s.length()`
    pub fn length(s: &BexStr) -> i64 {
        s.char_count() as i64
    }

    /// `s.at(i)`
    pub fn at(s: &BexStr, i: i64) -> Option<BexStr> {
        resolve_index(i, s.char_count()).map(|i| s.char_slice(i..i + 1))
    }

    /// `s.code_point_at(i)`
    pub fn code_point_at(s: &BexStr, i: i64) -> Option<i64> {
        resolve_index(i, s.char_count())
            .and_then(|i| s.char_at(i))
            .map(|c| i64::from(u32::from(c)))
    }

    /// `s.slice(start, end)`
    pub fn slice(s: &BexStr, start: i64, end: i64) -> BexStr {
        let len = s.char_count();
        let start = resolve_slice_bound(start, len);
        let end = resolve_slice_bound(end, len).max(start);
        s.char_slice(start..end)
    }

    /// `s.index_of(x)`
    pub fn index_of(s: &BexStr, x: &BexStr) -> Option<i64> {
        s.char_find(x.as_str()).map(|i| i as i64)
    }

    /// `baml.regex._index_of_from(s, x, from)` (behind `index_of` with a start)
    pub fn index_of_from(s: &BexStr, x: &BexStr, from: i64) -> Option<i64> {
        let from = usize::try_from(from).ok()?;
        s.char_find_from(x.as_str(), from).map(|i| i as i64)
    }

    /// `s.last_index_of(x)`
    pub fn last_index_of(s: &BexStr, x: &BexStr) -> Option<i64> {
        s.char_rfind(x.as_str()).map(|i| i as i64)
    }

    pub fn includes(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().contains(x.as_str())
    }

    pub fn starts_with(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().starts_with(x.as_str())
    }

    pub fn ends_with(s: &BexStr, x: &BexStr) -> bool {
        s.as_str().ends_with(x.as_str())
    }

    fn char_substrings(s: &BexStr) -> Vec<BexStr> {
        s.as_str()
            .char_indices()
            .map(|(start, ch)| s.byte_slice(start..start + ch.len_utf8()))
            .collect()
    }

    /// `s.chars()`
    pub fn chars(s: &BexStr) -> Vec<BexStr> {
        char_substrings(s)
    }

    /// `s.split(d)`
    pub fn split(s: &BexStr, d: &BexStr) -> Vec<BexStr> {
        let text = s.as_str();
        let d = d.as_str();
        if d.is_empty() {
            return char_substrings(s);
        }
        let base = text.as_ptr() as usize;
        text.split(d)
            .map(|part| {
                let start = part.as_ptr() as usize - base;
                s.byte_slice(start..start + part.len())
            })
            .collect()
    }

    pub fn to_lower_case(s: &BexStr) -> BexStr {
        BexStr::from(s.as_str().to_lowercase())
    }

    pub fn to_upper_case(s: &BexStr) -> BexStr {
        BexStr::from(s.as_str().to_uppercase())
    }

    pub fn repeat(s: &BexStr, n: i64) -> BexStr {
        s.repeat(usize::try_from(n.max(0)).unwrap_or(0))
    }

    /// `s.to_utf8()`
    pub fn to_utf8(s: &BexStr) -> Vec<u8> {
        s.as_bytes().to_vec()
    }

    /// `string.from_utf8(bytes)`
    pub fn from_utf8(bytes: &[u8]) -> Option<BexStr> {
        std::str::from_utf8(bytes).ok().map(BexStr::from)
    }

    /// `string.from_code_points([n])`; `None` where BAML throws (surrogates).
    pub fn from_code_point(n: i64) -> Option<BexStr> {
        let c = u32::try_from(n).ok().and_then(char::from_u32)?;
        let mut s = String::with_capacity(4);
        s.push(c);
        Some(BexStr::from(s))
    }

    /// `a + b`
    pub fn add(a: &BexStr, b: &BexStr) -> BexStr {
        BexStr::concat(a.clone(), b.clone())
    }
}

fn s(x: &str) -> BexStr {
    BexStr::from(x)
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

// ═══ V8: Strings/string-stringat.js ═══════════════════════════════════════════
//
// `charCodeAt` / `codePointAt` at a few fixed indices. `charCodeAt` maps to
// BAML `code_point_at` (BAML has no UTF-16 code-unit accessor). The
// `*Constant` suites differ from `*NonConstant` only by an `| 0` JIT type hint,
// so one port covers both.

const STRINGAT_INDICES: [i64; 4] = [1, 13, 32, 100];
const STRINGAT_INDICES_INBOUNDS: [i64; 4] = [1, 7, 13, 17];

fn code_point_sum(s: &BexStr, indices: &[i64]) -> i64 {
    // Out-of-range indices are NaN in JS and null in BAML; count them as 0.
    indices
        .iter()
        .map(|&i| baml::code_point_at(s, i).unwrap_or(0))
        .sum()
}

#[divan::bench]
fn v8_string_char_code_at_non_constant(bencher: Bencher) {
    let string = s("qweruiplkjhgfdsazxccvbnm");
    bencher.bench(|| code_point_sum(black_box(&string), &STRINGAT_INDICES));
}

#[divan::bench]
fn v8_string_char_code_at_non_constant_inbounds(bencher: Bencher) {
    let string = s("qweruiplkjhgfdsazxccvbnm");
    bencher.bench(|| code_point_sum(black_box(&string), &STRINGAT_INDICES_INBOUNDS));
}

/// The JS literal holds `U+FFFD` (written as a raw replacement character).
const UNICODE_STRING: &str = "qweräϠ\u{FFFD}𝌆krefdäϠ\u{FFFD}𝌆ccäϠ\u{FFFD}𝌆";

#[divan::bench]
fn v8_string_code_point_at_non_constant(bencher: Bencher) {
    let string = s(UNICODE_STRING);
    bencher.bench(|| code_point_sum(black_box(&string), &STRINGAT_INDICES));
}

#[divan::bench]
fn v8_string_code_point_at_non_constant_inbounds(bencher: Bencher) {
    let string = s(UNICODE_STRING);
    bencher.bench(|| code_point_sum(black_box(&string), &STRINGAT_INDICES_INBOUNDS));
}

// ═══ V8: Strings/string-stringat-comp.js ═════════════════════════════════════
//
// For every index of a short mixed-width string, compare characters.
// `charCodeAt` → `code_point_at`; `charAt` and `str[i]` → `at`. The input has
// one astral char (𝌆): JS walks 29 code units, BAML 28 chars, and BAML's `at`
// returns the whole 𝌆 where JS returns half of it (so `charAt_never` can match
// once in BAML).

const STRINGAT_COMP_INPUT: &str = "äϠ\u{FFFD}𝌆 Lorem ipsum test test";

fn stringat_comp(bencher: Bencher, f: impl Fn(&BexStr, i64) -> bool + Sync) {
    let input = s(STRINGAT_COMP_INPUT);
    bencher.bench(|| {
        let mut sum = 0;
        for i in 0..baml::length(&input) {
            sum += i64::from(f(black_box(&input), i));
        }
        sum
    });
}

#[divan::bench]
fn v8_char_code_at_const(bencher: Bencher) {
    stringat_comp(bencher, |s, i| baml::code_point_at(s, i) == Some(116));
}

#[divan::bench]
fn v8_char_code_at_both(bencher: Bencher) {
    stringat_comp(bencher, |s, i| {
        baml::code_point_at(s, i) == baml::code_point_at(s, i)
    });
}

#[divan::bench]
fn v8_char_at_const(bencher: Bencher) {
    let t = s("t");
    stringat_comp(bencher, move |s, i| baml::at(s, i).as_ref() == Some(&t));
}

#[divan::bench]
fn v8_char_at_never(bencher: Bencher) {
    let never = s("𝌆");
    stringat_comp(bencher, move |s, i| baml::at(s, i).as_ref() == Some(&never));
}

#[divan::bench]
fn v8_char_at_both(bencher: Bencher) {
    stringat_comp(bencher, |s, i| baml::at(s, i) == baml::at(s, i));
}

/// `str[i] === 't'`: BAML has no separate index operator, so this is `at`.
#[divan::bench]
fn v8_string_index_const(bencher: Bencher) {
    let t = s("t");
    stringat_comp(bencher, move |s, i| baml::at(s, i).as_ref() == Some(&t));
}

// ═══ V8: Strings/string-substring.js ═════════════════════════════════════════
//
// `sum += s.slice(...)` over six short subjects. BAML has one substring method
// (`slice`), so the `substr`/`substring` suites port to the same code as the
// `slice` suite of each group.

const SUBSTRING_SUBJECTS: [&str; 6] = [
    "abcde",
    "123456",
    "aqwsde",
    "nbvveqxu",
    "f03ks-120-3;jfkm;ajp3f",
    "sd-93u498thikefnow8y3-0rh1nalksfnwo8y3t19-3r8hoiefnw",
];

fn substring_bench(bencher: Bencher, f: fn(&BexStr) -> BexStr) {
    let subjects: Vec<BexStr> = SUBSTRING_SUBJECTS.iter().map(|x| s(x)).collect();
    bencher.bench(|| {
        let mut sum = BexStr::empty();
        for subject in black_box(&subjects) {
            sum = baml::add(&sum, &f(subject));
        }
        sum
    });
}

#[divan::bench]
fn v8_string_drop_first_slice(bencher: Bencher) {
    substring_bench(bencher, |x| baml::slice(x, 1, i64::MAX));
}

#[divan::bench]
fn v8_string_take_first_slice(bencher: Bencher) {
    substring_bench(bencher, |x| baml::slice(x, 0, 1));
}

#[divan::bench]
fn v8_string_drop_last_slice(bencher: Bencher) {
    substring_bench(bencher, |x| baml::slice(x, 0, -1));
}

#[divan::bench]
fn v8_string_take_last_slice(bencher: Bencher) {
    substring_bench(bencher, |x| baml::slice(x, -1, i64::MAX));
}

#[divan::bench]
fn v8_string_drop_first_and_last_slice(bencher: Bencher) {
    substring_bench(bencher, |x| baml::slice(x, 1, -1));
}

// ═══ V8: Strings/string-indexof.js ═══════════════════════════════════════════

#[divan::bench]
fn v8_string_index_of(bencher: Bencher) {
    let subject = s("aaaaaaaaaaaaaaaab");
    let searches = [s("a"), s("b"), s("c")];
    bencher.bench(|| {
        let mut sum = 0;
        for search in &searches {
            // JS returns -1 for a miss; BAML returns null.
            sum += baml::index_of(black_box(&subject), search).unwrap_or(-1);
        }
        sum
    });
}

// ═══ V8: Strings/string-split.js ═════════════════════════════════════════════

fn split_bench(bencher: Bencher, subject: BexStr, sep: &str) {
    let sep = s(sep);
    bencher.bench(|| baml::split(black_box(&subject), &sep));
}

fn long_string() -> BexStr {
    // `new Array(0x500).fill("abcde").join('')`: a flat 6400-char string.
    s(&"abcde".repeat(0x500))
}

fn long_two_bytes_string() -> BexStr {
    s(&"\u{0427}\u{0428}\u{0429}\u{0430}".repeat(0x500))
}

const SHORT_STRING: &str = "ababaabcdeaaaaaab";
const SHORT_TWO_BYTES_STRING: &str =
    "\u{0429}\u{0428}\u{0428}\u{0429}\u{0429}\u{0429}\u{0428}\u{0429}\u{0429}";

#[divan::bench]
fn v8_short_subject_empty_separator(bencher: Bencher) {
    split_bench(bencher, s(SHORT_STRING), "");
}

#[divan::bench]
fn v8_long_subject_empty_separator(bencher: Bencher) {
    split_bench(bencher, long_string(), "");
}

#[divan::bench]
fn v8_short_two_bytes_subject_empty_separator(bencher: Bencher) {
    split_bench(bencher, s(SHORT_TWO_BYTES_STRING), "");
}

#[divan::bench]
fn v8_long_two_bytes_subject_empty_separator(bencher: Bencher) {
    split_bench(bencher, long_two_bytes_string(), "");
}

#[divan::bench]
fn v8_short_subject(bencher: Bencher) {
    split_bench(bencher, s(SHORT_STRING), "a");
}

#[divan::bench]
fn v8_long_subject(bencher: Bencher) {
    split_bench(bencher, long_string(), "a");
}

#[divan::bench]
fn v8_short_two_bytes_subject(bencher: Bencher) {
    split_bench(bencher, s(SHORT_TWO_BYTES_STRING), "\u{0428}");
}

#[divan::bench]
fn v8_long_two_bytes_subject(bencher: Bencher) {
    split_bench(bencher, long_two_bytes_string(), "\u{0428}");
}

// ═══ V8: Strings/string-startswith.js ════════════════════════════════════════
//
// `%ConstructConsString(a, b)` → `a + b`. Every pair it is applied to is ≥ 13
// chars (the helpers skip shorter ones), so `a + b` yields a rope in V8, JSC
// and BexStr alike, and `starts_with` must flatten it. `ConsStringsConsSearch`
// is not ported: its skip condition is always true, so it does no work.

const STARTSWITH_INPUTS: [&str; 6] = [
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
    "Integer eu augue suscipit, accumsan ipsum nec, sagittis sem.",
    "In vitae pellentesque dolor. Curabitur leo nunc, luctus vitae",
    "risus eget, fermentum hendrerit justo.",
    "h",
    "",
];
const STARTSWITH_FIRSTS: [&str; 5] = ["I", "Integer", "Lorem", "risus", "hello"];

fn startswith_inputs() -> Vec<BexStr> {
    // 'hello'.repeat(1024) sits between "risus…" and "h" in the original.
    let mut v: Vec<BexStr> = STARTSWITH_INPUTS.iter().map(|x| s(x)).collect();
    v.insert(4, s(&"hello".repeat(1024)));
    v
}

#[divan::bench]
fn v8_direct_strings_direct_search(bencher: Bencher) {
    let inputs = startswith_inputs();
    let firsts: Vec<BexStr> = STARTSWITH_FIRSTS.iter().map(|x| s(x)).collect();
    bencher.bench(|| {
        let mut sum = 0;
        for input in black_box(&inputs) {
            for first in &firsts {
                sum += i64::from(baml::starts_with(input, first));
            }
        }
        sum
    });
}

#[divan::bench]
fn v8_cons_strings_direct_search(bencher: Bencher) {
    let inputs = startswith_inputs();
    let firsts: Vec<BexStr> = STARTSWITH_FIRSTS.iter().map(|x| s(x)).collect();
    bencher.bench(|| {
        let mut sum = 0;
        for one in black_box(&inputs) {
            for two in &inputs {
                for first in &firsts {
                    if baml::length(one) + baml::length(two) < 13 {
                        break;
                    }
                    sum += i64::from(baml::starts_with(&baml::add(one, two), first));
                }
            }
        }
        sum
    });
}

#[divan::bench]
fn v8_direct_strings_cons_search(bencher: Bencher) {
    let inputs = startswith_inputs();
    let firsts: Vec<BexStr> = STARTSWITH_FIRSTS.iter().map(|x| s(x)).collect();
    bencher.bench(|| {
        let mut sum = 0;
        for input in black_box(&inputs) {
            for one in &firsts {
                for two in &firsts {
                    if baml::length(one) + baml::length(two) < 13 {
                        break;
                    }
                    sum += i64::from(baml::starts_with(input, &baml::add(one, two)));
                }
            }
        }
        sum
    });
}

// ═══ V8: Strings/string-startsendswith-comp.js ═══════════════════════════════

const ENDSWITH_INPUTS: [&str; 5] = [
    "I\u{F1}t\u{EB}rn\u{E2}ti\u{F4}n\u{E0}liz\u{E6}ti\u{F8}n\u{2603}\u{1F4A9}\u{FFFD}",
    "Lorem ipsum dolor sit amet, consectetur adipiscing elit.",
    "Integer eu augue suscipit, accumsan ipsum nec, sagittis sem.",
    "In vitae pellentesque dolor. Curabitur leo nunc, luctus vitae",
    "risus eget, fermentum hendrerit justo.",
];

fn startsends_bench(bencher: Bencher, f: fn(&BexStr, &BexStr) -> bool) {
    let inputs: Vec<BexStr> = ENDSWITH_INPUTS.iter().map(|x| s(x)).collect();
    let first = s("I");
    bencher.bench(|| {
        let mut sum = 0;
        for input in black_box(&inputs) {
            sum += i64::from(f(input, &first));
        }
        sum
    });
}

#[divan::bench]
fn v8_starts_with(bencher: Bencher) {
    startsends_bench(bencher, baml::starts_with);
}

#[divan::bench]
fn v8_starts_index(bencher: Bencher) {
    startsends_bench(bencher, |x, first| baml::at(x, 0).as_ref() == Some(first));
}

#[divan::bench]
fn v8_ends_with(bencher: Bencher) {
    startsends_bench(bencher, baml::ends_with);
}

#[divan::bench]
fn v8_ends_index(bencher: Bencher) {
    startsends_bench(bencher, |x, first| {
        baml::at(x, baml::length(x) - 1).as_ref() == Some(first)
    });
}

// ═══ V8: Strings/harmony-string.js ═══════════════════════════════════════════

#[divan::bench]
fn v8_string_repeat(bencher: Bencher) {
    let source = s("abc");
    bencher.bench(|| baml::repeat(black_box(&source), 500));
}

#[divan::bench]
fn v8_string_starts_with(bencher: Bencher) {
    let (str, substr) = (s(&"abc".repeat(500)), s(&"abc".repeat(200)));
    bencher.bench(|| baml::starts_with(black_box(&str), &substr));
}

#[divan::bench]
fn v8_string_ends_with(bencher: Bencher) {
    let (str, substr) = (s(&"abc".repeat(500)), s(&"abc".repeat(200)));
    bencher.bench(|| baml::ends_with(black_box(&str), &substr));
}

#[divan::bench]
fn v8_string_includes(bencher: Bencher) {
    let str = s(&format!(
        "{}{}{}",
        "def".repeat(100),
        "abc".repeat(100),
        "qqq".repeat(100)
    ));
    let substr = s(&"abc".repeat(100));
    bencher.bench(|| baml::includes(black_box(&str), &substr));
}

const MAX_CODE_POINT: i64 = 0xFFFFF;
const K: i64 = 1024;

/// `String.fromCodePoint(i)` for every 1024th code point. JS returns a lone
/// surrogate for 0xD800 and 0xDC00; BAML's `from_code_points` throws there,
/// so those two calls yield `None`.
#[divan::bench]
fn v8_string_from_code_point() -> Vec<Option<BexStr>> {
    (0..=MAX_CODE_POINT)
        .step_by(K as usize)
        .map(|i| baml::from_code_point(black_box(i)))
        .collect()
}

/// As written, the original's setup overwrites `allCodePoints` with the last
/// single-character string (U+FFC00), so the loop calls `codePointAt` on a
/// one-character string at 1024 indices, all but index 0 out of range.
#[divan::bench]
fn v8_string_code_point_at(bencher: Bencher) {
    let all_code_points = baml::from_code_point(0xFFC00).unwrap();
    bencher.bench(|| {
        let mut result = 0;
        for i in (0..=MAX_CODE_POINT).step_by(K as usize) {
            result += baml::code_point_at(black_box(&all_code_points), i).unwrap_or(0);
        }
        result
    });
}

/// `payload = "a…z"` doubled `doublings` times with `payload += payload` (a
/// rope DAG), then `charCodeAt` over every index from the end. The original
/// uses 16 doublings (1,703,936 chars); a quadratic implementation needs far
/// too long for that, so this runs 8, 10 and 12 (see the results doc).
#[divan::bench(args = [8, 10, 12], sample_count = 3, sample_size = 1)]
fn v8_string_code_point_at_sum(bencher: Bencher, doublings: u32) {
    let mut payload = s("abcdefghijklmnopqrstuvwxyz");
    for _ in 0..doublings {
        payload = baml::add(&payload, &payload);
    }
    bencher.bench(|| {
        let mut c = 0;
        let mut j = baml::length(&payload) - 1;
        while j >= 0 {
            c += baml::code_point_at(black_box(&payload), j).unwrap_or(0);
            j -= 1;
        }
        c
    });
}

// ═══ V8: Strings/string-toLocaleCase.js (locale-free variants) ═══════════════
//
// `toLocaleUpperCase()` / `toLocaleLowerCase()` with no locale behave as the
// default-locale case mapping, which BAML's `to_upper_case` / `to_lower_case`
// provide. The Turkish-locale suites have no BAML equivalent.

const LOCALE_CASE_STRING: &str =
    "Îñţérñåţîöñåļîžåţîöñ Ļöçåļîžåţîöñ החןןם שםוןמ Γρεεκ ισ φθν 一二三";

#[divan::bench]
fn v8_string_to_locale_upper_case(bencher: Bencher) {
    let short_string = s(LOCALE_CASE_STRING);
    bencher.bench(|| baml::to_upper_case(black_box(&short_string)));
}

#[divan::bench]
fn v8_string_to_locale_lower_case(bencher: Bencher) {
    let short_string = s(LOCALE_CASE_STRING);
    bencher.bench(|| baml::to_lower_case(black_box(&short_string)));
}

// ═══ V8: StringIterators/string-iterator.js ══════════════════════════════════
//
// `[...string]` → `string.chars()`; `for (c of string) result += c` → the same
// loop over `chars()` with `+`. JS iterates by code point here, like BAML.

const ITER_ONE_BYTE: &str = "Alphabet-Soup";
const ITER_TWO_BYTE: &str = "\u{5FCD}\u{8005}\u{306E}\u{653B}\u{6483}";
const ITER_SURROGATES: &str = "\u{1F31F}\u{5FCD}\u{8005}\u{306E}\u{653B}\u{6483}\u{1F31F}";

fn spread_bench(bencher: Bencher, string: BexStr) {
    bencher.bench(|| baml::chars(black_box(&string)));
}

fn for_of_bench(bencher: Bencher, string: BexStr) {
    bencher.bench(|| {
        let mut result = BexStr::empty();
        for c in baml::chars(black_box(&string)) {
            result = baml::add(&result, &c);
        }
        result
    });
}

#[divan::bench]
fn v8_spread_one_byte_short(bencher: Bencher) {
    spread_bench(bencher, s(ITER_ONE_BYTE));
}

#[divan::bench]
fn v8_spread_two_byte_short(bencher: Bencher) {
    spread_bench(bencher, s(ITER_TWO_BYTE));
}

#[divan::bench]
fn v8_spread_with_surrogate_pairs_short(bencher: Bencher) {
    spread_bench(bencher, s(ITER_SURROGATES));
}

#[divan::bench]
fn v8_for_of_one_byte_short(bencher: Bencher) {
    for_of_bench(bencher, s(ITER_ONE_BYTE));
}

#[divan::bench]
fn v8_for_of_two_byte_short(bencher: Bencher) {
    for_of_bench(bencher, s(ITER_TWO_BYTE));
}

#[divan::bench]
fn v8_for_of_with_surrogate_pairs_short(bencher: Bencher) {
    for_of_bench(bencher, s(ITER_SURROGATES));
}

#[divan::bench]
fn v8_for_of_one_byte_long(bencher: Bencher) {
    for_of_bench(bencher, s(&"Alphabet-Soup|".repeat(128)));
}

/// The original registers this suite with the *one-byte* functions (a typo in
/// string-iterator.js); this ports the two-byte function it defines.
#[divan::bench]
fn v8_for_of_two_byte_long(bencher: Bencher) {
    for_of_bench(bencher, s(&ITER_TWO_BYTE.repeat(128)));
}

#[divan::bench]
fn v8_for_of_with_surrogate_pairs_long(bencher: Bencher) {
    for_of_bench(bencher, s(&format!("{ITER_SURROGATES}|").repeat(128)));
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
