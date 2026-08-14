// The catch binding `e` should highlight as a parameter, consistently between
// its declaration and its uses in the catch body.

function MayFail(x: int) -> string throws string {
  throw "boom"
}

function Handled(x: int) -> string {
  MayFail(x) catch (e) {
    _ => e
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// catch_param.baml:1:1 (comment) len=78 "// The catch binding `e` should highlight as a parameter, consistently between"
// catch_param.baml:2:1 (comment) len=50 "// its declaration and its uses in the catch body."
// catch_param.baml:4:1 (keyword) len=8 "function"
// catch_param.baml:4:10 (function) [declaration] len=7 "MayFail"
// catch_param.baml:4:18 (parameter) [declaration] len=1 "x"
// catch_param.baml:4:21 (type) [defaultLibrary] len=3 "int"
// catch_param.baml:4:29 (type) [defaultLibrary] len=6 "string"
// catch_param.baml:4:36 (keyword) len=6 "throws"
// catch_param.baml:4:43 (type) [defaultLibrary] len=6 "string"
// catch_param.baml:5:3 (keyword) len=5 "throw"
// catch_param.baml:5:9 (string) len=6 "\"boom\""
// catch_param.baml:8:1 (keyword) len=8 "function"
// catch_param.baml:8:10 (function) [declaration] len=7 "Handled"
// catch_param.baml:8:18 (parameter) [declaration] len=1 "x"
// catch_param.baml:8:21 (type) [defaultLibrary] len=3 "int"
// catch_param.baml:8:29 (type) [defaultLibrary] len=6 "string"
// catch_param.baml:9:3 (function) len=7 "MayFail"
// catch_param.baml:9:11 (parameter) len=1 "x"
// catch_param.baml:9:14 (keyword) len=5 "catch"
// catch_param.baml:9:21 (parameter) [declaration] len=1 "e"
// catch_param.baml:10:10 (parameter) len=1 "e"
