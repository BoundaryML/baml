enum Test {
    ONE
    TWO
    ONE
}

// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/duplicate_value.baml:2
//    | 
//  1 | enum Test {
//  2 |     ONE
//    | 
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/duplicate_value.baml:3
//    | 
//  2 |     ONE
//  3 |     TWO
//    | 
// error: Error validating Enum "Test": This field declaration is invalid. It is either missing a name or a type.
//   -->  enum/duplicate_value.baml:4
//    | 
//  3 |     TWO
//  4 |     ONE
//    | 
