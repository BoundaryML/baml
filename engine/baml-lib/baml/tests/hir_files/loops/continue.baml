function Factorial(limit: int) -> int {

	let result = 1;


    // used to make the loop break without relying on `break` implementation.
    let should_continue = true;
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

function ContinueNested() -> int {
    while (true) {
        while (true) {
            continue;
        }
    }
}

// function Factorial(limit) {
//   let result = 1;
//   let should_continue = true;
//   while should_continue {
//   result = result * limit;
//     limit = limit - 1;
//     if limit != 0 {
//     continue;
//     } else {
//     should_continue = false;
//     } 
//   }
//
//   result
// }
//
// function ContinueNested() {
//   while true {
//   while true {
//     continue;
//     }
//   }
// }
