// Runtime tests for the `baml.regex` standard-library namespace.
// See README.md in this directory for test provenance and coverage notes.
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
// (E0174), so every test of a *rejected* pattern builds it dynamically —
// see `new_dynamic` below. That is not a workaround: it is the exact
// split the design intends, and `dynamic_pattern_errors_reach_runtime`
// asserts it.

// ─── Matching ────────────────────────────────────────────────────────────────

test "regex_has_match" {
    let digits = baml.regex.new("\\d+");
    assert.equal(digits.has_match("a1"), true);
    assert.equal(digits.has_match("abc"), false)
}

test "regex_match_once_reports_text_and_offsets" {
    let m = baml.regex.new("(?<y>\\d{4})-(\\d{2})").match_once("on 2026-01 x");
    assert.equal(m?.text, "2026-01");
    assert.equal(m?.start, 3);
    assert.equal(m?.end, 10)
}

// `groups[0]` is the whole match; the rest are numbered left to right by
// opening paren. Named groups appear under both spellings.
test "regex_match_once_groups_and_names" {
    let m = baml.regex.new("(?<y>\\d{4})-(\\d{2})").match_once("on 2026-01 x");
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
    let m = baml.regex.new("(?<a>x)?(?<b>y)").match_once("y");
    let a = m?.groups.at(1);
    assert.equal(a, null);
    let b = m?.groups.at(2);
    assert.equal(b?.text, "y");
    assert.equal(m?.named.has("a"), true);
    let named_a = m?.named.get("a");
    assert.equal(named_a, null)
}

test "regex_match_once_returns_null_when_absent" {
    assert.equal(baml.regex.new("\\d+").match_once("abc"), null)
}

test "regex_match_all" {
    let ms = baml.regex.new("\\d+").match_all("a1 b22 c333");
    assert.equal(
        ms.map((m: baml.regex.Match) -> string {
            m.text
        }),
        ["1", "22", "333"],
    );
    assert.equal(baml.regex.new("z").match_all("abc"), [])
}

// A pattern that can match nothing would loop forever if the scan did not
// step past an empty match.
test "regex_match_all_terminates_on_empty_matches" {
    let ms = baml.regex.new("").match_all("ab");
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
    let re = baml.regex.new("cat|catalog");
    assert.equal(re.match_once("catalog")?.text, "cat");
    let flipped = baml.regex.new("catalog|cat");
    assert.equal(flipped.match_once("catalog")?.text, "catalog")
}

// ─── match_full ──────────────────────────────────────────────────────────────

test "regex_match_full" {
    let digits = baml.regex.new("\\d+");
    assert.equal(digits.match_full("42")?.text, "42");
    assert.equal(digits.match_full("42 a"), null);
    assert.equal(digits.match_full(""), null)
}

// The reason `match_full` exists: `^a|b$` anchors each alternative
// separately, so hand-anchoring accepts `"ab"`, which covers neither
// alternative end to end.
test "regex_match_full_beats_hand_anchoring" {
    let re = baml.regex.new("^a|b$");
    assert.equal(re.has_match("ab"), true);
    assert.equal(re.match_full("ab"), null);
    assert.equal(re.match_full("a")?.text, "a")
}

// Under `(?m)` the anchors mean line boundaries; `match_full` still means
// the whole string.
test "regex_match_full_ignores_multiline_flag" {
    let re = baml.regex.new("(?m)^a$");
    assert.equal(re.has_match("x\na\ny"), true);
    assert.equal(re.match_full("x\na\ny"), null);
    assert.equal(re.match_full("a")?.text, "a")
}

test "regex_match_full_preserves_extended_comments_and_captures" {
    for (let backtracking in [false, true]) {
        let re = baml.regex.new("(?x)(?<letter>a) # trailing comment", backtracking = backtracking);
        let found = re.match_full("a");
        assert.equal(found?.text, "a");
        let letter = found?.named.get("letter");
        assert.equal(letter?.text, "a");
        assert.equal(re.match_full("a\n"), null);
        assert.equal(
            baml.regex.new("a|ab", backtracking = backtracking).match_full("ab")?.text,
            "ab",
        )
    }
}

// ─── split ───────────────────────────────────────────────────────────────────

test "regex_split" {
    assert.equal(baml.regex.new(",").split("a,b,c"), ["a", "b", "c"]);
    // Never empty: no match yields the whole haystack.
    assert.equal(baml.regex.new(",").split("abc"), ["abc"]);
    // Empty segments are kept, including at the edges.
    assert.equal(baml.regex.new("x").split("x"), ["", ""]);
    assert.equal(baml.regex.new(",").split("a,,b"), ["a", "", "b"])
}

// A capturing group keeps the delimiter in the result; `(?:...)` is the
// opt-out.
test "regex_split_interleaves_captured_delimiters" {
    assert.equal(baml.regex.new("(<<\\d+>>)").split("a<<1>>b"), ["a", "<<1>>", "b"]);
    assert.equal(baml.regex.new("(?:<<\\d+>>)").split("a<<1>>b"), ["a", "b"])
}

// A group that did not participate contributes nothing, which is what keeps
// the return type `string[]` with no holes in it.
test "regex_split_drops_nonparticipating_groups" {
    assert.equal(baml.regex.new("(a)|(b)").split("xaybz"), ["x", "a", "y", "b", "z"])
}

// ─── escape ──────────────────────────────────────────────────────────────────

test "regex_escape" {
    assert.equal(baml.regex.escape("a.b"), "a\\.b");
    assert.equal(baml.regex.escape("plain"), "plain")
}

test "regex_escape_makes_dynamic_data_literal" {
    let tag = "a+b";
    let re = baml.regex.new(`<${baml.regex.escape(tag)}>`);
    assert.equal(re.has_match("x<a+b>y"), true);
    assert.equal(re.has_match("x<aab>y"), false)
}

// ─── Offsets are codepoints ──────────────────────────────────────────────────

// Both engines work in UTF-8 bytes; everything crossing into BAML is
// converted, so offsets feed `slice` directly.
test "regex_offsets_are_codepoints" {
    let haystack = "😀héllo wörld";
    let m = baml.regex.new("w\\w+").match_once(haystack);
    assert.equal(m?.start, 7);
    assert.equal(m?.end, 12);
    assert.equal(haystack.slice(m?.start ?? 0, m?.end ?? 0), "wörld")
}

test "regex_group_offsets_are_codepoints" {
    let haystack = "😀 x=42";
    let m = baml.regex.new("x=(\\d+)").match_once(haystack);
    let value = m?.groups.at(1);
    assert.equal(value?.start, 4);
    assert.equal(value?.end, 6);
    assert.equal(haystack.slice(value?.start ?? 0, value?.end ?? 0), "42")
}

test "regex_all_match_offsets_include_optional_and_nested_groups" {
    let haystack = "😀é x é";
    for (let backtracking in [false, true]) {
        let re = baml.regex.new("(?<outer>(é))|(x)", backtracking = backtracking);
        let matches = re.match_all(haystack);
        assert.equal(
            matches.map((m: baml.regex.Match) -> int {
                m.start
            }),
            [1, 3, 5],
        );
        for (let m in matches) {
            assert.equal(haystack.slice(m.start, m.end), m.text);
            for (let group in m.groups) {
                if (group != null) {
                    assert.equal(haystack.slice(group.start, group.end), group.text)
                }
            }
        }
        assert.equal(matches.at(1)?.named.get("outer"), null)
    }
}

// ─── Reuse is safe ───────────────────────────────────────────────────────────

// A `Regex` holds no scan position, so the second call sees exactly what the
// first did. (JavaScript's `/g` flag is the counterexample this rules out.)
test "regex_is_stateless_across_calls" {
    let re = baml.regex.new("\\d");
    assert.equal(re.match_once("a1b2")?.start, 1);
    assert.equal(re.match_once("a1b2")?.start, 1);
    assert.equal(re.match_all("a1b2").length(), 2);
    assert.equal(re.match_all("a1b2").length(), 2)
}

// ─── The shared dialect ──────────────────────────────────────────────────────

// `new`'s doc names the syntax both dialects accept. Two different engines
// back them, so "both" is a claim that has to be checked rather than assumed.

test "regex_inline_flags_in_both_dialects" {
    assert.equal(baml.regex.new("(?i)YES").has_match("yes"), true);
    assert.equal(baml.regex.new("(?s)a.b").has_match("a\nb"), true);
    assert.equal(baml.regex.new("(?m)^b$").has_match("a\nb"), true);
    assert.equal(baml.regex.new("(?x) \\d+  # digits").has_match("a1"), true);

    assert.equal(baml.regex.new("(?i)YES", backtracking = true).has_match("yes"), true);
    assert.equal(baml.regex.new("(?s)a.b", backtracking = true).has_match("a\nb"), true);
    assert.equal(baml.regex.new("(?m)^b$", backtracking = true).has_match("a\nb"), true);
    assert.equal(
        baml.regex.new("(?x) \\d+  # digits", backtracking = true).has_match("a1"),
        true,
    )
}

// Python's `(?P<name>...)` spelling is accepted alongside `(?<name>...)`, which
// is what makes a ported `re` pattern work unchanged.
test "regex_python_named_group_spelling" {
    let m = baml.regex.new("(?P<y>\\d+)").match_once("a12");
    assert.equal(m?.named.has("y"), true);
    let b = baml.regex.new("(?P<y>\\d+)", backtracking = true).match_once("a12");
    assert.equal(b?.named.has("y"), true)
}

// `\d`, `\w`, and `\b` are Unicode-aware in both dialects, and Unicode
// character properties are available in both.
test "regex_unicode_classes_in_both_dialects" {
    assert.equal(baml.regex.new("\\p{Greek}+").match_once("abαβγ")?.text, "αβγ");
    assert.equal(baml.regex.new("\\d+").match_once("٣٤")?.text, "٣٤");
    assert.equal(baml.regex.new("\\w+").match_once("héllo")?.text, "héllo");

    assert.equal(
        baml.regex.new("\\p{Greek}+", backtracking = true).match_once("abαβγ")?.text,
        "αβγ",
    );
    assert.equal(baml.regex.new("\\d+", backtracking = true).match_once("٣٤")?.text, "٣٤");
    assert.equal(baml.regex.new("\\w+", backtracking = true).match_once("héllo")?.text, "héllo")
}

// ─── The backtracking opt-in ─────────────────────────────────────────────────

// The Cloudflare 2019 shape. In a backtracking engine this explores an
// exponential number of ways to divide the input before failing; the default
// dialect answers in bounded time and is not even an error to write.
test "regex_default_dialect_handles_catastrophic_shape" {
    assert.equal(baml.regex.new(".*.*=.*").has_match("xxxxxxxxxxxxxxxxxxxxxxxxxxxxxx"), false);
    assert.equal(baml.regex.new(".*.*=.*").has_match("xxx=yyy"), true)
}

test "regex_backtracking_lookahead_and_lookbehind" {
    let before = baml.regex.new("\\d+(?= USD)", backtracking = true);
    assert.equal(before.match_once("Cost: USD 25 USD")?.text, "25");
    let after = baml.regex.new("(?<=USD )\\d+", backtracking = true);
    assert.equal(after.match_once("Cost: USD 25 USD")?.text, "25")
}

test "regex_backtracking_backreference" {
    let repeated = baml.regex.new("\\b(\\w+)\\s+\\1\\b", backtracking = true);
    assert.equal(repeated.match_once("the the cat")?.text, "the the");
    assert.equal(repeated.has_match("the cat"), false)
}

// A named group doubles as a subroutine: `\g<parens>` re-runs it, which is
// how bounded nesting is recognized without a hand-written scanner.
test "regex_backtracking_recursive_subroutine" {
    let parens = baml.regex.new("(?<parens>\\((?:[^()]|\\g<parens>)*\\))", backtracking = true);
    assert.equal(parens.match_full("()")?.text, "()");
    assert.equal(parens.match_full("(a(b)c)")?.text, "(a(b)c)");
    assert.equal(parens.match_full("(((a)))")?.text, "(((a)))");
    assert.equal(parens.match_full("(a(b)"), null);
    assert.equal(parens.match_full(")("), null)
}

// Past the 20-level limit a call simply fails to match, like any other failed
// branch — it is not a match-time error. `match_full` is how a caller
// requires the whole nested input to be accepted.
test "regex_recursion_depth_limit_fails_to_match" {
    let nested = baml.regex.new("(?<n>a\\g<n>?)", backtracking = true);
    assert.equal(nested.match_once("aaaa")?.text, "aaaa");
    // 22 `a`s: the match stops at the depth limit rather than throwing.
    let deep = "a".repeat(22);
    assert.equal(nested.match_once(deep)?.text.length(), 20);
    assert.equal(nested.match_full(deep), null)
}

// ─── Qualified and optional access ──────────────────────────────────────────

test "regex_match_through_a_qualified_path" {
    let re = baml.regex.new("\\d");
    assert.equal(baml.regex.Regex.match_once(re, "a7")?.text, "7");
    assert.equal(baml.regex.Regex.match_all(re, "a7b8").length(), 2);
    assert.equal(baml.regex.Regex.has_match(re, "ab"), false)
}

function optional_match(re: baml.regex.Regex?, haystack: string) -> string? {
    let found = re?.match_once(haystack);
    found?.text
}

test "regex_match_through_optional_access" {
    let re = baml.regex.new("\\d+");
    assert.equal(optional_match(re, "a42"), "42");
    assert.equal(optional_match(null, "a42"), null);
    assert.equal(re?.match_all("a42b7")?.length(), 2)
}

// ─── Errors ──────────────────────────────────────────────────────────────────

// Indirection past the constant-pattern check: the compiler only inspects a
// literal argument, so a pattern arriving through a parameter is a runtime
// concern, which is exactly what `baml.regex.Error` is for.
function new_dynamic(
    pattern: string,
    backtracking: bool,
) -> baml.regex.Regex throws baml.regex.Error {
    baml.regex.new(pattern, backtracking = backtracking)
}

test "regex_dynamic_pattern_errors_reach_runtime" {
    assert.equal(
        string.from(
            new_dynamic("(", false) catch (e) {
                let err: baml.regex.Error => string.from(err.kind),
            },
        ),
        "Syntax",
    )
}

// A construct the default dialect refuses but the backtracking one accepts is
// `Unsupported`, not `Syntax`: nothing is wrong with the pattern except where
// it was used.
test "regex_unsupported_is_distinct_from_syntax" {
    assert.equal(
        string.from(
            new_dynamic("(?=x)", false) catch (e) {
                let err: baml.regex.Error => string.from(err.kind),
            },
        ),
        "Unsupported",
    );
    // The same pattern compiles once the caller opts in.
    assert.equal(new_dynamic("(?=x)", true).has_match("xy"), true)
}

// A cycle that could recurse forever without consuming input is rejected at
// construction rather than looping at match time.
test "regex_unbounded_recursion_is_rejected" {
    assert.equal(
        string.from(
            new_dynamic("(?<expr>\\g<expr>a|a)", true) catch (e) {
                let err: baml.regex.Error => string.from(err.kind),
            },
        ),
        "Syntax",
    )
}

// The error carries the pattern it was given and, where the engine reports
// one, the codepoint span of the offending construct.
test "regex_error_carries_pattern_and_span" {
    assert.equal(
        string.from(
            new_dynamic("a(?=b)c", false) catch (e) {
                let err: baml.regex.Error => {
                    `${err.pattern}|${string.from(err.span_start)}|${string.from(err.span_end)}`
                },
            },
        ),
        "a(?=b)c|1|4",
    )
}

// Once a `Regex` exists, matching it cannot throw a regex error — that is the
// whole point of compiling separately from matching.
test "regex_matching_never_throws" {
    let re = new_dynamic("\\d+", false);
    assert.equal(re.has_match("1"), true);
    assert.equal(re.match_all("1 2").length(), 2);
    assert.equal(re.split("a1b"), ["a", "b"])
}
