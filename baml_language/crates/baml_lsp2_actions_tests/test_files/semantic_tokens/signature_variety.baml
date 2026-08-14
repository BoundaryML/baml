function InputNone() -> int {
  client: "openai/gpt-4o"
  prompt: `fa`
}

function InputSimple(a: string, b: int | bool) -> int {
  client: "openai/gpt-4o"
  prompt: `fa`
}

class Email {
    subject string
    body string
}

function InputObject(email: Email) -> string {
  client: "openai/gpt-4o"
  prompt: ` subject line ${email.subject} `
}

enum Color {
  RED
  BLUE
  GREEN
}

function InputEnum(color: Color) -> string {
  client: "openai/gpt-4o"
  prompt: ` color ${color} `
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
  client: "openai/gpt-4o"
  prompt: ` color ${color} `
}


function InputImage(img: image) -> string {
  client: "openai/gpt-4o"
  prompt: ` image ${img} `
}

// Shorthand client strings must be quoted
function ExtractName(text: string) -> string {
  client: "openai/gpt-4o-mini"
  prompt: `
    Extract the person's name from this text: ${text}
    Return only the name, nothing else.
  `
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// signature_variety.baml:1:1 (keyword) len=8 "function"
// signature_variety.baml:1:10 (function) [declaration] len=9 "InputNone"
// signature_variety.baml:1:25 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:2:3 (property) len=6 "client"
// signature_variety.baml:2:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:3:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:3:11 (string) len=1 "`"
// signature_variety.baml:3:12 (string) len=2 "fa"
// signature_variety.baml:3:14 (string) len=1 "`"
// signature_variety.baml:6:1 (keyword) len=8 "function"
// signature_variety.baml:6:10 (function) [declaration] len=11 "InputSimple"
// signature_variety.baml:6:22 (parameter) [declaration] len=1 "a"
// signature_variety.baml:6:25 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:6:33 (parameter) [declaration] len=1 "b"
// signature_variety.baml:6:36 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:6:40 (operator) len=1 "|"
// signature_variety.baml:6:42 (type) [defaultLibrary] len=4 "bool"
// signature_variety.baml:6:51 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:7:3 (property) len=6 "client"
// signature_variety.baml:7:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:8:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:8:11 (string) len=1 "`"
// signature_variety.baml:8:12 (string) len=2 "fa"
// signature_variety.baml:8:14 (string) len=1 "`"
// signature_variety.baml:11:1 (keyword) len=5 "class"
// signature_variety.baml:11:7 (class) [declaration] len=5 "Email"
// signature_variety.baml:12:5 (property) [declaration] len=7 "subject"
// signature_variety.baml:12:13 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:13:5 (property) [declaration] len=4 "body"
// signature_variety.baml:13:10 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:16:1 (keyword) len=8 "function"
// signature_variety.baml:16:10 (function) [declaration] len=11 "InputObject"
// signature_variety.baml:16:22 (parameter) [declaration] len=5 "email"
// signature_variety.baml:16:29 (class) len=5 "Email"
// signature_variety.baml:16:39 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:17:3 (property) len=6 "client"
// signature_variety.baml:17:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:18:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:18:11 (string) len=1 "`"
// signature_variety.baml:18:13 (string) len=7 "subject"
// signature_variety.baml:18:21 (string) len=4 "line"
// signature_variety.baml:18:28 (parameter) len=5 "email"
// signature_variety.baml:18:34 (property) len=7 "subject"
// signature_variety.baml:18:43 (string) len=1 "`"
// signature_variety.baml:21:1 (keyword) len=4 "enum"
// signature_variety.baml:21:6 (enum) [declaration] len=5 "Color"
// signature_variety.baml:22:3 (enumMember) [declaration] len=3 "RED"
// signature_variety.baml:23:3 (enumMember) [declaration] len=4 "BLUE"
// signature_variety.baml:24:3 (enumMember) [declaration] len=5 "GREEN"
// signature_variety.baml:27:1 (keyword) len=8 "function"
// signature_variety.baml:27:10 (function) [declaration] len=9 "InputEnum"
// signature_variety.baml:27:20 (parameter) [declaration] len=5 "color"
// signature_variety.baml:27:27 (enum) len=5 "Color"
// signature_variety.baml:27:37 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:28:3 (property) len=6 "client"
// signature_variety.baml:28:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:29:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:29:11 (string) len=1 "`"
// signature_variety.baml:29:13 (string) len=5 "color"
// signature_variety.baml:29:21 (parameter) len=5 "color"
// signature_variety.baml:29:28 (string) len=1 "`"
// signature_variety.baml:32:1 (comment) len=41 "// Try it with wierd spacing and comments"
// signature_variety.baml:34:1 (keyword) len=8 "function"
// signature_variety.baml:34:10 (function) [declaration] len=10 "InputEnum2"
// signature_variety.baml:35:3 (parameter) [declaration] len=5 "color"
// signature_variety.baml:35:10 (enum) len=5 "Color"
// signature_variety.baml:36:3 (parameter) [declaration] len=3 "bar"
// signature_variety.baml:36:8 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:36:16 (comment) len=5 "// ok"
// signature_variety.baml:38:3 (comment) len=23 "// This param is great!"
// signature_variety.baml:39:3 (parameter) [declaration] len=3 "foo"
// signature_variety.baml:39:8 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:41:3 (comment) len=32 "// But we should do something..."
// signature_variety.baml:42:3 (parameter) [declaration] len=3 "baz"
// signature_variety.baml:42:9 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:42:16 (operator) len=1 "|"
// signature_variety.baml:42:18 (type) [defaultLibrary] len=3 "int"
// signature_variety.baml:44:1 (comment) len=6 "// Err"
// signature_variety.baml:46:1 (comment) len=15 "// Do something"
// signature_variety.baml:47:1 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:49:1 (comment) len=10 "// Or else"
// signature_variety.baml:51:3 (property) len=6 "client"
// signature_variety.baml:51:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:52:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:52:11 (string) len=1 "`"
// signature_variety.baml:52:13 (string) len=5 "color"
// signature_variety.baml:52:21 (parameter) len=5 "color"
// signature_variety.baml:52:28 (string) len=1 "`"
// signature_variety.baml:56:1 (keyword) len=8 "function"
// signature_variety.baml:56:10 (function) [declaration] len=10 "InputImage"
// signature_variety.baml:56:21 (parameter) [declaration] len=3 "img"
// signature_variety.baml:56:26 (type) [defaultLibrary] len=5 "image"
// signature_variety.baml:56:36 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:57:3 (property) len=6 "client"
// signature_variety.baml:57:11 (string) len=15 "\"openai/gpt-4o\""
// signature_variety.baml:58:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:58:11 (string) len=1 "`"
// signature_variety.baml:58:13 (string) len=5 "image"
// signature_variety.baml:58:21 (parameter) len=3 "img"
// signature_variety.baml:58:26 (string) len=1 "`"
// signature_variety.baml:61:1 (comment) len=42 "// Shorthand client strings must be quoted"
// signature_variety.baml:62:1 (keyword) len=8 "function"
// signature_variety.baml:62:10 (function) [declaration] len=11 "ExtractName"
// signature_variety.baml:62:22 (parameter) [declaration] len=4 "text"
// signature_variety.baml:62:28 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:62:39 (type) [defaultLibrary] len=6 "string"
// signature_variety.baml:63:3 (property) len=6 "client"
// signature_variety.baml:63:11 (string) len=20 "\"openai/gpt-4o-mini\""
// signature_variety.baml:64:3 (property) [declaration] len=6 "prompt"
// signature_variety.baml:64:11 (string) len=1 "`"
// signature_variety.baml:65:5 (string) len=7 "Extract"
// signature_variety.baml:65:13 (string) len=3 "the"
// signature_variety.baml:65:17 (string) len=6 "person"
// signature_variety.baml:65:23 (string) len=1 "'"
// signature_variety.baml:65:24 (string) len=1 "s"
// signature_variety.baml:65:26 (string) len=4 "name"
// signature_variety.baml:65:31 (string) len=4 "from"
// signature_variety.baml:65:36 (string) len=4 "this"
// signature_variety.baml:65:41 (string) len=4 "text"
// signature_variety.baml:65:45 (string) len=1 ":"
// signature_variety.baml:65:49 (parameter) len=4 "text"
// signature_variety.baml:66:5 (string) len=6 "Return"
// signature_variety.baml:66:12 (string) len=4 "only"
// signature_variety.baml:66:17 (string) len=3 "the"
// signature_variety.baml:66:21 (string) len=4 "name"
// signature_variety.baml:66:25 (string) len=1 ","
// signature_variety.baml:66:27 (string) len=7 "nothing"
// signature_variety.baml:66:35 (string) len=4 "else"
// signature_variety.baml:66:39 (string) len=1 "."
// signature_variety.baml:67:3 (string) len=1 "`"
