enum Test {
  A @alias("a")
  // Comment
  @description(#"
    tests
  "#)

  B @alias("b")
}

//----
//- diagnostics
// <no-diagnostics-expected>
