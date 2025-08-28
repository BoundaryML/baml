fn Factorial(mut limit: int) -> int {

	let mut result = 1;


    // used to make the loop break without relying on `break` implementation.
    let mut should_continue = true;
	while (should_continue) {
		result = result * limit;
		limit = limit - 1;

        if (limit != 0) {
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
    while (true) {
        while (true) {
            continue;
        }
    }
}

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  loops/continue.baml:1
//    | 
//    | 
//  1 | fn Factorial(mut limit: int) -> int {
//  2 | 
//    | 
