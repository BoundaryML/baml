// Raw string literals as test names should work.
// BUG: Lowering only looks for STRING_LITERAL, not RAW_STRING_LITERAL,
// so the name is silently dropped.

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
