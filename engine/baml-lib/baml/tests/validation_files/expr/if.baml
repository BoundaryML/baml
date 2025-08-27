let x = if (true) {
    1
} else {
    2
};

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/if.baml:1
//    | 
//    | 
//  1 | let x = if (true) {
//  2 |     1
//  3 | } else {
//  4 |     2
//  5 | };
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/if.baml:3
//    | 
//  2 | let y = if false { 1 };
//  3 | let z = if true {
//  4 |     12
//  5 | } else {
//  6 |     13
//  7 | };
//    | 
