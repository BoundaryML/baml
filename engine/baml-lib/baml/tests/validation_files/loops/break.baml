function Factorial(limit: int) -> int {

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

function OutsideLoop() -> int {

    break;

    3
}

function Nested() -> int {
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

// error: Error validating: 'break' cannot be used outside of a loop
//   -->  loops/break.baml:19
//    | 
// 18 | 
// 19 |     break;
//    | 
