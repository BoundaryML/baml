function add(x: int, y: int) -> int throws never {
    x + y
}

function main() -> int throws never {
    let f: reflect.AnyFunction<Returns = int, Throws = never> = add;
    return reflect.call_any(f, { "x": 20, "y": 22 }) catch (e) {
        _ => -1
    };
}

test "call_any_dispatches_named_args" {
    assert.equal(main(), 42)
}
