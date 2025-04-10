fn NoRet(x: int) {
    1
}

// error: fn must have a return type: e.g. fn Foo() -> int
//   -->  expr/missing_return_value.baml:1
//    | 
//    | 
//  1 | fn NoRet(x: int) {
//  2 |     1
//  3 | }
//    | 
