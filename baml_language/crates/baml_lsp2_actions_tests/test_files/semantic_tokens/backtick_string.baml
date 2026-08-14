// BEP-049 backtick interpolated strings: literal text is `string`, while
// `${ expr }` interpolations are real code (resolved through the index).
function Greeting(name: string, count: int) -> string {
  `Hello ${name}, you have ${count} messages`
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// backtick_string.baml:1:1 (comment) len=73 "// BEP-049 backtick interpolated strings: literal text is `string`, while"
// backtick_string.baml:2:1 (comment) len=73 "// `${ expr }` interpolations are real code (resolved through the index)."
// backtick_string.baml:3:1 (keyword) len=8 "function"
// backtick_string.baml:3:10 (function) [declaration] len=8 "Greeting"
// backtick_string.baml:3:19 (parameter) [declaration] len=4 "name"
// backtick_string.baml:3:25 (type) [defaultLibrary] len=6 "string"
// backtick_string.baml:3:33 (parameter) [declaration] len=5 "count"
// backtick_string.baml:3:40 (type) [defaultLibrary] len=3 "int"
// backtick_string.baml:3:48 (type) [defaultLibrary] len=6 "string"
// backtick_string.baml:4:3 (string) len=1 "`"
// backtick_string.baml:4:4 (string) len=5 "Hello"
// backtick_string.baml:4:12 (parameter) len=4 "name"
// backtick_string.baml:4:17 (string) len=1 ","
// backtick_string.baml:4:19 (string) len=3 "you"
// backtick_string.baml:4:23 (string) len=4 "have"
// backtick_string.baml:4:30 (parameter) len=5 "count"
// backtick_string.baml:4:37 (string) len=8 "messages"
// backtick_string.baml:4:45 (string) len=1 "`"
