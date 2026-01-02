enum Test {
    ONE
    TWO
    ONE
}

// error: Value "ONE" is already defined on enum "Test".
//   -->  enum/duplicate_value.baml:4
//    | 
//  3 |     TWO
//  4 |     ONE
//  5 | }
//    |

//----
//- diagnostics
// Error: Duplicate variant 'ONE' in enum 'Test'
//    ╭─[ 0:4:5 ]
//    │
//  4 │     ONE
//    │     ─┬─  
//    │      ╰─── duplicate definition
//    │
//    ├─[ 0:4:5 ]
//    │
//  2 │     ONE
//    │     ─┬─  
//    │      ╰─── first definition here
//    │ 
//    │ Note: Error code: E0013
// ───╯
