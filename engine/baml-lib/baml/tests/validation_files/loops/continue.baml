fn Factorial(mut limit: int) -> int {

	let mut result = 1;


	while true {
		if limit == 0 {
			break;
		}
		result = result * limit;
		limit = limit - 1;
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
//   -->  loops/continue.baml:17
//    | 
// 16 | 
// 17 | fn ContinueOutsideLoop() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/continue.baml:25
//    | 
// 24 | // TODO: this one should be in bytecode
// 25 | fn ContinueNested() -> int {
//    | 
// error: Error validating: 'continue' cannot be used outside of a loop
//   -->  loops/continue.baml:19
//    | 
// 18 | 
// 19 |     continue;
//    | 
