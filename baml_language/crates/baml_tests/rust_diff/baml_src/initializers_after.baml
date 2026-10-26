// A second file forces the test chainer to pass the same live collector on.
test "initializer allocations survive registration" {
    assert.equal(INIT_MAPPED[95], 137);
    assert.equal(INIT_ALIAS[95].x, 95);
}
