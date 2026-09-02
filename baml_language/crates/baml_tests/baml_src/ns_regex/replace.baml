// Runtime tests for `string.replace` / `string.replace_all` once they accept
// `baml.regex.Regex` searches and callback replacements.
//
// The four combinations of (literal | pattern) search × (template | callback)
// replacement take four different roads through
// `baml_std/baml/string.baml`'s `_replace_impl`:
//
//   literal + string   -> the native `_replace_literal` / `_replace_all_literal`
//   pattern + string   -> the engine's own template expansion
//   literal + callback -> `_replace_literal_with`, a BAML scan loop
//   pattern + callback -> `_replace_pattern_with`, over `match` / `match_all`
//
// Each is covered here, plus the edges where they have to agree with each
// other (empty search, no match, offsets).

class Term {
    from: string,
    to: string,
}

class Unknown {
    term: string,
}

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

// ─── Pattern search, callback replacement ────────────────────────────────────

function upper(m: baml.regex.Match) -> string {
    m.text.to_upper_case()
}

test "replace_pattern_callback" {
    assert.equal("ab".replace(baml.regex.compile("."), upper), "Ab");
    assert.equal("ab".replace_all(baml.regex.compile("."), upper), "AB")
}

// The callback sees the whole typed match — text, offsets, and captures — so
// macro expansion needs no separate method.
test "replace_pattern_callback_reads_captures" {
    let expanded = "x {{a}} y {{bb}}".replace_all(baml.regex.compile("\\{\\{(.*?)\\}\\}"), (m: baml.regex.Match) -> string {
        let name = m.groups.at(1);
        match (name) {
            null => "",
            let g: baml.regex.Group => `<${g.text}>`,
        }
    });
    assert.equal(expanded, "x <a> y <bb>")
}

test "replace_pattern_callback_reads_offsets" {
    let marked = "a1b2".replace_all(baml.regex.compile("\\d"), (m: baml.regex.Match) -> string {
        string.from(m.start)
    });
    assert.equal(marked, "a1b3")
}

// The callback's return value is inserted verbatim — no template expansion is
// applied to it.
test "replace_pattern_callback_result_is_verbatim" {
    assert.equal(
        "a".replace_all(baml.regex.compile("a"), (m: baml.regex.Match) -> string {
            "$0"
        }),
        "$0",
    )
}

// ─── Literal search, callback replacement ────────────────────────────────────

// A literal search has no capture groups, so `groups` holds only the whole
// match and `named` is empty.
test "replace_literal_callback_match_shape" {
    let described = "xaby".replace("ab", (m: baml.regex.Match) -> string {
        let whole = match (m.groups.at(0)) {
            null => "-",
            let g: baml.regex.Group => g.text,
        };
        let counts = `${string.from(m.groups.length())}/${string.from(m.named.length())}`;
        `${m.text}/${string.from(m.start)}/${string.from(m.end)}/${counts}/${whole}`
    });
    // The callback's report replaces `ab` in place, so the surrounding `x`
    // and `y` bracket it in the result.
    assert.equal(described, "xab/1/3/1/0/aby")
}

test "replace_literal_callback_first_and_all" {
    // Both arms report each match's offset, so first-only and every-match give
    // different answers. A callback that mapped "X" back to "X" would not: the
    // assertion would hold even if nothing were replaced.
    assert.equal(
        "aXbXc".replace("X", (m: baml.regex.Match) -> string {
            string.from(m.start)
        }),
        "a1bXc",
    );
    assert.equal(
        "aXbXc".replace_all("X", (m: baml.regex.Match) -> string {
            string.from(m.start)
        }),
        "a1b3c",
    )
}

// ─── Empty-search parity ─────────────────────────────────────────────────────

// An empty search matches between every pair of characters. The literal and
// pattern roads, and the template and callback roads, must all agree.
test "replace_empty_search_agrees_across_roads" {
    assert.equal("ab".replace_all("", "-"), "-a-b-");
    assert.equal(
        "ab".replace_all("", (m: baml.regex.Match) -> string {
            "-"
        }),
        "-a-b-",
    );
    assert.equal("ab".replace_all(baml.regex.compile(""), "-"), "-a-b-");
    assert.equal(
        "ab".replace_all(baml.regex.compile(""), (m: baml.regex.Match) -> string {
            "-"
        }),
        "-a-b-",
    )
}

test "replace_empty_haystack" {
    assert.equal("".replace_all(baml.regex.compile("a"), "-"), "");
    assert.equal("".replace_all("a", upper), "")
}

// ─── Unicode ─────────────────────────────────────────────────────────────────

// The scan loops slice by codepoint offset; a multi-byte haystack must not
// shift the pieces.
test "replace_is_codepoint_correct" {
    assert.equal("😀a😀b".replace_all(baml.regex.compile("\\w"), upper), "😀A😀B");
    assert.equal(
        "😀a😀b".replace_all("😀", (m: baml.regex.Match) -> string {
            "[]"
        }),
        "[]a[]b",
    )
}

// ─── Effects ─────────────────────────────────────────────────────────────────

// A throwing callback contributes its own error type; the call site catches
// exactly that class.
function reject(m: baml.regex.Match) -> string throws Unknown {
    throw Unknown { term: m.text }
}

test "replace_callback_error_propagates" {
    // The pattern is compiled outside the `catch` so the only effect reaching
    // the arms is the callback's. (Leaving `compile` inside would add
    // `baml.regex.Error` to the caught union, which trips a pre-existing
    // catch-reachability warning on any inferred-effect call — `xs.map(f)`
    // does the same — unrelated to what this test is about.)
    let a = baml.regex.compile("a");
    let caught = "ab".replace(a, reject) catch (e) {
        let u: Unknown => u.term,
    };
    assert.equal(string.from(caught), "a")
}

// A string replacement contributes no effect of its own: with the pattern
// already compiled, `replace_all` here is `throws never`. (`compile` keeps
// its own `throws baml.regex.Error` — its signature does not change just
// because the argument happens to be constant — so the pattern is a
// parameter.) This function compiling at all is the assertion.
function rewrite_without_effects(text: string, spaces: baml.regex.Regex) -> string throws never {
    text.replace_all(spaces, " ")
}

test "replace_string_replacement_infers_no_effect" {
    assert.equal(rewrite_without_effects("a  b\tc", baml.regex.compile("\\s+")), "a b c")
}

// ─── The dynamic-vocabulary job ──────────────────────────────────────────────

// The shape the design is aimed at: terms supplied at runtime, replaced as
// whole words, case-adapted through application code, with no chance of a
// term becoming pattern syntax.
function apply_terms(text: string, terms: Term[]) -> string throws baml.regex.Error {
    let out = text;
    for (let t in terms) {
        out
            = out.replace_all(baml.regex.word(t.from, ignore_case = true), (m: baml.regex.Match) -> string {
                if (m.text == m.text.to_upper_case()) {
                    t.to.to_upper_case()
                } else {
                    t.to
                }
            });
    }
    out
}

test "replace_dynamic_vocabulary" {
    let terms = [Term { from: "colour", to: "color" }, Term { from: "c.f.", to: "cf." }];
    assert.equal(
        apply_terms("The COLOUR of colours, c.f. the guide.", terms),
        "The COLOR of colours, cf. the guide.",
    )
}
