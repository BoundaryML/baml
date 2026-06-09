function UseReserved() -> int {
  let await = 1;
  await
}

// error: Error validating: 'await' is a reserved keyword.
//   -->  expr/var_keyword_let_await.baml:2
//    |
//  1 | function UseReserved() -> int {
//  2 |   let await = 1;
//    |

//----
//- diagnostics
// error: Expected identifier after 'let', found await
//    ╭─[ expr_var_keyword_let_await.baml:2:7 ]
//    │
//  2 │   let await = 1;
//    │       ──┬──  
//    │         ╰──── Expected identifier after 'let', found await
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected initializer (=), found await
//    ╭─[ expr_var_keyword_let_await.baml:2:7 ]
//    │
//  2 │   let await = 1;
//    │       ──┬──  
//    │         ╰──── Expected initializer (=), found await
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected expression, found '='
//    ╭─[ expr_var_keyword_let_await.baml:2:13 ]
//    │
//  2 │   let await = 1;
//    │             ┬  
//    │             ╰── Expected expression, found '='
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected expression, found '}'
//    ╭─[ expr_var_keyword_let_await.baml:4:1 ]
//    │
//  4 │ }
//    │ ┬  
//    │ ╰── Expected expression, found '}'
//    │ 
//    │ Note: Error code: E0010
// ───╯
// error: Expected '}', found EOF
//     ╭─[ expr_var_keyword_let_await.baml:11:7 ]
//     │
//  11 │ //    |
//     │       ┬  
//     │       ╰── Expected '}', found EOF
//     │ 
//     │ Note: Error code: E0010
// ────╯
