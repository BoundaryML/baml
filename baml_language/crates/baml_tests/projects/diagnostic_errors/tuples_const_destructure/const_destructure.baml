// A destructure started with `const` binds only with `const`, recursively;
// one started with `let` may mix introducers. Each function isolates one case.

class CdPoint {
    x: int,
    y: (int, int),
}

// Allowed: a `let` destructure may contain `const`.
function let_mixes_introducers(pair: (int, int)) -> int {
    let (let a, const b) = pair;
    a + b
}

// Rejected: `let` inside a `const` destructure.
function let_inside_const(pair: (int, int)) -> int {
    const (let a, const b) = pair;
    a + b
}

// Rejected: the restriction reaches nested tuple patterns.
function let_nested_in_const(nested: (int, (int, int))) -> int {
    const (const a, (const b, let c)) = nested;
    a + b + c
}

// Rejected: a nested `const` destructure inside a `let` one imposes its own
// restriction.
function let_inside_nested_const(nested: (int, (int, int))) -> int {
    let (let a, const inner: (const b, let c)) = nested;
    a + b + c + inner[0]
}

// Rejected: the rule covers every destructuring form, not just tuples.
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
