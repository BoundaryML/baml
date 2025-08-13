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

// TODO: this one should be in bytecode
fn ContinueNested() -> int {
    while true {
        while true {
            continue;
        }
    }
}

// error: Expected let expression or for loop
//   -->  loops/continue.baml:8
//    | 
//  7 | if limit == 0 {
//  8 | break;
//  9 | }
//    | 
// error: Expected let expression or for loop
//   -->  loops/continue.baml:19
//    | 
// 18 | 
// 19 |     continue;
// 20 | 
//    | 
// error: Expected let expression or for loop
//   -->  loops/continue.baml:28
//    | 
// 27 |         while true {
// 28 |             continue;
// 29 |         }
//    | 
