// `const` takes every destructuring head `let` does, tuples included. A
// `const` destructure binds only with `const` (recursively); a `let` one may
// mix introducers. `const` still behaves like `let` (each introducer carries
// that warning).

class TpcPoint {
    x: int,
    y: (int, int),
}

function tpc_const_pair() -> int {
    const (const a, const b) = (1, 2);
    a * 10 + b
}

function tpc_const_nested() -> int {
    const (const a, (const b, const c)) = (1, (2, 3));
    a * 100 + b * 10 + c
}

function tpc_const_singleton() -> int {
    const (const only,) = (7,);
    only
}

function tpc_let_mixes() -> int {
    let (let a, const b) = (4, 5);
    a * 10 + b
}

function tpc_const_else(pair: (int, int)) -> int {
    const (0, const b) = pair else { return -1; };
    b
}

function tpc_while_const(pair: (int, int)?) -> int {
    while const (const a, _) = pair {
        return a;
    }
    0
}

function tpc_for_const() -> int {
    let total = 0;
    for (const (const a, const b) in [(1, 2), (3, 4)]) {
        total += a * b;
    }
    total
}

function tpc_const_class() -> int {
    const TpcPoint { x: const px, y: (const a, const b) } = TpcPoint { x: 1, y: (2, 3) };
    px + a + b
}

function tpc_const_list() -> int {
    const [const first, const second] = [8, 9] else { return 0; };
    first + second
}

test "const_destructuring_matches_let" {
    assert.equal(tpc_const_pair(), 12)
    assert.equal(tpc_const_nested(), 123)
    assert.equal(tpc_const_singleton(), 7)
    assert.equal(tpc_let_mixes(), 45)
    assert.equal(tpc_const_else((0, 6)), 6)
    assert.equal(tpc_const_else((1, 6)), -1)
    assert.equal(tpc_while_const((3, 4)), 3)
    assert.equal(tpc_while_const(null), 0)
    assert.equal(tpc_for_const(), 14)
    assert.equal(tpc_const_class(), 6)
    assert.equal(tpc_const_list(), 17)
}
