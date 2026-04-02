// Raw string literals as test names should work.

test #"raw string name"# {
  assert.is_true(true)
}

test #"has "quotes" inside"# {
  assert.is_true(true)
}

testset #"raw testset name"# {
  test #"nested raw name"# {
    assert.is_true(true)
  }
}
