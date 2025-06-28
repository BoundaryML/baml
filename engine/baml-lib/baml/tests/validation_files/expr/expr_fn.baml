function Foo(x: int) -> int {
    let y = x;
    x
}

fn Bar(x: int) -> int {
    // test
    let y = x;
    x
}

test TestBar {
    functions [Foo]
    args {
        x 1
    }
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/expr_fn.baml:1
//    | 
//    | 
//  1 | function Foo(x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/expr_fn.baml:6
//    | 
//  5 | 
//  6 | fn Bar(x: int) -> int {
//    | 
