// BAML has no destructuring assignment: the left-hand side of `=` (or a
// compound `+=`, …) must be a place - a variable, a field, or an index. Any
// other expression is rejected at lowering. These previously compiled and
// silently did nothing: MIR evaluated the target into a temporary and stored
// the value there, leaving `a`, `b`, `x`, and `y` unchanged.

class P {
    a int
    b string
}

function mk() -> P {
    P { a: 3, b: "x" }
}

function assign_to_class_literal() -> string {
    let a = 0;
    let b = "";
    P { a: a, b: b } = mk();
    b + a.to_string()
}

function assign_to_array_literal() -> string {
    let x = 0;
    let y = 0;
    [x, y] = [1, 2];
    x.to_string() + y.to_string()
}

function compound_assign_to_array_literal() -> int {
    let x = 0;
    [x] += [1];
    x
}

function assign_to_call() -> int {
    mk() = mk();
    0
}

function assign_to_literal() -> int {
    1 = 2;
    0
}

// Places remain assignable, including through `?.` and parentheses.
function valid_targets(p: P, xs: int[], q: P?) -> int {
    let n = 0;
    n = 1;
    (n) = 2;
    p.a = 3;
    xs[0] = 4;
    q?.a = 5;
    n += xs[0];
    n
}
