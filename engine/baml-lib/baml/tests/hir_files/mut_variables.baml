function DeclareMutableInFunction(x: int) -> int {

	let y = 3;

	y = 5;

	y
}

function MutableInArg(x: int) -> int {
	x = 3;
	x
}

// function DeclareMutableInFunction(x) {
//   let y = 3;
//   y = 5;
//
//   y
// }
//
// function MutableInArg(x) {
//   x = 3;
//
//   x
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
//   -->  test.baml:27
//    | 
// 26 | // }
// 27 | <<<<<<< HEAD
// 28 | //
//    | 
