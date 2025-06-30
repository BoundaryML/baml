function Id(x: int) -> int {
    x
}

function Go(x: int) -> int[] {
 [x, Id(x), 3]
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/expr_list.baml:1
//    | 
//    | 
//  1 | function Id(x: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  expr/expr_list.baml:5
//    | 
//  4 | 
//  5 | function Go(x: int) -> int[] {
//    | 
