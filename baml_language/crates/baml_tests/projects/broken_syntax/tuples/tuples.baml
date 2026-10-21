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

// Tuple indices are plain decimal integers.
function bad_index(t: (int, int)) -> int {
    t.01
}
