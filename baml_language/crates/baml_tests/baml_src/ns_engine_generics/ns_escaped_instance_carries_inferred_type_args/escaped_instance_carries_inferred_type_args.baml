// An instance constructed inside a generic frame (`Box<T>` in `mk<T>`)
// carries the caller-resolved type args, observable via `is` after the
// value escapes to a non-generic frame. The `mk("hi")` call writes no
// explicit type args — `T = string` is threaded from TIR's inferred
// instantiation.

class Box<T> {
    v: T,
}

function mk<T>(x: T) -> Box<T> {
    let b: Box<T> = Box { v: x };
    b
}

function main() -> int {
    let b = mk("hi");
    if (b is Box<string>) {
        1
    } else {
        0
    }
}

test "escaped_instance_carries_inferred_type_args" {
    assert.equal(main(), 1)
}
