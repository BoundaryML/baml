function myFunc(x: int, y: string) -> int {
    x
}

function CallTest() -> int {
    let result = myFunc(42, "hello");
    result
}

// function myFunc(x, y) {
//   return x;
// }
//
// function CallTest() {
//   let result = myFunc(42, "hello");
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