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
// class Example {
//   a: int
//   b: string
// }
