fn Factorial(limit: int) -> int {

	let result = 1;


	while (true) {
		if (limit == 0) {
			break;
		}
		result = result * limit;
		limit = limit - 1;
	}

	result
}

fn OutsideLoop() -> int {

    break;

    3
}

fn Nested() -> int {
    let a = 5;
    while (true) {
        while (true) {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
    a // should be 2
}

// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/break.baml:1
//    | 
//    | 
//  1 | fn Factorial(limit: int) -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/break.baml:17
//    | 
// 16 | 
// 17 | fn OutsideLoop() -> int {
//    | 
// warning: Workflow functions are experimental, and will break in the future.
//   -->  loops/break.baml:24
//    | 
// 23 | 
// 24 | fn Nested() -> int {
//    | 
// error: Error validating: 'break' cannot be used outside of a loop
//   -->  loops/break.baml:19
//    | 
// 18 | 
// 19 |     break;
//    | 
