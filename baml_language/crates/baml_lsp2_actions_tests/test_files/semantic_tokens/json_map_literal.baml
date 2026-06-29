// Phase 1: Verify that nested map/array literals type-check against the
// recursive `json` alias body.

function NestedMapLiteral() -> json {
  {"a": 1, "b": [2, 3], "c": {"nested": null}}
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// json_map_literal.baml:1:1 (comment) len=72 "// Phase 1: Verify that nested map/array literals type-check against the"
// json_map_literal.baml:2:1 (comment) len=31 "// recursive `json` alias body."
// json_map_literal.baml:4:1 (keyword) len=8 "function"
// json_map_literal.baml:4:10 (function) [declaration] len=16 "NestedMapLiteral"
// json_map_literal.baml:4:32 (type) [defaultLibrary] len=4 "json"
// json_map_literal.baml:5:4 (string) len=3 "\"a\""
// json_map_literal.baml:5:9 (number) len=1 "1"
// json_map_literal.baml:5:12 (string) len=3 "\"b\""
// json_map_literal.baml:5:18 (number) len=1 "2"
// json_map_literal.baml:5:21 (number) len=1 "3"
// json_map_literal.baml:5:25 (string) len=3 "\"c\""
// json_map_literal.baml:5:31 (string) len=8 "\"nested\""
// json_map_literal.baml:5:41 (type) [defaultLibrary] len=4 "null"
