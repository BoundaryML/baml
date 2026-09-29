// BAML destructures only through `let` patterns, never into an assignment:
// the left-hand side of `=` (or a compound `+=`, …) must be a place - a
// variable, a field, or an index. Any other expression is rejected at
// lowering. These previously compiled and
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

// The diagnostic suggests the closest valid `let` destructure, which binds
// (or rebinds) the same names. Renames and nesting carry over; a leaf that is
// a field or an index has no pattern equivalent, so there is no suggestion.
class Pair {
    left: P,
    right: int,
}

function assign_to_nested_literal() -> int {
    let n = 0;
    let s = "";
    let r = 0;
    Pair { left: P { a: n, b: s }, right: r } = Pair { left: mk(), right: 1 };
    n + r
}

function assign_to_literal_with_field_leaf(p: P) -> int {
    let y = 0;
    [p.a, y] = [1, 2];
    y
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

// A path is only a place when its root is a local binding. A function or an
// enum variant has no storage (`mk = mk` previously crashed MIR lowering with
// an internal error; `Shape.Circle = Shape.Circle` silently did nothing).
enum Shape {
    Circle
    Square
}

function assign_to_function() -> int {
    mk = mk;
    0
}

function assign_to_enum_variant() -> Shape {
    Shape.Circle = Shape.Circle;
    Shape.Circle
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
