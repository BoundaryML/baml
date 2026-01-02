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

//----
//- diagnostics
// Error: Expected Unexpected token in enum body, found '|'
//    ╭─[ 0:2:11 ]
//    │
//  2 │     A int | string
//    │           ┬  
//    │           ╰── Expected Unexpected token in enum body, found '|'
//    │ 
//    │ Note: Error code: E0010
// ───╯
