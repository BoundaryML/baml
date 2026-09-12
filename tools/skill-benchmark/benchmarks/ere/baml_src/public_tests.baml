class PublicMatchCase {
    name: string,
    pattern: string,
    input: string,
    expected: EreMatch?,
}

class PublicErrorCase {
    name: string,
    pattern: string,
}

function capture(text: string, start: int, end: int) -> Capture {
    Capture { text: text, start: start, end: end }
}

function ere_match(captures: Capture?[]) -> EreMatch {
    EreMatch { captures: captures }
}

function public_match_cases() -> PublicMatchCase[] {
    [
        PublicMatchCase {
            name: "literal search",
            pattern: "abc",
            input: "xxabcxx",
            expected: ere_match([capture("abc", 2, 5)]),
        },
        PublicMatchCase { name: "literal no match", pattern: "abc", input: "ac", expected: null },
        PublicMatchCase {
            name: "earliest start wins",
            pattern: "ab|z",
            input: "zab",
            expected: ere_match([capture("z", 0, 1)]),
        },
        PublicMatchCase {
            name: "longest alternative wins",
            pattern: "a|ab",
            input: "ab",
            expected: ere_match([capture("ab", 0, 2)]),
        },
        PublicMatchCase {
            name: "dot",
            pattern: "a.c",
            input: "xxa-cyy",
            expected: ere_match([capture("a-c", 2, 5)]),
        },
        PublicMatchCase {
            name: "dot matches newline",
            pattern: ".",
            input: "\n",
            expected: ere_match([capture("\n", 0, 1)]),
        },
        PublicMatchCase {
            name: "start anchor",
            pattern: "^ab",
            input: "abcab",
            expected: ere_match([capture("ab", 0, 2)]),
        },
        PublicMatchCase {
            name: "end anchor",
            pattern: "ab$",
            input: "abcab",
            expected: ere_match([capture("ab", 3, 5)]),
        },
        PublicMatchCase {
            name: "empty anchored match",
            pattern: "^$",
            input: "",
            expected: ere_match([capture("", 0, 0)]),
        },
        PublicMatchCase {
            name: "star is longest",
            pattern: "ab*",
            input: "xxabbbcy",
            expected: ere_match([capture("abbb", 2, 6)]),
        },
        PublicMatchCase {
            name: "plus",
            pattern: "ab+",
            input: "acabb",
            expected: ere_match([capture("abb", 2, 5)]),
        },
        PublicMatchCase {
            name: "question mark absent",
            pattern: "^(a)?b$",
            input: "b",
            expected: ere_match([capture("b", 0, 1), null]),
        },
        PublicMatchCase {
            name: "question mark present",
            pattern: "^(a)?b$",
            input: "ab",
            expected: ere_match([capture("ab", 0, 2), capture("a", 0, 1)]),
        },
        PublicMatchCase {
            name: "exact interval",
            pattern: "a{3}",
            input: "baaaac",
            expected: ere_match([capture("aaa", 1, 4)]),
        },
        PublicMatchCase {
            name: "open interval",
            pattern: "a{2,}",
            input: "baaaac",
            expected: ere_match([capture("aaaa", 1, 5)]),
        },
        PublicMatchCase {
            name: "bounded interval",
            pattern: "a{2,3}",
            input: "baaaac",
            expected: ere_match([capture("aaa", 1, 4)]),
        },
        PublicMatchCase {
            name: "zero interval",
            pattern: "a{0}b",
            input: "ab",
            expected: ere_match([capture("b", 1, 2)]),
        },
        PublicMatchCase {
            name: "capture order",
            pattern: "^(a)(b(c))$",
            input: "abc",
            expected: ere_match([capture("abc", 0, 3), capture("a", 0, 1), capture("bc", 1, 3), capture("c", 2, 3)]),
        },
        PublicMatchCase {
            name: "repeated capture is final iteration",
            pattern: "^(..)*$",
            input: "abcd",
            expected: ere_match([capture("abcd", 0, 4), capture("cd", 2, 4)]),
        },
        PublicMatchCase {
            name: "capture in selected alternative",
            pattern: "a(b)|c(d)|a(e)f",
            input: "aef",
            expected: ere_match([capture("aef", 0, 3), null, null, capture("e", 1, 2)]),
        },
        PublicMatchCase {
            name: "capture disambiguation",
            pattern: "(a*)(a|aa)",
            input: "aaaa",
            expected: ere_match([capture("aaaa", 0, 4), capture("aaa", 0, 3), capture("a", 3, 4)]),
        },
        PublicMatchCase {
            name: "empty repeated capture terminates",
            pattern: "(a*)*",
            input: "x",
            expected: ere_match([capture("", 0, 0), capture("", 0, 0)]),
        },
        PublicMatchCase {
            name: "bracket list",
            pattern: "[abc]+",
            input: "xxcabdz",
            expected: ere_match([capture("cab", 2, 5)]),
        },
        PublicMatchCase {
            name: "bracket range",
            pattern: "[a-c]+",
            input: "zzabccdz",
            expected: ere_match([capture("abcc", 2, 6)]),
        },
        PublicMatchCase {
            name: "negated bracket",
            pattern: "[^a]+",
            input: "a123a",
            expected: ere_match([capture("123", 1, 4)]),
        },
        PublicMatchCase {
            name: "digit class",
            pattern: "[[:digit:]]+",
            input: "id=2048!",
            expected: ere_match([capture("2048", 3, 7)]),
        },
        PublicMatchCase {
            name: "hex class",
            pattern: "^[[:xdigit:]]+$",
            input: "09aF",
            expected: ere_match([capture("09aF", 0, 4)]),
        },
        PublicMatchCase {
            name: "escaped metacharacter",
            pattern: `a\+b`,
            input: "a+b",
            expected: ere_match([capture("a+b", 0, 3)]),
        },
    ]
}

function public_error_cases() -> PublicErrorCase[] {
    [
        PublicErrorCase { name: "unterminated group", pattern: "(abc" },
        PublicErrorCase { name: "unterminated bracket", pattern: "[abc" },
        PublicErrorCase { name: "descending range", pattern: "[z-a]" },
        PublicErrorCase { name: "quantifier without atom", pattern: "*a" },
        PublicErrorCase { name: "multiple quantifiers", pattern: "a**" },
        PublicErrorCase { name: "descending interval", pattern: "a{3,2}" },
        PublicErrorCase { name: "interval above limit", pattern: "a{256}" },
        PublicErrorCase { name: "unsupported lookaround", pattern: "(?=a)" },
    ]
}

function pattern_compiles(pattern: string) -> bool {
    let _ = exec_ere(pattern, "");
    true
}

testset "ere_public_matches" {
    for (let c in public_match_cases()) {
        test c.name {
            assert.equal(exec_ere(c.pattern, c.input), c.expected)
        }
    }
}

testset "ere_public_errors" {
    for (let c in public_error_cases()) {
        test c.name {
            let accepted = pattern_compiles(c.pattern) catch (e) {
                baml.errors.ParseError => false
            };
            assert.equal(accepted, false)
        }
    }
}
