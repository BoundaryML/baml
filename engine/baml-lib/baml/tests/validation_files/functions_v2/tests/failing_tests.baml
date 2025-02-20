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

// error: Test "Foo" is already defined for function "InputImage"
//   -->  functions_v2/tests/failing_tests.baml:72
//    | 
// 71 | test Foo {
// 72 |   functions [InputImage]
//    | 
//   -->  functions_v2/tests/failing_tests.baml:66
//    | 
// 65 | test Foo {
// 66 |   functions [InputImage]
//    | 
// error: Test "Foo" is already defined for function "InputImage"
//   -->  functions_v2/tests/failing_tests.baml:66
//    | 
// 65 | test Foo {
// 66 |   functions [InputImage]
//    | 
//   -->  functions_v2/tests/failing_tests.baml:72
//    | 
// 71 | test Foo {
// 72 |   functions [InputImage]
//    | 
// error: Test "Bar" is already defined for function "InputEnum"
//   -->  functions_v2/tests/failing_tests.baml:78
//    | 
// 77 | test Bar {
// 78 |   functions [InputEnum, InputEnum]
//    | 
//   -->  functions_v2/tests/failing_tests.baml:78
//    | 
// 77 | test Bar {
// 78 |   functions [InputEnum, InputEnum]
//    | 
// error: Test "Bar" is already defined for function "InputEnum"
//   -->  functions_v2/tests/failing_tests.baml:78
//    | 
// 77 | test Bar {
// 78 |   functions [InputEnum, InputEnum]
//    | 
//   -->  functions_v2/tests/failing_tests.baml:78
//    | 
// 77 | test Bar {
// 78 |   functions [InputEnum, InputEnum]
//    | 
