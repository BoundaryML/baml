function Greeting(name: string) -> string {
  `Hello, ${name}!`
}

function NoParams() -> string {
  `A static template.`
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// template_string.baml:1:1 (keyword) len=8 "function"
// template_string.baml:1:10 (function) [declaration] len=8 "Greeting"
// template_string.baml:1:19 (parameter) [declaration] len=4 "name"
// template_string.baml:1:25 (type) [defaultLibrary] len=6 "string"
// template_string.baml:1:36 (type) [defaultLibrary] len=6 "string"
// template_string.baml:2:3 (string) len=1 "`"
// template_string.baml:2:4 (string) len=5 "Hello"
// template_string.baml:2:9 (string) len=1 ","
// template_string.baml:2:13 (parameter) len=4 "name"
// template_string.baml:2:18 (string) len=1 "!"
// template_string.baml:2:19 (string) len=1 "`"
// template_string.baml:5:1 (keyword) len=8 "function"
// template_string.baml:5:10 (function) [declaration] len=8 "NoParams"
// template_string.baml:5:24 (type) [defaultLibrary] len=6 "string"
// template_string.baml:6:3 (string) len=1 "`"
// template_string.baml:6:4 (string) len=1 "A"
// template_string.baml:6:6 (string) len=6 "static"
// template_string.baml:6:13 (string) len=8 "template"
// template_string.baml:6:21 (string) len=1 "."
// template_string.baml:6:22 (string) len=1 "`"
