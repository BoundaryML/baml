enum Test {
    A,
    B,
    C,
}

// error: Error validating: This line is not an enum value definition. Check for extra commas
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/enum/invalid_commas.baml:2
//    | 
//  1 | enum Test {
//  2 |     A,
//  3 |     B,
//    | 
// error: Error validating: This line is not an enum value definition. Check for extra commas
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/enum/invalid_commas.baml:3
//    | 
//  2 |     A,
//  3 |     B,
//  4 |     C,
//    | 
// error: Error validating: This line is not an enum value definition. Check for extra commas
//   -->  /Users/aaronvillalpando/Projects/gloo-lang/engine/baml/baml/tests/validation_files/enum/invalid_commas.baml:4
//    | 
//  3 |     B,
//  4 |     C,
//  5 | }
//    | 
