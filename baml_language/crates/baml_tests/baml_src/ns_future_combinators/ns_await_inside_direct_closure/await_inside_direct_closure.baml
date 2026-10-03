// Control: awaiting inside a closure called DIRECTLY (not via a native
// continuation). Isolates whether the bug is closure-general or map-specific.

function one() -> int {
    7
}

function main() -> int {
    let fut = spawn { one() };
    let g = (x: baml.future.Future<int, never>) -> {
        await x
    };
    g(fut)
}

test "await_inside_direct_closure" {
    assert.equal(main(), 7)
}
