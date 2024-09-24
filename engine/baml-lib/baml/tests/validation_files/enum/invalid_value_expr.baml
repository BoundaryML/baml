enum Test {
    A int | string
}

// error: Error validating: Unexpected type specified for value `A`
//   -->  enum/invalid_value_expr.baml:2
//    | 
//  1 | enum Test {
//  2 |     A int | string
//  3 | }
//    | 
