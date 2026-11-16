function clamp(value: int, low: int, high: int) -> int {
    value.clamp(low, high)
}

test "clamp keeps values in range" {
    assert.equal(clamp(5, 0, 10), 5);
    assert.equal(clamp(-3, 0, 10), 0);
    assert.equal(clamp(42, 0, 10), 10)
}
