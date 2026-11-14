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

function rest_pattern(t: (int, int, int)) -> int {
    match (t) {
        (let a, ..) => a,
    }
}

function postfix_annotation(t: (int, int)) -> int {
    let (let a, let b): (int, int) = t;
    a + b
}

function destructured_parameter((let a, let b): (int, int)) -> int {
    0
}
