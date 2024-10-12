class TestLiterals {
  a "SingleLiteral"
  b 2 | 3.25
}

// error: Error validating: Float literal values are not supported: 3.25
//   -->  class/unsupported_literal_types.baml:3
//    | 
//  2 |   a "SingleLiteral"
//  3 |   b 2 | 3.25
//    | 
