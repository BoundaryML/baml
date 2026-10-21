test "with quorum" with testing.Quorum(5, 3) {
  let result = "hello"
  assert.not_null(result)
}
