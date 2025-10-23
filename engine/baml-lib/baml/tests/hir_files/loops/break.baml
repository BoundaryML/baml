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
     // should be 2
    a
}

// function Factorial(limit) {
//   let result = 1;
//   while true {
//   if limit == 0 {
//     break;
//     } 
//     result = result * limit;
//     limit = limit - 1;
//   }
//
//   result
// }
//
// function Nested() {
//   let a = 5;
//   while true {
//   while true {
//     a = a + 1;
//       break;
//     }
//     a = a + 1;
//     break;
//   }
//
//   a
// }
