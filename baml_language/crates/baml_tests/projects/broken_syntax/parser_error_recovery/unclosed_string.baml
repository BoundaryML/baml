class Test {
  field1 string @alias("closed string")
  field2 string @alias("unclosed string
  field3 string @alias("another closed string")  // Should still parse this
}
