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
//
//   example.a
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
// class Example {
//   a: int
//   b: string
// }
//
// enum baml.HttpMethod {
//   Get
//   Post
//   Put
//   Delete
//   Patch
// }
