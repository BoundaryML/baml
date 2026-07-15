function risky() -> void throws string {
  throw "boom"
}

test "throwing body becomes failure" {
  risky()
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// test_expr_throwing_body.baml:1:1 (keyword) len=8 "function"
// test_expr_throwing_body.baml:1:10 (function) [declaration] len=5 "risky"
// test_expr_throwing_body.baml:1:21 (type) [defaultLibrary] len=4 "void"
// test_expr_throwing_body.baml:1:26 (keyword) len=6 "throws"
// test_expr_throwing_body.baml:1:33 (type) [defaultLibrary] len=6 "string"
// test_expr_throwing_body.baml:2:3 (keyword) len=5 "throw"
// test_expr_throwing_body.baml:2:9 (string) len=6 "\"boom\""
// test_expr_throwing_body.baml:5:1 (keyword) len=4 "test"
// test_expr_throwing_body.baml:5:6 (string) len=31 "\"throwing body becomes failure\""
// test_expr_throwing_body.baml:6:3 (function) len=5 "risky"
