function assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

function assertNotOk() -> int {
	assert 3 == 1;

	2
}

// function assertOk() {
//   assert 2 + 2 == 4;
//
//   3
// }
//
// function assertNotOk() {
//   assert 3 == 1;
//
//   2
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
