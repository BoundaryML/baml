// New-style programmatic test blocks: `testset` / `test { <code> }` with
// bindings, loops, dynamic names, and `assert` (BEP testset migration).
function Double(n: int) -> int {
  n * 2
}

testset "doubling" {
  test "simple" {
    let result = Double(21);
    assert.equal(result, 42);
  }

  let cases = ["a", "b", "c"];
  for (let c in cases) {
    test c {
      assert.equal(Double(1), 2);
    }
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// testset.baml:1:1 (comment) len=73 "// New-style programmatic test blocks: `testset` / `test { <code> }` with"
// testset.baml:2:1 (comment) len=72 "// bindings, loops, dynamic names, and `assert` (BEP testset migration)."
// testset.baml:3:1 (keyword) len=8 "function"
// testset.baml:3:10 (function) [declaration] len=6 "Double"
// testset.baml:3:17 (parameter) [declaration] len=1 "n"
// testset.baml:3:20 (type) [defaultLibrary] len=3 "int"
// testset.baml:3:28 (type) [defaultLibrary] len=3 "int"
// testset.baml:4:3 (parameter) len=1 "n"
// testset.baml:4:5 (operator) len=1 "*"
// testset.baml:4:7 (number) len=1 "2"
// testset.baml:7:1 (keyword) len=7 "testset"
// testset.baml:7:9 (string) len=10 "\"doubling\""
// testset.baml:8:3 (keyword) len=4 "test"
// testset.baml:8:8 (string) len=8 "\"simple\""
// testset.baml:9:5 (keyword) len=3 "let"
// testset.baml:9:9 (variable) [declaration] len=6 "result"
// testset.baml:9:16 (operator) len=1 "="
// testset.baml:9:18 (function) len=6 "Double"
// testset.baml:9:25 (number) len=2 "21"
// testset.baml:10:5 (namespace) [defaultLibrary] len=6 "assert"
// testset.baml:10:11 (namespace) [defaultLibrary] len=1 "."
// testset.baml:10:12 (function) len=5 "equal"
// testset.baml:10:18 (variable) len=6 "result"
// testset.baml:10:26 (number) len=2 "42"
// testset.baml:13:3 (keyword) len=3 "let"
// testset.baml:13:7 (variable) [declaration] len=5 "cases"
// testset.baml:13:13 (operator) len=1 "="
// testset.baml:13:16 (string) len=3 "\"a\""
// testset.baml:13:21 (string) len=3 "\"b\""
// testset.baml:13:26 (string) len=3 "\"c\""
// testset.baml:14:3 (keyword) len=3 "for"
// testset.baml:14:8 (keyword) len=3 "let"
// testset.baml:14:12 (variable) [declaration] len=1 "c"
// testset.baml:14:14 (keyword) len=2 "in"
// testset.baml:14:17 (variable) len=5 "cases"
// testset.baml:15:5 (keyword) len=4 "test"
// testset.baml:15:10 (variable) len=1 "c"
// testset.baml:16:7 (namespace) [defaultLibrary] len=6 "assert"
// testset.baml:16:13 (namespace) [defaultLibrary] len=1 "."
// testset.baml:16:14 (function) len=5 "equal"
// testset.baml:16:20 (function) len=6 "Double"
// testset.baml:16:27 (number) len=1 "1"
// testset.baml:16:31 (number) len=1 "2"
