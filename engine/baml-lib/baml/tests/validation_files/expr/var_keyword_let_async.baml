function UseReservedAsync() -> int {
  let async = 1;
  async
}

// error: Error validating: 'async' is a reserved keyword.
//   -->  expr/var_keyword_let_async.baml:2
//    | 
//  1 | function UseReservedAsync() -> int {
//  2 |   let async = 1;
//    | 
