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
<<<<<<< HEAD
//
// class baml.HttpRequest {
//   url: string
//   method: baml.HttpMethod
//   headers: (map<string, string> | null)
//   query_params: (map<string, string> | null)
//   json: (ANY | null)
// }
//
// enum baml.HttpMethod {
//   Get
//   Post
//   Put
//   Delete
//   Patch
// }
=======
>>>>>>> canary

// error: Error validating: This line is invalid. It does not start with any known Baml schema keyword.
//   -->  test.baml:57
//    | 
// 56 | // }
// 57 | <<<<<<< HEAD
// 58 | //
//    | 
