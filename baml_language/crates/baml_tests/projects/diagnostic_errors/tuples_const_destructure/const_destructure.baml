
class CdPoint {
    x: int,
    y: (int, int),
}

function let_mixes_introducers(pair: (int, int)) -> int {
    let (let a, const b) = pair;
    a + b
}

function let_inside_const(pair: (int, int)) -> int {
    const (let a, const b) = pair;
    a + b
}

function let_nested_in_const(nested: (int, (int, int))) -> int {
    const (const a, (const b, let c)) = nested;
    a + b + c
}

function let_inside_nested_const(nested: (int, (int, int))) -> int {
    let (let a, const inner: (const b, let c)) = nested;
    a + b + c + inner.0
}

function let_inside_const_list(xs: int[]) -> int {
    const [const first, let second] = xs else { return 0; };
    first + second
}

function let_inside_const_class(p: CdPoint) -> int {
    const CdPoint { x: let px, y: (const a, let b) } = p;
    px + a + b
}

function let_inside_const_while_let(pair: (int, int)?) -> int {
    while const (let a, _) = pair {
        return a;
    }
    0
}

function let_inside_const_for(pairs: (int, int)[]) -> int {
    let total = 0;
    for (const (const a, let b) in pairs) {
        total += a + b;
    }
    total
}
