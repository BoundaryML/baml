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
<<<<<<< HEAD
// class baml.HttpRequest {
//   url: string
//   method: baml.HttpMethod
//   headers: (map<string, string> | null)
//   query_params: (map<string, string> | null)
//   json: (ANY | null)
// }
//
=======
>>>>>>> canary
// class Example {
//   a: int
//   b: string
// }
<<<<<<< HEAD
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
//   -->  test.baml:21
//    | 
// 20 | //
// 21 | <<<<<<< HEAD
// 22 | // class baml.HttpRequest {
//    | 
