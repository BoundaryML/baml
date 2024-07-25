function InputNone() -> int {
  client "openai/gpt-4o"
  prompt #"fa"#
}

function InputSimple(a: string, b: int | bool) -> int {
  client "openai/gpt-4o"
  prompt #"fa"#
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client "openai/gpt-4o"
  prompt #" subject line {{ email.subject }} "#
}

enum Color {
  RED 
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client "openai/gpt-4o"
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
  client "openai/gpt-4o"
  prompt #" color {{ color }} "#
}


function InputImage(image: image) -> string {
  client "openai/gpt-4o"
  prompt #" image {{ image }} "#
}

// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:2
//    | 
//  1 | function InputNone() -> int {
//  2 |   client "openai/gpt-4o"
//    | 
// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:7
//    | 
//  6 | function InputSimple(a: string, b: int | bool) -> int {
//  7 |   client "openai/gpt-4o"
//    | 
// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:17
//    | 
// 16 | function InputObject(email: Email) -> string {
// 17 |   client "openai/gpt-4o"
//    | 
// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:28
//    | 
// 27 | function InputEnum(color: Color) -> string {
// 28 |   client "openai/gpt-4o"
//    | 
// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:51
//    | 
// 50 | {
// 51 |   client "openai/gpt-4o"
//    | 
// error: Client openai/gpt-4o not found.
//   -->  functions_v2/shorthand_clients.baml:57
//    | 
// 56 | function InputImage(image: image) -> string {
// 57 |   client "openai/gpt-4o"
//    | 
