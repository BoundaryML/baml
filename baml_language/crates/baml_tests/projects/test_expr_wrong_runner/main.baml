// Runner expression is the wrong type — should produce a type diagnostic
// if the runner is actually wired through lowering.
// BUG: Currently the runner is discarded (hardcoded to null), so no error appears.
// Expected: diagnostics for type mismatch on 42 and "not a runner".

test "wrong runner type" with 42 {
  assert.is_true(true)
}

test "string as runner" with "not a runner" {
  assert.is_true(true)
}

testset "wrong testset runner" with 42 {
  test "inner" {
    assert.is_true(true)
  }
}
