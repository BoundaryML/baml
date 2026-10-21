// Expression names — string concatenation in test names.

testset "concat names" {
  test "prefix_" + "suffix" {
    null
  }

  testset "nested_" + "group" {
    test "inner" {
      null
    }
  }
}
