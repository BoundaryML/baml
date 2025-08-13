fn Sum(xs: int[]) -> int {
	let mut result = 0;

	for x in xs {
		result += x;
	}

	result
}

// function Sum(xs) {
//   var result = 0;
//   for x in xs {
//   result += x;
//   }
//   return result;
// }
//
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// enum std::HttpMethod {
//   Get
// }