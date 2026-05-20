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
// error: Duplicate variant `Test.ONE`
//    ╭─[ enum_duplicate_value.baml:4:5 ]
//    │
//  2 │     ONE
//    │     ─┬─  
//    │      ╰─── first defined as variant here
//    │ 
//  4 │     ONE
//    │     ─┬─  
//    │      ╰─── duplicate variant definition
//    │ 
//    │ Note: Error code: E0012
// ───╯
