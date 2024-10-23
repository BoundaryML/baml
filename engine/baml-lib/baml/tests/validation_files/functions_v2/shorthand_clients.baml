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

// Try it with weird spacing and comments

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