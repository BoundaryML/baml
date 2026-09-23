// Runtime tests for `string.replace` / `string.replace_all` once they accept
// `baml.regex.Regex` searches.
// See README.md in this directory for test provenance and coverage notes.
//
// Literal and pattern searches take two different roads through
// `baml_std/baml/string.baml`'s replacement implementation:
//
//   literal -> the native `_replace_literal` / `_replace_all_literal`
//   pattern -> the engine's own template expansion
//
// Each is covered here, plus the edges where they have to agree.

// ─── Literal search, literal replacement (the pre-existing contract) ─────────

test "replace_literal_first_and_all" {
    assert.equal("a.b.c".replace(".", "-"), "a-b.c");
    assert.equal("a.b.c".replace_all(".", "-"), "a-b-c")
}

// A literal search keeps a literal replacement: adding pattern support must
// not give `$1` a new meaning in code that already existed.
test "replace_literal_replacement_stays_literal" {
    assert.equal("ab".replace_all("a", "$1"), "$1b");
    assert.equal("ab".replace_all("a", "$$"), "$$b")
}

test "replace_literal_no_match_returns_input" {
    assert.equal("abc".replace("z", "-"), "abc");
    assert.equal("abc".replace_all("z", "-"), "abc")
}

// ─── Pattern search, template replacement ────────────────────────────────────

// The search union is what decides whether `.` is a dot or a wildcard.
test "replace_pattern_search_is_a_pattern" {
    assert.equal("a.b".replace(".", "-"), "a-b");
    assert.equal("a.b".replace(baml.regex.compile("."), "-"), "-.b")
}

test "replace_pattern_template_expands_groups" {
    assert.equal("5 cm".replace(baml.regex.compile("(\\d+) cm"), "${1}mm"), "5mm");
    assert.equal("x1y2".replace_all(baml.regex.compile("\\d"), "<$0>"), "x<1>y<2>");
    assert.equal("ab".replace_all(baml.regex.compile("(?<c>a)"), "[$c]"), "[a]b");
    assert.equal("ab".replace_all(baml.regex.compile("(?<c>a)"), "[${c}]"), "[a]b")
}

// `$$` is a literal `$`; a group that did not participate, or does not exist,
// expands to nothing.
test "replace_pattern_template_edge_cases" {
    assert.equal("a".replace_all(baml.regex.compile("a"), "$$"), "$");
    assert.equal("a".replace_all(baml.regex.compile("(x)?a"), "[$1]"), "[]");
    assert.equal("a".replace_all(baml.regex.compile("a"), "[$9]"), "[]")
}

test "replace_pattern_first_only_vs_all" {
    let digits = baml.regex.compile("\\d");
    assert.equal("a1b2".replace(digits, "#"), "a#b2");
    assert.equal("a1b2".replace_all(digits, "#"), "a#b#")
}

// ─── Empty-search parity ─────────────────────────────────────────────────────

// An empty search matches between every pair of characters. The literal and
// pattern roads must agree.
test "replace_empty_search_agrees_across_roads" {
    assert.equal("ab".replace_all("", "-"), "-a-b-");
    assert.equal("ab".replace_all(baml.regex.compile(""), "-"), "-a-b-")
}

test "replace_empty_haystack" {
    assert.equal("".replace_all(baml.regex.compile("a"), "-"), "")
}
