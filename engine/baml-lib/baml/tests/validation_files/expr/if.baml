let x = if true { 1 } else { 2 };
let y = if false { 1 };
let z = if true {
    12
} else {
    13
};

// warning: Variable assignment is experimental, and will break in the future.
//   -->  expr/if.baml:1
//    |
//    |
//  1 | let x = if true { 1 } else { 2 };
//    |
