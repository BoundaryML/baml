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
