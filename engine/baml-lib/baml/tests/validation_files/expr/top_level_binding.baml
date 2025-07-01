let x = 1;

let y = {
    let b = 2;
    [1,2,3]
};

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/top_level_binding.baml:1
//    | 
//    | 
//  1 | let x = 1;
//    | 
// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/top_level_binding.baml:3
//    | 
//  2 | 
//  3 | let y = {
//  4 |     let b = 2;
//  5 |     [1,2,3]
//  6 | };
//    | 
