// Tuple patterns in conditional positions, and the annotated scrutinee form,
// which needs its own outer parentheses: `match ((a, b): T)`.

function tpp_annotated_scrutinee(a: int, b: string?) -> string {
    match ((a, b): (int, string?)) {
        (0, null) => "zero",
        (let n, null) => `${n}`,
        (_, let s: string) => s,
    }
}

test "tuple_match_annotated_scrutinee" {
    assert.equal(tpp_annotated_scrutinee(0, null), "zero")
    assert.equal(tpp_annotated_scrutinee(3, null), "3")
    assert.equal(tpp_annotated_scrutinee(3, "s"), "s")
}

function tpp_if_let(pair: (int, string)?) -> string {
    if let (let n, let s) = pair {
        `${s}${n}`
    } else {
        "none"
    }
}

// A refutable element only matches its value.
function tpp_if_let_refutable(pair: (int, string)) -> string {
    if let (0, let s) = pair {
        s
    } else {
        "nonzero"
    }
}

function tpp_if_const(pair: (int, int)?) -> int {
    if const (const a, const b) = pair {
        a + b
    } else {
        -1
    }
}

test "tuple_if_let" {
    assert.equal(tpp_if_let((1, "a")), "a1")
    assert.equal(tpp_if_let(null), "none")
    assert.equal(tpp_if_let_refutable((0, "zero")), "zero")
    assert.equal(tpp_if_let_refutable((1, "one")), "nonzero")
    assert.equal(tpp_if_const((2, 3)), 5)
    assert.equal(tpp_if_const(null), -1)
}
