enum Test {
    A,
    B,
    C,
}

// error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be all caps.
//   -->  enum/invalid_commas.baml:2
//    | 
//  1 | enum Test {
//  2 |     A,
//  3 |     B,
//    | 
// error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be all caps.
//   -->  enum/invalid_commas.baml:3
//    | 
//  2 |     A,
//  3 |     B,
//  4 |     C,
//    | 
// error: Error validating: This line is not an enum value definition. BAML enums don't have commas, and all values must be all caps.
//   -->  enum/invalid_commas.baml:4
//    | 
//  3 |     B,
//  4 |     C,
//  5 | }
//    | 
