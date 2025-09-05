fn assertOk() -> int {

	assert 2 + 2 == 4;

	3
}

fn assertNotOk() -> int {
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
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// enum std::HttpMethod {
//   Get
// }
