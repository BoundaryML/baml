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
