class TestLiterals {
  a "SingleLiteral"
  b "Field" | "With" | "Multiple" | "Literals"
  c "Field" | "With" | "Some" @description("Description")
  d 2 | 3
  e "Optional"?
  f 2 | "SomeString"
}
