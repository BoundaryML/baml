function main() -> int throws never {
    let double = (x: int) -> int throws never {
        x * 2
    };
    let f: reflect.AnyFunction<Returns = int, Throws = never> = double;
    return reflect.call_any(f, { "x": 21 }) catch (e) {
        _ => -1
    };
}

test "call_any_lambda_callee" {
    assert.equal(main(), 42)
}
