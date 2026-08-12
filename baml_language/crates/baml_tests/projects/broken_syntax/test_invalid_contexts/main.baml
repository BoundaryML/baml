// test/testset should NOT be allowed inside regular functions.

function helper() -> null {
  test "inside function" {
    assert.is_true(true)
  }
}

function another() -> null {
  testset "inside function set" {
    test "nested" {
      assert.is_true(true)
    }
  }
}

// But top-level test/testset should still work.
test "top level is fine" {
  assert.is_true(true)
}

testset "top level set is fine" {
  test "nested is fine" {
    assert.is_true(true)
  }
}
