// A free generic function called with explicit type args at the call site:
// `identity<int>(5)`. The callee name (`identity`) sits in a `PATH_EXPR` whose
// segment span covers the whole `identity<int>`, so the call-site name must be
// narrowed to match the WORD token; the `<int>` arg resolves as a type.
function identity<T>(x: T) -> T {
  x
}

function use_identity() -> int {
  let a = identity<int>(5);
  a
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// generic_function_call.baml:1:1 (comment) len=75 "// A free generic function called with explicit type args at the call site:"
// generic_function_call.baml:2:1 (comment) len=79 "// `identity<int>(5)`. The callee name (`identity`) sits in a `PATH_EXPR` whose"
// generic_function_call.baml:3:1 (comment) len=79 "// segment span covers the whole `identity<int>`, so the call-site name must be"
// generic_function_call.baml:4:1 (comment) len=72 "// narrowed to match the WORD token; the `<int>` arg resolves as a type."
// generic_function_call.baml:5:1 (keyword) len=8 "function"
// generic_function_call.baml:5:10 (function) [declaration] len=8 "identity"
// generic_function_call.baml:5:18 (operator) len=1 "<"
// generic_function_call.baml:5:19 (typeParameter) [declaration] len=1 "T"
// generic_function_call.baml:5:20 (operator) len=1 ">"
// generic_function_call.baml:5:22 (parameter) [declaration] len=1 "x"
// generic_function_call.baml:5:25 (type) len=1 "T"
// generic_function_call.baml:5:31 (type) len=1 "T"
// generic_function_call.baml:6:3 (parameter) len=1 "x"
// generic_function_call.baml:9:1 (keyword) len=8 "function"
// generic_function_call.baml:9:10 (function) [declaration] len=12 "use_identity"
// generic_function_call.baml:9:28 (type) [defaultLibrary] len=3 "int"
// generic_function_call.baml:10:3 (keyword) len=3 "let"
// generic_function_call.baml:10:7 (variable) [declaration] len=1 "a"
// generic_function_call.baml:10:9 (operator) len=1 "="
// generic_function_call.baml:10:11 (function) len=8 "identity"
// generic_function_call.baml:10:19 (operator) len=1 "<"
// generic_function_call.baml:10:20 (type) [defaultLibrary] len=3 "int"
// generic_function_call.baml:10:23 (operator) len=1 ">"
// generic_function_call.baml:10:25 (number) len=1 "5"
// generic_function_call.baml:11:3 (variable) len=1 "a"
