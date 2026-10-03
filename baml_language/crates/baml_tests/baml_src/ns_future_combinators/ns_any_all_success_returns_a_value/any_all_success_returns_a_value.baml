// `any` happy path: all inputs succeed; the first success is returned.

function five() -> int {
    5
}

function six() -> int {
    6
}

function main() -> int {
    let fs = [spawn { five() }, spawn { six() }];
    await baml.future.any(fs) catch (e) {
        let e => -1
    }
}

test "any_all_success_returns_a_value" {
    let result = main();
    assert.is_true(result == 5 || result == 6)
}
