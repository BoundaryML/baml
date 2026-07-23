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
// E0010
//
//   × unexpected token
//    ╭─[expr_var_keyword_let_await.baml:2:7]
//  2 │   let await = 1;
//    ·       ──┬──
//    ·         ╰── expected `identifier after 'let'`, found `await`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[expr_var_keyword_let_await.baml:2:13]
//  2 │   let await = 1;
//    ·             ┬
//    ·             ╰── expected `expression`, found `'='`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[expr_var_keyword_let_await.baml:4:1]
//  4 │ }
//    · ┬
//    · ╰── expected `expression`, found `'}'`
//    ╰────
// E0010
//
//   × unexpected token
//    ╭─[expr_var_keyword_let_await.baml:4:2]
//  4 │ }
//    ·  ▲
//    ·  ╰── expected `'}'`, found `EOF`
//    ╰────
