function top_generic<T>(value: T) -> unknown throws never {
    match value {
        (let a, let b) => {
            let first = a;
             // ^^^^^ unknown
            first
        },
        _ => null,
    }
}

function generic_union<T>(value: T | (int, bool)) -> unknown throws never {
    match value {
        (let a, let b) => {
            let first = a;
             // ^^^^^ unknown
            let second = b;
             // ^^^^^^ unknown
            first
        },
        _ => null,
    }
}

function generic_typed_slot<T>(value: T | (int, bool)) -> int throws never {
    match value {
        (let n: int, _) => {
            let number = n;
             // ^^^^^^ int
            number
        },
        _ => 0,
    }
}

function nested_generic<T>(value: (T, string)) -> unknown throws never {
    match value {
        ((let a, _), _) => {
            let first = a;
             // ^^^^^ unknown
            first
        },
        _ => null,
    }
}
