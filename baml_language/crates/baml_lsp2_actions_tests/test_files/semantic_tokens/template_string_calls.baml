function WithParams(a: int) -> string {
  `Value: ${a}`
}

function GoodCall1() -> string {
  `${WithParams(a = 2)}`
}

function GoodCall2() -> string {
  `${WithParams(2)}`
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// template_string_calls.baml:1:1 (keyword) len=8 "function"
// template_string_calls.baml:1:10 (function) [declaration] len=10 "WithParams"
// template_string_calls.baml:1:21 (parameter) [declaration] len=1 "a"
// template_string_calls.baml:1:24 (type) [defaultLibrary] len=3 "int"
// template_string_calls.baml:1:32 (type) [defaultLibrary] len=6 "string"
// template_string_calls.baml:2:3 (string) len=1 "`"
// template_string_calls.baml:2:4 (string) len=5 "Value"
// template_string_calls.baml:2:9 (string) len=1 ":"
// template_string_calls.baml:2:13 (parameter) len=1 "a"
// template_string_calls.baml:2:15 (string) len=1 "`"
// template_string_calls.baml:5:1 (keyword) len=8 "function"
// template_string_calls.baml:5:10 (function) [declaration] len=9 "GoodCall1"
// template_string_calls.baml:5:25 (type) [defaultLibrary] len=6 "string"
// template_string_calls.baml:6:3 (string) len=1 "`"
// template_string_calls.baml:6:6 (function) len=10 "WithParams"
// template_string_calls.baml:6:17 (parameter) len=1 "a"
// template_string_calls.baml:6:19 (operator) len=1 "="
// template_string_calls.baml:6:21 (number) len=1 "2"
// template_string_calls.baml:6:24 (string) len=1 "`"
// template_string_calls.baml:9:1 (keyword) len=8 "function"
// template_string_calls.baml:9:10 (function) [declaration] len=9 "GoodCall2"
// template_string_calls.baml:9:25 (type) [defaultLibrary] len=6 "string"
// template_string_calls.baml:10:3 (string) len=1 "`"
// template_string_calls.baml:10:6 (function) len=10 "WithParams"
// template_string_calls.baml:10:17 (number) len=1 "2"
// template_string_calls.baml:10:20 (string) len=1 "`"
