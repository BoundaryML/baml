// Dynamic test generation — for loops and if conditions containing test/testset.

testset "dynamic tests" {
  let cases = ["hello", "world", "foo"]

  for (let case in cases) {
    test "check case" {
      assert.not_null(case)
    }
  }
}

testset "conditional tests" {
  let run_slow = false

  test "always runs" {
    assert.is_true(true)
  }

  if (run_slow) {
    test "only when slow" {
      assert.is_true(true)
    }
  }
}
