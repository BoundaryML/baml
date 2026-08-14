// `with` is a CONTEXTUAL keyword: the runner clause of `spawn`/`test`/`testset`
// (`... with <runner> { ... }`) re-lexes it as `KW_WITH`, but it stays an
// ordinary identifier as a variable / field / member name.

// `with` as a keyword: the test runner clause. (`testset ... with <runner>` and
// `spawn ... with <opts>` re-lex `with` to the same `KW_WITH` token.)
test "test with runner" with testing.Quorum(5, 3) {
  let result = "hello"
  assert.not_null(result)
}

// `with` as an ordinary identifier (NOT a keyword): a variable and a field.
class HasWith {
  with int
}

function use_with_as_identifiers() -> int {
  let with = 42
  let h = HasWith { with: 1 }
  with + h.with
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// with_keyword.baml:1:1 (comment) len=80 "// `with` is a CONTEXTUAL keyword: the runner clause of `spawn`/`test`/`testset`"
// with_keyword.baml:2:1 (comment) len=74 "// (`... with <runner> { ... }`) re-lexes it as `KW_WITH`, but it stays an"
// with_keyword.baml:3:1 (comment) len=59 "// ordinary identifier as a variable / field / member name."
// with_keyword.baml:5:1 (comment) len=80 "// `with` as a keyword: the test runner clause. (`testset ... with <runner>` and"
// with_keyword.baml:6:1 (comment) len=70 "// `spawn ... with <opts>` re-lex `with` to the same `KW_WITH` token.)"
// with_keyword.baml:7:1 (keyword) len=4 "test"
// with_keyword.baml:7:6 (string) len=18 "\"test with runner\""
// with_keyword.baml:7:25 (keyword) len=4 "with"
// with_keyword.baml:7:30 (namespace) [defaultLibrary] len=7 "testing"
// with_keyword.baml:7:37 (namespace) [defaultLibrary] len=1 "."
// with_keyword.baml:7:38 (function) len=6 "Quorum"
// with_keyword.baml:7:45 (number) len=1 "5"
// with_keyword.baml:7:48 (number) len=1 "3"
// with_keyword.baml:8:3 (keyword) len=3 "let"
// with_keyword.baml:8:7 (variable) [declaration] len=6 "result"
// with_keyword.baml:8:14 (operator) len=1 "="
// with_keyword.baml:8:16 (string) len=7 "\"hello\""
// with_keyword.baml:9:3 (namespace) [defaultLibrary] len=6 "assert"
// with_keyword.baml:9:9 (namespace) [defaultLibrary] len=1 "."
// with_keyword.baml:9:10 (function) len=8 "not_null"
// with_keyword.baml:9:19 (variable) len=6 "result"
// with_keyword.baml:12:1 (comment) len=76 "// `with` as an ordinary identifier (NOT a keyword): a variable and a field."
// with_keyword.baml:13:1 (keyword) len=5 "class"
// with_keyword.baml:13:7 (class) [declaration] len=7 "HasWith"
// with_keyword.baml:14:3 (property) [declaration] len=4 "with"
// with_keyword.baml:14:8 (type) [defaultLibrary] len=3 "int"
// with_keyword.baml:17:1 (keyword) len=8 "function"
// with_keyword.baml:17:10 (function) [declaration] len=23 "use_with_as_identifiers"
// with_keyword.baml:17:39 (type) [defaultLibrary] len=3 "int"
// with_keyword.baml:18:3 (keyword) len=3 "let"
// with_keyword.baml:18:7 (variable) [declaration] len=4 "with"
// with_keyword.baml:18:12 (operator) len=1 "="
// with_keyword.baml:18:14 (number) len=2 "42"
// with_keyword.baml:19:3 (keyword) len=3 "let"
// with_keyword.baml:19:7 (variable) [declaration] len=1 "h"
// with_keyword.baml:19:9 (operator) len=1 "="
// with_keyword.baml:19:11 (class) len=7 "HasWith"
// with_keyword.baml:19:21 (property) len=4 "with"
// with_keyword.baml:19:27 (number) len=1 "1"
// with_keyword.baml:20:3 (variable) len=4 "with"
// with_keyword.baml:20:8 (operator) len=1 "+"
// with_keyword.baml:20:10 (variable) len=1 "h"
// with_keyword.baml:20:12 (property) len=4 "with"
