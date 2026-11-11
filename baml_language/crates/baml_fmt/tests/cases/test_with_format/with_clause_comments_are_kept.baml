class A {}
class B {}

test "t" with /* w1 */ A /* w2 */, /* w3 */ B /* w4 */ {
    assert.is_true(true)
}
