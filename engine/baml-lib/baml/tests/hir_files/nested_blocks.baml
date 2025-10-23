function Foo() -> int {
    let x = {
        let y = 1
        y
    }
    x
}

// error: Statement must end with a semicolon.
//   -->  test.baml:3
//    | 
//  2 |     let x = {
//  3 |         let y = 1
//  4 |         y
//    | 
// error: Statement must end with a semicolon.
//   -->  test.baml:2
//    | 
//  1 | function Foo() -> int {
//  2 |     let x = {
//  3 |         let y = 1
//  4 |         y
//  5 |     }
//  6 |     x
//    | 
