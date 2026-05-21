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


function InputImage(img: image) -> string {
  client "openai/gpt-4o"
  prompt #" image {{ img }} "#
}

// Test unquoted client string (no quotes around the client name)
function ExtractName(text: string) -> string {
  client openai/gpt-4o-mini
  prompt #"
    Extract the person's name from this text: {{ text }}
    Return only the name, nothing else.
  "#
}

//----
//- diagnostics
// error: unresolved name: openai
//     ╭─[ functions_v2_shorthand_clients.baml:62:45 ]
//     │
//  62 │ ╭─▶ function ExtractName(text: string) -> string {
//     ┆ ┆   
//  68 │ ├─▶ }
//     │ │      
//     │ ╰────── unresolved name: openai
//     │     
//     │     Note: Error code: E0003
// ────╯
// error: unresolved name: openai
//     ╭─[ functions_v2_shorthand_clients.baml:59:2 ]
//     │
//  59 │ ╭─▶ }
//     ┆ ┆   
//  68 │ ├─▶ }
//     │ │      
//     │ ╰────── unresolved name: openai
//     │     
//     │     Note: Error code: E0003
// ────╯
