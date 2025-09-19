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
