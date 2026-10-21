function scale(x: int, factor: int = x + 1) -> int throws never {
    x * factor
}

function main() -> int throws never {
    let f: reflect.AnyFunction<Returns = int, Throws = never> = scale;
    // Absent: the callee's own (non-constant) default fires: 5 * 6.
    let defaulted = reflect.call_any(f, { "x": 5 }) catch (e) {
        _ => -1
    };
    // Present: the supplied value wins: 5 * 10.
    let supplied = reflect.call_any(f, { "x": 5, "factor": 10 }) catch (e) {
        _ => -1
    };
    return defaulted * 1000 + supplied;
}

test "call_any_absent_optional_fires_callee_default" {
    assert.equal(main(), 30050)
}
