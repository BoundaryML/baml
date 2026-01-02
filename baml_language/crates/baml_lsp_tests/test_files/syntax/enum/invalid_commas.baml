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

//----
//- diagnostics
// Error: Expected Unexpected token in enum body, found ','
//    ╭─[ 0:2:6 ]
//    │
//  2 │     A,
//    │      ┬  
//    │      ╰── Expected Unexpected token in enum body, found ','
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in enum body, found ','
//    ╭─[ 0:3:6 ]
//    │
//  3 │     B,
//    │      ┬  
//    │      ╰── Expected Unexpected token in enum body, found ','
//    │ 
//    │ Note: Error code: E0010
// ───╯
// Error: Expected Unexpected token in enum body, found ','
//    ╭─[ 0:4:6 ]
//    │
//  4 │     C,
//    │      ┬  
//    │      ╰── Expected Unexpected token in enum body, found ','
//    │ 
//    │ Note: Error code: E0010
// ───╯
