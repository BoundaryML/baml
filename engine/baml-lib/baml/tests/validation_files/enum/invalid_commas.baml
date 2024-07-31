enum Test {
    A,
    B,
    C,
}

// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/invalid_commas.baml:2
//    | 
//  1 | enum Test {
//  2 |     A,
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/invalid_commas.baml:2
//    | 
//  1 | enum Test {
//  2 |     A,
//  3 |     B,
//    | 
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/invalid_commas.baml:3
//    | 
//  2 |     A,
//  3 |     B,
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/invalid_commas.baml:3
//    | 
//  2 |     A,
//  3 |     B,
//  4 |     C,
//    | 
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/invalid_commas.baml:4
//    | 
//  3 |     B,
//  4 |     C,
//    | 
// error: Error validating: This line is not a valid field or attribute definition. A valid class property looks like: 'myProperty string[] @description("This is a description")'
//   -->  enum/invalid_commas.baml:4
//    | 
//  3 |     B,
//  4 |     C,
//  5 | }
//    | 
