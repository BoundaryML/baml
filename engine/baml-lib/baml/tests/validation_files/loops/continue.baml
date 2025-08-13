fn ContinueOutsideLoop() -> int {

    continue;

    3
}

fn ContinueNested() -> int {
    while true {
        while true {
            continue;
        }
    }
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/continue.baml:1
//    | 
//    | 
//  1 | fn ContinueOutsideLoop() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/continue.baml:8
//    | 
//  7 | 
//  8 | fn ContinueNested() -> int {
//    | 
// error: Error validating: 'continue' cannot be used outside of a loop
//   -->  loops/continue.baml:3
//    | 
//  2 | 
//  3 |     continue;
//    | 
