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

fn OutsideLoop() -> int {

    break;

    3
}

// TODO: this one should be in bytecode/execution.
fn Nested() -> int {
    let mut a = 5;
    while true {
        while true {
            a = a + 1;
            break;
        }
        a = a + 1;
        break;
    }
    a // should be 2
}

// error: Expected let expression or for loop
//   -->  loops/break.baml:8
//    | 
//  7 | if limit == 0 {
//  8 | break;
//  9 | }
//    | 
// error: Expected let expression or for loop
//   -->  loops/break.baml:19
//    | 
// 18 | 
// 19 |     break;
// 20 | 
//    | 
// error: Expected let expression or for loop
//   -->  loops/break.baml:30
//    | 
// 29 |             a = a + 1;
// 30 |             break;
// 31 |         }
//    | 
// error: Expected let expression or for loop
//   -->  loops/break.baml:33
//    | 
// 32 |         a = a + 1;
// 33 |         break;
// 34 |     }
//    | 
