enum Test {
  A @alias("a")
  // Comment
  @description(#"
    tests
  "#)
  
  B @alias("b")
}

// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/alias_is_valid.baml:2
//    | 
//  1 | enum Test {
//  2 |   A @alias("a")
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/alias_is_valid.baml:4
//    | 
//  3 |   // Comment
//  4 |   @description(#"
//  5 |     tests
//    | 
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/alias_is_valid.baml:5
//    | 
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
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/alias_is_valid.baml:8
//    | 
//  7 |   
//  8 |   B @alias("b")
//    | 
