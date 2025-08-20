class Example {
  a int
  b string
}

function TestClass() -> int {
  let example = Example {
    a: 1,
    b: "hello",
  };

  example.a
}

// function TestClass() {
//   let example = Example { a: 1, b: "hello" };
//   return example.a;
// }
//
// class std::Request {
//   base_url: string
//   headers: map<string, string>
//   query_params: map<string, string>
// }
//
// class Example {
//   a: int
//   b: string
// }
//
// enum std::HttpMethod {
//   Get
// }
