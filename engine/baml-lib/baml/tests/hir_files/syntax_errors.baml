function Test() -> int {
  let x = 1
  x
}

// error: Statement must end with a semicolon.
//   -->  test.baml:2
//    | 
//  1 | function Test() -> int {
//  2 |   let x = 1
//  3 |   x
//    | 
