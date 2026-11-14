// Deliberately wrong: ignores `high`, so the test below fails.
function clamp(value: int, low: int, high: int) -> int {
    value.max(low)
}

test "clamp caps at high" {
    assert.equal(clamp(42, 0, 10), 10)
}
