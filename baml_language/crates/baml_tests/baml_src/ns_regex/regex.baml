// Runtime tests for the `baml.regex` standard-library namespace.
//
// Three layers are exercised:
// - `crates/sys_regex`, which owns both engines, the dialect split, and the
//   error classification. Its own unit tests cover the engine seam; these
//   cover what a BAML program sees.
// - `crates/bex_vm/src/package_baml/regex.rs`, which marshals matches into
//   `Match` / `Group` values and converts byte offsets to codepoints.
// - The BAML declarations in `baml_std/baml/ns_regex/regex.baml`.
//
// Patterns written as constants here are also checked by the compiler
// (E0171), so every test of a *rejected* pattern builds it dynamically —
// see `compile_dynamic` below. That is not a workaround: it is the exact
// split the design intends, and `dynamic_pattern_errors_reach_runtime`
// asserts it.

// ─── Matching ────────────────────────────────────────────────────────────────

test "regex_is_match" {
    let digits = baml.regex.compile("\\d+");
    assert.equal(digits.is_match("a1"), true);
    assert.equal(digits.is_match("abc"), false)
}

test "regex_match_reports_text_and_offsets" {
    let m = baml.regex.compile("(?<y>\\d{4})-(\\d{2})").match("on 2026-01 x");
    assert.equal(m?.text, "2026-01");
    assert.equal(m?.start, 3);
    assert.equal(m?.end, 10)
}

// `groups[0]` is the whole match; the rest are numbered left to right by
// opening paren. Named groups appear under both spellings.
test "regex_match_groups_and_names" {
    let m = baml.regex.compile("(?<y>\\d{4})-(\\d{2})").match("on 2026-01 x");
    let whole = m?.groups.at(0);
    assert.equal(whole?.text, "2026-01");
    let year = m?.groups.at(1);
    assert.equal(year?.text, "2026");
    assert.equal(year?.start, 3);
    assert.equal(year?.end, 7);
    let month = m?.groups.at(2);
    assert.equal(month?.text, "01");
    let named = m?.named.get("y");
    assert.equal(named?.text, "2026")
}

// A group that did not participate is `null`, not an empty `Group` — and its
// name is still a key in `named`, so reading code never has to tell "no such
// group" from "did not match this time".
test "regex_nonparticipating_group_is_null" {
    let m = baml.regex.compile("(?<a>x)?(?<b>y)").match("y");
    let a = m?.groups.at(1);
    assert.equal(a, null);
    let b = m?.groups.at(2);
    assert.equal(b?.text, "y");
    assert.equal(m?.named.has("a"), true);
    let named_a = m?.named.get("a");
    assert.equal(named_a, null)
}

test "regex_match_returns_null_when_absent" {
    assert.equal(baml.regex.compile("\\d+").match("abc"), null)
}

test "regex_match_all" {
    let ms = baml.regex.compile("\\d+").match_all("a1 b22 c333");
    assert.equal(
        ms.map((m: baml.regex.Match) -> string {
            m.text
        }),
        ["1", "22", "333"],
    );
    assert.equal(baml.regex.compile("z").match_all("abc"), [])
}

// A pattern that can match nothing would loop forever if the scan did not
// step past an empty match.
test "regex_match_all_terminates_on_empty_matches" {
    let ms = baml.regex.compile("").match_all("ab");
    assert.equal(ms.length(), 3);
    assert.equal(
        ms.map((m: baml.regex.Match) -> int {
            m.start
        }),
        [0, 1, 2],
    )
}

// Alternation is leftmost-first: among alternatives at the same position the
// first one listed wins, which is what lets an exception branch precede a
// general one.
test "regex_alternation_is_leftmost_first" {
    let re = baml.regex.compile("cat|catalog");
    assert.equal(re.match("catalog")?.text, "cat");
    let flipped = baml.regex.compile("catalog|cat");
    assert.equal(flipped.match("catalog")?.text, "catalog")
}

// ─── exact_match ─────────────────────────────────────────────────────────────

test "regex_exact_match" {
    let digits = baml.regex.compile("\\d+");
    assert.equal(digits.exact_match("42")?.text, "42");
    assert.equal(digits.exact_match("42 a"), null);
    assert.equal(digits.exact_match(""), null)
}

// The reason `exact_match` exists: `^a|b$` anchors each alternative
// separately, so hand-anchoring accepts `"ab"`, which covers neither
// alternative end to end.
test "regex_exact_match_beats_hand_anchoring" {
    let re = baml.regex.compile("^a|b$");
    assert.equal(re.is_match("ab"), true);
    assert.equal(re.exact_match("ab"), null);
    assert.equal(re.exact_match("a")?.text, "a")
}

// Under `(?m)` the anchors mean line boundaries; `exact_match` still means
// the whole string.
test "regex_exact_match_ignores_multiline_flag" {
    let re = baml.regex.compile("(?m)^a$");
    assert.equal(re.is_match("x\na\ny"), true);
    assert.equal(re.exact_match("x\na\ny"), null);
    assert.equal(re.exact_match("a")?.text, "a")
}

// ─── split ───────────────────────────────────────────────────────────────────

test "regex_split" {
    assert.equal(baml.regex.compile(",").split("a,b,c"), ["a", "b", "c"]);
    // Never empty: no match yields the whole haystack.
    assert.equal(baml.regex.compile(",").split("abc"), ["abc"]);
    // Empty segments are kept, including at the edges.
    assert.equal(baml.regex.compile("x").split("x"), ["", ""]);
    assert.equal(baml.regex.compile(",").split("a,,b"), ["a", "", "b"])
}

// A capturing group keeps the delimiter in the result; `(?:...)` is the
// opt-out.
test "regex_split_interleaves_captured_delimiters" {
    assert.equal(baml.regex.compile("(<<\\d+>>)").split("a<<1>>b"), ["a", "<<1>>", "b"]);
    assert.equal(baml.regex.compile("(?:<<\\d+>>)").split("a<<1>>b"), ["a", "b"])
}

// A group that did not participate contributes nothing, which is what keeps
// the return type `string[]` with no holes in it.
test "regex_split_drops_nonparticipating_groups" {
    assert.equal(baml.regex.compile("(a)|(b)").split("xaybz"), ["x", "a", "y", "b", "z"])
}

// ─── word ────────────────────────────────────────────────────────────────────

// `word` picks its boundary assertion from the literal's outermost
// characters. `\b` after `%` would demand a word character *after* it, which
// is backwards — so a literal ending in punctuation gets `\B` instead.
test "regex_word_boundaries_follow_the_literal" {
    let pct = baml.regex.word("2.5%");
    assert.equal(pct.is_match("a 2.5% raise"), true);
    assert.equal(pct.is_match("a 2.5%raise"), false);

    let cat = baml.regex.word("cat");
    assert.equal(cat.is_match("a cat."), true);
    assert.equal(cat.is_match("concatenate"), false);
    assert.equal(cat.is_match("cat"), true)
}

// The literal is escaped, so a term read from a vocabulary cannot become
// pattern syntax.
test "regex_word_escapes_its_literal" {
    assert.equal(baml.regex.word("a.c").is_match("a.c"), true);
    assert.equal(baml.regex.word("a.c").is_match("abc"), false)
}

test "regex_word_ignore_case" {
    assert.equal(baml.regex.word("cat").is_match("a Cat!"), false);
    assert.equal(baml.regex.word("cat", ignore_case = true).is_match("a Cat!"), true)
}

// A blank entry in a user-maintained correction list must rewrite nothing.
test "regex_word_empty_literal_matches_nowhere" {
    assert.equal(baml.regex.word("").is_match("anything"), false);
    assert.equal(baml.regex.word("").is_match(""), false);
    assert.equal(baml.regex.word("").match_all("abc"), [])
}

// Whitespace inside the literal stays exact — `word` adds boundaries, it does
// not normalize.
test "regex_word_keeps_interior_whitespace_exact" {
    let phrase = baml.regex.word("New York");
    assert.equal(phrase.is_match("in New York today"), true);
    assert.equal(phrase.is_match("in New  York today"), false)
}

// ─── escape ──────────────────────────────────────────────────────────────────

test "regex_escape" {
    assert.equal(baml.regex.escape("a.b"), "a\\.b");
    assert.equal(baml.regex.escape("plain"), "plain")
}

test "regex_escape_makes_dynamic_data_literal" {
    let tag = "a+b";
    let re = baml.regex.compile(`<${baml.regex.escape(tag)}>`);
    assert.equal(re.is_match("x<a+b>y"), true);
    assert.equal(re.is_match("x<aab>y"), false)
}

// ─── Offsets are codepoints ──────────────────────────────────────────────────

// Both engines work in UTF-8 bytes; everything crossing into BAML is
// converted, so offsets feed `slice` directly.
test "regex_offsets_are_codepoints" {
    let haystack = "😀héllo wörld";
    let m = baml.regex.compile("w\\w+").match(haystack);
    assert.equal(m?.start, 7);
    assert.equal(m?.end, 12);
    assert.equal(haystack.slice(m?.start ?? 0, m?.end ?? 0), "wörld")
}

test "regex_group_offsets_are_codepoints" {
    let haystack = "😀 x=42";
    let m = baml.regex.compile("x=(\\d+)").match(haystack);
    let value = m?.groups.at(1);
    assert.equal(value?.start, 4);
    assert.equal(value?.end, 6);
    assert.equal(haystack.slice(value?.start ?? 0, value?.end ?? 0), "42")
}

// ─── Reuse is safe ───────────────────────────────────────────────────────────

// A `Regex` holds no scan position, so the second call sees exactly what the
// first did. (JavaScript's `/g` flag is the counterexample this rules out.)
test "regex_is_stateless_across_calls" {
    let re = baml.regex.compile("\\d");
    assert.equal(re.match("a1b2")?.start, 1);
    assert.equal(re.match("a1b2")?.start, 1);
    assert.equal(re.match_all("a1b2").length(), 2);
    assert.equal(re.match_all("a1b2").length(), 2)
}

// ─── The shared dialect ──────────────────────────────────────────────────────

// `compile`'s doc names the syntax both dialects accept. Two different engines
// back them, so "both" is a claim that has to be checked rather than assumed.

test "regex_inline_flags_in_both_dialects" {
    assert.equal(baml.regex.compile("(?i)YES").is_match("yes"), true);
    assert.equal(baml.regex.compile("(?s)a.b").is_match("a\nb"), true);
    assert.equal(baml.regex.compile("(?m)^b$").is_match("a\nb"), true);
    assert.equal(baml.regex.compile("(?x) \\d+  # digits").is_match("a1"), true);

    assert.equal(baml.regex.compile("(?i)YES", backtracking = true).is_match("yes"), true);
    assert.equal(baml.regex.compile("(?s)a.b", backtracking = true).is_match("a\nb"), true);
    assert.equal(baml.regex.compile("(?m)^b$", backtracking = true).is_match("a\nb"), true);
    assert.equal(
        baml.regex.compile("(?x) \\d+  # digits", backtracking = true).is_match("a1"),
        true,
    )
}

// Python's `(?P<name>...)` spelling is accepted alongside `(?<name>...)`, which
// is what makes a ported `re` pattern work unchanged.
test "regex_python_named_group_spelling" {
    let m = baml.regex.compile("(?P<y>\\d+)").match("a12");
    assert.equal(m?.named.has("y"), true);
    let b = baml.regex.compile("(?P<y>\\d+)", backtracking = true).match("a12");
    assert.equal(b?.named.has("y"), true)
}

// `\d`, `\w`, and `\b` are Unicode-aware in both dialects, and Unicode
// character properties are available in both.
test "regex_unicode_classes_in_both_dialects" {
    assert.equal(baml.regex.compile("\\p{Greek}+").match("abαβγ")?.text, "αβγ");
    assert.equal(baml.regex.compile("\\d+").match("٣٤")?.text, "٣٤");
    assert.equal(baml.regex.compile("\\w+").match("héllo")?.text, "héllo");

    assert.equal(
        baml.regex.compile("\\p{Greek}+", backtracking = true).match("abαβγ")?.text,
        "αβγ",
    );
    assert.equal(baml.regex.compile("\\d+", backtracking = true).match("٣٤")?.text, "٣٤");
    assert.equal(baml.regex.compile("\\w+", backtracking = true).match("héllo")?.text, "héllo")
}

// ─── The backtracking opt-in ─────────────────────────────────────────────────

// The Cloudflare 2019 shape. In a backtracking engine this explores an
// exponential number of ways to divide the input before failing; the default
// dialect answers in bounded time and is not even an error to write.
test "regex_default_dialect_handles_catastrophic_shape" {
    assert.equal(baml.regex.compile(".*.*=.*").is_match("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"), false);
    assert.equal(baml.regex.compile(".*.*=.*").is_match("xxx=yyy"), true)
}

test "regex_backtracking_lookahead_and_lookbehind" {
    let before = baml.regex.compile("\\d+(?= USD)", backtracking = true);
    assert.equal(before.match("Cost: USD 25 USD")?.text, "25");
    let after = baml.regex.compile("(?<=USD )\\d+", backtracking = true);
    assert.equal(after.match("Cost: USD 25 USD")?.text, "25")
}

test "regex_backtracking_backreference" {
    let repeated = baml.regex.compile("\\b(\\w+)\\s+\\1\\b", backtracking = true);
    assert.equal(repeated.match("the the cat")?.text, "the the");
    assert.equal(repeated.is_match("the cat"), false)
}

// A named group doubles as a subroutine: `\g<parens>` re-runs it, which is
// how bounded nesting is recognized without a hand-written scanner.
test "regex_backtracking_recursive_subroutine" {
    let parens = baml.regex.compile("(?<parens>\\((?:[^()]|\\g<parens>)*\\))", backtracking = true);
    assert.equal(parens.exact_match("()")?.text, "()");
    assert.equal(parens.exact_match("(a(b)c)")?.text, "(a(b)c)");
    assert.equal(parens.exact_match("(((a)))")?.text, "(((a)))");
    assert.equal(parens.exact_match("(a(b)"), null);
    assert.equal(parens.exact_match(")("), null)
}

// Past the 20-level limit a call simply fails to match, like any other failed
// branch — it is not a match-time error. `exact_match` is how a caller
// requires the whole nested input to be accepted.
test "regex_recursion_depth_limit_fails_to_match" {
    let nested = baml.regex.compile("(?<n>a\\g<n>?)", backtracking = true);
    assert.equal(nested.match("aaaa")?.text, "aaaa");
    // 22 `a`s: the match stops at the depth limit rather than throwing.
    let deep = "a".repeat(22);
    assert.equal(nested.match(deep)?.text.length(), 20);
    assert.equal(nested.exact_match(deep), null)
}

// ─── Errors ──────────────────────────────────────────────────────────────────

// Indirection past the constant-pattern check: the compiler only inspects a
// literal argument, so a pattern arriving through a parameter is a runtime
// concern, which is exactly what `baml.regex.Error` is for.
function compile_dynamic(
    pattern: string,
    backtracking: bool,
) -> baml.regex.Regex throws baml.regex.Error {
    baml.regex.compile(pattern, backtracking = backtracking)
}

test "regex_dynamic_pattern_errors_reach_runtime" {
    let kind = compile_dynamic("(", false) catch (e) {
        let err: baml.regex.Error => string.from(err.kind),
    };
    assert.equal(string.from(kind), "Syntax")
}

// A construct the default dialect refuses but the backtracking one accepts is
// `Unsupported`, not `Syntax`: nothing is wrong with the pattern except where
// it was used.
test "regex_unsupported_is_distinct_from_syntax" {
    let kind = compile_dynamic("(?=x)", false) catch (e) {
        let err: baml.regex.Error => string.from(err.kind),
    };
    assert.equal(string.from(kind), "Unsupported");
    // The same pattern compiles once the caller opts in.
    assert.equal(compile_dynamic("(?=x)", true).is_match("xy"), true)
}

// A cycle that could recurse forever without consuming input is rejected at
// construction rather than looping at match time.
test "regex_unbounded_recursion_is_rejected" {
    let kind = compile_dynamic("(?<expr>\\g<expr>a|a)", true) catch (e) {
        let err: baml.regex.Error => string.from(err.kind),
    };
    assert.equal(string.from(kind), "Syntax")
}

// The error carries the pattern it was given and, where the engine reports
// one, the codepoint span of the offending construct.
test "regex_error_carries_pattern_and_span" {
    let described = compile_dynamic("a(?=b)c", false) catch (e) {
        let err: baml.regex.Error => {
            `${err.pattern}|${string.from(err.span_start)}|${string.from(err.span_end)}`
        },
    };
    assert.equal(string.from(described), "a(?=b)c|1|4")
}

// Once a `Regex` exists, matching it cannot throw a regex error — that is the
// whole point of compiling separately from matching.
test "regex_matching_never_throws" {
    let re = compile_dynamic("\\d+", false);
    assert.equal(re.is_match("1"), true);
    assert.equal(re.match_all("1 2").length(), 2);
    assert.equal(re.split("a1b"), ["a", "b"])
}
