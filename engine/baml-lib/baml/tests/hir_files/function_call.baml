function myFunc(x: int, y: string) -> int {
    x
}

function CallTest() -> int {
    let result = myFunc(42, "hello")
    result
}

// error: Statement must end with a semicolon.
//   -->  test.baml:6
//    | 
//  5 | function CallTest() -> int {
//  6 |     let result = myFunc(42, "hello")
//  7 |     result
//    | 
