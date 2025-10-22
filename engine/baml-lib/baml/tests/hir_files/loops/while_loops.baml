function GCD(a: int, b: int) -> int {

	while (a != b) {
	   if (a > b) {
	   	a = a - b;
	   } else {
	   	b = b - a;
	   }
	}

	a
}

// function GCD(a, b) {
//   while a != b {
//   if a > b {
//     a = a - b;
//     } else {
//     b = b - a;
//     } 
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
//   -->  test.baml:25
//    | 
// 24 | // }
// 25 | <<<<<<< HEAD
// 26 | //
//    | 
