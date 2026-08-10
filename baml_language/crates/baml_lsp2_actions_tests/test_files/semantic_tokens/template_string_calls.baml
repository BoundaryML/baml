template_string WithParams(a: int) `
  ...
`

template_string GoodCall1 `
  ${WithParams(a=2)}
`

template_string GoodCall2 `
  ${WithParams(2)}
`

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// template_string_calls.baml:1:1 (keyword) len=15 "template_string"
// template_string_calls.baml:1:17 (function) [declaration] len=10 "WithParams"
// template_string_calls.baml:1:28 (parameter) [declaration] len=1 "a"
// template_string_calls.baml:1:31 (type) [defaultLibrary] len=3 "int"
// template_string_calls.baml:1:36 (string) len=1 "`"
// template_string_calls.baml:2:3 (string) len=3 "..."
// template_string_calls.baml:3:1 (string) len=1 "`"
// template_string_calls.baml:5:1 (keyword) len=15 "template_string"
// template_string_calls.baml:5:17 (function) [declaration] len=9 "GoodCall1"
// template_string_calls.baml:5:27 (string) len=1 "`"
// template_string_calls.baml:6:16 (parameter) len=1 "a"
// template_string_calls.baml:6:17 (operator) len=1 "="
// template_string_calls.baml:6:18 (number) len=1 "2"
// template_string_calls.baml:7:1 (string) len=1 "`"
// template_string_calls.baml:9:1 (keyword) len=15 "template_string"
// template_string_calls.baml:9:17 (function) [declaration] len=9 "GoodCall2"
// template_string_calls.baml:9:27 (string) len=1 "`"
// template_string_calls.baml:10:16 (number) len=1 "2"
// template_string_calls.baml:11:1 (string) len=1 "`"
