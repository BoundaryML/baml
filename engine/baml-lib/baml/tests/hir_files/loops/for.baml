function Sum(xs: int[]) -> int {
	let result = 0;

	for (let x in xs) {
		result += x;
	}

	result
}

// function Sum(xs) {
//   let result = 0;
//   for x in xs {
//   result += x;
//   }
//
//   result
// }
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
