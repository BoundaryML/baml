// `match`, `throw`, and `catch` error handling.
function MayFail(x: int) -> string {
  match (x) {
    0 => throw "boom",
    _ => "ok"
  }
}

function Handled(x: int) -> string {
  MayFail(x) catch (e) {
    _ => "caught"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// catch_throw.baml:1:1 (comment) len=48 "// `match`, `throw`, and `catch` error handling."
// catch_throw.baml:2:1 (keyword) len=8 "function"
// catch_throw.baml:2:10 (function) [declaration] len=7 "MayFail"
// catch_throw.baml:2:18 (parameter) [declaration] len=1 "x"
// catch_throw.baml:2:21 (type) [defaultLibrary] len=3 "int"
// catch_throw.baml:2:29 (type) [defaultLibrary] len=6 "string"
// catch_throw.baml:3:3 (keyword) len=5 "match"
// catch_throw.baml:3:10 (parameter) len=1 "x"
// catch_throw.baml:4:5 (number) len=1 "0"
// catch_throw.baml:4:10 (keyword) len=5 "throw"
// catch_throw.baml:4:16 (string) len=6 "\"boom\""
// catch_throw.baml:5:10 (string) len=4 "\"ok\""
// catch_throw.baml:9:1 (keyword) len=8 "function"
// catch_throw.baml:9:10 (function) [declaration] len=7 "Handled"
// catch_throw.baml:9:18 (parameter) [declaration] len=1 "x"
// catch_throw.baml:9:21 (type) [defaultLibrary] len=3 "int"
// catch_throw.baml:9:29 (type) [defaultLibrary] len=6 "string"
// catch_throw.baml:10:3 (function) len=7 "MayFail"
// catch_throw.baml:10:11 (parameter) len=1 "x"
// catch_throw.baml:10:14 (keyword) len=5 "catch"
// catch_throw.baml:10:21 (parameter) [declaration] len=1 "e"
// catch_throw.baml:11:10 (string) len=8 "\"caught\""
