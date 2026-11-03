// There is no empty tuple: `()` is an error in value, type and pattern
// position (a zero-parameter lambda `() => { … }` is unaffected).
function empty_tuple_value() -> int {
    let e = ();
    0
}

function empty_tuple_type(x: ()) -> int {
    0
}

function empty_tuple_pattern(x: int) -> int {
    match (x) {
        () => 1,
        _ => 0,
    }
}

// Tuple elements are read by index, `t[0]`, not as `t.0`.
function dot_index(t: (int, int)) -> int {
    t.0
}

// Tuple elements have no labels: `(x: int, y: string)` without an arrow is one
// error at the first label, in type and pattern position alike. With an arrow
// it is a function type.
function labeled_elements(p: (x: int, y: string)) -> int {
    0
}

function labeled_pattern() -> int {
    let q: (x: int, y: string) = (1, "a");
    0
}

function labeled_function_type(f: (x: int, y: string) -> bool) -> bool {
    f(1, "a")
}

// There are no rest patterns.
function rest_pattern(t: (int, int, int)) -> int {
    match (t) {
        (let a, ..) => a,
    }
}

// A pattern takes no postfix annotation: write `let whole: (let a, let b)`
// or annotate the elements.
function postfix_annotation(t: (int, int)) -> int {
    let (let a, let b): (int, int) = t;
    a + b
}

// Parameter lists do not destructure.
function destructured_parameter((let a, let b): (int, int)) -> int {
    0
}
