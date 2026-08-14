// Phase 2: Verify that baml.json.parse / stringify / stringify_pretty compile
// and lower correctly through TIR and MIR.

function parse_and_roundtrip(s: string) -> string {
  let j: json = baml.json.parse(s)
  baml.json.stringify(j)
}

function parse_and_roundtrip_pretty(s: string) -> string {
  let j: json = baml.json.parse(s)
  baml.json.stringify_pretty(j)
}

function match_parsed(s: string) -> string {
  let j: json = baml.json.parse(s)
  match (j) {
    let arr: json[] => baml.json.stringify(arr)
    let m: map<string, json> => baml.json.stringify(m)
    let n: int => "int"
    let f: float => "float"
    let b: bool => "bool"
    let st: string => st
    null => "null"
  }
}

//----
//- diagnostics
// <no-diagnostics-expected>
//
//- semantic_tokens
// json_parse_stringify_intrinsics.baml:1:1 (comment) len=78 "// Phase 2: Verify that baml.json.parse / stringify / stringify_pretty compile"
// json_parse_stringify_intrinsics.baml:2:1 (comment) len=43 "// and lower correctly through TIR and MIR."
// json_parse_stringify_intrinsics.baml:4:1 (keyword) len=8 "function"
// json_parse_stringify_intrinsics.baml:4:10 (function) [declaration] len=19 "parse_and_roundtrip"
// json_parse_stringify_intrinsics.baml:4:30 (parameter) [declaration] len=1 "s"
// json_parse_stringify_intrinsics.baml:4:33 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:4:44 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:5:3 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:5:7 (variable) [declaration] len=1 "j"
// json_parse_stringify_intrinsics.baml:5:10 (type) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:5:15 (operator) len=1 "="
// json_parse_stringify_intrinsics.baml:5:17 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:5:21 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:5:22 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:5:26 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:5:27 (function) len=5 "parse"
// json_parse_stringify_intrinsics.baml:5:33 (parameter) len=1 "s"
// json_parse_stringify_intrinsics.baml:6:3 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:6:7 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:6:8 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:6:12 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:6:13 (function) len=9 "stringify"
// json_parse_stringify_intrinsics.baml:6:23 (variable) len=1 "j"
// json_parse_stringify_intrinsics.baml:9:1 (keyword) len=8 "function"
// json_parse_stringify_intrinsics.baml:9:10 (function) [declaration] len=26 "parse_and_roundtrip_pretty"
// json_parse_stringify_intrinsics.baml:9:37 (parameter) [declaration] len=1 "s"
// json_parse_stringify_intrinsics.baml:9:40 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:9:51 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:10:3 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:10:7 (variable) [declaration] len=1 "j"
// json_parse_stringify_intrinsics.baml:10:10 (type) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:10:15 (operator) len=1 "="
// json_parse_stringify_intrinsics.baml:10:17 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:10:21 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:10:22 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:10:26 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:10:27 (function) len=5 "parse"
// json_parse_stringify_intrinsics.baml:10:33 (parameter) len=1 "s"
// json_parse_stringify_intrinsics.baml:11:3 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:11:7 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:11:8 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:11:12 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:11:13 (function) len=16 "stringify_pretty"
// json_parse_stringify_intrinsics.baml:11:30 (variable) len=1 "j"
// json_parse_stringify_intrinsics.baml:14:1 (keyword) len=8 "function"
// json_parse_stringify_intrinsics.baml:14:10 (function) [declaration] len=12 "match_parsed"
// json_parse_stringify_intrinsics.baml:14:23 (parameter) [declaration] len=1 "s"
// json_parse_stringify_intrinsics.baml:14:26 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:14:37 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:15:3 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:15:7 (variable) [declaration] len=1 "j"
// json_parse_stringify_intrinsics.baml:15:10 (type) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:15:15 (operator) len=1 "="
// json_parse_stringify_intrinsics.baml:15:17 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:15:21 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:15:22 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:15:26 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:15:27 (function) len=5 "parse"
// json_parse_stringify_intrinsics.baml:15:33 (parameter) len=1 "s"
// json_parse_stringify_intrinsics.baml:16:3 (keyword) len=5 "match"
// json_parse_stringify_intrinsics.baml:16:10 (variable) len=1 "j"
// json_parse_stringify_intrinsics.baml:17:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:17:9 (variable) [declaration] len=3 "arr"
// json_parse_stringify_intrinsics.baml:17:14 (type) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:17:24 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:17:28 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:17:29 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:17:33 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:17:34 (function) len=9 "stringify"
// json_parse_stringify_intrinsics.baml:17:44 (variable) len=3 "arr"
// json_parse_stringify_intrinsics.baml:18:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:18:9 (variable) [declaration] len=1 "m"
// json_parse_stringify_intrinsics.baml:18:12 (type) [defaultLibrary] len=3 "map"
// json_parse_stringify_intrinsics.baml:18:15 (operator) len=1 "<"
// json_parse_stringify_intrinsics.baml:18:16 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:18:24 (type) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:18:28 (operator) len=1 ">"
// json_parse_stringify_intrinsics.baml:18:33 (namespace) [defaultLibrary] len=4 "baml"
// json_parse_stringify_intrinsics.baml:18:37 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:18:38 (namespace) [defaultLibrary] len=4 "json"
// json_parse_stringify_intrinsics.baml:18:42 (namespace) [defaultLibrary] len=1 "."
// json_parse_stringify_intrinsics.baml:18:43 (function) len=9 "stringify"
// json_parse_stringify_intrinsics.baml:18:53 (variable) len=1 "m"
// json_parse_stringify_intrinsics.baml:19:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:19:9 (variable) [declaration] len=1 "n"
// json_parse_stringify_intrinsics.baml:19:12 (type) [defaultLibrary] len=3 "int"
// json_parse_stringify_intrinsics.baml:19:19 (string) len=5 "\"int\""
// json_parse_stringify_intrinsics.baml:20:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:20:9 (variable) [declaration] len=1 "f"
// json_parse_stringify_intrinsics.baml:20:12 (type) [defaultLibrary] len=5 "float"
// json_parse_stringify_intrinsics.baml:20:21 (string) len=7 "\"float\""
// json_parse_stringify_intrinsics.baml:21:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:21:9 (variable) [declaration] len=1 "b"
// json_parse_stringify_intrinsics.baml:21:12 (type) [defaultLibrary] len=4 "bool"
// json_parse_stringify_intrinsics.baml:21:20 (string) len=6 "\"bool\""
// json_parse_stringify_intrinsics.baml:22:5 (keyword) len=3 "let"
// json_parse_stringify_intrinsics.baml:22:9 (variable) [declaration] len=2 "st"
// json_parse_stringify_intrinsics.baml:22:13 (type) [defaultLibrary] len=6 "string"
// json_parse_stringify_intrinsics.baml:22:23 (variable) len=2 "st"
// json_parse_stringify_intrinsics.baml:23:5 (type) [defaultLibrary] len=4 "null"
// json_parse_stringify_intrinsics.baml:23:13 (string) len=6 "\"null\""
