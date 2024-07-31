client<llm> Bar {
  provider baml-openai-chat
}

function InputNone() -> int {
  client Bar
  prompt #"fa"#
}

function InputSimple(a: string, b: int | bool) -> int {
  client Bar
  prompt #"fa"#
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client Bar
  prompt #" subject line {{ email.subject }} "#
}

enum Color {
  RED 
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client Bar
  prompt #" color {{ color }} "#
}

// Try it with wierd spacing and comments

function InputEnum2(
  color: Color, 
  bar: string, // ok

  // This param is great!
  foo: string[],

  // But we should do something...
  baz: (string | int?)[]
)
// Err
-> 
// Do something
string

// Or else
{
  client Bar
  prompt #" color {{ color }} "#
}


function InputImage(image: image) -> string {
  client Bar
  prompt #" image {{ image }} "#
}

test Foo {
  functions [InputImage]
  args {
  }
}

test Foo {
  functions [InputImage]
  args {
  }
}

test Bar {
  functions [InputEnum, InputEnum]
  args {
  }
}

// error: Error validating function "Bar": Invalid function syntax. Valid function syntax is
// ```
// function Bar(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/tests/failing_tests.baml:1
//    | 
//    | 
//  1 | client<llm> Bar {
//  2 |   provider baml-openai-chat
//  3 | }
//    | 
// error: Error validating function "Foo": Invalid function syntax. Valid function syntax is
// ```
// function Foo(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/tests/failing_tests.baml:65
//    | 
// 64 | 
// 65 | test Foo {
// 66 |   functions [InputImage]
// 67 |   args {
// 68 |   }
// 69 | }
//    | 
// error: Error validating function "Foo": Invalid function syntax. Valid function syntax is
// ```
// function Foo(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/tests/failing_tests.baml:71
//    | 
// 70 | 
// 71 | test Foo {
// 72 |   functions [InputImage]
// 73 |   args {
// 74 |   }
// 75 | }
//    | 
// error: Error validating function "Bar": Invalid function syntax. Valid function syntax is
// ```
// function Bar(param1: String, param2: String) -> ReturnType {
//     client SomeClient
//     prompt #"..."#
// }
// ```
//   -->  functions_v2/tests/failing_tests.baml:77
//    | 
// 76 | 
// 77 | test Bar {
// 78 |   functions [InputEnum, InputEnum]
// 79 |   args {
// 80 |   }
// 81 | }
//    | 
