enum Test {
  A @alias("a")
  // Comment
  @description(#"
    tests
  "#)
  
  B @alias("b")
}

// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/alias_is_valid.baml:4
//    | 
//  3 |   // Comment
//  4 |   @description(#"
//  5 |     tests
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/alias_is_valid.baml:6
//    | 
//  5 |     tests
//  6 |   "#)
//  7 |   
//    | 
