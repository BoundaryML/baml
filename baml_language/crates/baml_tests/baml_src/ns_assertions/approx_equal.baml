test "approx_equal accepts ordinary floating-point rounding" {
    let total = 9.99 + 5.50 + 2.00;
    assert.approx_equal(total, 17.49, 0.000001)
}
