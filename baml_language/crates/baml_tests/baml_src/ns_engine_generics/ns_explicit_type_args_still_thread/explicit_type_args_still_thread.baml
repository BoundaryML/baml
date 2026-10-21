// Explicit call-site type args still win over inference (regression guard
// for the explicit path the inferred fallback was added next to).

class Box<T> {
    v: T,
}

function mk<T>(x: T) -> Box<T> {
    let b: Box<T> = Box { v: x };
    b
}

function main() -> int {
    let b = mk<string>("hi");
    if (b is Box<string>) {
        1
    } else {
        0
    }
}

test "explicit_type_args_still_thread" {
    assert.equal(main(), 1)
}
