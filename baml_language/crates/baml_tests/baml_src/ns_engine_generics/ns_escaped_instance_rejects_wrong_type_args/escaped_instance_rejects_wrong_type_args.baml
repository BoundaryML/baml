// Negative control for the test above: the same escaped instance does NOT
// match a differently-instantiated type.

class Box<T> {
    v: T,
}

function mk<T>(x: T) -> Box<T> {
    let b: Box<T> = Box { v: x };
    b
}

function main() -> int {
    let b: unknown = mk(5);
    if (b is Box<string>) {
        1
    } else {
        0
    }
}

test "escaped_instance_rejects_wrong_type_args" {
    assert.equal(main(), 0)
}
