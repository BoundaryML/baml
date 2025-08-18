fn Factorial(mut limit: int) -> int {

	let mut result = 1;


    // used to make the loop break without relying on `break` implementation.
    let mut should_continue = true;
	while should_continue {
		result = result * limit;
		limit = limit - 1;

        if limit != 0 {
            continue;
        } else {
            should_continue = false;
        }

	}

	result
}

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
//  1 | fn Factorial(mut limit: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/continue.baml:23
//    | 
// 22 | 
// 23 | fn ContinueOutsideLoop() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/continue.baml:30
//    | 
// 29 | 
// 30 | fn ContinueNested() -> int {
//    | 
// error: Error validating: 'continue' cannot be used outside of a loop
//   -->  loops/continue.baml:25
//    | 
// 24 | 
// 25 |     continue;
//    | 
